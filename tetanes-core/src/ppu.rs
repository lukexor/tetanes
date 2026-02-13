//! NES PPU (Picture Processing Unit) implementation.

use crate::{
    common::{Clock, ClockTo, NesRegion, Regional, Reset, ResetKind},
    cpu::Cpu,
    debug::PpuDebugger,
    mapper::{BusKind, Mapper, OnBusRead, OnBusWrite},
    mem::{ConstSlice, RamState, Read, Write},
    ppu::{bus::Bus, frame::Frame},
};
use ctrl::Ctrl;
use mask::Mask;
use scroll::Scroll;
use serde::{Deserialize, Serialize};
use sprite::Sprite;
use status::Status;
use std::cmp::Ordering;
use tracing::trace;

pub mod bus;
pub mod ctrl;
pub mod frame;
pub mod mask;
pub mod scroll;
pub mod sprite;
pub mod status;

/// Nametable Mirroring Mode
///
/// <https://wiki.nesdev.org/w/index.php/Mirroring#Nametable_Mirroring>
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum Mirroring {
    Vertical = 0,
    #[default]
    Horizontal = 1,
    SingleScreenA = 2,
    SingleScreenB = 3,
    FourScreen = 4,
}

/// Trait for PPU Registers.
pub trait Registers {
    /// $2000 PPUCTRL
    fn write_ctrl(&mut self, val: u8);
    /// Write $2001 PPUMASK
    fn write_mask(&mut self, val: u8);
    /// Read $2002 PPUSTATUS
    fn read_status(&mut self) -> u8;
    /// Peek $2002 PPUSTATUS
    fn peek_status(&self) -> u8;
    /// Write $2003 OAMADDR
    fn write_oamaddr(&mut self, val: u8);
    /// Read $2004 OAMDATA
    fn read_oamdata(&mut self) -> u8;
    /// Peek $2004 OAMDATA
    fn peek_oamdata(&self) -> u8;
    /// Write $2004 OAMDATA
    fn write_oamdata(&mut self, val: u8);
    /// Write $2005 PPUSCROLL
    fn write_scroll(&mut self, val: u8);
    /// Write $2006 PPUADDR
    fn write_addr(&mut self, val: u8);
    /// Read $2007 PPUDATA
    fn read_data(&mut self) -> u8;
    /// Peek $2007 PPUDATA
    fn peek_data(&self) -> u8;
    /// Write $2007 PPUDATA
    fn write_data(&mut self, val: u8);
}

/// NES PPU.
///
/// See: <https://wiki.nesdev.org/w/index.php/PPU>
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Ppu {
    /// Master clock.
    pub master_clock: u64,
    /// Master clock divider.
    pub clock_divider: u64,
    /// (0, 340) cycles per scanline.
    pub cycle: u32,
    /// (0, 261) NTSC or (0, 311) PAL/Dendy scanlines per frame.
    pub scanline: u32,
    /// Scanline that Vertical Blank (VBlank) starts on.
    pub vblank_scanline: u32,
    /// Scanline that Prerender starts on.
    pub prerender_scanline: u32,
    /// Scanline that Sprite Evaluation for PAL starts on.
    pub pal_spr_eval_scanline: u32,
    /// Whether PPU is skipping rendering (used for
    /// [`HeadlessMode`](crate::control_deck::HeadlessMode)).
    pub skip_rendering: bool,

    /// $2005 PPUSCROLL and $2006 PPUADDR (write-only).
    pub scroll: Scroll,
    /// $2001 PPUMASK (write-only).
    pub mask: Mask,
    /// $2000 PPUCTRL (write-only).
    pub ctrl: Ctrl,
    /// $2002 PPUSTATUS (read-only).
    pub status: Status,
    /// PPU Memory/Data Bus.
    pub bus: Bus,

    pub curr_palette: u8,
    pub prev_palette: u8,
    pub next_palette: u8,
    pub tile_shift_lo: u16,
    pub tile_shift_hi: u16,
    pub tile_lo: u8,
    pub tile_hi: u8,
    pub tile_addr: u16,

    pub oamaddr_lo: u8,
    pub oamaddr_hi: u8,
    /// $2003 OAM addr (write-only).
    pub oamaddr: u8,
    pub oam_fetch: u8,
    pub oam_eval_done: bool,
    pub secondary_oamaddr: u8,
    pub overflow_count: u8,
    pub spr_in_range: bool,
    pub spr_zero_in_range: bool,
    pub spr_zero_visible: bool,
    pub spr_count: usize,
    /// $2007 PPUDATA buffer.
    pub vram_buffer: u8,

    /// $2004 Object Attribute Memory (OAM) data (read/write).
    pub oamdata: ConstSlice<u8, 256>,
    /// Secondary OAM data on a given scanline.
    pub secondary_oamdata: ConstSlice<u8, 32>,
    /// Each scanline can hold 8 sprites at a time before the `spr_overflow` flag is set.
    pub sprites: [Sprite; 8],
    /// Whether a sprite is present at the given x-coordinate. Used for `spr_zero_hit` detection.
    // This is a per-frame optimization, shouldn't need to be saved
    #[serde(skip)]
    pub spr_present: ConstSlice<bool, 256>,

    pub prevent_vbl: bool,
    pub frame: Frame,

    pub region: NesRegion,
    pub cycle_count: usize,
    /// Internal signal that clears status registers and prevents writes and cleared at the end of
    /// VBlank.
    ///
    /// See: <https://www.nesdev.org/wiki/PPU_power_up_state>
    pub reset_signal: bool,
    pub emulate_warmup: bool,

    pub open_bus: u8,

    // Don't save debug state
    #[serde(skip)]
    pub debugger: Option<PpuDebugger>,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(NesRegion::Ntsc, RamState::default())
    }
}

impl Ppu {
    pub const WIDTH: u32 = 256;
    pub const HEIGHT: u32 = 240;
    pub const SIZE: usize = (Self::WIDTH * Self::HEIGHT) as usize;

    pub const NT_START: u16 = 0x2000;
    pub const NT_SIZE: u16 = 0x0400;
    pub const ATTR_OFFSET: u16 = 0x03C0;
    pub const PALETTE_START: u16 = 0x3F00;
    pub const PALETTE_END: u16 = 0x3F20;

    pub const OAM_SIZE: usize = 256; // 64 4-byte sprites per frame
    pub const SECONDARY_OAM_SIZE: usize = 32; // 8 4-byte sprites per scanline

    // Cycles
    // https://www.nesdev.org/wiki/PPU_rendering
    pub const VBLANK: u32 = 1; // When VBlank flag gets set
    pub const VISIBLE_START: u32 = 1; // Tile data fetching starts
    pub const OAM_CLEAR_START: u32 = 1;
    pub const OAM_CLEAR_END: u32 = 64;
    pub const SPR_EVAL_START: u32 = 65;
    pub const SPR_EVAL_END: u32 = 256;
    pub const INC_Y: u32 = 256; // Increase Y scroll when it reaches end of the screen
    pub const VISIBLE_END: u32 = 256; // 2 cycles each for 4 fetches = 32 tiles
    pub const SPR_FETCH_START: u32 = 257; // Sprites for next scanline fetch starts
    pub const COPY_Y_START: u32 = 280; // Copy Y scroll start
    pub const COPY_Y_END: u32 = 304; // Copy Y scroll stop
    pub const SPR_FETCH_END: u32 = 320; // 2 cycles each for 4 fetches = 8 sprites
    pub const BG_PREFETCH_START: u32 = 321; // Tile data for next scanline fetched
    pub const BG_PREFETCH_END: u32 = 336; // 2 cycles each for 4 fetches = 2 tiles
    pub const BG_DUMMY_START: u32 = 337; // Dummy fetches - use is unknown
    pub const ODD_SKIP: u32 = 339; // Odd frames skip the last cycle
    pub const CYCLE_END: u32 = 340; // 2 cycles each for 2 fetches

    // Scanlines
    pub const VISIBLE_SCANLINE_END: u32 = 239; // Rendering graphics for the screen
    pub const PRERENDER_SCANLINE_NTSC: u32 = 261;
    pub const PRERENDER_SCANLINE_PAL: u32 = 311;
    pub const PRERENDER_SCANLINE_DENDY: u32 = Self::PRERENDER_SCANLINE_PAL;
    pub const VBLANK_SCANLINE_NTSC: u32 = 241;
    pub const VBLANK_SCANLINE_PAL: u32 = Self::VBLANK_SCANLINE_NTSC;
    pub const VBLANK_SCANLINE_DENDY: u32 = 291;

    // Clock
    pub const CLOCK_DIVIDER_NTSC: u64 = 4;
    pub const CLOCK_DIVIDER_PAL: u64 = 5;
    pub const CLOCK_DIVIDER_DENDY: u64 = Self::CLOCK_DIVIDER_PAL;

    pub const NTSC_PALETTE: &'static [u8] = include_bytes!("../ntscpalette.pal");

    /// NES PPU System Palette
    /// 64 total possible colors, though only 32 can be loaded at a time
    #[rustfmt::skip]
    pub const SYSTEM_PALETTE: [(u8,u8,u8); 64] = [
        // 0x00
        (0x54, 0x54, 0x54), (0x00, 0x1E, 0x74), (0x08, 0x10, 0x90), (0x30, 0x00, 0x88), // $00-$03
        (0x44, 0x00, 0x64), (0x5C, 0x00, 0x30), (0x54, 0x04, 0x00), (0x3C, 0x18, 0x00), // $04-$07
        (0x20, 0x2A, 0x00), (0x08, 0x3A, 0x00), (0x00, 0x40, 0x00), (0x00, 0x3C, 0x00), // $08-$0B
        (0x00, 0x32, 0x3C), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $0C-$0F
        // 0x10
        (0x98, 0x96, 0x98), (0x08, 0x4C, 0xC4), (0x30, 0x32, 0xEC), (0x5C, 0x1E, 0xE4), // $10-$13
        (0x88, 0x14, 0xB0), (0xA0, 0x14, 0x64), (0x98, 0x22, 0x20), (0x78, 0x3C, 0x00), // $14-$17
        (0x54, 0x5A, 0x00), (0x28, 0x72, 0x00), (0x08, 0x7C, 0x00), (0x00, 0x76, 0x28), // $18-$1B
        (0x00, 0x66, 0x78), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $1C-$1F
        // 0x20
        (0xEC, 0xEE, 0xEC), (0x4C, 0x9A, 0xEC), (0x78, 0x7C, 0xEC), (0xB0, 0x62, 0xEC), // $20-$23
        (0xE4, 0x54, 0xEC), (0xEC, 0x58, 0xB4), (0xEC, 0x6A, 0x64), (0xD4, 0x88, 0x20), // $24-$27
        (0xA0, 0xAA, 0x00), (0x74, 0xC4, 0x00), (0x4C, 0xD0, 0x20), (0x38, 0xCC, 0x6C), // $28-$2B
        (0x38, 0xB4, 0xCC), (0x3C, 0x3C, 0x3C), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $2C-$2F
        // 0x30
        (0xEC, 0xEE, 0xEC), (0xA8, 0xCC, 0xEC), (0xBC, 0xBC, 0xEC), (0xD4, 0xB2, 0xEC), // $30-$33
        (0xEC, 0xAE, 0xEC), (0xEC, 0xAE, 0xD4), (0xEC, 0xB4, 0xB0), (0xE4, 0xC4, 0x90), // $34-$37
        (0xCC, 0xD2, 0x78), (0xB4, 0xDE, 0x78), (0xA8, 0xE2, 0x90), (0x98, 0xE2, 0xB4), // $38-$3B
        (0xA0, 0xD6, 0xE4), (0xA0, 0xA2, 0xA0), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), // $3C-$3F
    ];

    /// Create a new PPU instance.
    pub fn new(region: NesRegion, ram_state: RamState) -> Self {
        let mut ppu = Self {
            master_clock: 0,
            clock_divider: 0,
            cycle: 0,
            scanline: 0,
            vblank_scanline: 0,
            prerender_scanline: 0,
            pal_spr_eval_scanline: 0,
            skip_rendering: false,

            scroll: Scroll::new(),
            mask: Mask::new(region),
            ctrl: Ctrl::new(),
            status: Status::new(),
            bus: Bus::new(ram_state),

            prev_palette: 0x00,
            curr_palette: 0x00,
            next_palette: 0x00,
            tile_shift_lo: 0x0000,
            tile_shift_hi: 0x0000,
            tile_lo: 0x00,
            tile_hi: 0x00,
            tile_addr: 0x0000,

            oamaddr_lo: 0x00,
            oamaddr_hi: 0x00,
            oamaddr: 0x0000,
            oam_fetch: 0x00,
            oam_eval_done: false,
            secondary_oamaddr: 0x0000,
            overflow_count: 0,
            spr_in_range: false,
            spr_zero_in_range: false,
            spr_zero_visible: false,
            spr_count: 0,
            vram_buffer: 0x00,

            oamdata: ConstSlice::new(),
            secondary_oamdata: ConstSlice::new(),
            sprites: [Sprite::new(); 8],
            spr_present: ConstSlice::new(),

            prevent_vbl: false,
            frame: Frame::new(),

            region,
            cycle_count: 0,
            reset_signal: false,
            emulate_warmup: false,

            open_bus: 0x00,

            debugger: None,
        };

        ppu.set_region(ppu.region);

        ppu
    }

    /// Return the current frame buffer.
    #[inline]
    #[must_use]
    pub fn frame_buffer(&self) -> &[u16] {
        self.frame.buffer()
    }

    /// Return the current frame number.
    #[inline]
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.frame.number()
    }

    /// Return whether current frame number is odd.
    #[inline]
    #[must_use]
    pub const fn is_odd_frame(&self) -> bool {
        self.frame.is_odd()
    }

    /// Get the pixel pixel brightness at the given coordinates.
    #[inline]
    #[must_use]
    pub fn pixel_brightness(&self, x: u32, y: u32) -> u32 {
        self.frame.pixel_brightness(x, y)
    }

    /// Load a Mapper into the PPU.
    #[inline]
    pub fn load_mapper(&mut self, mapper: Mapper) {
        self.bus.mapper = mapper;
    }

    /// Return the current Nametable mirroring mode.
    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        self.bus.mirroring()
    }

    /// Clone the PPU state, excluding internal transient state, the current frame buffer.
    pub fn clone_state(&self) -> Self {
        Self {
            master_clock: self.master_clock,
            clock_divider: self.clock_divider,
            cycle: self.cycle,
            scanline: self.scanline,
            vblank_scanline: self.vblank_scanline,
            prerender_scanline: self.prerender_scanline,
            pal_spr_eval_scanline: self.pal_spr_eval_scanline,
            scroll: self.scroll,
            mask: self.mask,
            ctrl: self.ctrl,
            status: self.status,
            bus: self.bus.clone(), // We actually want to clone CHR ROM
            curr_palette: self.curr_palette,
            secondary_oamaddr: self.secondary_oamaddr,
            oamdata: self.oamdata.clone(),
            secondary_oamdata: self.secondary_oamdata.clone(),
            sprites: self.sprites,
            region: self.region,
            cycle_count: self.cycle_count,
            ..Default::default()
        }
    }

    /// Load the passed given buffer with RGBA pixels from the current nametables.
    pub fn load_nametables(&self, nametables: &mut [u8]) {
        for i in 0..4 {
            let base_addr = Ppu::NT_START + (i as u16) * Ppu::NT_SIZE;
            let x_offset = (i % 2) * Ppu::WIDTH;
            let y_offset = (i / 2) * Ppu::HEIGHT;

            for addr in base_addr..(base_addr + Ppu::NT_SIZE - 64) {
                let x_scroll = addr & Scroll::COARSE_X_MASK;
                let y_scroll = (addr & Scroll::COARSE_Y_MASK) >> 5;

                let base_nametable_addr =
                    Ppu::NT_START | (addr & (Scroll::NT_X_MASK | Scroll::NT_Y_MASK));
                let base_attr_addr = base_nametable_addr + Ppu::ATTR_OFFSET;

                let tile_index = u16::from(self.bus.peek_ciram(addr));
                let tile_addr = self.ctrl.bg_select | (tile_index << 4);

                let supertile = ((y_scroll & 0xFC) << 1) + (x_scroll >> 2);
                let attr = u16::from(self.bus.peek_ciram(base_attr_addr + supertile));
                let attr_shift = (x_scroll & 0x02) | ((y_scroll & 0x02) << 1);
                let palette_addr = ((attr >> attr_shift) & 0x03) << 2;

                let tile_num = x_scroll + (y_scroll << 5);
                let tile_x = (tile_num % 32) << 3;
                let tile_y = (tile_num / 32) << 3;

                // self.nametable_ids[(addr - Ppu::NT_START) as usize] = tile;
                for y in 0..8 {
                    let tile_addr = tile_addr + y;
                    let tile_lo = self.bus.peek_chr(tile_addr);
                    let tile_hi = self.bus.peek_chr(tile_addr + 8);
                    for x in 0..8 {
                        let tile_palette = (((tile_hi >> x) & 1) << 1) | (tile_lo >> x) & 1;
                        let palette = palette_addr | u16::from(tile_palette);
                        let color = self.bus.peek_palette(
                            Ppu::PALETTE_START | ((palette & 0x03 > 0) as u16 * palette),
                        );
                        let x = u32::from(tile_x + (7 - x));
                        let y = u32::from(tile_y + y);
                        Self::set_pixel(
                            u16::from(color & self.mask.grayscale) | self.mask.emphasis,
                            x + x_offset,
                            y + y_offset,
                            2 * Ppu::WIDTH,
                            nametables,
                        );
                    }
                }
            }
        }
    }

    /// Load the given buffer with RGBA pixels from the current pattern tables.
    pub fn load_pattern_tables(&self, pattern_tables: &mut [u8]) {
        for i in 0..2 {
            let start = (i as u16) * 0x1000;
            let end = start + 0x1000;
            let x_offset = (i % 2) * Ppu::WIDTH / 2;
            for tile_addr in (start..end).step_by(16) {
                let tile_x = ((tile_addr % 0x1000) % 256) / 2;
                let tile_y = ((tile_addr % 0x1000) / 256) * 8;
                for y in 0..8 {
                    let tile_lo = u16::from(self.bus.peek_chr(tile_addr + y));
                    let tile_hi = u16::from(self.bus.peek_chr(tile_addr + y + 8));
                    for x in 0..8 {
                        let palette = (((tile_hi >> x) & 0x01) << 1) | ((tile_lo >> x) & 0x01);
                        let color = u16::from(self.bus.peek_palette(Ppu::PALETTE_START | palette));
                        let x = u32::from(tile_x + (7 - x));
                        let y = u32::from(tile_y + y);
                        Self::set_pixel(color, x + x_offset, y, Ppu::WIDTH, pattern_tables);
                    }
                }
            }
        }
    }

    /// Load the given buffer with RGBA pixels from the current pattern tables.
    pub fn load_oam(
        &self,
        oam_table: &mut [u8],
        sprite_nametable: &mut [u8],
        sprites: &mut [Sprite],
    ) {
        // TODO: de-duplicate this with load_sprites
        for (i, oamdata) in self.oamdata.chunks(4).enumerate() {
            if let [y, tile_index, attr, x] = oamdata {
                let sprite_x = u32::from(*x);
                let sprite_y = u32::from(*y);
                let tile_index = u16::from(*tile_index);
                let palette = ((attr & 0x03) << 2) | 0x10;
                let bg_priority = (attr & 0x20) == 0x20;
                let flip_horizontal = (attr & 0x40) == 0x40;
                let flip_vertical = (attr & 0x80) == 0x80;

                let height = self.ctrl.spr_height;
                let tile_addr = if height == 16 {
                    // Use bit 0 of tile index to determine pattern table
                    ((tile_index & 0x01) * 0x1000) | ((tile_index & 0xFE) << 4)
                } else {
                    self.ctrl.spr_select | (tile_index << 4)
                };

                sprites[i] = Sprite {
                    x: sprite_x,
                    y: sprite_y,
                    tile_addr,
                    palette,
                    bg_priority,
                    flip_horizontal,
                    flip_vertical,
                    ..Sprite::default()
                };

                let tile_x = (i % 8) as u32 * 8;
                let tile_y = (i / 8) as u32 * 8;
                for y in 0..8 {
                    let mut line_offset = if flip_vertical {
                        (height as u16) - 1 - y
                    } else {
                        y
                    };
                    if height == 16 && line_offset >= 8 {
                        line_offset += 8;
                    }
                    let tile_lo = self.bus.peek_chr(tile_addr + line_offset);
                    let tile_hi = self.bus.peek_chr(tile_addr + line_offset + 8);
                    let y = u32::from(y);
                    for x in 0..8 {
                        // let spr_color = (((tile_hi >> x) & 0x01) << 1) | ((tile_lo >> x) & 0x01);
                        let spr_color = if flip_horizontal {
                            (((tile_hi >> x) & 0x01) << 1) | ((tile_lo >> x) & 0x01)
                        } else {
                            (((tile_hi << x) & 0x80) >> 6) | (((tile_lo << x) & 0x80) >> 7)
                        };
                        let palette = palette + spr_color;
                        let color = self.bus.peek_palette(
                            Self::PALETTE_START
                                | ((palette & 0x03 > 0) as u16 * u16::from(palette)),
                        );

                        Self::set_pixel(u16::from(color), tile_x + x, tile_y + y, 64, oam_table);

                        let x = sprite_x + x;
                        let y = sprite_y + y;
                        let show_left_bg = self.mask.show_left_bg;
                        let show_left_spr = self.mask.show_left_spr;
                        let show_bg = self.mask.show_bg;
                        let show_spr = self.mask.show_spr;
                        let fine_x = self.scroll.fine_x;

                        let left_clip_bg = x < 8 && !show_left_bg;
                        let bg_color = if show_bg && !left_clip_bg {
                            ((((self.tile_shift_hi << fine_x) & 0x8000) >> 14)
                                | (((self.tile_shift_lo << fine_x) & 0x8000) >> 15))
                                as u8
                        } else {
                            0
                        };

                        let left_clip_spr = x < 8 && !show_left_spr;
                        if show_spr && !left_clip_spr && x < Ppu::WIDTH && y < Ppu::HEIGHT {
                            let color = if bg_color == 0 || !bg_priority {
                                color
                            } else if (fine_x + ((x & 0x07) as u16)) < 8 {
                                self.prev_palette + bg_color
                            } else {
                                self.curr_palette + bg_color
                            };
                            Self::set_pixel(u16::from(color), x, y, Ppu::WIDTH, sprite_nametable);
                        }
                    }
                }
            }
        }
    }

    /// Load the given buffer with RGBA pixels from the current palettes.
    pub fn load_palettes(&self, palettes: &mut [u8], colors: &mut [u8]) {
        for addr in Ppu::PALETTE_START..Ppu::PALETTE_END {
            let offset = addr - Ppu::PALETTE_START;
            let x = u32::from(offset % 16);
            let y = u32::from(offset / 16);
            let color = self.bus.peek_palette(addr);
            colors[offset as usize] = color;
            Self::set_pixel(u16::from(color), x, y, 16, palettes);
        }
    }

    fn set_pixel(color: u16, x: u32, y: u32, width: u32, pixels: &mut [u8]) {
        let index = (color as usize) * 3;
        let idx = 4 * (x + y * width) as usize;
        assert!(Ppu::NTSC_PALETTE.len() > index + 2);
        assert!(pixels.len() > 2);
        assert!(idx + 2 < pixels.len());
        pixels[idx] = Ppu::NTSC_PALETTE[index];
        pixels[idx + 1] = Ppu::NTSC_PALETTE[index + 1];
        pixels[idx + 2] = Ppu::NTSC_PALETTE[index + 2];
        pixels[idx + 3] = 0xFF;
    }
}

impl Ppu {
    const fn increment_vram_addr(&mut self) {
        // During rendering, v increments coarse X and coarse Y simultaneously
        if self.mask.rendering_enabled
            && (self.scanline == self.prerender_scanline
                || self.scanline <= Self::VISIBLE_SCANLINE_END)
        {
            self.scroll.increment_x();
            self.scroll.increment_y();
        } else {
            self.scroll.increment(self.ctrl.vram_increment);
        }
    }

    fn start_vblank(&mut self) {
        trace!("Start VBL - PPU:{:3},{:3}", self.cycle, self.scanline);
        if !self.prevent_vbl {
            self.status.set_in_vblank(true);
            if self.ctrl.nmi_enabled {
                Cpu::set_nmi();
                trace!("VBL NMI - PPU:{:3},{:3}", self.cycle, self.scanline,);
            }
        }
        self.prevent_vbl = false;
        let val = self.peek_status();
        self.bus.mapper.on_bus_write(0x2002, val, BusKind::Ppu);
    }

    fn stop_vblank(&mut self) {
        trace!(
            "Stop VBL, Sprite0 Hit, Overflow - PPU:{:3},{:3}",
            self.cycle, self.scanline
        );
        self.status.set_spr_zero_hit(false);
        self.status.set_spr_overflow(false);
        self.status.reset_in_vblank();
        self.reset_signal = false;
        Cpu::clear_nmi();
        self.open_bus = 0; // Clear open bus every frame
        let val = self.peek_status();
        self.bus.mapper.on_bus_write(0x2002, val, BusKind::Ppu);
    }

    /// Fetch BG nametable byte.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    fn fetch_bg_nt_byte(&mut self) {
        self.prev_palette = self.curr_palette;
        self.curr_palette = self.next_palette;

        self.tile_shift_lo |= u16::from(self.tile_lo);
        self.tile_shift_hi |= u16::from(self.tile_hi);

        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = Self::NT_START | (self.scroll.addr() & nametable_addr_mask);
        let tile_index = u16::from(self.bus.read_ciram(addr));
        self.tile_addr = self.ctrl.bg_select | (tile_index << 4) | self.scroll.fine_y;
    }

    /// Fetch BG attribute byte.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    fn fetch_bg_attr_byte(&mut self) {
        let addr = self.scroll.attr_addr();
        let shift = self.scroll.attr_shift();
        self.next_palette = ((self.bus.read_ciram(addr) >> shift) & 0x03) << 2;
    }

    /// Fetch 4 tiles and write out shift registers every 8th cycle.
    /// Each tile fetch takes 2 cycles.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    fn fetch_background(&mut self) {
        match self.cycle & 0x07 {
            0 => {
                // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
                self.scroll.increment_x();
                // 256, Increment Fine Y when we reach the end of the screen
                if self.cycle == Self::INC_Y {
                    self.scroll.increment_y();
                }
            }
            1 => self.fetch_bg_nt_byte(),
            3 => self.fetch_bg_attr_byte(),
            5 => self.tile_lo = self.bus.read_chr(self.tile_addr),
            7 => self.tile_hi = self.bus.read_chr(self.tile_addr + 8),
            _ => (),
        }
    }

    fn evaluate_sprites(&mut self) {
        // Local variables improve cache locality
        let cycle = self.cycle;

        match cycle {
            // 1. Clear Secondary OAM
            // 1..=64
            Self::OAM_CLEAR_START..=Self::OAM_CLEAR_END => {
                self.oam_fetch = 0xFF;
                self.secondary_oamdata = ConstSlice::filled(0xFF);
            }
            // 2. Read OAM to find first eight sprites on this scanline
            // 3. With > 8 sprites, check (wrongly) for more sprites to set overflow flag
            // 64..=256
            Self::SPR_EVAL_START..=Self::SPR_EVAL_END => {
                if cycle == Self::SPR_EVAL_START {
                    self.spr_in_range = false;
                    self.spr_zero_in_range = false;
                    self.secondary_oamaddr = 0x00;
                    self.oam_eval_done = false;
                    self.oamaddr_hi = (self.oamaddr >> 2) & 0x3F;
                    self.oamaddr_lo = (self.oamaddr) & 0x03;
                } else if cycle == Self::SPR_EVAL_END {
                    self.spr_zero_visible = self.spr_zero_in_range;
                    self.spr_count = (self.secondary_oamaddr >> 2) as usize;
                }

                if cycle & 0x01 == 0x01 {
                    // Odd cycles are reads from OAM
                    self.oam_fetch = self.oamdata[self.oamaddr as usize];
                } else {
                    self.spr_eval_even_cycle();
                }
            }
            _ => (),
        }
    }

    fn spr_eval_even_cycle(&mut self) {
        // Local variables improve cache locality
        let scanline = self.scanline;
        let mut oam_eval_done = self.oam_eval_done;
        let mut secondary_oamaddr = self.secondary_oamaddr;
        let mut oam_fetch = self.oam_fetch;
        let mut spr_in_range = self.spr_in_range;
        let mut spr_zero_in_range = self.spr_zero_in_range;
        let spr_zero_visible = self.spr_zero_visible;
        let spr_count = self.spr_count;

        let mut oamaddr_hi = self.oamaddr_hi;
        let mut oamaddr_lo = self.oamaddr_lo;
        let secondary_oamindex = secondary_oamaddr as usize & 0x1F;
        debug_assert!(secondary_oamindex < self.secondary_oamdata.len());

        // oamaddr rolled over, so we're done reading
        if oam_eval_done {
            oamaddr_hi = (oamaddr_hi + 1) & 0x3F;
            if secondary_oamaddr >= 0x20 {
                oam_fetch = self.secondary_oamdata[secondary_oamindex];
            }
        } else {
            // If previously not in range, interpret this byte as y
            let y = u32::from(oam_fetch);
            let height = self.ctrl.spr_height;
            spr_in_range |= !spr_in_range && (y..y + height).contains(&scanline);

            // Even cycles are writes to Secondary OAM
            if secondary_oamaddr < 0x20 {
                self.secondary_oamdata[secondary_oamindex] = oam_fetch;

                if spr_in_range {
                    oamaddr_lo += 1;
                    secondary_oamaddr += 1;

                    spr_zero_in_range |= oamaddr_hi == 0x00;
                    if oamaddr_lo == 0x04 {
                        spr_in_range = false;
                        oamaddr_lo = 0x00;
                        oamaddr_hi = (oamaddr_hi + 1) & 0x3F;
                        oam_eval_done |= oamaddr_hi == 0x00;
                    }
                } else {
                    oamaddr_hi = (oamaddr_hi + 1) & 0x3F;
                    oam_eval_done |= oamaddr_hi == 0x00;
                }
            } else {
                oam_fetch = self.secondary_oamdata[secondary_oamindex];
                if spr_in_range {
                    self.status.set_spr_overflow(true);
                    oamaddr_lo += 1;
                    if oamaddr_lo == 0x04 {
                        oamaddr_lo = 0x00;
                        oamaddr_hi = (oamaddr_hi + 1) & 0x3F;
                    }

                    match self.overflow_count.cmp(&0) {
                        Ordering::Equal => self.overflow_count = 3,
                        Ordering::Greater => {
                            self.overflow_count -= 1;
                            let no_overflow = self.overflow_count == 0;
                            oam_eval_done |= no_overflow;
                            if no_overflow {
                                oamaddr_lo = 0;
                            }
                        }
                        Ordering::Less => (),
                    }
                } else {
                    oamaddr_hi = (oamaddr_hi + 1) & 0x3F;
                    oamaddr_lo = (oamaddr_lo + 1) & 0x03;
                    oam_eval_done |= oamaddr_hi == 0x00;
                }
            }
        }

        self.oamaddr = (oamaddr_hi << 2) | (oamaddr_lo & 0x03);
        self.oamaddr_hi = oamaddr_hi;
        self.oamaddr_lo = oamaddr_lo;

        self.oam_eval_done = oam_eval_done;
        self.secondary_oamaddr = secondary_oamaddr;
        self.oam_fetch = oam_fetch;
        self.spr_in_range = spr_in_range;
        self.spr_zero_in_range = spr_zero_in_range;
        self.spr_zero_visible = spr_zero_visible;
        self.spr_count = spr_count;
    }

    fn load_sprites(&mut self) {
        // Local variables improve cache locality
        let cycle = self.cycle;
        let scanline = self.scanline;
        let spr_count = self.spr_count;

        let idx = (cycle - Self::SPR_FETCH_START) as usize / 8;
        let oam_idx = idx << 2;

        if let [y, tile_index, attr, x] = (*self.secondary_oamdata)[oam_idx..=oam_idx + 3] {
            let x = u32::from(x);
            let y = u32::from(y);
            let mut tile_index = u16::from(tile_index);
            let flip_vertical = (attr & 0x80) == 0x80;

            let height = self.ctrl.spr_height;
            // Should be in the range 0..=7 or 0..=15 depending on sprite height
            let mut line_offset = if (y..y + height).contains(&scanline) {
                scanline - y
            } else {
                0
            };
            if flip_vertical {
                line_offset = height - 1 - line_offset;
            }

            if idx >= spr_count {
                line_offset = 0;
                tile_index = 0xFF;
            }

            let tile_addr = if height == 16 {
                // Use bit 0 of tile index to determine pattern table
                let sprite_select = (tile_index & 0x01) * 0x1000;
                if line_offset >= 8 {
                    line_offset += 8;
                }
                sprite_select | ((tile_index & 0xFE) << 4) | line_offset as u16
            } else {
                self.ctrl.spr_select | (tile_index << 4) | line_offset as u16
            };

            if idx < spr_count {
                let sprite = &mut self.sprites[idx];
                sprite.x = x;
                sprite.y = y;
                sprite.tile_lo = self.bus.read_chr(tile_addr);
                sprite.tile_hi = self.bus.read_chr(tile_addr + 8);
                sprite.palette = ((attr & 0x03) << 2) | 0x10;
                sprite.bg_priority = (attr & 0x20) == 0x20;
                sprite.flip_horizontal = (attr & 0x40) == 0x40;
                sprite.flip_vertical = flip_vertical;
                for spr in self.spr_present.iter_mut().skip(sprite.x as usize).take(8) {
                    *spr = true;
                }
            } else {
                // Fetches for remaining sprites/hidden fetch tile $FF - used by MMC3 IRQ
                // counter
                let _ = self.bus.read_chr(tile_addr);
                let _ = self.bus.read_chr(tile_addr + 8);
            }
        }
    }

    // https://wiki.nesdev.org/w/index.php/PPU_OAM
    fn fetch_sprites(&mut self) {
        // OAMADDR set to $00 on prerender and visible scanlines
        self.write_oamaddr(0x00);

        match self.cycle & 0x07 {
            // Garbage NT sprite fetch (257, 265, 273, etc.) - Required for proper // MC-ACC IRQs
            // (MMC3 clone)
            1 => self.fetch_bg_nt_byte(),   // Garbage NT fetch
            3 => self.fetch_bg_attr_byte(), // Garbage attr fetch
            // Cycle 260, 268, etc. This is an approximation (each tile is actually loaded in 8
            // steps (e.g from 257 to 264))
            4 => self.load_sprites(),
            _ => (),
        }
    }

    fn pixel_palette(&mut self) -> u8 {
        let x = self.cycle - 1;
        let show_bg = self.mask.show_bg;
        let show_spr = self.mask.show_spr;

        let bg_color = if show_bg && (self.mask.show_left_bg || x >= 8) {
            let shift = 15 - self.scroll.fine_x;
            ((((self.tile_shift_hi >> shift) & 0x01) << 1) | ((self.tile_shift_lo >> shift) & 0x01))
                as u8
        } else {
            0
        };

        if show_spr && (self.mask.show_left_spr || x >= 8) && self.spr_present[x as usize] {
            for (i, sprite) in self.sprites.iter().take(self.spr_count).enumerate() {
                let shift = x.wrapping_sub(sprite.x);
                if shift > 7 {
                    continue;
                }

                let shift = if sprite.flip_horizontal {
                    shift
                } else {
                    7 - shift
                };
                let spr_color =
                    (((sprite.tile_hi >> shift) & 0x01) << 1) | ((sprite.tile_lo >> shift) & 0x01);

                if spr_color != 0 {
                    if i == 0
                        && bg_color != 0
                        && x != 255
                        && self.spr_zero_visible
                        && self.mask.rendering_enabled
                        && !self.status.spr_zero_hit
                    {
                        self.status.set_spr_zero_hit(true);
                    }

                    if bg_color == 0 || !sprite.bg_priority {
                        return sprite.palette + spr_color;
                    }
                    break;
                }
            }
        }

        if (self.scroll.fine_x + ((x & 0x07) as u16)) < 8 {
            self.prev_palette + bg_color
        } else {
            self.curr_palette + bg_color
        }
    }

    fn headless_sprite_zero_hit(&mut self) {
        if !self.mask.rendering_enabled || !self.spr_zero_visible || self.status.spr_zero_hit {
            return;
        }

        let x = self.cycle - 1;
        if x == 255
            || (x < 8 && (!self.mask.show_left_bg || !self.mask.show_left_spr))
            || !self.spr_present[x as usize]
        {
            return;
        }

        let shift = 15 - self.scroll.fine_x;
        let bg_color =
            (((self.tile_shift_hi >> shift) & 0x01) << 1) | ((self.tile_shift_lo >> shift) & 0x01);
        if bg_color == 0 {
            return;
        }

        let sprite = &self.sprites[0];
        let shift = x.wrapping_sub(sprite.x);
        if shift > 7 {
            return;
        }
        let shift = if sprite.flip_horizontal {
            shift
        } else {
            7 - shift
        };
        let spr_color =
            (((sprite.tile_hi >> shift) & 0x01) << 1) | ((sprite.tile_lo >> shift) & 0x01);
        if spr_color != 0 {
            self.status.set_spr_zero_hit(true);
        }
    }

    fn render_pixel(&mut self) {
        let addr = self.scroll.addr();
        let color =
            if self.mask.rendering_enabled || (addr & Self::PALETTE_START) != Self::PALETTE_START {
                let palette = u16::from(self.pixel_palette());
                self.bus
                    .peek_palette(Self::PALETTE_START | ((palette & 0x03 > 0) as u16 * palette))
            } else {
                self.bus.peek_palette(addr)
            };

        self.frame.set_pixel(
            self.cycle - 1,
            self.scanline,
            u16::from(color & self.mask.grayscale) | self.mask.emphasis,
        );
    }

    fn tick(&mut self) {
        // Local variables improve cache locality
        let cycle = self.cycle;
        let scanline = self.scanline;
        let visible_cycle = matches!(cycle, Self::VISIBLE_START..=Self::VISIBLE_END);
        let bg_prefetch_cycle = matches!(cycle, Self::BG_PREFETCH_START..=Self::BG_PREFETCH_END);
        let bg_fetch_cycle = bg_prefetch_cycle || visible_cycle;
        let visible_scanline = scanline <= Self::VISIBLE_SCANLINE_END;

        if self.mask.rendering_enabled {
            let prerender_scanline = scanline == self.prerender_scanline;
            let render_scanline = prerender_scanline || visible_scanline;
            let region = self.region;

            if render_scanline {
                match cycle {
                    // 1..=256
                    Self::VISIBLE_START..=Self::VISIBLE_END => {
                        if visible_scanline {
                            self.evaluate_sprites();
                        }
                        self.fetch_background();
                        if prerender_scanline && cycle <= 8 && self.oamaddr >= 0x08 {
                            // If OAMADDR is not less than eight when rendering starts, the eight bytes
                            // starting at OAMADDR & 0xF8 are copied to the first eight bytes of OAM
                            let addr = cycle as usize - 1;
                            let oamindex = (self.oamaddr as usize & 0xF8) + addr;
                            self.oamdata[addr] = self.oamdata[oamindex];
                        }
                    }
                    // 257..=320
                    Self::SPR_FETCH_START..=Self::SPR_FETCH_END => {
                        // 257
                        if cycle == Self::SPR_FETCH_START {
                            // Copy X bits at the start of a new line since we're going to start writing
                            // new x values to t
                            self.scroll.copy_x();
                            self.spr_present = ConstSlice::new();
                        }
                        if prerender_scanline
                            // 280..=304
                            && matches!(cycle, Self::COPY_Y_START..=Self::COPY_Y_END)
                        {
                            // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
                            // if rendering is enabled
                            // https://wiki.nesdev.org/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
                            self.scroll.copy_y();
                        }
                        self.fetch_sprites();
                    }
                    // 321..=340
                    Self::BG_PREFETCH_START..=Self::CYCLE_END => {
                        // 336
                        if cycle <= Self::BG_PREFETCH_END {
                            self.fetch_background();
                        } else if cycle >= Self::BG_DUMMY_START {
                            // 337..=340
                            self.fetch_bg_nt_byte();
                        }
                        self.oam_fetch = self.secondary_oamdata[0];

                        if prerender_scanline
                            && cycle == Self::ODD_SKIP
                            && region.is_ntsc()
                            && self.is_odd_frame()
                        {
                            // NTSC behavior while rendering - each odd PPU frame is one clock shorter
                            // (skipping from 339 over 340 to 0)
                            trace!(
                                "Skipped odd frame cycle: {} - PPU:{cycle:3},{scanline:3}",
                                self.frame_number()
                            );
                            self.cycle = Self::CYCLE_END;
                        }
                    }
                    _ => (),
                }
            } else if region.is_pal() && scanline >= self.pal_spr_eval_scanline {
                self.evaluate_sprites();
                // 257..=320
                if matches!(cycle, Self::SPR_FETCH_START..=Self::SPR_FETCH_END) {
                    self.write_oamaddr(0x00);
                }
            }
        }

        if self.scroll.delayed_update() {
            // MMC3 clocks using A12
            let addr = self.scroll.addr();
            self.bus.mapper.on_bus_read(addr, BusKind::Ppu);
        }

        // Pixels should be put even if rendering is disabled, as this is what blanks out the
        // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
        if visible_scanline && visible_cycle {
            if self.skip_rendering {
                self.headless_sprite_zero_hit();
            } else {
                self.render_pixel();
            }
        }
        // Update shift registers after rendering
        if bg_fetch_cycle {
            self.tile_shift_lo <<= 1;
            self.tile_shift_hi <<= 1;
        }
    }
}

impl Registers for Ppu {
    // $2000 | RW  | PPUCTRL
    //       | 0-1 | Name Table to show:
    //       |     |
    //       |     |           +-----------+-----------+
    //       |     |           | 2 ($2800) | 3 ($2C00) |
    //       |     |           +-----------+-----------+
    //       |     |           | 0 ($2000) | 1 ($2400) |
    //       |     |           +-----------+-----------+
    //       |     |
    //       |     | Remember, though, that because of the mirroring, there are
    //       |     | only 2 real Name Tables, not 4.
    //       |   2 | Vertical Write, 1 = PPU memory address increments by 32:
    //       |     |
    //       |     |    Name Table, VW=0          Name Table, VW=1
    //       |     |   +----------------+        +----------------+
    //       |     |   |----> write     |        | | write        |
    //       |     |   |                |        | V              |
    //       |     |
    //       |   3 | Sprite Pattern Table address, 1 = $1000, 0 = $0000
    //       |   4 | Screen Pattern Table address, 1 = $1000, 0 = $0000
    //       |   5 | Sprite Size, 1 = 8x16, 0 = 8x8
    //       |   6 | Hit Switch, 1 = generate interrupts on Hit (incorrect ???)
    //       |   7 | VBlank Switch, 1 = generate interrupts on VBlank
    fn write_ctrl(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal && self.emulate_warmup {
            return;
        }
        self.ctrl.write(val);
        self.scroll.write_nametable_select(val);

        trace!(
            "$2000 NMI Enabled: {} - PPU:{:3},{:3}",
            self.ctrl.nmi_enabled, self.cycle, self.scanline,
        );

        // By toggling NMI (bit 7) during VBlank without reading $2002, /NMI can be pulled low
        // multiple times, causing multiple NMIs to be generated.
        if !self.ctrl.nmi_enabled {
            Cpu::clear_nmi();
        } else if self.status.in_vblank {
            trace!(
                "$2000 NMI During VBL - PPU:{:3},{:3}",
                self.cycle, self.scanline
            );
            Cpu::set_nmi();
        }
    }

    // $2001 | RW  | PPUMASK
    //       |   0 | Unknown (???)
    //       |   1 | BG Mask, 0 = don't show background in left 8 columns
    //       |   2 | Sprite Mask, 0 = don't show sprites in left 8 columns
    //       |   3 | BG Switch, 1 = show background, 0 = hide background
    //       |   4 | Sprites Switch, 1 = show sprites, 0 = hide sprites
    //       | 5-7 | Unknown (???)
    fn write_mask(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal && self.emulate_warmup {
            return;
        }
        self.mask.write(val);
    }

    // $2002 | R   | PPUSTATUS
    //       | 0-5 | Unknown (???)
    //       |   6 | Sprite0 Hit Flag, 1 = PPU rendering has hit sprite #0
    //       |     | This flag resets to 0 when VBlank starts, or CPU reads $2002
    //       |   7 | VBlank Flag, 1 = PPU is generating a Vertical Blanking Impulse
    //       |     | This flag resets to 0 when VBlank ends, or CPU reads $2002
    fn read_status(&mut self) -> u8 {
        let status = self.peek_status();
        if Cpu::nmi_pending() {
            trace!("$2002 NMI Ack - PPU:{:3},{:3}", self.cycle, self.scanline,);
        }
        Cpu::clear_nmi();
        self.status.reset_in_vblank();
        self.scroll.reset_latch();

        if self.scanline == self.vblank_scanline && self.cycle == Self::VBLANK - 1 {
            // Reading PPUSTATUS one clock before the start of vertical blank will read as clear
            // and never set the flag or generate an NMI for that frame
            trace!(
                "$2002 Prevent VBL - PPU:{:3},{:3}",
                self.cycle, self.scanline
            );
            self.prevent_vbl = true;
        }
        self.open_bus |= status & 0xE0;
        self.bus.mapper.on_bus_write(0x2002, status, BusKind::Ppu);
        status
    }

    // $2002 | R   | PPUSTATUS
    //       | 0-5 | Unknown (???)
    //       |   6 | Sprite0 Hit Flag, 1 = PPU rendering has hit sprite #0
    //       |     | This flag resets to 0 when VBlank starts, or CPU reads $2002
    //       |   7 | VBlank Flag, 1 = PPU is generating a Vertical Blanking Impulse
    //       |     | This flag resets to 0 when VBlank ends, or CPU reads $2002
    //
    // Non-mutating version of `read_status`.
    fn peek_status(&self) -> u8 {
        // Only upper 3 bits are connected for this register
        (self.status.read() & 0xE0) | (self.open_bus & 0x1F)
    }

    // $2003 | W   | OAMADDR
    //       |     | Used to set the address in the 256-byte Sprite Memory to be
    //       |     | accessed via $2004. This address will increment by 1 after
    //       |     | each access to $2004. The Sprite Memory contains coordinates,
    //       |     | colors, and other attributes of the sprites.
    fn write_oamaddr(&mut self, val: u8) {
        self.open_bus = val;
        self.oamaddr = val;
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    fn read_oamdata(&mut self) -> u8 {
        let val = self.peek_oamdata();
        self.open_bus = val;
        val
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    // Non-mutating version of `read_oamdata`.
    fn peek_oamdata(&self) -> u8 {
        // Reading OAMDATA during rendering will expose OAM accesses during sprite evaluation and loading
        if self.scanline <= Self::VISIBLE_SCANLINE_END
            && self.mask.rendering_enabled
            && matches!(self.cycle, Self::SPR_FETCH_START..=Self::SPR_FETCH_END)
        {
            self.secondary_oamdata[self.secondary_oamaddr as usize]
        } else {
            self.oamdata[self.oamaddr as usize]
        }
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to write the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    fn write_oamdata(&mut self, mut val: u8) {
        self.open_bus = val;
        if self.mask.rendering_enabled
            && (self.scanline <= Self::VISIBLE_SCANLINE_END
                || self.scanline == self.prerender_scanline
                || (self.region.is_pal() && self.scanline >= self.pal_spr_eval_scanline))
        {
            // https://www.nesdev.org/wiki/PPU_registers#OAMDATA
            // Writes to OAMDATA during rendering do not modify values, but do perform a glitch
            // increment of OAMADDR, bumping only the high 6 bits
            self.oamaddr = self.oamaddr.wrapping_add(4);
        } else {
            if self.oamaddr & 0x03 == 0x02 {
                // Bits 2-4 of sprite attr (byte 2) are unimplemented and always read back as 0
                val &= 0xE3;
            }
            self.oamdata[self.oamaddr as usize] = val;
            self.oamaddr = self.oamaddr.wrapping_add(1);
        }
    }

    // $2005 | W   | PPUSCROLL
    //       |     | There are two scroll registers, vertical and horizontal,
    //       |     | which are both written via this port. The first value written
    //       |     | will go into the Vertical Scroll Register (unless it is >239,
    //       |     | then it will be ignored). The second value will appear in the
    //       |     | Horizontal Scroll Register. The Name Tables are assumed to be
    //       |     | arranged in the following way:
    //       |     |
    //       |     |           +-----------+-----------+
    //       |     |           | 2 ($2800) | 3 ($2C00) |
    //       |     |           +-----------+-----------+
    //       |     |           | 0 ($2000) | 1 ($2400) |
    //       |     |           +-----------+-----------+
    //       |     |
    //       |     | When scrolled, the picture may span over several Name Tables.
    //       |     | Remember, though, that because of the mirroring, there are
    //       |     | only 2 real Name Tables, not 4.
    fn write_scroll(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal && self.emulate_warmup {
            return;
        }
        self.scroll.write(val);
    }

    // $2006 | W   | PPUADDR
    fn write_addr(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal && self.emulate_warmup {
            return;
        }
        self.scroll.write_addr(val);
        // MMC3 clocks using A12
        self.bus
            .mapper
            .on_bus_write(self.scroll.addr(), val, BusKind::Ppu);
    }

    // $2007 | RW  | PPUDATA
    fn read_data(&mut self) -> u8 {
        let addr = self.scroll.addr();
        self.increment_vram_addr();

        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in $0000 - $3EFF
        let val = self.bus.read(addr);
        let val = if addr < Self::PALETTE_START {
            let buffer = self.vram_buffer;
            self.vram_buffer = val;
            buffer
        } else {
            // Set internal buffer with mirrors of nametable when reading palettes
            // Since we're reading from > $3EFF subtract $1000 to fill
            // buffer with nametable mirror data
            self.vram_buffer = self.bus.read(addr - 0x1000);
            // Hi 2 bits of palette should be open bus
            val | (self.open_bus & 0xC0)
        };

        self.open_bus = val;
        // MMC3 clocks using A12
        self.bus
            .mapper
            .on_bus_read(self.scroll.addr(), BusKind::Ppu);

        trace!(
            "PPU $2007 read: {val:02X} - PPU:{:3},{:3}",
            self.cycle, self.scanline
        );

        val
    }

    // $2007 | RW  | PPUDATA
    //
    // Non-mutating version of `read_data`.
    fn peek_data(&self) -> u8 {
        let addr = self.scroll.addr();
        if addr < Self::PALETTE_START {
            self.vram_buffer
        } else {
            // Hi 2 bits of palette should be open bus
            self.bus.peek(addr) | (self.open_bus & 0xC0)
        }
    }

    // $2007 | RW  | PPUDATA
    fn write_data(&mut self, val: u8) {
        self.open_bus = val;
        let addr = self.scroll.addr();
        trace!(
            "PPU $2007 write: ${addr:04X} -> {val:02X} - PPU:{:3},{:3}",
            self.cycle, self.scanline
        );
        self.increment_vram_addr();
        self.bus.write(addr, val);

        // MMC3 clocks using A12
        let addr = self.scroll.addr();
        self.bus.mapper.on_bus_write(addr, val, BusKind::Ppu);
    }
}

impl Clock for Ppu {
    fn clock(&mut self) -> u64 {
        if self.cycle < Self::CYCLE_END {
            self.cycle += 1;
            self.tick();

            if self.cycle == Self::VBLANK {
                if self.scanline == self.vblank_scanline {
                    self.start_vblank();
                } else if self.scanline == self.prerender_scanline {
                    self.stop_vblank();
                }
            }
        } else {
            self.cycle = 0;
            self.scanline += 1;
            // Post-render line
            if self.scanline == self.vblank_scanline - 1 {
                self.frame.increment();
            } else if self.scanline > self.prerender_scanline {
                // Wrap scanline back to 0
                self.scanline = 0;
                // Force prerender scanline sprite fetches to load the dummy $FF tiles (fixes
                // shaking in Ninja Gaiden 3 stage 1 after beating boss)
                self.spr_count = 0;
            }
        }

        self.cycle_count = self.cycle_count.wrapping_add(1);

        if let Some(inspector) = &self.debugger {
            if self.scanline == inspector.scanline && self.cycle == inspector.cycle {
                (*inspector.callback)(self.clone_state());
            }
        }

        1
    }
}

impl ClockTo for Ppu {
    fn clock_to(&mut self, clock: u64) -> u64 {
        let mut cycles = 0;
        while self.master_clock + self.clock_divider <= clock {
            cycles += self.clock();
            self.master_clock += self.clock_divider;
        }
        cycles
    }
}

impl Regional for Ppu {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        // https://www.nesdev.org/wiki/Cycle_reference_chart
        let (clock_divider, vblank_scanline, prerender_scanline) = match region {
            NesRegion::Auto | NesRegion::Ntsc => (
                Self::CLOCK_DIVIDER_NTSC,
                Self::VBLANK_SCANLINE_NTSC,
                Self::PRERENDER_SCANLINE_NTSC,
            ),
            NesRegion::Pal => (
                Self::CLOCK_DIVIDER_PAL,
                Self::VBLANK_SCANLINE_PAL,
                Self::PRERENDER_SCANLINE_PAL,
            ),
            NesRegion::Dendy => (
                Self::CLOCK_DIVIDER_DENDY,
                Self::VBLANK_SCANLINE_DENDY,
                Self::PRERENDER_SCANLINE_DENDY,
            ),
        };
        self.region = region;
        self.clock_divider = clock_divider;
        self.vblank_scanline = vblank_scanline;
        self.prerender_scanline = prerender_scanline;
        // PAL refreshes OAM later due to extended vblank to avoid OAM decay
        self.pal_spr_eval_scanline = self.vblank_scanline + 24;
        self.bus.set_region(region);
        self.mask.set_region(region);
    }
}

impl Reset for Ppu {
    fn reset(&mut self, kind: ResetKind) {
        self.ctrl.reset(kind);
        self.mask.reset(kind);
        self.status.reset(kind);
        self.scroll.reset(kind);
        if kind == ResetKind::Soft {
            self.reset_signal = self.emulate_warmup;
        }
        if kind == ResetKind::Hard {
            self.oamdata = ConstSlice::new();
            self.oamaddr = 0x0000;
        }
        self.secondary_oamaddr = 0x0000;
        self.vram_buffer = 0x00;
        self.cycle = 0;
        self.scanline = 0;
        self.master_clock = 0;
        self.prevent_vbl = false;
        self.frame.reset(kind);
        self.oam_fetch = 0x00;
        self.oam_eval_done = false;
        self.overflow_count = 0;
        self.spr_in_range = false;
        self.spr_zero_in_range = false;
        self.spr_zero_visible = false;
        self.spr_count = 0;
        self.sprites = [Sprite::new(); 8];
        self.spr_present = ConstSlice::new();
        self.open_bus = 0x00;
        self.bus.reset(kind);
    }
}

impl std::fmt::Debug for Ppu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ppu")
            .field("region", &self.region)
            .field("cycle_count", &self.cycle_count)
            .field("bus", &self.bus)
            .field("ctrl", &self.ctrl)
            .field("mask", &self.mask)
            .field("status", &self.status)
            .field("oamaddr", &self.oamaddr)
            .field("oamaddr_lo", &self.oamaddr_lo)
            .field("oamaddr_hi", &self.oamaddr_hi)
            .field("secondary_oamaddr", &self.secondary_oamaddr)
            .field("scroll", &self.scroll)
            .field("vram_buffer", &self.vram_buffer)
            .field("cycle", &self.cycle)
            .field("scanline", &self.scanline)
            .field("master_clock", &self.master_clock)
            .field("clock_divider", &self.clock_divider)
            .field("vblank_scanline", &self.vblank_scanline)
            .field("prerender_scanline", &self.prerender_scanline)
            .field("pal_spr_eval_scanline", &self.pal_spr_eval_scanline)
            .field("prevent_vbl", &self.prevent_vbl)
            .field("frame", &self.frame)
            .field("tile_shift_lo", &self.tile_shift_lo)
            .field("tile_shift_hi", &self.tile_shift_hi)
            .field("tile_lo", &self.tile_lo)
            .field("tile_hi", &self.tile_hi)
            .field("tile_addr", &self.tile_addr)
            .field("prev_palette", &self.prev_palette)
            .field("curr_palette", &self.curr_palette)
            .field("next_palette", &self.next_palette)
            .field("oam_fetch", &self.oam_fetch)
            .field("oam_eval_done", &self.oam_eval_done)
            .field("overflow_count", &self.overflow_count)
            .field("spr_in_range", &self.spr_in_range)
            .field("spr_zero_in_range", &self.spr_zero_in_range)
            .field("spr_zero_visible", &self.spr_zero_visible)
            .field("spr_count", &self.spr_count)
            .field("sprites", &self.sprites)
            .field("open_bus", &self.open_bus)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::Cart,
        mapper::{Mirrored, Mmc1Revision, Sxrom},
    };

    #[test]
    fn vram_writes() {
        let mut ppu = Ppu::default();
        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_data(0x66); // write to $2305

        assert_eq!(ppu.bus.read_ciram(0x2305), 0x66);
    }

    #[test]
    fn vram_reads() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0x00);
        ppu.bus.write(0x2305, 0x66);

        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.scroll.addr(), 0x2306);
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.scroll.addr(), 0x2307);
    }

    #[test]
    fn vram_read_pagecross() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0x00);
        ppu.bus.write(0x21FF, 0x66);
        ppu.bus.write(0x2200, 0x77);

        ppu.write_addr(0x21);
        ppu.write_addr(0xFF);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.read_data(), 0x77);
    }

    #[test]
    fn vram_read_vertical_increment() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0b100);
        ppu.bus.write(0x21FF, 0x66);
        ppu.bus.write(0x21FF + 32, 0x77);
        ppu.bus.write(0x21FF + 64, 0x88);

        ppu.write_addr(0x21);
        ppu.write_addr(0xFF);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.read_data(), 0x77);
        assert_eq!(ppu.read_data(), 0x88);
    }

    // Horizontal: https://wiki.nesdev.org/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 a ]
    //   [0x2800 B ] [0x2C00 b ]
    #[test]
    fn vram_horizontal_mirror() {
        let mut ppu = Ppu::default();
        ppu.write_addr(0x24);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_data(0x66); // write to a at $2405

        ppu.write_addr(0x28);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_data(0x77); // write to B at $2805

        ppu.write_addr(0x20);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66); // read A from $2005

        ppu.write_addr(0x2C);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x77); // read b from $2C05
    }

    // Vertical: https://wiki.nesdev.org/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 B ]
    //   [0x2800 a ] [0x2C00 b ]
    #[test]
    fn vram_vertical_mirror() {
        let mut ppu = Ppu::default();
        let mut cart = Cart::default();
        let mut mapper = Sxrom::load(&mut cart, Mmc1Revision::BC).unwrap();
        mapper.set_mirroring(Mirroring::Vertical);
        ppu.load_mapper(mapper);

        ppu.write_addr(0x20);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_data(0x66); // write to A at $2005

        ppu.write_addr(0x2C);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_data(0x77); // write to b at $2C05

        ppu.write_addr(0x28);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66); // read a from $2805

        ppu.write_addr(0x24);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x77); // read B from $2405
    }

    #[test]
    fn read_status_resets_latch() {
        let mut ppu = Ppu::default();
        ppu.bus.write(0x2305, 0x66);

        ppu.write_addr(0x21);
        ppu.write_addr(0x23);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_ne!(ppu.read_data(), 0x66);

        ppu.read_status();

        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66);
    }

    #[test]
    fn vram_mirroring() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0);
        ppu.bus.write(0x2305, 0x66);

        ppu.write_addr(0x63); // 0x6305 mirrors to 0x2305
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.read_data(); // buffer read
        assert_eq!(ppu.scroll.addr(), 0x2306);
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.scroll.addr(), 0x2307);
    }

    #[test]
    fn read_status_resets_vblank() {
        let mut ppu = Ppu::default();
        ppu.status.set_in_vblank(true);

        let status = ppu.read_status();
        assert_eq!(status >> 7, 1);
        assert_eq!(ppu.status.read() >> 7, 0);
    }

    #[test]
    fn sprite_zero_hit_headless_visible_cycle() {
        let mut ppu = Ppu::default();
        ppu.write_mask(0x18);
        ppu.skip_rendering = true;
        ppu.scanline = 0;
        ppu.cycle = 10;
        ppu.scroll.fine_x = 0;

        ppu.tile_shift_lo = 0x8000;
        ppu.tile_shift_hi = 0x0000;

        ppu.spr_zero_visible = true;
        ppu.spr_present[9] = true;

        ppu.sprites[0].x = 8;
        ppu.sprites[0].tile_lo = 0b0000_0010;
        ppu.sprites[0].tile_hi = 0x00;
        ppu.sprites[0].flip_horizontal = true;
        ppu.sprites[0].bg_priority = false;
        ppu.status.set_spr_zero_hit(false);

        ppu.tick();

        assert!(ppu.status.spr_zero_hit);
    }

    #[test]
    fn oam_read_write() {
        let mut ppu = Ppu::default();
        ppu.write_oamaddr(0x10);
        ppu.write_oamdata(0x66);
        ppu.write_oamdata(0x77);

        ppu.write_oamaddr(0x10);
        assert_eq!(ppu.read_oamdata(), 0x66);

        ppu.write_oamaddr(0x11);
        assert_eq!(ppu.read_oamdata(), 0x77);
    }
}
