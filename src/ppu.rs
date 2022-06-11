//! Picture Processing Unit (PPU)
//!
//! <http://wiki.nesdev.com/w/index.php/PPU>

use crate::{
    cart::Cart,
    common::{Clocked, NesRegion, Powered},
    mapper::Mapped,
    memory::{MemRead, MemWrite, Memory, RamState},
    ppu::{frame::NTSC_PALETTE, vram::ATTR_OFFSET},
};
use frame::Frame;
use ppu_regs::{PpuRegs, COARSE_X_MASK, COARSE_Y_MASK, NT_X_MASK, NT_Y_MASK};
use serde::{Deserialize, Serialize};
use sprite::Sprite;
use std::{cmp::Ordering, fmt};
use vram::{
    Vram, ATTR_START, NT_SIZE, NT_START, PALETTE_END, PALETTE_SIZE, PALETTE_START, SYSTEM_PALETTE,
    SYSTEM_PALETTE_SIZE,
};

pub mod frame;
pub mod ppu_regs;
pub mod sprite;
pub mod vram;

// Screen/Render
pub const RENDER_WIDTH: u32 = 256;
pub const RENDER_HEIGHT: u32 = 240;
pub const RENDER_CHANNELS: usize = 4;
pub const RENDER_PITCH: usize = RENDER_CHANNELS * RENDER_WIDTH as usize;

const _TOTAL_CYCLES: u32 = 341;
const _TOTAL_SCANLINES: u32 = 262;
const RENDER_PIXELS: usize = (RENDER_WIDTH * RENDER_HEIGHT) as usize;
pub const RENDER_SIZE: usize = RENDER_CHANNELS * RENDER_PIXELS;

pub const PATTERN_WIDTH: u32 = RENDER_WIDTH / 2;
pub const PATTERN_PIXELS: usize = (PATTERN_WIDTH * PATTERN_WIDTH) as usize;
pub const PATTERN_SIZE: usize = RENDER_CHANNELS * PATTERN_PIXELS;

// Cycles
const IDLE_CYCLE: u32 = 0; // PPU is idle this cycle
const VISIBLE_CYCLE_START: u32 = 1; // Tile data fetching starts
const VBLANK_CYCLE: u32 = 1;
const VISIBLE_CYCLE_END: u32 = 256; // 2 cycles each for 4 fetches = 32 tiles
const OAM_CLEAR_CYCLE_END: u32 = 64;
const SPR_EVAL_CYCLE_START: u32 = 65;
const SPR_EVAL_CYCLE_END: u32 = 256;
const SPR_FETCH_CYCLE_START: u32 = 257; // Sprites for next scanline fetch starts
const SPR_FETCH_CYCLE_END: u32 = 320; // 2 cycles each for 4 fetches = 8 sprites
const COPY_Y_CYCLE_START: u32 = 280; // Copy Y scroll start
const COPY_Y_CYCLE_END: u32 = 304; // Copy Y scroll stop
const INC_Y_CYCLE: u32 = 256; // Increase Y scroll when it reaches end of the screen
const COPY_X_CYCLE: u32 = 257; // Copy X scroll when starting a new scanline
const BG_PREFETCH_CYCLE_START: u32 = 321; // Tile data for next scanline fetched
const BG_PREFETCH_CYCLE_END: u32 = 336; // 2 cycles each for 4 fetches = 2 tiles
const BG_DUMMY_CYCLE_START: u32 = 337; // Dummy fetches - use is unknown
const CYCLE_SKIP: u32 = 339; // Odd frames skip the last cycle
const CYCLE_END: u32 = 340; // 2 cycles each for 2 fetches
const POWER_ON_CYCLES: usize = 29658 * 3; // https://wiki.nesdev.com/w/index.php/PPU_power_up_state

// Scanlines
const VISIBLE_SCANLINE_END: u32 = 239; // Rendering graphics for the screen

pub const OAM_SIZE: usize = 64 * 4; // 64 entries * 4 bytes each
pub const SECONDARY_OAM_SIZE: usize = 8 * 4; // 8 entries * 4 bytes each

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum VideoFilter {
    None,
    Ntsc,
}

impl VideoFilter {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::None, Self::Ntsc]
    }
}

impl Default for VideoFilter {
    fn default() -> Self {
        Self::Ntsc
    }
}

impl AsRef<str> for VideoFilter {
    fn as_ref(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Ntsc => "NTSC",
        }
    }
}

impl From<usize> for VideoFilter {
    fn from(value: usize) -> Self {
        if value == 1 {
            Self::Ntsc
        } else {
            Self::None
        }
    }
}

/// Nametable Mirroring Mode
///
/// <http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring>
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenA,
    SingleScreenB,
    FourScreen,
}

impl Default for Mirroring {
    fn default() -> Self {
        Mirroring::Horizontal
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct Viewer {
    pub scanline: u32,
    pub nametables: Vec<Vec<u8>>,
    pub nametable_ids: Vec<u8>,
    pub pattern_tables: [Vec<u8>; 2],
    pub palette: Vec<u8>,
    pub palette_ids: Vec<u8>,
}

impl Default for Viewer {
    fn default() -> Self {
        Self {
            scanline: 0,
            nametables: vec![
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
            ],
            nametable_ids: vec![0; 4 * NT_SIZE as usize],
            pattern_tables: [vec![0; PATTERN_SIZE], vec![0; PATTERN_SIZE]],
            palette: vec![0; (PALETTE_SIZE + 4) * 4],
            palette_ids: vec![0; (PALETTE_SIZE + 4) * 4],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Ppu {
    pub cycle: u32,         // (0, 340) 341 cycles happen per scanline
    pub cycle_count: usize, // Total number of PPU cycles run
    pub scanline: u32,      // (0, 261) 262 total scanlines per frame
    pub nes_region: NesRegion,
    pub master_clock: u64,
    pub clock_divider: u64,
    pub vblank_scanline: u32,
    pub prerender_scanline: u32,
    pub pal_spr_eval_scanline: u32,
    pub nmi_pending: bool, // Whether the CPU should trigger an NMI next cycle
    pub prevent_vbl: bool,
    pub oam_dma: bool,
    pub oam_dma_offset: u8,
    pub vram: Vram,    // $2007 PPUDATA
    pub regs: PpuRegs, // Registers
    // Addr Low Nibble
    // $00, $04, $08, $0C   Sprite Y coord
    // $01, $05, $09, $0D   Sprite tile #
    // $02, $06, $0A, $0E   Sprite attribute
    // $03, $07, $0B, $0F   Sprite X coord
    pub oamaddr_lo: u8,
    pub oamaddr_hi: u8,
    pub oamaddr: u8, // $2003 OAMADDR write-only
    pub oam: Memory, // $2004 OAM data read/write - Object Attribute Memory for Sprites
    pub secondary_oamaddr: u8,
    pub secondary_oam: Memory, // Secondary OAM data for Sprites on a given scanline
    pub oam_fetch: u8,
    pub oam_eval_done: bool,
    pub overflow_count: u8,
    pub sprite_in_range: bool,
    pub sprite0_in_range: bool,
    pub sprite0_visible: bool,
    pub sprite_count: u8,
    pub sprites: [Sprite; 8], // Each scanline can hold 8 sprites at a time
    pub frame: Frame,         // Frame data keeps track of data and shift registers between frames
    pub filter: VideoFilter,
    #[serde(skip)]
    pub viewer: Option<Viewer>,
}

impl Ppu {
    pub fn new(nes_region: NesRegion) -> Self {
        let mut ppu = Self {
            cycle: 0,
            cycle_count: 0,
            scanline: 0,
            nes_region,
            master_clock: 0,
            clock_divider: 0,
            vblank_scanline: 0,
            prerender_scanline: 0,
            pal_spr_eval_scanline: 0,
            nmi_pending: false,
            prevent_vbl: false,
            oam_dma: false,
            oam_dma_offset: 0x00,
            regs: PpuRegs::new(nes_region),
            oamaddr_lo: 0x00,
            oamaddr_hi: 0x00,
            oamaddr: 0x00,
            oam: Memory::ram(OAM_SIZE, RamState::AllOnes),
            secondary_oamaddr: 0x00,
            secondary_oam: Memory::ram(SECONDARY_OAM_SIZE, RamState::AllOnes),
            oam_fetch: 0xFF,
            oam_eval_done: false,
            overflow_count: 0,
            sprite_in_range: false,
            sprite0_in_range: false,
            sprite0_visible: false,
            sprite_count: 0,
            sprites: [Sprite::new(); 8],
            vram: Vram::new(),
            frame: Frame::new(),
            filter: VideoFilter::Ntsc,
            viewer: None,
        };
        ppu.set_nes_region(nes_region);
        ppu
    }

    pub fn set_nes_region(&mut self, nes_region: NesRegion) {
        let (clock_divider, vblank_scanline, prerender_scanline) = match nes_region {
            NesRegion::Ntsc => (4, 241, 261),
            NesRegion::Pal => (5, 241, 311),
            NesRegion::Dendy => (5, 291, 311),
        };
        self.nes_region = nes_region;
        self.clock_divider = clock_divider;
        self.vblank_scanline = vblank_scanline;
        self.prerender_scanline = prerender_scanline;
        self.pal_spr_eval_scanline = self.vblank_scanline + 24; // PAL refreshes OAM later due to extended vblank to avoid OAM decay
        self.regs.set_nes_region(nes_region);
    }

    #[inline]
    pub fn load_cart(&mut self, cart: &mut Box<Cart>) {
        self.vram.cart = &mut **cart;
    }

    #[inline]
    pub fn open_viewer(&mut self) {
        self.viewer = Some(Viewer::default());
        self.load_nametables();
        self.load_pattern_tables();
        self.load_palettes();
    }

    #[inline]
    pub fn close_viewer(&mut self) {
        self.viewer = None;
    }

    #[inline]
    pub fn update_viewer(&mut self) {
        if let Some(ref viewer) = self.viewer {
            if self.cycle == IDLE_CYCLE && self.scanline == viewer.scanline {
                self.load_nametables();
                self.load_pattern_tables();
                self.load_palettes();
            }
        }
    }

    #[inline]
    pub fn set_viewer_scanline(&mut self, scanline: u32) {
        if let Some(ref mut viewer) = self.viewer {
            viewer.scanline = scanline;
        }
    }

    // Returns a fully rendered frame of RENDER_SIZE RGB colors
    #[must_use]
    #[inline]
    pub fn frame_buffer(&mut self) -> &[u8] {
        match self.filter {
            VideoFilter::None => self.decode_buffer(),
            VideoFilter::Ntsc => self.apply_ntsc_filter(),
        }
    }

    fn decode_buffer(&mut self) -> &[u8] {
        for (idx, color) in self.frame.current_buffer.iter().enumerate() {
            let color_idx = ((*color as usize) & (SYSTEM_PALETTE_SIZE - 1)) * 3;
            if let [red, green, blue] = SYSTEM_PALETTE[color_idx..=color_idx + 2] {
                let idx = idx << 2;
                self.frame.output_buffer[idx..=idx + 3].copy_from_slice(&[red, green, blue, 255]);
            }
        }
        &self.frame.output_buffer
    }

    // Amazing implementation Bisqwit! Much faster than my original, but boy what a pain
    // to translate it to Rust
    // Source: https://bisqwit.iki.fi/jutut/kuvat/programming_examples/nesemu1/nesemu1.cc
    // http://wiki.nesdev.com/w/index.php/NTSC_video
    fn apply_ntsc_filter(&mut self) -> &[u8] {
        for (idx, pixel) in self.frame.current_buffer.iter().enumerate() {
            let x = idx % 256;
            let y = idx / 256;
            let even_phase = if self.frame.num & 0x01 == 0x01 { 0 } else { 1 };
            let phase = (2 + y * 341 + x + even_phase) % 3;
            let color = if x == 0 {
                // Remove pixel 0 artifact from not having a valid previous pixel
                0
            } else {
                NTSC_PALETTE[phase][(self.frame.prev_pixel & 0x3F) as usize][*pixel as usize]
            };
            self.frame.prev_pixel = u32::from(*pixel);
            let idx = idx << 2;
            let red = (color >> 16 & 0xFF) as u8;
            let green = (color >> 8 & 0xFF) as u8;
            let blue = (color & 0xFF) as u8;
            self.frame.output_buffer[idx..=idx + 3].copy_from_slice(&[red, green, blue, 255]);
        }
        &self.frame.output_buffer
    }

    fn load_nametables(&mut self) {
        if let Some(ref mut viewer) = self.viewer {
            for (i, nametable) in viewer.nametables.iter_mut().enumerate() {
                let base_addr = NT_START + (i as u16) * NT_SIZE;
                for addr in base_addr..(base_addr + NT_SIZE - 64) {
                    let x_scroll = addr & COARSE_X_MASK;
                    let y_scroll = (addr & COARSE_Y_MASK) >> 5;

                    let nt_base_addr = NT_START + (addr & (NT_X_MASK | NT_Y_MASK));
                    let tile = self.vram.peek(addr);
                    let tile_addr = self.regs.background_select() + u16::from(tile) * 16;
                    let supertile = (x_scroll / 4) + (y_scroll / 4) * 8;
                    let attr = u16::from(self.vram.peek(nt_base_addr + ATTR_OFFSET + supertile));
                    let corner = ((x_scroll % 4) / 2 + (y_scroll % 4) / 2 * 2) << 1;
                    let mask = 0x03 << corner;
                    let palette = (attr & mask) >> corner;

                    let tile_num = x_scroll + y_scroll * 32;
                    let tile_x = (tile_num % 32) * 8;
                    let tile_y = (tile_num / 32) * 8;

                    viewer.nametable_ids[(addr - NT_START) as usize] = tile;
                    for y in 0..8 {
                        let lo = u16::from(self.vram.peek(tile_addr + y));
                        let hi = u16::from(self.vram.peek(tile_addr + y + 8));
                        for x in 0..8 {
                            let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                            let palette_idx =
                                self.vram.peek(PALETTE_START + palette * 4 + pix_type) as usize;
                            let x = u32::from(tile_x + (7 - x));
                            let y = u32::from(tile_y + y);
                            Self::put_pixel(palette_idx, x, y, RENDER_WIDTH, nametable);
                        }
                    }
                }
            }
        }
    }

    fn load_pattern_tables(&mut self) {
        if let Some(ref mut viewer) = self.viewer {
            let width = RENDER_WIDTH / 2;
            for (i, pattern_table) in viewer.pattern_tables.iter_mut().enumerate() {
                let start = (i as u16) * 0x1000;
                let end = start + 0x1000;
                for tile_addr in (start..end).step_by(16) {
                    let tile_x = ((tile_addr % 0x1000) % 256) / 2;
                    let tile_y = ((tile_addr % 0x1000) / 256) * 8;
                    for y in 0..8 {
                        let lo = u16::from(self.vram.peek(tile_addr + y));
                        let hi = u16::from(self.vram.peek(tile_addr + y + 8));
                        for x in 0..8 {
                            let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                            let palette_idx = self.vram.peek(PALETTE_START + pix_type) as usize;
                            let x = u32::from(tile_x + (7 - x));
                            let y = u32::from(tile_y + y);
                            Self::put_pixel(palette_idx, x, y, width, pattern_table);
                        }
                    }
                }
            }
        }
    }

    fn load_palettes(&mut self) {
        if let Some(ref mut viewer) = self.viewer {
            // Global  // BG 0 ----------------------------------  // Unused    // SPR 0 -------------------------------
            // 0x3F00: 0,0  0x3F01: 1,0  0x3F02: 2,0  0x3F03: 3,0  0x3F10: 5,0  0x3F11: 6,0  0x3F12: 7,0  0x3F13: 8,0
            // Unused  // BG 1 ----------------------------------  // Unused    // SPR 1 -------------------------------
            // 0x3F04: 0,1  0x3F05: 1,1  0x3F06: 2,1  0x3F07: 3,1  0x3F14: 5,1  0x3F15: 6,1  0x3F16: 7,1  0x3F17: 8,1
            // Unused  // BG 2 ----------------------------------  // Unused    // SPR 2 -------------------------------
            // 0x3F08: 0,2  0x3F09: 1,2  0x3F0A: 2,2  0x3F0B: 3,2  0x3F18: 5,2  0x3F19: 6,2  0x3F1A: 7,2  0x3F1B: 8,2
            // Unused  // BG 3 ----------------------------------  // Unused    // SPR 3 -------------------------------
            // 0x3F0C: 0,3  0x3F0D: 1,3  0x3F0E: 2,3  0x3F0F: 3,3  0x3F1C: 5,3  0x3F1D: 6,3  0x3F1E: 7,3  0x3F1F: 8,3
            let width = 16;
            for addr in PALETTE_START..PALETTE_END {
                let x = u32::from((addr - PALETTE_START) % 16);
                let y = u32::from((addr - PALETTE_START) / 16);
                let palette_idx = self.vram.peek(addr);
                viewer.palette_ids[y as usize * width + x as usize] = palette_idx;
                Self::put_pixel(
                    palette_idx as usize,
                    x,
                    y,
                    width as u32,
                    &mut viewer.palette,
                );
            }
        }
    }

    #[inline]
    fn run_cycle(&mut self) {
        let visible_cycle = matches!(self.cycle, VISIBLE_CYCLE_START..=VISIBLE_CYCLE_END);
        let bg_prefetch_cycle =
            matches!(self.cycle, BG_PREFETCH_CYCLE_START..=BG_PREFETCH_CYCLE_END);
        let bg_dummy_cycle = matches!(self.cycle, BG_DUMMY_CYCLE_START..=CYCLE_END);
        let bg_fetch_cycle = bg_prefetch_cycle || visible_cycle;
        let spr_eval_cycle = matches!(self.cycle, VISIBLE_CYCLE_START..=SPR_EVAL_CYCLE_END);
        let spr_fetch_cycle = matches!(self.cycle, SPR_FETCH_CYCLE_START..=SPR_FETCH_CYCLE_END);
        let spr_dummy_cycle = matches!(self.cycle, BG_PREFETCH_CYCLE_START..=CYCLE_END);

        let visible_scanline = self.scanline <= VISIBLE_SCANLINE_END;
        let prerender_scanline = self.scanline == self.prerender_scanline;
        let render_scanline = prerender_scanline || visible_scanline;

        // Pixels should be put even if rendering is disabled, as this is what blanks out the
        // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
        if visible_cycle && visible_scanline {
            self.render_pixel();
        }

        if self.rendering_enabled() {
            if visible_scanline
                || (self.nes_region == NesRegion::Pal
                    && self.scanline >= self.pal_spr_eval_scanline)
            {
                if spr_eval_cycle {
                    self.evaluate_sprites();
                } else if spr_fetch_cycle {
                    // OAMADDR set to $00 on prerender and visible scanlines
                    self.write_oamaddr(0x00);
                }
            }

            if render_scanline {
                // (1, 0) - (256, 239) - visible cycles/scanlines
                // (1, 261) - (256, 261) - prefetch scanline
                // (321, 0) - (336, 239) - next scanline fetch cycles
                if bg_fetch_cycle {
                    self.fetch_background();

                    // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
                    if self.cycle & 0x07 == 0x00 {
                        self.regs.increment_x();
                    }
                } else if bg_dummy_cycle {
                    // Dummy byte fetches
                    // (337, 0) - (337, 239)
                    self.fetch_bg_nt_byte();
                }

                match self.cycle {
                    VISIBLE_CYCLE_START..=8 if prerender_scanline && self.oamaddr >= 0x08 => {
                        // If OAMADDR is not less than eight when rendering starts, the eight bytes
                        // starting at OAMADDR & 0xF8 are copied to the first eight bytes of OAM
                        let addr = self.cycle as usize - 1;
                        self.oam[addr] = self.oam[(self.oamaddr as usize & 0xF8) + addr];
                    }
                    // Increment Fine Y when we reach the end of the screen
                    INC_Y_CYCLE => self.regs.increment_y(),
                    // Copy X bits at the start of a new line since we're going to start writing
                    // new x values to t
                    COPY_X_CYCLE => self.regs.copy_x(),
                    // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
                    // if rendering is enabled
                    // http://wiki.nesdev.com/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
                    COPY_Y_CYCLE_START..=COPY_Y_CYCLE_END if prerender_scanline => {
                        self.regs.copy_y();
                    }
                    _ => (),
                }

                if prerender_scanline {
                    // Force prerender scanline sprite fetches to load the dummy $FF tiles (fixes
                    // shaking in Ninja Gaiden 3 stage 1 after beating boss)
                    self.sprite_count = 0;
                }
                if spr_fetch_cycle {
                    self.fetch_sprites();
                }
                if spr_dummy_cycle {
                    self.oam_fetch = self.secondary_oam[0];
                }

                if self.cycle == CYCLE_SKIP
                    && prerender_scanline
                    && self.frame.num & 0x01 == 0x01
                    && self.nes_region == NesRegion::Ntsc
                {
                    // NTSC behavior while rendering - each odd PPU frame is one clock shorter
                    // (skipping from 339 over 340 to 0)
                    log::trace!(
                        "({}, {}): Skipped odd frame cycle: {}",
                        self.cycle,
                        self.scanline,
                        self.frame.num
                    );
                    self.cycle = CYCLE_END;
                }
            }
        }
    }

    #[inline]
    fn fetch_bg_nt_byte(&mut self) {
        // Fetch BG nametable
        // https://wiki.nesdev.com/w/index.php/PPU_scrolling#Tile_and_attribute_fetching
        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = NT_START | (self.regs.v & nametable_addr_mask);
        self.frame.nametable = u16::from(self.vram.read(addr));
    }

    #[inline]
    fn fetch_bg_attr_byte(&mut self) {
        // Fetch BG attribute table
        // https://wiki.nesdev.com/w/index.php/PPU_scrolling#Tile_and_attribute_fetching
        // NN 1111 YYY XXX
        // || |||| ||| +++-- high 3 bits of coarse X (x/4)
        // || |||| +++------ high 3 bits of coarse Y (y/4)
        // || ++++---------- attribute offset (960 bytes)
        // ++--------------- nametable select
        let v = self.regs.v;
        let nametable_select = v & (NT_X_MASK | NT_Y_MASK);
        let y_bits = (v >> 4) & 0x38;
        let x_bits = (v >> 2) & 0x07;
        let addr = ATTR_START | nametable_select | y_bits | x_bits;
        self.frame.attribute = self.vram.read(addr);
        // If the top bit of the low 3 bits is set, shift to next quadrant
        if self.regs.coarse_y() & 2 > 0 {
            self.frame.attribute >>= 4;
        }
        if self.regs.coarse_x() & 2 > 0 {
            self.frame.attribute >>= 2;
        }
        self.frame.attribute = (self.frame.attribute & 3) << 2;
    }

    #[inline]
    fn fetch_background(&mut self) {
        self.frame.tile_data <<= 4;
        // Fetch 4 tiles and write out shift registers every 8th cycle
        // Each tile fetch takes 2 cycles
        match self.cycle & 0x07 {
            1 => self.fetch_bg_nt_byte(),
            3 => self.fetch_bg_attr_byte(),
            5 => {
                // Fetch BG tile lo bitmap
                let tile_addr =
                    self.regs.background_select() + self.frame.nametable * 16 + self.regs.fine_y();
                self.frame.tile_lo = self.vram.read(tile_addr);
            }
            7 => {
                // Fetch BG tile hi bitmap
                let tile_addr =
                    self.regs.background_select() + self.frame.nametable * 16 + self.regs.fine_y();
                self.frame.tile_hi = self.vram.read(tile_addr + 8);
            }
            0 => {
                // Cycles 9, 17, 25, ..., 257
                // Store tiles
                let mut data = 0u32;
                let a = self.frame.attribute;
                for _ in 0..8 {
                    let p1 = (self.frame.tile_lo & 0x80) >> 7;
                    let p2 = (self.frame.tile_hi & 0x80) >> 6;
                    self.frame.tile_lo <<= 1;
                    self.frame.tile_hi <<= 1;
                    data <<= 4;
                    data |= u32::from(a | p1 | p2);
                }
                self.frame.tile_data |= u64::from(data);
            }
            _ => (),
        }
    }

    #[inline]
    fn load_sprites(&mut self) {
        let idx = (self.cycle - SPR_FETCH_CYCLE_START) as usize / 8;
        let oam_idx = idx << 2;

        if let [y, tile_number, attr, x] = self.secondary_oam[oam_idx..=oam_idx + 3] {
            let x = u32::from(x);
            let y = u32::from(y);
            let mut tile_number = u16::from(tile_number);
            let palette = ((attr & 0x03) << 2) | 0x10;
            let bg_priority = (attr & 0x20) == 0x20;
            let flip_horizontal = (attr & 0x40) == 0x40;
            let flip_vertical = (attr & 0x80) == 0x80;

            let height = self.regs.sprite_height();
            // Should be in the range 0..=7 or 0..=15 depending on sprite height
            let mut line_offset = if (y..y + height).contains(&self.scanline) {
                self.scanline - y
            } else {
                0
            };
            if flip_vertical {
                line_offset = height - 1 - line_offset;
            }

            if idx >= self.sprite_count.into() {
                line_offset = 0;
                tile_number = 0xFF;
            }

            let tile_addr = if height == 16 {
                // Use bit 0 of tile index to determine pattern table
                let sprite_select = if tile_number & 0x01 == 0x01 {
                    0x1000
                } else {
                    0x0000
                };
                if line_offset >= 8 {
                    line_offset += 8;
                }
                sprite_select | ((tile_number & 0xFE) << 4) | line_offset as u16
            } else {
                self.regs.sprite_select() | (tile_number << 4) | line_offset as u16
            };

            if idx < self.sprite_count.into() {
                let mut sprite = &mut self.sprites[idx];
                sprite.x = x;
                sprite.y = y;
                sprite.tile_lo = self.vram.read(tile_addr);
                sprite.tile_hi = self.vram.read(tile_addr + 8);
                sprite.palette = palette;
                sprite.bg_priority = bg_priority;
                sprite.flip_horizontal = flip_horizontal;
                sprite.flip_vertical = flip_vertical;
            } else {
                // Fetches for remaining sprites/hidden fetch tile $FF - used by MMC3 IRQ
                // counter
                let _ = self.vram.read(tile_addr);
                let _ = self.vram.read(tile_addr + 8);
            }
        }
    }

    // http://wiki.nesdev.com/w/index.php/PPU_OAM
    #[inline]
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

    #[inline]
    fn evaluate_sprites(&mut self) {
        match self.cycle {
            // 1. Clear Secondary OAM
            VISIBLE_CYCLE_START..=OAM_CLEAR_CYCLE_END => {
                self.oam_fetch = 0xFF;
                self.secondary_oam = Memory::ram(SECONDARY_OAM_SIZE, RamState::AllOnes);
            }
            // 2. Read OAM to find first eight sprites on this scanline
            // 3. With > 8 sprites, check (wrongly) for more sprites to set overflow flag
            SPR_EVAL_CYCLE_START..=SPR_EVAL_CYCLE_END => {
                if self.cycle == SPR_EVAL_CYCLE_START {
                    self.sprite_in_range = false;
                    self.sprite0_in_range = false;
                    self.secondary_oamaddr = 0x00;
                    self.oam_eval_done = false;
                    self.oamaddr_hi = (self.oamaddr >> 2) & 0x3F;
                    self.oamaddr_lo = (self.oamaddr) & 0x03;
                } else if self.cycle == SPR_EVAL_CYCLE_END {
                    self.sprite0_visible = self.sprite0_in_range;
                    self.sprite_count = self.secondary_oamaddr >> 2;
                }

                if self.cycle & 0x01 == 0x01 {
                    // Odd cycles are reads from OAM
                    self.oam_fetch = self.oam[self.oamaddr as usize];
                } else {
                    // oamaddr rolled over, so we're done reading
                    if self.oam_eval_done {
                        self.oamaddr_hi = (self.oamaddr_hi + 1) & 0x3F;
                        if self.secondary_oamaddr >= 0x20 {
                            self.oam_fetch =
                                self.secondary_oam[self.secondary_oamaddr as usize & 0x1F];
                        }
                    } else {
                        // If previously not in range, interpret this byte as y
                        let y = u32::from(self.oam_fetch);
                        let height = self.regs.sprite_height();
                        if !self.sprite_in_range && (y..y + height).contains(&self.scanline) {
                            self.sprite_in_range = true;
                        }

                        // Even cycles are writes to Secondary OAM
                        if self.secondary_oamaddr < 0x20 {
                            self.secondary_oam[self.secondary_oamaddr as usize] = self.oam_fetch;

                            if self.sprite_in_range {
                                self.oamaddr_lo += 1;
                                self.secondary_oamaddr += 1;

                                if self.oamaddr_hi == 0x00 {
                                    self.sprite0_in_range = true;
                                }
                                if self.oamaddr_lo == 0x04 {
                                    self.sprite_in_range = false;
                                    self.oamaddr_lo = 0x00;
                                    self.oamaddr_hi = (self.oamaddr_hi + 1) & 0x3F;
                                    if self.oamaddr_hi == 0x00 {
                                        self.oam_eval_done = true;
                                    }
                                }
                            } else {
                                self.oamaddr_hi = (self.oamaddr_hi + 1) & 0x3F;
                                if self.oamaddr_hi == 0x00 {
                                    self.oam_eval_done = true;
                                }
                            }
                        } else {
                            self.oam_fetch =
                                self.secondary_oam[self.secondary_oamaddr as usize & 0x1F];
                            if self.sprite_in_range {
                                self.regs.set_sprite_overflow(true);
                                self.oamaddr_lo += 1;
                                if self.oamaddr_lo == 0x04 {
                                    self.oamaddr_lo = 0x00;
                                    self.oamaddr_hi = (self.oamaddr_hi + 1) & 0x3F;
                                }

                                match self.overflow_count.cmp(&0) {
                                    Ordering::Equal => self.overflow_count = 3,
                                    Ordering::Greater => {
                                        self.overflow_count -= 1;
                                        if self.overflow_count == 0 {
                                            self.oam_eval_done = true;
                                            self.oamaddr_lo = 0x00;
                                        }
                                    }
                                    Ordering::Less => (),
                                }
                            } else {
                                self.oamaddr_hi = (self.oamaddr_hi + 1) & 0x3F;
                                self.oamaddr_lo = (self.oamaddr_lo + 1) & 0x03;
                                if self.oamaddr_hi == 0x00 {
                                    self.oam_eval_done = true;
                                }
                            }
                        }
                    }
                    self.oamaddr = (self.oamaddr_hi << 2) | (self.oamaddr_lo & 0x03);
                }
            }
            _ => (),
        }
    }

    #[inline]
    fn render_pixel(&mut self) {
        let x = self.cycle - 1;
        let y = self.scanline;
        let palette_addr =
            if self.rendering_enabled() || (self.read_ppuaddr() & PALETTE_START) != PALETTE_START {
                let color = self.pixel_color();
                if color & 0x03 > 0 {
                    u16::from(color)
                } else {
                    0
                }
            } else {
                self.read_ppuaddr() & 0x1F
            };
        let mut palette = u16::from(self.vram.read(PALETTE_START + palette_addr));
        palette &= if self.regs.grayscale() { 0x30 } else { 0x3F };
        palette |= u16::from(self.regs.emphasis()) << 1;
        self.frame.put_pixel(x, y, palette);
    }

    #[inline]
    fn put_pixel(palette_idx: usize, x: u32, y: u32, width: u32, pixels: &mut [u8]) {
        let palette_idx = (palette_idx % SYSTEM_PALETTE_SIZE) * 3;
        let red = SYSTEM_PALETTE[palette_idx];
        let green = SYSTEM_PALETTE[palette_idx + 1];
        let blue = SYSTEM_PALETTE[palette_idx + 2];
        let idx = RENDER_CHANNELS * (x + y * width) as usize;
        pixels[idx] = red;
        pixels[idx + 1] = green;
        pixels[idx + 2] = blue;
    }

    #[inline]
    #[must_use]
    pub fn pixel_brightness(&self, x: u32, y: u32) -> u32 {
        if x >= RENDER_WIDTH || y >= RENDER_HEIGHT {
            return 0;
        }
        // Used by `Zapper`
        let color = self.frame.back_buffer[(x + (y << 8)) as usize] as usize;
        let palette_idx = (color & (SYSTEM_PALETTE_SIZE - 1)) * 3;
        if let [red, green, blue] = SYSTEM_PALETTE[palette_idx..=palette_idx + 2] {
            u32::from(red) + u32::from(green) + u32::from(blue)
        } else {
            0
        }
    }

    #[inline]
    const fn background_color(&self) -> u8 {
        // 43210
        // |||||
        // |||++- Pixel value from tile data
        // |++--- Palette number from attribute table or OAM
        // +----- Background/Sprite select

        let tile_data = (self.frame.tile_data >> 32) as u32;
        let data = tile_data >> ((7 - self.regs.fine_x()) * 4);
        (data & 0x0F) as u8
    }

    #[inline]
    fn pixel_color(&mut self) -> u8 {
        let x = self.cycle - 1;

        let left_clip_bg = x < 8 && !self.regs.show_left_background();
        let left_clip_spr = x < 8 && !self.regs.show_left_sprites();
        let bg_color = if self.regs.show_background() && !left_clip_bg {
            self.background_color()
        } else {
            0
        };
        let bg_opaque = bg_color & 0x03 != 0;

        if self.regs.show_sprites() && !left_clip_spr {
            for (i, sprite) in self
                .sprites
                .iter()
                .take(self.sprite_count as usize)
                .enumerate()
            {
                let shift = x as i16 - sprite.x as i16;
                if (0..=7).contains(&shift) {
                    let color = if sprite.flip_horizontal {
                        (((sprite.tile_hi >> shift) & 0x01) << 1)
                            | ((sprite.tile_lo >> shift) & 0x01)
                    } else {
                        (((sprite.tile_hi << shift) & 0x80) >> 6)
                            | ((sprite.tile_lo << shift) & 0x80) >> 7
                    };
                    if color != 0 {
                        if i == 0
                            && bg_opaque
                            && self.sprite0_visible
                            && x != 255
                            && self.rendering_enabled()
                            && !self.regs.sprite0_hit()
                        {
                            self.regs.set_sprite0_hit(true);
                        }

                        if !bg_opaque || !sprite.bg_priority {
                            return sprite.palette + color;
                        }
                        break;
                    }
                }
            }
        }
        if bg_opaque {
            bg_color
        } else {
            0
        }
    }

    #[must_use]
    #[inline]
    pub const fn rendering_enabled(&self) -> bool {
        self.regs.show_background() || self.regs.show_sprites()
    }

    #[must_use]
    #[inline]
    pub const fn nmi_enabled(&self) -> bool {
        self.regs.nmi_enabled()
    }

    #[must_use]
    #[inline]
    pub const fn nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    // Register read/writes

    /*
     * $2000 PPUCTRL
     */

    #[inline]
    fn write_ppuctrl(&mut self, val: u8) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_ctrl(val);

        log::trace!(
            "({}, {}): $2000 NMI Enabled: {}",
            self.cycle,
            self.scanline,
            self.nmi_enabled()
        );

        // By toggling NMI (bit 7) during VBlank without reading $2002, /NMI can be pulled low
        // multiple times, causing multiple NMIs to be generated.
        if !self.nmi_enabled() {
            log::trace!("({}, {}): $2000 NMI Disable", self.cycle, self.scanline);
            self.nmi_pending = false;
        } else if self.vblank_started() {
            log::trace!("({}, {}): $2000 NMI During VBL", self.cycle, self.scanline);
            self.nmi_pending = true;
        }
    }

    /*
     * $2001 PPUMASK
     */

    #[inline]
    fn write_ppumask(&mut self, val: u8) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_mask(val);
    }

    /*
     * $2002 PPUSTATUS
     */

    #[inline]
    pub fn read_ppustatus(&mut self) -> u8 {
        let status = self.regs.read_status();
        log::trace!("({}, {}): $2002 NMI Ack", self.cycle, self.scanline);
        self.nmi_pending = false;

        if self.scanline == self.vblank_scanline && self.cycle == VBLANK_CYCLE - 1 {
            // Reading PPUSTATUS one clock before the start of vertical blank will read as clear
            // and never set the flag or generate an NMI for that frame
            log::trace!("({}, {}): $2002 Prevent VBL", self.cycle, self.scanline);
            self.prevent_vbl = true;
        }

        // read_status() modifies register, so make sure mapper is aware
        // of new status
        self.vram
            .cart_mut()
            .ppu_write(0x2002, self.regs.peek_status());

        // Only upper 3 bits are connected for this register
        (status & 0xE0) | (self.regs.open_bus & 0x1F)
    }

    #[inline]
    const fn peek_ppustatus(&self) -> u8 {
        // Only upper 3 bits are connected for this register
        (self.regs.peek_status() & 0xE0) | (self.regs.open_bus & 0x1F)
    }

    #[inline]
    fn start_vblank(&mut self) {
        log::trace!("({}, {}): Set VBL flag", self.cycle, self.scanline);
        if !self.prevent_vbl {
            self.regs.start_vblank();
            self.nmi_pending = self.nmi_enabled();
            log::trace!(
                "({}, {}): VBL NMI: {}",
                self.cycle,
                self.scanline,
                self.nmi_pending
            );
        }
        self.prevent_vbl = false;
        // Ensure our mapper knows vbl changed
        self.vram
            .cart_mut()
            .ppu_write(0x2002, self.regs.peek_status());
    }

    #[inline]
    fn stop_vblank(&mut self) {
        log::trace!("({}, {}): Clear VBL flag", self.cycle, self.scanline);
        self.regs.stop_vblank();
        self.nmi_pending = false;
        // Ensure our mapper knows vbl changed
        self.vram
            .cart_mut()
            .ppu_write(0x2002, self.regs.peek_status());
    }

    #[must_use]
    #[inline]
    pub const fn vblank_started(&self) -> bool {
        self.regs.vblank_started()
    }

    /*
     * $2003 OAM addr
     */

    #[must_use]
    #[inline]
    pub const fn read_oamaddr(&self) -> u8 {
        self.oamaddr
    }

    #[inline]
    fn write_oamaddr(&mut self, val: u8) {
        self.oamaddr = val;
    }

    /*
     * $2004 OAM data
     */

    #[must_use]
    #[inline]
    fn read_oamdata(&mut self) -> u8 {
        // Reading OAMDATA during rendering will expose OAM accesses during sprite evaluation and loading
        if self.scanline <= VISIBLE_SCANLINE_END
            && self.rendering_enabled()
            && matches!(self.cycle, SPR_FETCH_CYCLE_START..=SPR_FETCH_CYCLE_END)
        {
            let step = ((self.cycle - SPR_FETCH_CYCLE_START) & 0x07).min(3);
            self.secondary_oamaddr = ((self.cycle - SPR_FETCH_CYCLE_START) / 8 * 4 + step) as u8;
            self.oam_fetch = self.secondary_oam[self.secondary_oamaddr as usize & 0x1F];
        }
        self.peek_oamdata()
    }

    #[inline]
    fn peek_oamdata(&self) -> u8 {
        if self.scanline <= VISIBLE_SCANLINE_END && self.rendering_enabled() {
            self.oam_fetch
        } else {
            self.oam[self.oamaddr as usize]
        }
    }

    #[inline]
    fn write_oamdata(&mut self, mut val: u8) {
        if self.rendering_enabled()
            && (self.scanline <= VISIBLE_SCANLINE_END
                || self.scanline == self.prerender_scanline
                || (self.nes_region == NesRegion::Pal
                    && self.scanline >= self.pal_spr_eval_scanline))
        {
            // https://www.nesdev.org/wiki/PPU_registers#OAMDATA
            // Writes to OAMDATA during rendering do not modify values, but do perform a glitch
            // increment of OAMADDR, bumping only the high 6 bits
            self.write_oamaddr(self.oamaddr.wrapping_add(4));
        } else {
            if self.oamaddr & 0x03 == 0x02 {
                // Bits 2-4 of sprite attr (byte 2) are unimplemented and always read back as 0
                val &= 0xE3;
            }
            self.oam[self.oamaddr as usize] = val;
            self.write_oamaddr(self.oamaddr.wrapping_add(1));
        }
    }

    /*
     * $2005 PPUSCROLL
     */

    #[inline]
    fn write_ppuscroll(&mut self, val: u8) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_scroll(val);
    }

    /*
     * $2006 PPUADDR
     */

    #[must_use]
    #[inline]
    pub const fn read_ppuaddr(&self) -> u16 {
        self.regs.read_addr()
    }

    #[inline]
    fn write_ppuaddr(&mut self, val: u8) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_addr(val);
        self.vram.cart_mut().ppu_addr(self.regs.v);
    }

    /*
     * $2007 PPUDATA
     */

    #[inline]
    fn update_vram_addr(&mut self) {
        // During rendering, v increments coarse X and coarse Y at the simultaneously
        if self.rendering_enabled()
            && (self.scanline == self.prerender_scanline || self.scanline <= VISIBLE_SCANLINE_END)
        {
            self.regs.increment_x();
            self.regs.increment_y();
        } else {
            self.regs.increment_v();
        }
    }

    #[inline]
    fn read_ppudata(&mut self) -> u8 {
        let val = self.vram.read(self.read_ppuaddr());
        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in 0 - $3EFF
        // Keep addr within 15 bits
        let val = if self.read_ppuaddr() <= 0x3EFF {
            let buffer = self.vram.buffer;
            self.vram.buffer = val;
            buffer
        } else {
            // Set internal buffer with mirrors of nametable when reading palettes
            // Since we're reading from > 0x3EFF subtract 0x1000 to fill
            // buffer with nametable mirror data
            self.vram.buffer = self.vram.read(self.read_ppuaddr() - 0x1000);
            // Hi 2 bits of palette should be open bus
            val | (self.regs.open_bus & 0xC0)
        };
        self.update_vram_addr();
        // Update cart (needed by MMC3 IRQ counter)
        // Clocks when A12 changes to 1 via $2007 read/write
        self.vram.cart_mut().ppu_addr(self.regs.v);
        val
    }

    #[inline]
    fn peek_ppudata(&self) -> u8 {
        let val = self.vram.peek(self.read_ppuaddr());
        if self.read_ppuaddr() <= 0x3EFF {
            self.vram.buffer
        } else {
            val | (self.regs.open_bus & 0xC0)
        }
    }

    #[inline]
    fn write_ppudata(&mut self, val: u8) {
        self.vram.write(self.read_ppuaddr(), val);
        self.update_vram_addr();
        // Update cart (needed by MMC3 IRQ counter)
        // Clocks when A12 changes to 1 via $2007 read/write
        self.vram.cart_mut().ppu_addr(self.regs.v);
    }

    #[inline]
    pub fn run(&mut self, clock: u64) {
        while self.master_clock + self.clock_divider <= clock {
            self.clock();
            self.master_clock += self.clock_divider;
        }
    }
}

impl Clocked for Ppu {
    // http://wiki.nesdev.com/w/index.php/PPU_rendering
    fn clock(&mut self) -> usize {
        // Clear open bus roughly once every frame
        if self.scanline == 0 {
            self.regs.open_bus = 0x00;
        }
        self.cycle_count = self.cycle_count.wrapping_add(1);

        if self.cycle >= CYCLE_END {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline == self.vblank_scanline - 1 {
                self.frame.increment();
                self.frame.swap_buffers();
            } else if self.scanline > self.prerender_scanline {
                self.scanline = 0;
            }

            self.update_viewer();
        } else {
            // cycle > 0
            self.cycle += 1;
            self.run_cycle();

            if self.cycle == VBLANK_CYCLE {
                if self.scanline == self.vblank_scanline {
                    self.start_vblank();
                }
                if self.scanline == self.prerender_scanline {
                    log::trace!(
                        "({}, {}): Clear Sprite0 Hit, Overflow",
                        self.cycle,
                        self.scanline
                    );
                    self.regs.set_sprite0_hit(false);
                    self.regs.set_sprite_overflow(false);
                    self.stop_vblank();
                }
            }
        }

        1
    }
}

impl MemRead for Ppu {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x2002 => self.read_ppustatus(),
            0x2004 => self.read_oamdata(),
            0x2007 => self.read_ppudata(),
            // 0x2000 PPUCTRL is write-only
            // 0x2001 PPUMASK is write-only
            // 0x2003 OAMADDR is write-only
            // 0x2005 PPUSCROLL is write-only
            // 0x2006 PPUADDR is write-only
            _ => self.regs.open_bus,
        };
        self.regs.open_bus = val;
        val
    }

    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x2002 => self.peek_ppustatus(),
            0x2004 => self.peek_oamdata(),
            0x2007 => self.peek_ppudata(),
            // 0x2000 PPUCTRL is write-only
            // 0x2001 PPUMASK is write-only
            // 0x2003 OAMADDR is write-only
            // 0x2005 PPUSCROLL is write-only
            // 0x2006 PPUADDR is write-only
            _ => self.regs.open_bus,
        }
    }
}

impl MemWrite for Ppu {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        self.vram.cart_mut().ppu_write(addr, val);
        self.regs.open_bus = val;
        match addr {
            0x2000 => self.write_ppuctrl(val),
            0x2001 => self.write_ppumask(val),
            0x2003 => self.write_oamaddr(val),
            0x2004 => self.write_oamdata(val),
            0x2005 => self.write_ppuscroll(val),
            0x2006 => self.write_ppuaddr(val),
            0x2007 => self.write_ppudata(val),
            // 0x2002 PPUSTATUS is read-only
            _ => (),
        }
    }
}

impl Powered for Ppu {
    fn reset(&mut self) {
        self.cycle = 0;
        self.cycle_count = 0;
        self.scanline = 0;
        self.master_clock = 0;
        self.prevent_vbl = false;
        self.oam_dma = false;
        self.oam_dma_offset = 0x00;
        self.vram.reset();
        self.regs.w = false;
        self.regs.set_sprite0_hit(false);
        self.regs.set_sprite_overflow(false);
        self.secondary_oamaddr = 0x00;
        self.oam_fetch = 0xFF;
        self.oam_eval_done = false;
        self.overflow_count = 0;
        self.sprite_in_range = false;
        self.sprite0_in_range = false;
        self.sprite0_visible = false;
        self.sprite_count = 0;
        self.sprites = [Sprite::new(); 8];
        self.frame.reset();
        self.regs.write_ctrl(0);
        self.regs.write_mask(0);
        // PPUSTATUS unchanged on reset
        self.regs.write_scroll(0);
        // FIXME: Technically PPUADDR should remain unchanged on reset.
        // https://wiki.nesdev.org/w/index.php?title=PPU_power_up_state
        // However, it results in glitched sprites in some games
        self.regs.write_addr(0);
    }
    fn power_cycle(&mut self) {
        self.oamaddr_lo = 0x00;
        self.oamaddr_hi = 0x00;
        self.oamaddr = 0x00;
        self.reset();
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(NesRegion::Ntsc)
    }
}

impl fmt::Debug for Ppu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ppu")
            .field("cycle", &self.cycle)
            .field("cycle_count", &self.cycle_count)
            .field("scanline", &self.scanline)
            .field("nes_region", &self.nes_region)
            .field("master_clock", &self.master_clock)
            .field("clock_divider", &self.clock_divider)
            .field("vblank_scanline", &self.vblank_scanline)
            .field("prerender_scanline", &self.prerender_scanline)
            .field("nmi_pending", &self.nmi_pending)
            .field("prevent_vbl", &self.prevent_vbl)
            .field("oam_dma", &self.oam_dma)
            .field("dma_offset", &format_args!("${:02X}", &self.oam_dma_offset))
            .field("vram", &self.vram)
            .field("regs", &self.regs)
            .field("oamaddr", &self.oamaddr)
            .field("oam", &self.oam)
            .field("secondary_oamaddr", &self.secondary_oamaddr)
            .field("secondary_oam", &self.secondary_oam)
            .field("oam_fetch", &self.oam_fetch)
            .field("oam_eval_done", &self.oam_eval_done)
            .field("sprite_in_range", &self.sprite_in_range)
            .field("sprite0_in_range", &self.sprite0_in_range)
            .field("sprite0_visible", &self.sprite0_visible)
            .field("sprite_count", &self.sprite_count)
            .field("sprites", &self.sprites)
            .field("frame", &self.frame)
            .field("filter", &self.filter)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cart::Cart, test_roms};

    #[test]
    fn scrolling_registers() {
        let mut ppu = Ppu::default();
        let mut cart = Box::new(Cart::new());
        ppu.load_cart(&mut cart);
        while ppu.cycle_count < POWER_ON_CYCLES {
            ppu.clock();
        }

        let ppuctrl = 0x2000;
        let ppustatus = 0x2002;
        let ppuscroll = 0x2005;
        let ppuaddr = 0x2006;

        // Test write to ppuctrl
        let ctrl_write: u8 = 0b11; // Write two 1 bits
        let t_result: u16 = 0b11 << 10; // Make sure they're in the NN place of t
        ppu.write(ppuctrl, ctrl_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, 0);

        // Test read to ppustatus
        ppu.read(ppustatus);
        assert!(!ppu.regs.w);

        // Test 1st write to ppuscroll
        let scroll_write: u8 = 0b0111_1101;
        let t_result: u16 = 0b000_1100_0000_1111;
        let x_result: u16 = 0b101;
        ppu.write(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert!(ppu.regs.w);

        // Test 2nd write to ppuscroll
        let scroll_write: u8 = 0b0101_1110;
        let t_result: u16 = 0b110_1101_0110_1111;
        ppu.write(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert!(!ppu.regs.w);

        // Test 1st write to ppuaddr
        let addr_write: u8 = 0b0011_1101;
        let t_result: u16 = 0b011_1101_0110_1111;
        ppu.write(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert!(ppu.regs.w);

        // Test 2nd write to ppuaddr
        let addr_write: u8 = 0b1111_0000;
        let t_result: u16 = 0b011_1101_1111_0000;
        ppu.write(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert!(!ppu.regs.w);

        // Test a 2006/2005/2005/2006 write
        // http://forums.nesdev.com/viewtopic.php?p=78593#p78593
        ppu.write(ppuaddr, 0b0000_1000); // nametable select $10
        ppu.write(ppuscroll, 0b0100_0101); // $01 hi bits coarse Y scroll, $101 fine Y scroll
        ppu.write(ppuscroll, 0b0000_0011); // $011 fine X scroll
        ppu.write(ppuaddr, 0b1001_0110); // $100 lo bits coarse Y scroll, $10110 coarse X scroll
        let t_result: u16 = 0b101_1001_1001_0110;
        assert_eq!(ppu.regs.v, t_result);
    }

    test_roms!(
        "test_roms/ppu",
        _240pee, // TODO: Run each test
        color,   // TODO: Test all color combinations
        ntsc_torture,
        oam_read,
        oam_stress,
        open_bus,
        palette,
        palette_ram,
        read_buffer,
        scanline,
        spr_hit_alignment,
        spr_hit_basics,
        spr_hit_corners,
        spr_hit_double_height,
        spr_hit_edge_timing,
        spr_hit_flip,
        spr_hit_left_clip,
        spr_hit_right_edge,
        spr_hit_screen_bottom,
        spr_hit_timing_basics,
        spr_hit_timing_order,
        spr_overflow_basics,
        spr_overflow_details,
        spr_overflow_emulator,
        spr_overflow_obscure,
        spr_overflow_timing,
        sprite_ram,
        tv,
        vbl_nmi_basics,
        vbl_nmi_clear_timing,
        vbl_nmi_control,
        vbl_nmi_disable,
        vbl_nmi_even_odd_frames,
        #[ignore = "clock is skipped too late relative to enabling BG Failed #3"]
        vbl_nmi_even_odd_timing,
        vbl_nmi_frame_basics,
        vbl_nmi_off_timing,
        vbl_nmi_on_timing,
        vbl_nmi_set_time,
        vbl_nmi_suppression,
        vbl_nmi_timing,
        vbl_timing,
        vram_access,
    );
}
