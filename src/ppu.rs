use crate::{
    common::{Clock, Kind, NesRegion, Regional, Reset},
    mapper::{Mapped, Mapper},
    mem::{Access, Mem},
    ppu::{bus::PpuBus, frame::Frame},
};
use ctrl::PpuCtrl;
use mask::PpuMask;
use scroll::PpuScroll;
use serde::{Deserialize, Serialize};
use sprite::Sprite;
use status::PpuStatus;
use std::cmp::Ordering;

pub mod bus;
pub mod ctrl;
pub mod frame;
pub mod mask;
pub mod scroll;
pub mod sprite;
pub mod status;

/// Nametable Mirroring Mode
///
/// <http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring>
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

pub trait PpuRegisters {
    fn write_ctrl(&mut self, val: u8); // $2000 PPUCTRL
    fn write_mask(&mut self, val: u8); // $2001 PPUMASK
    fn read_status(&mut self) -> u8; // $2002 PPUSTATUS
    fn peek_status(&self) -> u8; // $2002 PPUSTATUS
    fn write_oamaddr(&mut self, val: u8); // $2003 OAMADDR
    fn read_oamdata(&mut self) -> u8; // $2004 OAMDATA
    fn peek_oamdata(&self) -> u8; // $2004 OAMDATA
    fn write_oamdata(&mut self, val: u8); // $2004 OAMDATA
    fn write_scroll(&mut self, val: u8); // $2005 PPUSCROLL
    fn write_addr(&mut self, val: u8); // $2006 PPUADDR
    fn read_data(&mut self) -> u8; // $2007 PPUDATA
    fn peek_data(&self) -> u8; // $2007 PPUDATA
    fn write_data(&mut self, val: u8); // $2007 PPUDATA
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Ppu {
    region: NesRegion,
    cycle_count: usize,
    // Internal signal that clears status registers and prevents writes and cleared at the end of VBlank
    // https://www.nesdev.org/wiki/PPU_power_up_state
    reset_signal: bool,
    bus: PpuBus,

    ctrl: PpuCtrl,     // $2000 PPUCTRL write-only
    mask: PpuMask,     // $2001 PPUMASK write-only
    status: PpuStatus, // $2002 PPUSTATUS read-only
    oamaddr: u8,       // $2003 OAM addr write-only
    oamaddr_lo: u8,
    oamaddr_hi: u8,
    oamdata: Vec<u8>, // $2004 OAM data read/write - Object Attribute Memory for Sprites
    secondary_oamaddr: u8,
    secondary_oamdata: [u8; Self::SECONDARY_OAM_SIZE], // Secondary OAM data for Sprites on a given scanline
    scroll: PpuScroll, // $2005 PPUSCROLL and $2006 PPUADDR write-only
    vram_buffer: u8,   // $2007 PPUDATA buffer

    cycle: u32,    // (0, 340) cycles per scanline
    scanline: u32, // (0,happen  261) NTSC or (0, 311) PAL/Dendy scanlines per frame
    master_clock: u64,
    clock_divider: u64,
    vblank_scanline: u32,
    prerender_scanline: u32,
    pal_spr_eval_scanline: u32,

    nmi_pending: bool,
    prevent_vbl: bool,
    frame: Frame,

    tile_shift_lo: u16,
    tile_shift_hi: u16,
    tile_lo: u8,
    tile_hi: u8,
    tile_addr: u16,
    prev_palette: u8,
    curr_palette: u8,
    next_palette: u8,

    oam_fetch: u8,
    oam_eval_done: bool,
    overflow_count: u8,
    spr_in_range: bool,
    spr_zero_in_range: bool,
    spr_zero_visible: bool,
    spr_count: usize,
    sprites: [Sprite; 8], // Each scanline can hold 8 sprites at a time
    spr_present: Vec<bool>,

    open_bus: u8,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    pub const WIDTH: u32 = 256;
    pub const HEIGHT: u32 = 240;
    pub const SIZE: usize = (Self::WIDTH * Self::HEIGHT) as usize;

    pub(crate) const NT_START: u16 = 0x2000;
    pub(crate) const NT_SIZE: u16 = 0x0400;
    pub(crate) const PALETTE_START: u16 = 0x3F00;
    pub(crate) const PALETTE_END: u16 = 0x3F20;

    const OAM_SIZE: usize = 256; // 64 4-byte sprites per frame
    const SECONDARY_OAM_SIZE: usize = 32; // 8 4-byte sprites per scanline

    // Cycles
    // https://www.nesdev.org/wiki/PPU_rendering
    const VBLANK: u32 = 1; // When VBlank flag gets set
    const VISIBLE_START: u32 = 1; // Tile data fetching starts
    const VISIBLE_END: u32 = 256; // 2 cycles each for 4 fetches = 32 tiles
    const OAM_CLEAR_START: u32 = 1;
    const OAM_CLEAR_END: u32 = 64;
    const SPR_EVAL_START: u32 = 65;
    const SPR_EVAL_END: u32 = 256;
    const SPR_FETCH_START: u32 = 257; // Sprites for next scanline fetch starts
    const SPR_FETCH_END: u32 = 320; // 2 cycles each for 4 fetches = 8 sprites
    const COPY_Y_START: u32 = 280; // Copy Y scroll start
    const COPY_Y_END: u32 = 304; // Copy Y scroll stop
    const INC_Y: u32 = 256; // Increase Y scroll when it reaches end of the screen
    const COPY_X: u32 = 257; // Copy X scroll when starting a new scanline
    const BG_PREFETCH_START: u32 = 321; // Tile data for next scanline fetched
    const BG_PREFETCH_END: u32 = 336; // 2 cycles each for 4 fetches = 2 tiles
    const BG_DUMMY_START: u32 = 337; // Dummy fetches - use is unknown
    const ODD_SKIP: u32 = 339; // Odd frames skip the last cycle
    const CYCLE_END: u32 = 340; // 2 cycles each for 2 fetches

    // Scanlines
    const VISIBLE_SCANLINE_END: u32 = 239; // Rendering graphics for the screen

    // 64 total possible colors, though only 32 can be loaded at a time
    #[rustfmt::skip]
    const SYSTEM_PALETTE: [(u8,u8,u8); 64] = [
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

    pub fn new() -> Self {
        let mut ppu = Self {
            region: NesRegion::default(),
            cycle_count: 0,
            reset_signal: false,
            bus: PpuBus::new(),

            ctrl: PpuCtrl::new(),
            mask: PpuMask::new(),
            status: PpuStatus::new(),
            oamaddr: 0x0000,
            oamaddr_lo: 0x00,
            oamaddr_hi: 0x00,
            oamdata: vec![0xFF; Self::OAM_SIZE],
            secondary_oamaddr: 0x0000,
            secondary_oamdata: [0xFF; Self::SECONDARY_OAM_SIZE],
            scroll: PpuScroll::new(),
            vram_buffer: 0x00,

            cycle: 0,
            scanline: 0,
            master_clock: 0,
            clock_divider: 0,
            vblank_scanline: 0,
            prerender_scanline: 0,
            pal_spr_eval_scanline: 0,

            nmi_pending: false,
            prevent_vbl: false,
            frame: Frame::new(),

            tile_shift_lo: 0x0000,
            tile_shift_hi: 0x0000,
            tile_lo: 0x00,
            tile_hi: 0x00,
            tile_addr: 0x0000,
            prev_palette: 0x00,
            curr_palette: 0x00,
            next_palette: 0x00,

            oam_fetch: 0x00,
            oam_eval_done: false,
            overflow_count: 0,
            spr_in_range: false,
            spr_zero_in_range: false,
            spr_zero_visible: false,
            spr_count: 0,
            sprites: [Sprite::new(); 8],
            spr_present: vec![false; Self::VISIBLE_END as usize],

            open_bus: 0x00,
        };
        ppu.set_region(ppu.region);
        ppu
    }

    #[inline]
    #[must_use]
    pub const fn system_palette(pixel: u16) -> (u8, u8, u8) {
        Self::SYSTEM_PALETTE[(pixel as usize) & (Self::SYSTEM_PALETTE.len() - 1)]
    }

    #[inline]
    #[must_use]
    pub const fn cycle(&self) -> u32 {
        self.cycle
    }

    #[inline]
    #[must_use]
    pub const fn scanline(&self) -> u32 {
        self.scanline
    }

    #[inline]
    pub const fn ctrl(&self) -> PpuCtrl {
        self.ctrl
    }

    #[inline]
    #[must_use]
    pub fn frame_buffer(&self) -> &[u16] {
        self.frame.buffer()
    }

    #[inline]
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.frame.number()
    }

    #[must_use]
    pub fn pixel_brightness(&self, x: u32, y: u32) -> u32 {
        self.frame.pixel_brightness(x, y)
    }

    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        self.bus.mirroring()
    }

    #[inline]
    pub fn update_mirroring(&mut self) {
        self.bus.update_mirroring();
    }

    #[inline]
    pub fn load_chr_rom(&mut self, chr_rom: Vec<u8>) {
        self.bus.load_chr_rom(chr_rom);
    }

    #[inline]
    pub fn load_chr_ram(&mut self, chr_ram: Vec<u8>) {
        self.bus.load_chr_ram(chr_ram);
    }

    #[inline]
    pub fn load_ex_ram(&mut self, ex_ram: Vec<u8>) {
        self.bus.load_ex_ram(ex_ram);
    }

    #[inline]
    pub fn load_mapper(&mut self, mapper: Mapper) {
        self.bus.load_mapper(mapper);
        self.update_mirroring();
    }

    #[inline]
    pub const fn mapper(&self) -> &Mapper {
        self.bus.mapper()
    }

    #[inline]
    pub fn mapper_mut(&mut self) -> &mut Mapper {
        self.bus.mapper_mut()
    }

    #[must_use]
    #[inline]
    pub const fn nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    #[inline]
    #[must_use]
    pub const fn addr(&self) -> u16 {
        self.scroll.read_addr()
    }

    #[inline]
    #[must_use]
    pub const fn oamaddr(&self) -> u8 {
        self.oamaddr
    }

    #[inline]
    #[must_use]
    pub const fn open_bus(&self) -> u8 {
        self.open_bus
    }

    #[inline]
    pub fn set_open_bus(&mut self, val: u8) {
        self.open_bus = val;
    }
}

impl Ppu {
    #[inline]
    #[must_use]
    const fn rendering_enabled(&self) -> bool {
        self.mask.show_bg() || self.mask.show_spr()
    }

    #[inline]
    fn increment_vram_addr(&mut self) {
        // During rendering, v increments coarse X and coarse Y simultaneously
        if self.rendering_enabled()
            && (self.scanline == self.prerender_scanline
                || self.scanline <= Self::VISIBLE_SCANLINE_END)
        {
            self.scroll.increment_x();
            self.scroll.increment_y();
        } else {
            self.scroll.increment(self.ctrl.vram_increment());
        }
    }

    fn start_vblank(&mut self) {
        log::trace!("({}, {}): Set VBL flag", self.cycle, self.scanline);
        if !self.prevent_vbl {
            self.status.set_in_vblank(true);
            self.nmi_pending = self.ctrl.nmi_enabled();
            log::trace!(
                "({}, {}): VBL NMI: {}",
                self.cycle,
                self.scanline,
                self.nmi_pending
            );
        }
        self.prevent_vbl = false;
        let val = self.peek_status();
        self.mapper_mut().ppu_bus_write(0x2002, val);
    }

    fn stop_vblank(&mut self) {
        log::trace!(
            "({}, {}): Clear Sprite0 Hit, Overflow",
            self.cycle,
            self.scanline
        );
        log::trace!("({}, {}): Clear VBL flag", self.cycle, self.scanline);
        self.status.set_spr_zero_hit(false);
        self.status.set_spr_overflow(false);
        self.status.reset_in_vblank();
        self.nmi_pending = false;
        self.reset_signal = false;
        let val = self.peek_status();
        self.mapper_mut().ppu_bus_write(0x2002, val);
    }

    fn fetch_bg_nt_byte(&mut self) {
        // Fetch BG nametable
        // https://wiki.nesdev.com/w/index.php/PPU_scrolling#Tile_and_attribute_fetching

        self.prev_palette = self.curr_palette;
        self.curr_palette = self.next_palette;

        self.tile_shift_lo |= u16::from(self.tile_lo);
        self.tile_shift_hi |= u16::from(self.tile_hi);

        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = Self::NT_START | (self.addr() & nametable_addr_mask);
        let tile_index = u16::from(self.bus.read(addr, Access::Read));
        self.tile_addr = self.ctrl.bg_select() | (tile_index << 4) | self.scroll.fine_y();
    }

    #[inline]
    fn fetch_bg_attr_byte(&mut self) {
        let addr = self.scroll.attr_addr();
        let shift = self.scroll.attr_shift();
        self.next_palette = ((self.bus.read(addr, Access::Read) >> shift) & 0x03) << 2;
    }

    fn fetch_background(&mut self) {
        // Fetch 4 tiles and write out shift registers every 8th cycle
        // Each tile fetch takes 2 cycles
        match self.cycle & 0x07 {
            1 => self.fetch_bg_nt_byte(),
            3 => self.fetch_bg_attr_byte(),
            5 => self.tile_lo = self.bus.read(self.tile_addr, Access::Read),
            7 => self.tile_hi = self.bus.read(self.tile_addr + 8, Access::Read),
            _ => (),
        }
    }

    fn evaluate_sprites(&mut self) {
        match self.cycle {
            // 1. Clear Secondary OAM
            Self::OAM_CLEAR_START..=Self::OAM_CLEAR_END => {
                self.oam_fetch = 0xFF;
                self.secondary_oamdata.fill(0xFF);
            }
            // 2. Read OAM to find first eight sprites on this scanline
            // 3. With > 8 sprites, check (wrongly) for more sprites to set overflow flag
            Self::SPR_EVAL_START..=Self::SPR_EVAL_END => {
                if self.cycle == Self::SPR_EVAL_START {
                    self.spr_in_range = false;
                    self.spr_zero_in_range = false;
                    self.secondary_oamaddr = 0x00;
                    self.oam_eval_done = false;
                    self.oamaddr_hi = (self.oamaddr >> 2) & 0x3F;
                    self.oamaddr_lo = (self.oamaddr) & 0x03;
                } else if self.cycle == Self::SPR_EVAL_END {
                    self.spr_zero_visible = self.spr_zero_in_range;
                    self.spr_count = (self.secondary_oamaddr >> 2) as usize;
                }

                if self.cycle & 0x01 == 0x01 {
                    // Odd cycles are reads from OAM
                    self.oam_fetch = self.oamdata[self.oamaddr as usize];
                } else {
                    // oamaddr rolled over, so we're done reading
                    if self.oam_eval_done {
                        self.oamaddr_hi = (self.oamaddr_hi + 1) & 0x3F;
                        if self.secondary_oamaddr >= 0x20 {
                            self.oam_fetch =
                                self.secondary_oamdata[self.secondary_oamaddr as usize & 0x1F];
                        }
                    } else {
                        // If previously not in range, interpret this byte as y
                        let y = u32::from(self.oam_fetch);
                        let height = self.ctrl.spr_height();
                        if !self.spr_in_range && (y..y + height).contains(&self.scanline) {
                            self.spr_in_range = true;
                        }

                        // Even cycles are writes to Secondary OAM
                        if self.secondary_oamaddr < 0x20 {
                            self.secondary_oamdata[self.secondary_oamaddr as usize] =
                                self.oam_fetch;

                            if self.spr_in_range {
                                self.oamaddr_lo += 1;
                                self.secondary_oamaddr += 1;

                                if self.oamaddr_hi == 0x00 {
                                    self.spr_zero_in_range = true;
                                }
                                if self.oamaddr_lo == 0x04 {
                                    self.spr_in_range = false;
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
                                self.secondary_oamdata[self.secondary_oamaddr as usize & 0x1F];
                            if self.spr_in_range {
                                self.status.set_spr_overflow(true);
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

    fn load_sprites(&mut self) {
        let idx = (self.cycle - Self::SPR_FETCH_START) as usize / 8;
        let oam_idx = idx << 2;

        if let [y, tile_number, attr, x] = self.secondary_oamdata[oam_idx..=oam_idx + 3] {
            let x = u32::from(x);
            let y = u32::from(y);
            let mut tile_number = u16::from(tile_number);
            let palette = ((attr & 0x03) << 2) | 0x10;
            let bg_priority = (attr & 0x20) == 0x20;
            let flip_horizontal = (attr & 0x40) == 0x40;
            let flip_vertical = (attr & 0x80) == 0x80;

            let height = self.ctrl.spr_height();
            // Should be in the range 0..=7 or 0..=15 depending on sprite height
            let mut line_offset = if (y..y + height).contains(&self.scanline) {
                self.scanline - y
            } else {
                0
            };
            if flip_vertical {
                line_offset = height - 1 - line_offset;
            }

            if idx >= self.spr_count {
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
                self.ctrl.spr_select() | (tile_number << 4) | line_offset as u16
            };

            if idx < self.spr_count {
                let mut sprite = &mut self.sprites[idx];
                sprite.x = x;
                sprite.y = y;
                sprite.tile_lo = self.bus.read(tile_addr, Access::Read);
                sprite.tile_hi = self.bus.read(tile_addr + 8, Access::Read);
                sprite.palette = palette;
                sprite.bg_priority = bg_priority;
                sprite.flip_horizontal = flip_horizontal;
                sprite.flip_vertical = flip_vertical;
                for spr in self.spr_present.iter_mut().skip(sprite.x as usize).take(8) {
                    *spr = true;
                }
            } else {
                // Fetches for remaining sprites/hidden fetch tile $FF - used by MMC3 IRQ
                // counter
                let _ = self.bus.read(tile_addr, Access::Read);
                let _ = self.bus.read(tile_addr + 8, Access::Read);
            }
        }
    }

    // http://wiki.nesdev.com/w/index.php/PPU_OAM
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

    fn pixel_color(&mut self) -> u8 {
        let x = self.cycle - 1;

        let left_clip_bg = x < 8 && !self.mask.show_left_bg();
        let bg_color = if self.mask.show_bg() && !left_clip_bg {
            let offset = self.scroll.fine_x();
            ((((self.tile_shift_hi << offset) & 0x8000) >> 14)
                | (((self.tile_shift_lo << offset) & 0x8000) >> 15)) as u8
        } else {
            0
        };

        let left_clip_spr = x < 8 && !self.mask.show_left_spr();
        if self.mask.show_spr() && !left_clip_spr && self.spr_present[x as usize] {
            for (i, sprite) in self.sprites.iter().take(self.spr_count).enumerate() {
                let shift = x as i16 - sprite.x as i16;
                if (0..=7).contains(&shift) {
                    let spr_color = if sprite.flip_horizontal {
                        (((sprite.tile_hi >> shift) & 0x01) << 1)
                            | ((sprite.tile_lo >> shift) & 0x01)
                    } else {
                        (((sprite.tile_hi << shift) & 0x80) >> 6)
                            | ((sprite.tile_lo << shift) & 0x80) >> 7
                    };
                    if spr_color != 0 {
                        if i == 0
                            && bg_color != 0
                            && self.spr_zero_visible
                            && x != 255
                            && self.rendering_enabled()
                            && !self.status.spr_zero_hit()
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
        }
        let palette = if (self.scroll.fine_x() + ((x & 0x07) as u16)) < 8 {
            self.prev_palette
        } else {
            self.curr_palette
        };
        palette + bg_color
    }

    fn render_pixel(&mut self) {
        let x = self.cycle - 1;
        let y = self.scanline;
        let palette_addr = if self.rendering_enabled()
            || (self.addr() & Self::PALETTE_START) != Self::PALETTE_START
        {
            let color = self.pixel_color();
            if color & 0x03 > 0 {
                u16::from(color)
            } else {
                0
            }
        } else {
            self.addr() & 0x1F
        };
        let mut color = self
            .bus
            .read(Self::PALETTE_START + palette_addr, Access::Read)
            .into();
        color &= if self.mask.grayscale() { 0x30 } else { 0x3F };
        color |= u16::from(self.mask.emphasis(self.region)) << 1;
        self.frame.set_pixel(x, y, color);
    }

    fn tick(&mut self) {
        let visible_cycle = matches!(self.cycle, Self::VISIBLE_START..=Self::VISIBLE_END);
        let bg_prefetch_cycle =
            matches!(self.cycle, Self::BG_PREFETCH_START..=Self::BG_PREFETCH_END);
        let bg_dummy_cycle = matches!(self.cycle, Self::BG_DUMMY_START..=Self::CYCLE_END);
        let bg_fetch_cycle = bg_prefetch_cycle || visible_cycle;
        let spr_eval_cycle = matches!(self.cycle, Self::VISIBLE_START..=Self::SPR_EVAL_END);
        let spr_fetch_cycle = matches!(self.cycle, Self::SPR_FETCH_START..=Self::SPR_FETCH_END);
        let spr_dummy_cycle = matches!(self.cycle, Self::BG_PREFETCH_START..=Self::CYCLE_END);

        let visible_scanline = self.scanline <= Self::VISIBLE_SCANLINE_END;
        let prerender_scanline = self.scanline == self.prerender_scanline;
        let render_scanline = prerender_scanline || visible_scanline;

        if self.rendering_enabled() {
            if visible_scanline
                || (self.region == NesRegion::Pal && self.scanline >= self.pal_spr_eval_scanline)
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
                        self.scroll.increment_x();
                    }
                } else if bg_dummy_cycle {
                    // Dummy byte fetches
                    // (337, 0) - (337, 239)
                    self.fetch_bg_nt_byte();
                }

                match self.cycle {
                    Self::VISIBLE_START..=8 if prerender_scanline && self.oamaddr >= 0x08 => {
                        // If OAMADDR is not less than eight when rendering starts, the eight bytes
                        // starting at OAMADDR & 0xF8 are copied to the first eight bytes of OAM
                        let addr = self.cycle as usize - 1;
                        self.oamdata[addr] = self.oamdata[(self.oamaddr as usize & 0xF8) + addr];
                    }
                    // Increment Fine Y when we reach the end of the screen
                    Self::INC_Y => self.scroll.increment_y(),
                    // Copy X bits at the start of a new line since we're going to start writing
                    // new x values to t
                    Self::COPY_X => self.scroll.copy_x(),
                    // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
                    // if rendering is enabled
                    // http://wiki.nesdev.com/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
                    Self::COPY_Y_START..=Self::COPY_Y_END if prerender_scanline => {
                        self.scroll.copy_y();
                    }
                    _ => (),
                }

                if prerender_scanline {
                    // Force prerender scanline sprite fetches to load the dummy $FF tiles (fixes
                    // shaking in Ninja Gaiden 3 stage 1 after beating boss)
                    self.spr_count = 0;
                }
                if spr_fetch_cycle {
                    if self.cycle == Self::SPR_FETCH_START {
                        self.spr_present.fill(false);
                    }
                    self.fetch_sprites();
                }
                if spr_dummy_cycle {
                    self.oam_fetch = self.secondary_oamdata[0];
                }

                if self.cycle == Self::ODD_SKIP
                    && prerender_scanline
                    && self.frame_number() & 0x01 == 0x01
                    && self.region == NesRegion::Ntsc
                {
                    // NTSC behavior while rendering - each odd PPU frame is one clock shorter
                    // (skipping from 339 over 340 to 0)
                    log::trace!(
                        "({}, {}): Skipped odd frame cycle: {}",
                        self.cycle,
                        self.scanline,
                        self.frame_number()
                    );
                    self.cycle = Self::CYCLE_END;
                }
            }
        }

        // Pixels should be put even if rendering is disabled, as this is what blanks out the
        // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
        if visible_cycle && visible_scanline {
            self.render_pixel();
        }
        if bg_fetch_cycle {
            self.tile_shift_lo <<= 1;
            self.tile_shift_hi <<= 1;
        }
    }
}

impl PpuRegisters for Ppu {
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
    #[inline]
    fn write_ctrl(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal {
            return;
        }
        self.ctrl.write(val);
        self.scroll.write_nametable_select(val);

        log::trace!(
            "({}, {}): $2000 NMI Enabled: {}",
            self.cycle,
            self.scanline,
            self.ctrl.nmi_enabled()
        );

        // By toggling NMI (bit 7) during VBlank without reading $2002, /NMI can be pulled low
        // multiple times, causing multiple NMIs to be generated.
        if !self.ctrl.nmi_enabled() {
            log::trace!("({}, {}): $2000 NMI Disable", self.cycle, self.scanline);
            self.nmi_pending = false;
        } else if self.status.in_vblank() {
            log::trace!("({}, {}): $2000 NMI During VBL", self.cycle, self.scanline);
            self.nmi_pending = true;
        }
    }

    // $2001 | RW  | PPUMASK
    //       |   0 | Unknown (???)
    //       |   1 | BG Mask, 0 = don't show background in left 8 columns
    //       |   2 | Sprite Mask, 0 = don't show sprites in left 8 columns
    //       |   3 | BG Switch, 1 = show background, 0 = hide background
    //       |   4 | Sprites Switch, 1 = show sprites, 0 = hide sprites
    //       | 5-7 | Unknown (???)
    #[inline]
    fn write_mask(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal {
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
    #[inline]
    fn read_status(&mut self) -> u8 {
        let status = self.peek_status();
        if self.nmi_pending() {
            log::trace!("({}, {}): $2002 NMI Ack", self.cycle, self.scanline);
        }
        self.nmi_pending = false;
        self.status.reset_in_vblank();
        self.scroll.reset_latch();

        if self.scanline == self.vblank_scanline && self.cycle == Self::VBLANK - 1 {
            // Reading PPUSTATUS one clock before the start of vertical blank will read as clear
            // and never set the flag or generate an NMI for that frame
            log::trace!("({}, {}): $2002 Prevent VBL", self.cycle, self.scanline);
            self.prevent_vbl = true;
        }
        self.open_bus |= status & 0xE0;
        self.mapper_mut().ppu_bus_write(0x2002, status);
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
    #[inline]
    fn peek_status(&self) -> u8 {
        // Only upper 3 bits are connected for this register
        (self.status.read() & 0xE0) | (self.open_bus & 0x1F)
    }

    // $2003 | W   | OAMADDR
    //       |     | Used to set the address in the 256-byte Sprite Memory to be
    //       |     | accessed via $2004. This address will increment by 1 after
    //       |     | each access to $2004. The Sprite Memory contains coordinates,
    //       |     | colors, and other attributes of the sprites.
    #[inline]
    fn write_oamaddr(&mut self, val: u8) {
        self.open_bus = val;
        self.oamaddr = val;
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    #[inline]
    #[must_use]
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
    #[inline]
    #[must_use]
    fn peek_oamdata(&self) -> u8 {
        // Reading OAMDATA during rendering will expose OAM accesses during sprite evaluation and loading
        if self.scanline <= Self::VISIBLE_SCANLINE_END
            && self.rendering_enabled()
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
    #[inline]
    fn write_oamdata(&mut self, mut val: u8) {
        self.open_bus = val;
        if self.rendering_enabled()
            && (self.scanline <= Self::VISIBLE_SCANLINE_END
                || self.scanline == self.prerender_scanline
                || (self.region == NesRegion::Pal && self.scanline >= self.pal_spr_eval_scanline))
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
    #[inline]
    fn write_scroll(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal {
            return;
        }
        self.scroll.write(val);
    }

    // $2006 | W   | PPUADDR
    #[inline]
    fn write_addr(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal {
            return;
        }
        self.scroll.write_addr(val);
        // MMC3 clocks using A12
        let addr = self.scroll.read_addr();
        self.mapper_mut().ppu_bus_write(addr, val);
    }

    // $2007 | RW  | PPUDATA
    #[inline]
    #[must_use]
    fn read_data(&mut self) -> u8 {
        let addr = self.scroll.read_addr();
        self.increment_vram_addr();

        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in $0000 - $3EFF
        let val = self.bus.read(addr, Access::Read);
        let val = if addr < Self::PALETTE_START {
            let buffer = self.vram_buffer;
            self.vram_buffer = val;
            buffer
        } else {
            // Set internal buffer with mirrors of nametable when reading palettes
            // Since we're reading from > $3EFF subtract $1000 to fill
            // buffer with nametable mirror data
            self.vram_buffer = self.bus.read(addr - 0x1000, Access::Dummy);
            // Hi 2 bits of palette should be open bus
            val | (self.open_bus & 0xC0)
        };

        self.open_bus = val;
        // MMC3 clocks using A12
        let addr = self.scroll.read_addr();
        self.mapper_mut().ppu_bus_write(addr, val);

        val
    }

    // $2007 | RW  | PPUDATA
    //
    // Non-mutating version of `read_data`.
    #[inline]
    #[must_use]
    fn peek_data(&self) -> u8 {
        let addr = self.scroll.read_addr();
        if addr < Self::PALETTE_START {
            self.vram_buffer
        } else {
            // Hi 2 bits of palette should be open bus
            self.bus.peek(addr, Access::Dummy) | (self.open_bus & 0xC0)
        }
    }

    // $2007 | RW  | PPUDATA
    #[inline]
    fn write_data(&mut self, val: u8) {
        self.open_bus = val;
        let addr = self.scroll.read_addr();
        self.increment_vram_addr();
        self.bus.write(addr, val, Access::Write);

        // MMC3 clocks using A12
        let addr = self.scroll.read_addr();
        self.mapper_mut().ppu_bus_write(addr, val);
    }
}

impl Mem for Ppu {
    fn peek(&self, addr: u16, access: Access) -> u8 {
        self.bus.peek(addr, access)
    }

    fn write(&mut self, addr: u16, val: u8, access: Access) {
        self.bus.write(addr, val, access);
    }
}

impl Clock for Ppu {
    fn clock(&mut self) -> usize {
        // Clear open bus roughly once every frame
        if self.scanline == 0 {
            self.open_bus = 0x00;
        }

        if self.cycle >= Self::CYCLE_END {
            self.cycle = 0;
            self.scanline += 1;
            // Post-render line
            if self.scanline == self.vblank_scanline - 1 {
                self.frame.increment();
            } else if self.scanline > self.prerender_scanline {
                self.scanline = 0;
            }
        } else {
            // cycle > 0
            self.cycle += 1;
            self.tick();

            if self.cycle == Self::VBLANK {
                if self.scanline == self.vblank_scanline {
                    self.start_vblank();
                }
                if self.scanline == self.prerender_scanline {
                    self.stop_vblank();
                }
            }
        }

        self.cycle_count = self.cycle_count.wrapping_add(1);
        1
    }

    fn clock_to(&mut self, clock: u64) {
        while self.master_clock + self.clock_divider <= clock {
            self.clock();
            self.master_clock += self.clock_divider;
        }
    }
}

impl Regional for Ppu {
    #[inline]
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        let (clock_divider, vblank_scanline, prerender_scanline) = match region {
            NesRegion::Ntsc => (4, 241, 261),
            NesRegion::Pal => (5, 241, 311),
            NesRegion::Dendy => (5, 291, 311),
        };
        self.region = region;
        self.clock_divider = clock_divider;
        self.vblank_scanline = vblank_scanline;
        self.prerender_scanline = prerender_scanline;
        // PAL refreshes OAM later due to extended vblank to avoid OAM decay
        self.pal_spr_eval_scanline = self.vblank_scanline + 24;
        self.bus.set_region(region);
    }
}

impl Reset for Ppu {
    fn reset(&mut self, kind: Kind) {
        self.reset_signal = true;
        self.ctrl.reset(kind);
        self.mask.reset(kind);
        self.status.reset(kind);
        if kind == Kind::Hard {
            self.oamaddr = 0x0000;
        }
        self.secondary_oamaddr = 0x0000;
        self.scroll.reset(kind);
        self.vram_buffer = 0x00;
        self.cycle = 0;
        self.scanline = 0;
        self.master_clock = 0;
        self.nmi_pending = false;
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
        self.spr_present.fill(false);
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
            .field("oamdata_len", &self.oamdata.len())
            .field("secondary_oamaddr", &self.secondary_oamaddr)
            .field("secondary_oamdata_len", &self.secondary_oamdata.len())
            .field("scroll", &self.scroll)
            .field("vram_buffer", &self.vram_buffer)
            .field("cycle", &self.cycle)
            .field("scanline", &self.scanline)
            .field("master_clock", &self.master_clock)
            .field("clock_divider", &self.clock_divider)
            .field("vblank_scanline", &self.vblank_scanline)
            .field("prerender_scanline", &self.prerender_scanline)
            .field("pal_spr_eval_scanline", &self.pal_spr_eval_scanline)
            .field("nmi_pending", &self.nmi_pending)
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
            .field("spr_present_len", &self.spr_present.len())
            .field("open_bus", &self.open_bus)
            .finish()
    }
}

#[cfg(test)]
impl Ppu {
    pub(crate) const fn master_clock(&self) -> u64 {
        self.master_clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::Cart,
        mapper::{Mapped, Mmc1Revision, Sxrom},
    };

    #[test]
    fn vram_writes() {
        let mut ppu = Ppu::default();
        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        ppu.write_data(0x66); // write to $2305

        assert_eq!(ppu.bus.read(0x2305, Access::Read), 0x66);
    }

    #[test]
    fn vram_reads() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0x00);
        ppu.bus.write(0x2305, 0x66, Access::Write);

        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.scroll.read_addr(), 0x2306);
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.scroll.read_addr(), 0x2307);
    }

    #[test]
    fn vram_read_pagecross() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0x00);
        ppu.bus.write(0x21FF, 0x66, Access::Write);
        ppu.bus.write(0x2200, 0x77, Access::Write);

        ppu.write_addr(0x21);
        ppu.write_addr(0xFF);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.read_data(), 0x77);
    }

    #[test]
    fn vram_read_vertical_increment() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0b100);
        ppu.bus.write(0x21FF, 0x66, Access::Write);
        ppu.bus.write(0x21FF + 32, 0x77, Access::Write);
        ppu.bus.write(0x21FF + 64, 0x88, Access::Write);

        ppu.write_addr(0x21);
        ppu.write_addr(0xFF);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.read_data(), 0x77);
        assert_eq!(ppu.read_data(), 0x88);
    }

    // Horizontal: https://wiki.nesdev.com/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 a ]
    //   [0x2800 B ] [0x2C00 b ]
    #[test]
    fn vram_horizontal_mirror() {
        let mut ppu = Ppu::default();
        ppu.write_addr(0x24);
        ppu.write_addr(0x05);
        ppu.write_data(0x66); // write to a at $2405

        ppu.write_addr(0x28);
        ppu.write_addr(0x05);
        ppu.write_data(0x77); // write to B at $2805

        ppu.write_addr(0x20);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66); // read A from $2005

        ppu.write_addr(0x2C);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x77); // read b from $2C05
    }

    // Vertical: https://wiki.nesdev.com/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 B ]
    //   [0x2800 a ] [0x2C00 b ]
    #[test]
    fn vram_vertical_mirror() {
        let mut ppu = Ppu::default();
        let mut cart = Cart::default();
        let mut mapper = Sxrom::load(&mut cart, Mmc1Revision::BC);
        mapper.set_mirroring(Mirroring::Vertical);
        ppu.load_mapper(mapper);

        ppu.write_addr(0x20);
        ppu.write_addr(0x05);
        ppu.write_data(0x66); // write to A at $2005

        ppu.write_addr(0x2C);
        ppu.write_addr(0x05);
        ppu.write_data(0x77); // write to b at $2C05

        ppu.write_addr(0x28);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66); // read a from $2805

        ppu.write_addr(0x24);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x77); // read B from $2405
    }

    #[test]
    fn read_status_resets_latch() {
        let mut ppu = Ppu::default();
        ppu.bus.write(0x2305, 0x66, Access::Write);

        ppu.write_addr(0x21);
        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_ne!(ppu.read_data(), 0x66);

        ppu.read_status();

        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.read_data(), 0x66);
    }

    #[test]
    fn vram_mirroring() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0);
        ppu.bus.write(0x2305, 0x66, Access::Write);

        ppu.write_addr(0x63); // 0x6305 mirrors to 0x2305
        ppu.write_addr(0x05);
        ppu.read_data(); // buffer read
        assert_eq!(ppu.scroll.read_addr(), 0x2306);
        assert_eq!(ppu.read_data(), 0x66);
        assert_eq!(ppu.scroll.read_addr(), 0x2307);
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
