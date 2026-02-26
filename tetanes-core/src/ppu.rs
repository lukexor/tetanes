//! NES PPU (Picture Processing Unit) implementation.

use crate::{
    common::{Clock, ClockTo, NesRegion, Regional, Reset, ResetKind},
    cpu::Cpu,
    debug::PpuDebugger,
    mapper::{Map, Mapper},
    mem::{ConstArray, Read, Write},
    ppu::frame::Frame,
};
use ctrl::Ctrl;
use mask::Mask;
use scroll::Scroll;
use serde::{Deserialize, Serialize};
use sprite::Sprite;
use status::Status;
use std::{
    cmp::Ordering,
    ops::{Index, IndexMut},
};
use tracing::{error, trace};

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

/// Palette RAM which enforces mirroring.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(transparent)]
pub struct PaletteRam(ConstArray<u8, 32>);

impl PaletteRam {
    /// Return palette address, mirrored.
    #[inline(always)]
    const fn mirror(addr: u16) -> usize {
        const PALETTE_MIRROR: [u8; 32] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 17, 18, 19, 4, 21, 22, 23, 8,
            25, 26, 27, 12, 29, 30, 31,
        ];
        PALETTE_MIRROR[(addr & 0x1F) as usize] as usize
    }
}

impl Read for PaletteRam {
    #[inline(always)]
    fn peek(&self, addr: u16) -> u8 {
        self.0[Self::mirror(addr)]
    }
}

impl Write for PaletteRam {
    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8) {
        self.0[Self::mirror(addr)] = val;
    }
}

/// Console-Internal RAM (VRAM) which enforces mirroring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(transparent)]
pub struct CIRam(Box<ConstArray<u8, { size::VRAM }>>);

impl CIRam {
    // Maps addresses to nametable pages based on mirroring mode
    //
    // Vram:            [ A ] [ B ]
    //
    // Horizontal:      [ A ] [ a ]
    //                  [ B ] [ b ]
    //
    // Vertical:        [ A ] [ B ]
    //                  [ a ] [ b ]
    //
    // Single Screen A: [ A ] [ a ]
    //                  [ a ] [ a ]
    //
    // Single Screen B: [ b ] [ B ]
    //                  [ b ] [ b ]
    //
    // Fourscreen should not use this method and instead should rely on mapper translation.
    #[inline(always)]
    pub const fn mirror(addr: u16, mirroring: Mirroring) -> usize {
        let nametable = (addr >> mirroring as u16) & size::NAMETABLE;
        (nametable | (!nametable & addr & 0x03FF)) as usize
    }

    #[inline(always)]
    pub fn read(&mut self, addr: u16, mirroring: Mirroring) -> u8 {
        self.0[Self::mirror(addr, mirroring)]
    }

    #[inline(always)]
    pub fn peek(&self, addr: u16, mirroring: Mirroring) -> u8 {
        self.0[Self::mirror(addr, mirroring)]
    }

    #[inline(always)]
    pub fn write(&mut self, addr: u16, val: u8, mirroring: Mirroring) {
        self.0[Self::mirror(addr, mirroring)] = val
    }
}

impl Index<usize> for CIRam {
    type Output = u8;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.0.index(index)
    }
}

impl IndexMut<usize> for CIRam {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.0.index_mut(index)
    }
}

pub trait PpuAddr {
    /// Returns whether this value can be used to fetch a nametable attribute byte.
    fn is_attr(&self) -> bool;
    /// Returns whether this value is a palette address.
    fn is_palette(&self) -> bool;
}

impl PpuAddr for u16 {
    #[inline(always)]
    fn is_attr(&self) -> bool {
        (*self & (size::NAMETABLE - 1)) >= addr::ATTR_OFFSET
    }

    #[inline(always)]
    fn is_palette(&self) -> bool {
        *self >= addr::PALETTE_START
    }
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Ppu {
    /// Master clock synced to Cpu master clock.
    pub master_clock: u32,
    /// (0, 340) cycles per scanline.
    pub cycle: u16,
    /// (0, 261) NTSC or (0, 311) PAL/Dendy scanlines per frame.
    pub scanline: u16,
    /// $2001 PPUMASK (write-only).
    pub mask: Mask,
    // === 20 ===
    /// $2000 PPUCTRL (write-only).
    pub ctrl: Ctrl,
    // === 30 ===
    /// $2005 PPUSCROLL and $2006 PPUADDR (write-only).
    pub scroll: Scroll,
    /// Scanline that Vertical Blank (VBlank) starts on.
    pub vblank_scanline: u16,
    /// Scanline that Prerender starts on.
    pub prerender_scanline: u16,
    /// Tile shift low byte.
    pub tile_shift_lo: u16,
    /// Tile shift high byte.
    pub tile_shift_hi: u16,
    /// Tile address.
    pub tile_addr: u16,
    /// Tile fetch buffer low byte.
    pub tile_lo: u8,
    /// Tile fetch buffer high byte.
    pub tile_hi: u8,
    /// Master clock divider.
    pub clock_divider: u8,
    /// Whatever was last read or written to to the Ppu.
    pub open_bus: u8,
    /// Internal signal that clears status registers and prevents writes and cleared at the end of
    /// VBlank.
    /// See: <https://www.nesdev.org/wiki/PPU_power_up_state>
    pub reset_signal: bool,

    /// Current tile palette.
    pub curr_palette: u8,
    /// Previous tile palette.
    pub prev_palette: u8,
    /// Next tile palette.
    pub next_palette: u8,
    /// Whether PPU is skipping rendering (used for
    /// [`HeadlessMode`](crate::control_deck::HeadlessMode)).
    pub skip_rendering: bool,

    /// Scanline is visible.
    pub is_visible_scanline: bool,
    /// Scanline is a pre-render scanline.
    pub is_prerender_scanline: bool,
    /// Scanline is a render scanline.
    pub is_render_scanline: bool,

    // === 64 : end of cache line ===
    /// $2002 PPUSTATUS (read-only).
    pub status: Status,
    /// Scanline is a PAL sprite evaluation scanline.
    pub is_pal_spr_eval_scanline: bool,

    // Sprite/OAM evaluation.
    /// Sprite is in scanline range.
    pub spr_in_range: bool,
    /// Sprite 0 is in scanline range.
    pub spr_zero_in_range: bool,
    /// Secondary OAM address.
    pub secondary_oamaddr: u8,
    /// OAM evaluation is complete for scanline.
    pub oam_eval_done: bool,
    /// OAM address low byte.
    pub oamaddr_lo: u8,
    /// OAM address high byte.
    pub oamaddr_hi: u8,
    /// OAM data fetch buffer.
    pub oam_fetch: u8,
    /// $2003 OAM addr (write-only).
    pub oamaddr: u8,
    /// Sprite 0 is visible.
    pub spr_zero_visible: bool,
    /// Number of sprites on the current scanline.
    pub spr_count: u8,
    /// Sprite overflow count (> 8 on a scanline).
    pub overflow_count: u8,

    /// Current PPU frame buffer.
    pub frame: Frame,
    /// Console-Internal RAM (CIRAM).
    pub ciram: CIRam,

    // === 128 : end of cache line ===
    // Palette RAM
    pub palette: PaletteRam,
    /// Secondary OAM data on a given scanline.
    pub secondary_oamdata: ConstArray<u8, 32>,

    // === 192 : end of cache line ===
    /// Each scanline can hold 8 sprites at a time before the `spr_overflow` flag is set.
    pub sprites: Box<[Sprite; 8]>,
    /// Whether a sprite is present at the given x-coordinate. Used for `spr_zero_hit` detection.
    // This is a per-frame optimization, shouldn't need to be saved
    #[serde(skip)]
    pub spr_present: ConstArray<bool, 256>,
    // === 384 : end of cache line
    /// $2004 Object Attribute Memory (OAM) data (read/write).
    pub oamdata: ConstArray<u8, 256>,

    // === 640 : end of cache line
    /// Mapper.
    pub mapper: Mapper,

    /// $2007 PPUDATA buffer.
    pub vram_buffer: u8,
    /// Prevents VBL from being triggered this frame.
    pub prevent_vbl: bool,
    /// Current NesRegion.
    pub region: NesRegion,
    /// Whether to emulate PPU warmup on power up.
    pub emulate_warmup: bool,

    /// Attached Ppu Debugger.
    // Don't save debug state
    #[serde(skip)]
    pub debugger: PpuDebugger,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(NesRegion::default())
    }
}

pub mod addr {
    //! Address constants.

    pub const NAMETABLE_START: u16 = 0x2000;
    pub const ATTR_OFFSET: u16 = 0x03C0;

    pub const PALETTE_START: u16 = 0x3F00;
    pub const PALETTE_END: u16 = 0x3F20;
}

pub mod size {
    //! Memory size constants.

    pub const WIDTH: u16 = 256;
    pub const HEIGHT: u16 = 240;
    pub const FRAME: usize = (WIDTH * HEIGHT) as usize;

    pub const NAMETABLE: u16 = 0x0400;
    pub const OAM: usize = 256; // 64 4-byte sprites per frame
    pub const SECONDARY_OAM: usize = 32; // 8 4-byte sprites per scanline

    pub const VRAM: usize = 0x0800; // Two 1k Nametables
    pub const PALETTE: usize = 32; // 32 possible colors at a time
}

pub mod cycle {
    //! Cycle constants.
    //! https://www.nesdev.org/wiki/PPU_rendering

    use std::ops::RangeInclusive;

    pub const START: u16 = 0;
    pub const ODD_SKIP: u16 = 339; // Odd frames skip the last cycle
    pub const END: u16 = 340;

    pub const VISIBLE_START: u16 = 1; // Tile data fetching starts
    pub const VISIBLE_END: u16 = 256; // 2 cycles each for 4 fetches = 32 tiles

    pub const VBLANK: u16 = VISIBLE_START; // When VBlank flag gets set/cleared

    pub const OAM_CLEAR_START: u16 = 1;
    pub const OAM_CLEAR_END: u16 = 64;

    pub const SPR_EVAL_START: u16 = 65;
    pub const SPR_EVAL_START1: u16 = 66; // Used to split up match arms
    pub const SPR_EVAL_END0: u16 = 255; // Used to split up match arms
    pub const SPR_EVAL_END: u16 = 256;
    pub const SPR_FETCH_START: u16 = 257; // Sprites for next scanline fetch starts
    pub const SPR_FETCH_END: u16 = 320; // 2 cycles each for 4 fetches = 8 sprites
    pub const SPR_FETCH_RANGE: RangeInclusive<u16> = SPR_FETCH_START..=SPR_FETCH_END;

    pub const BG_PREFETCH_START: u16 = 321; // Tile data for next scanline fetched
    pub const BG_PREFETCH_END: u16 = 336; // 2 cycles each for 4 fetches = 2 tiles
    pub const BG_PREFETCH_RANGE: RangeInclusive<u16> = BG_PREFETCH_START..=BG_PREFETCH_END;

    pub const BG_DUMMY_START: u16 = 337; // Dummy fetches - use is unknown
    pub const BG_DUMMY_END: u16 = END;

    pub const INC_Y: u16 = 256; // Increase Y scroll when it reaches end of the screen
    pub const COPY_Y_START: u16 = 280; // Copy Y scroll start
    pub const COPY_Y_END: u16 = 304; // Copy Y scroll stop
    pub const COPY_Y_RANGE: RangeInclusive<u16> = COPY_Y_START..=COPY_Y_END;

    // Clock dividers
    pub const DIVIDER_NTSC: u8 = 4;
    pub const DIVIDER_PAL: u8 = 5;
    pub const DIVIDER_DENDY: u8 = DIVIDER_PAL;
}

pub mod scanline {
    //! Scanline constants.
    //! https://www.nesdev.org/wiki/PPU_rendering

    pub const START: u16 = 0;

    pub const VISIBLE_START: u16 = START;
    pub const VISIBLE_END: u16 = 239;

    pub const POSTRENDER: u16 = 240;
    pub const PRERENDER_NTSC: u16 = 261;
    pub const PRERENDER_PAL: u16 = 311;
    pub const PRERENDER_DENDY: u16 = PRERENDER_PAL;

    pub const VBLANK_NTSC: u16 = 241;
    pub const VBLANK_PAL: u16 = VBLANK_NTSC;
    pub const VBLANK_DENDY: u16 = 291;
}

impl Ppu {
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
    pub fn new(region: NesRegion) -> Self {
        let mut ppu = Self {
            master_clock: 0,
            clock_divider: 0,
            cycle: 0,
            scanline: 0,
            vblank_scanline: 0,
            prerender_scanline: 0,
            is_visible_scanline: true,
            is_prerender_scanline: false,
            is_render_scanline: true,
            is_pal_spr_eval_scanline: false,
            open_bus: 0x00,

            mask: Mask::new(region),
            scroll: Scroll::new(),
            ctrl: Ctrl::new(),

            // NOTE: PPU RAM is a bit more predictable at power on - games like Huge Insect don't
            // properly initialize both nametables, which can result in garbage sprites when
            // randomizing CIRAM.
            palette: PaletteRam(ConstArray::new()),
            mapper: Mapper::none(),
            ciram: CIRam(Box::new(ConstArray::new())),

            prev_palette: 0x00,
            curr_palette: 0x00,
            next_palette: 0x00,
            tile_shift_lo: 0x0000,
            tile_shift_hi: 0x0000,
            tile_lo: 0x00,
            tile_hi: 0x00,
            tile_addr: 0x0000,

            status: Status::new(),

            oam_fetch: 0x00,
            oamaddr: 0x0000,
            oamaddr_lo: 0x00,
            oamaddr_hi: 0x00,
            oam_eval_done: false,
            secondary_oamaddr: 0x0000,
            overflow_count: 0,
            spr_in_range: false,
            spr_zero_in_range: false,
            spr_zero_visible: false,
            spr_count: 0,
            vram_buffer: 0x00,

            oamdata: ConstArray::new(),
            secondary_oamdata: ConstArray::new(),
            sprites: [Sprite::new(); 8].into(),
            spr_present: ConstArray::new(),

            prevent_vbl: false,
            frame: Frame::new(),

            region,
            skip_rendering: false,
            reset_signal: false,
            emulate_warmup: false,

            debugger: Default::default(),
        };

        ppu.set_region(ppu.region);

        ppu
    }

    /// Read a byte from CHR-ROM/RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_read(&mut self, addr: u16) -> u8 {
        self.mapper.chr_read(addr, &self.ciram)
    }

    /// Peek a byte from CHR-ROM/RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16) -> u8 {
        self.mapper.chr_peek(addr, &self.ciram)
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8) {
        self.mapper.chr_write(addr, val, &mut self.ciram)
    }

    /// Read from `addr` on Ppu bus.
    #[inline]
    fn bus_read(&mut self, addr: u16) -> u8 {
        self.open_bus = match addr {
            0x0000..=0x3EFF => self.chr_read(addr),
            0x3F00..=0x3FFF => self.palette.read(addr),
            _ => {
                error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        };
        self.open_bus
    }

    #[inline]
    fn bus_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3EFF => self.chr_peek(addr),
            0x3F00..=0x3FFF => self.palette.peek(addr),
            _ => {
                error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        }
    }

    /// Write `val` to `addr` on Ppu bus.
    #[inline]
    fn bus_write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
            0x0000..=0x3EFF => self.chr_write(addr, val),
            0x3F00..=0x3FFF => self.palette.write(addr, val),
            _ => error!("unexpected PPU memory access at ${:04X}", addr),
        }
    }

    /// Return the current frame buffer.
    #[inline]
    #[must_use]
    pub fn frame_buffer(&self) -> &[u16] {
        self.frame.buffer()
    }

    /// Return the current frame number.
    #[inline(always)]
    #[must_use]
    pub const fn frame_number(&self) -> u32 {
        self.frame.number()
    }

    /// Get the pixel pixel brightness at the given coordinates.
    #[inline]
    #[must_use]
    pub fn pixel_brightness(&self, x: u16, y: u16) -> u32 {
        self.frame.pixel_brightness(x, y)
    }

    /// Load a Mapper into the PPU.
    #[inline]
    pub fn load_mapper(&mut self, mapper: Mapper) {
        self.mapper = mapper;
    }

    /// Return the current Nametable mirroring mode.
    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        self.mapper.mirroring()
    }

    /// Snapshot the PPU state, excluding internal transient state, the current frame buffer.
    pub fn snapshot(&self) -> Self {
        Self {
            master_clock: self.master_clock,
            clock_divider: self.clock_divider,
            cycle: self.cycle,
            scanline: self.scanline,
            vblank_scanline: self.vblank_scanline,
            prerender_scanline: self.prerender_scanline,
            is_visible_scanline: self.is_visible_scanline,
            is_prerender_scanline: self.is_prerender_scanline,
            is_render_scanline: self.is_render_scanline,
            is_pal_spr_eval_scanline: self.is_pal_spr_eval_scanline,
            open_bus: self.open_bus,

            mask: self.mask,
            scroll: self.scroll,
            ctrl: self.ctrl,

            palette: self.palette,
            ciram: self.ciram.clone(),
            mapper: self.mapper.clone(),

            curr_palette: self.curr_palette,

            status: self.status,

            secondary_oamaddr: self.secondary_oamaddr,

            oamdata: self.oamdata,
            secondary_oamdata: self.secondary_oamdata,

            sprites: self.sprites.clone(),

            ..Default::default()
        }
    }

    /// Load the passed given buffer with RGBA pixels from the current nametables.
    pub fn load_nametables(&self, nametables: &mut [u8]) {
        for i in 0..4 {
            let base_addr = addr::NAMETABLE_START + i * size::NAMETABLE;
            let x_offset = (i % 2) * size::WIDTH;
            let y_offset = (i / 2) * size::HEIGHT;

            for addr in base_addr..(base_addr + size::NAMETABLE - 64) {
                let x_scroll = addr & Scroll::COARSE_X_MASK;
                let y_scroll = (addr & Scroll::COARSE_Y_MASK) >> 5;

                let base_nametable_addr =
                    addr::NAMETABLE_START | (addr & (Scroll::NT_X_MASK | Scroll::NT_Y_MASK));
                let base_attr_addr = base_nametable_addr + addr::ATTR_OFFSET;

                let tile_index = u16::from(self.chr_peek(addr));
                let tile_addr = self.ctrl.bg_select | (tile_index << 4);

                let supertile = ((y_scroll & 0xFC) << 1) + (x_scroll >> 2);
                let attr = u16::from(self.chr_peek(base_attr_addr + supertile));
                let attr_shift = (x_scroll & 0x02) | ((y_scroll & 0x02) << 1);
                let palette_addr = ((attr >> attr_shift) & 0x03) << 2;

                let tile_num = x_scroll + (y_scroll << 5);
                let tile_x = (tile_num % 32) << 3;
                let tile_y = (tile_num / 32) << 3;

                for y in 0..8 {
                    let tile_addr = tile_addr + y;
                    let tile_lo = self.chr_peek(tile_addr);
                    let tile_hi = self.chr_peek(tile_addr + 8);
                    for x in 0..8 {
                        let tile_palette = (((tile_hi >> x) & 1) << 1) | (tile_lo >> x) & 1;
                        let palette = palette_addr | u16::from(tile_palette);
                        let color = self
                            .palette
                            .peek(addr::PALETTE_START | ((palette & 0x03 > 0) as u16 * palette));
                        let x = tile_x + (7 - x);
                        let y = tile_y + y;
                        Self::set_pixel(
                            u16::from(color & self.mask.grayscale) | self.mask.emphasis,
                            x + x_offset,
                            y + y_offset,
                            2 * size::WIDTH,
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
            let start = i * 0x1000;
            let end = start + 0x1000;
            let x_offset = (i % 2) * size::WIDTH / 2;
            for tile_addr in (start..end).step_by(16) {
                let tile_x = ((tile_addr % 0x1000) % 256) / 2;
                let tile_y = ((tile_addr % 0x1000) / 256) * 8;
                for y in 0..8 {
                    let tile_lo = u16::from(self.chr_peek(tile_addr + y));
                    let tile_hi = u16::from(self.chr_peek(tile_addr + y + 8));
                    for x in 0..8 {
                        let palette = (((tile_hi >> x) & 0x01) << 1) | ((tile_lo >> x) & 0x01);
                        let color = u16::from(self.palette.peek(addr::PALETTE_START | palette));
                        let x = tile_x + (7 - x);
                        let y = tile_y + y;
                        Self::set_pixel(color, x + x_offset, y, size::WIDTH, pattern_tables);
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
                let sprite_x = u16::from(*x);
                let sprite_y = u16::from(*y);
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
                    ..Sprite::default()
                };

                let tile_x = (i % 8) as u16 * 8;
                let tile_y = (i / 8) as u16 * 8;
                for y in 0..8 {
                    let mut line_offset = if flip_vertical { (height) - 1 - y } else { y };
                    if height == 16 && line_offset >= 8 {
                        line_offset += 8;
                    }
                    let tile_lo = self.chr_peek(tile_addr + line_offset);
                    let tile_hi = self.chr_peek(tile_addr + line_offset + 8);
                    for x in 0..8 {
                        let spr_color = if flip_horizontal {
                            (((tile_hi >> x) & 0x01) << 1) | ((tile_lo >> x) & 0x01)
                        } else {
                            (((tile_hi << x) & 0x80) >> 6) | (((tile_lo << x) & 0x80) >> 7)
                        };
                        let palette = palette + spr_color;
                        let color = self.palette.peek(
                            addr::PALETTE_START
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
                        if show_spr && !left_clip_spr && x < size::WIDTH && y < size::HEIGHT {
                            let color = if bg_color == 0 || !bg_priority {
                                color
                            } else if (fine_x + (x & 0x07)) < 8 {
                                self.prev_palette + bg_color
                            } else {
                                self.curr_palette + bg_color
                            };
                            Self::set_pixel(u16::from(color), x, y, size::WIDTH, sprite_nametable);
                        }
                    }
                }
            }
        }
    }

    /// Load the given buffer with RGBA pixels from the current palettes.
    pub fn load_palettes(&self, palettes: &mut [u8], colors: &mut [u8]) {
        for addr in addr::PALETTE_START..addr::PALETTE_END {
            let offset = addr - addr::PALETTE_START;
            let x = offset % 16;
            let y = offset / 16;
            let color = self.palette.peek(addr);
            colors[usize::from(offset)] = color;
            Self::set_pixel(u16::from(color), x, y, 16, palettes);
        }
    }

    fn set_pixel(color: u16, x: u16, y: u16, width: u16, pixels: &mut [u8]) {
        let index = (color as usize) * 3;
        let idx = 4 * (usize::from(x) + usize::from(y) * usize::from(width));
        assert!(Ppu::NTSC_PALETTE.len() > index + 2);
        assert!(pixels.len() > 2);
        assert!(idx + 2 < pixels.len());
        pixels[idx] = Ppu::NTSC_PALETTE[index];
        pixels[idx + 1] = Ppu::NTSC_PALETTE[index + 1];
        pixels[idx + 2] = Ppu::NTSC_PALETTE[index + 2];
        pixels[idx + 3] = 0xFF;
    }

    #[inline(always)]
    const fn increment_vram_addr(&mut self) {
        // During rendering, v increments coarse X and coarse Y simultaneously
        if self.scanline > scanline::VISIBLE_END || !self.mask.rendering_enabled {
            self.scroll
                .increment(self.ctrl.vram_increment as u16 * 31 + 1);
        } else {
            self.scroll.increment_x();
            self.scroll.increment_y();
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
    }

    /// Fetch BG nametable byte.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    #[inline]
    fn fetch_bg_nt_byte(&mut self) {
        self.prev_palette = self.curr_palette;
        self.curr_palette = self.next_palette;

        self.tile_shift_lo |= u16::from(self.tile_lo);
        self.tile_shift_hi |= u16::from(self.tile_hi);

        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = addr::NAMETABLE_START | (self.scroll.addr() & nametable_addr_mask);
        let tile_index = u16::from(self.chr_read(addr));
        self.tile_addr = self.ctrl.bg_select | (tile_index << 4) | self.scroll.fine_y;
    }

    /// Fetch BG attribute byte.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    #[inline(always)]
    fn fetch_bg_attr_byte(&mut self) {
        let addr = self.scroll.attr_addr();
        let shift = self.scroll.attr_shift();
        self.next_palette = ((self.chr_read(addr) >> shift) & 0x03) << 2;
    }

    /// Fetch 4 tiles and write out shift registers every 8th cycle.
    /// Each tile fetch takes 2 cycles.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    #[inline]
    fn bg_fetch_cycle(&mut self) {
        let phase = self.cycle & 0x07;
        if self.mask.prev_rendering_enabled && phase == 0 {
            // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
            self.scroll.increment_x();
            // 256, Increment Fine Y when we reach the end of the screen
            if self.cycle == cycle::INC_Y {
                self.scroll.increment_y();
            }
            return;
        }

        match phase {
            1 => self.fetch_bg_nt_byte(),
            3 => self.fetch_bg_attr_byte(),
            5 => self.tile_lo = self.chr_read(self.tile_addr),
            7 => self.tile_hi = self.chr_read(self.tile_addr + 8),
            _ => (),
        }
    }

    fn oam_eval_cycle(&mut self) {
        if self.cycle & 0x01 == 0x01 {
            // Odd cycles are reads from OAM
            self.oam_fetch = self.oamdata[self.oamaddr as usize];
        } else {
            // Local variables improve cache locality
            let scanline = self.scanline;
            let mut oam_eval_done = self.oam_eval_done;
            let mut secondary_oamaddr = self.secondary_oamaddr;
            let mut oam_fetch = self.oam_fetch;
            let mut spr_in_range = self.spr_in_range;
            let mut spr_zero_in_range = self.spr_zero_in_range;

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
                let y = u16::from(oam_fetch);
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
        }
    }

    fn spr_eval_cycle(&mut self) {
        // Local variables improve cache locality
        match self.cycle {
            // 1. Clear Secondary OAM
            // 1..=64
            cycle::OAM_CLEAR_START..=cycle::OAM_CLEAR_END => {
                self.oam_fetch = 0xFF;
                self.secondary_oamdata = ConstArray::filled(0xFF);
            }
            // 2. Read OAM to find first eight sprites on this scanline
            // 3. With > 8 sprites, check (wrongly) for more sprites to set overflow flag
            // 64..=256
            cycle::SPR_EVAL_START => {
                self.spr_in_range = false;
                self.spr_zero_in_range = false;
                self.secondary_oamaddr = 0x00;
                self.oam_eval_done = false;
                self.oamaddr_hi = (self.oamaddr >> 2) & 0x3F;
                self.oamaddr_lo = self.oamaddr & 0x03;
                self.oam_eval_cycle();
            }
            cycle::SPR_EVAL_END => {
                self.spr_zero_visible = self.spr_zero_in_range;
                self.spr_count = self.secondary_oamaddr >> 2;
                self.oam_eval_cycle();
            }
            cycle::SPR_EVAL_START1..=cycle::SPR_EVAL_END0 => self.oam_eval_cycle(),
            _ => (),
        }
    }

    fn load_sprites(&mut self) {
        // Local variables improve cache locality
        let cycle = self.cycle;
        let scanline = self.scanline;
        let spr_count = usize::from(self.spr_count);

        let idx = (cycle - cycle::SPR_FETCH_START) as usize / 8;
        let oam_idx = idx << 2;

        if let [y, tile_index, attr, x] = self.secondary_oamdata[oam_idx..=oam_idx + 3] {
            let x = u16::from(x);
            let y = u16::from(y);
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
                sprite_select | ((tile_index & 0xFE) << 4) | line_offset
            } else {
                self.ctrl.spr_select | (tile_index << 4) | line_offset
            };

            if idx < spr_count {
                self.sprites[idx] = Sprite {
                    x,
                    y,
                    tile_addr,
                    tile_lo: self.chr_read(tile_addr),
                    tile_hi: self.chr_read(tile_addr + 8),
                    palette: ((attr & 0x03) << 2) | 0x10,
                    bg_priority: (attr & 0x20) == 0x20,
                    flip_horizontal: (attr & 0x40) == 0x40,
                };
                let cycle = usize::from(x + 1);
                self.spr_present[cycle..(cycle + 8).min(256)].fill(true);
            } else {
                // Fetches for remaining sprites/hidden fetch tile $FF
                // Required for accurate MMC3 IRQ
                let _ = self.chr_read(tile_addr);
                let _ = self.chr_read(tile_addr + 8);
            }
        }
    }

    // https://wiki.nesdev.org/w/index.php/PPU_OAM
    #[inline]
    fn spr_fetch_cycle(&mut self) {
        // OAMADDR set to $00 on prerender and visible scanlines
        self.write_oamaddr(0x00);

        match self.cycle & 0x07 {
            // Garbage NT sprite fetch (257, 265, 273, etc.)
            // Required for proper MC-ACC IRQs (MMC3 clone)
            1 => self.fetch_bg_nt_byte(),   // Garbage NT fetch
            3 => self.fetch_bg_attr_byte(), // Garbage attr fetch
            // Cycle 260, 268, etc. This is an approximation (each tile is actually loaded in 8
            // steps (e.g from 257 to 264))
            4 => self.load_sprites(),
            _ => (),
        }
    }

    #[inline]
    fn pixel_palette(&mut self) -> u8 {
        let cycle = self.cycle;
        let show_left_bg = self.mask.show_left_bg;
        let show_left_spr = self.mask.show_left_spr;
        let show_bg = self.mask.show_bg;
        let show_spr = self.mask.show_spr;
        let fine_x = self.scroll.fine_x;
        let bg_shift = 15 - fine_x;

        let min_render_x = cycle >= 9;
        let bg_mask = u8::from(show_bg & (show_left_bg | min_render_x));
        let bg_color = bg_mask
            * ((((self.tile_shift_hi >> bg_shift) & 0x01) << 1)
                | ((self.tile_shift_lo >> bg_shift) & 0x01)) as u8;

        let count = usize::from(self.spr_count);
        if (count > 0)
            & (show_spr & (show_left_spr | min_render_x))
            & self.spr_present[usize::from(cycle)]
        {
            for (i, sprite) in self.sprites.iter().take(count).enumerate() {
                let spr_shift = cycle.wrapping_sub(sprite.x).wrapping_sub(1);
                if spr_shift <= 7 {
                    let spr_shift = if sprite.flip_horizontal {
                        spr_shift
                    } else {
                        7 - spr_shift
                    };
                    let spr_color = (((sprite.tile_hi >> spr_shift) & 0x01) << 1)
                        | ((sprite.tile_lo >> spr_shift) & 0x01);

                    if spr_color != 0 {
                        if self.mask.rendering_enabled
                            & !self.status.spr_zero_hit
                            & self.spr_zero_visible
                            & (cycle != 256)
                            & (i == 0)
                            & (bg_color != 0)
                        {
                            self.status.set_spr_zero_hit(true);
                        }

                        if !sprite.bg_priority | (bg_color == 0) {
                            return sprite.palette + spr_color;
                        }
                        break;
                    }
                }
            }
        }

        let palette_mask = u8::from((fine_x + (cycle & 0x07)) < 9);
        let palette = palette_mask * self.prev_palette + (1 - palette_mask) * self.curr_palette;
        palette + bg_color
    }

    #[inline]
    fn headless_sprite_zero_hit(&mut self) {
        if !self.spr_zero_visible || self.status.spr_zero_hit {
            return;
        }

        let cycle = self.cycle;
        let show_left_bg = self.mask.show_left_bg;
        let show_left_spr = self.mask.show_left_spr;
        let show_bg = self.mask.show_bg;
        let show_spr = self.mask.show_spr;
        let min_render_x = cycle >= 9;

        let bg_mask = u8::from(show_bg & (show_left_bg | min_render_x));
        if (bg_mask == 0)
            | !(show_spr & (show_left_spr | min_render_x))
            | (cycle == 256)
            | !self.spr_present[usize::from(cycle)]
        {
            return;
        }

        let bg_shift = 15 - self.scroll.fine_x;
        let bg_color = bg_mask
            * ((((self.tile_shift_hi >> bg_shift) & 0x01) << 1)
                | ((self.tile_shift_lo >> bg_shift) & 0x01)) as u8;
        if bg_color == 0 {
            return;
        }

        let sprite = &self.sprites[0];
        let spr_shift = cycle.wrapping_sub(sprite.x).wrapping_sub(1);
        if spr_shift <= 7 {
            let spr_shift = if sprite.flip_horizontal {
                spr_shift
            } else {
                7 - spr_shift
            };
            let spr_color = (((sprite.tile_hi >> spr_shift) & 0x01) << 1)
                | ((sprite.tile_lo >> spr_shift) & 0x01);
            if spr_color != 0 {
                self.status.set_spr_zero_hit(true);
            }
        }
    }

    #[inline(always)]
    fn render_pixel(&mut self) {
        let addr = self.scroll.addr();
        let color = if self.mask.rendering_enabled || !addr.is_palette() {
            let palette = u16::from(self.pixel_palette());
            self.palette
                .read(addr::PALETTE_START | ((palette & 0x03 > 0) as u16 * palette))
        } else {
            self.palette.read(addr)
        };

        self.frame.set_pixel(
            self.cycle - 1,
            self.scanline,
            u16::from(color & self.mask.grayscale) | self.mask.emphasis,
        );
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
        if self.reset_signal {
            return;
        }
        self.ctrl.write(val);
        self.scroll.write_nametable_select(val);
        // MMC5 tracks changes to PPUCTRL
        self.mapper.ppu_write(0x2000, val);

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
    #[inline(always)]
    fn write_mask(&mut self, val: u8) {
        self.open_bus = val;
        if self.reset_signal {
            return;
        }
        self.mask.write(val);
        // MMC5 tracks changes to PPUMASK
        self.mapper.ppu_write(0x2001, val);
    }

    // $2002 | R   | PPUSTATUS
    //       | 0-5 | Unknown (???)
    //       |   6 | Sprite0 Hit Flag, 1 = PPU rendering has hit sprite #0
    //       |     | This flag resets to 0 when VBlank starts, or CPU reads $2002
    //       |   7 | VBlank Flag, 1 = PPU is generating a Vertical Blanking Impulse
    //       |     | This flag resets to 0 when VBlank ends, or CPU reads $2002
    fn read_status(&mut self) -> u8 {
        let status = self.peek_status();
        // Top three bits ignored for open bus
        self.open_bus |= status & 0xE0;

        if Cpu::nmi_pending() {
            trace!("$2002 NMI Ack - PPU:{:3},{:3}", self.cycle, self.scanline,);
        }
        Cpu::clear_nmi();

        self.status.reset_in_vblank();
        self.scroll.reset_latch();

        if self.scanline == self.vblank_scanline && self.cycle == cycle::START {
            // Reading PPUSTATUS one clock before the start of vertical blank will read as clear
            // and never set the flag or generate an NMI for that frame
            trace!(
                "$2002 Prevent VBL - PPU:{:3},{:3}",
                self.cycle, self.scanline
            );
            self.prevent_vbl = true;
        }

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
    #[inline(always)]
    fn peek_status(&self) -> u8 {
        // Only upper 3 bits are connected for this register
        (self.status.read() & 0xE0) | (self.open_bus & 0x1F)
    }

    // $2003 | W   | OAMADDR
    //       |     | Used to set the address in the 256-byte Sprite Memory to be
    //       |     | accessed via $2004. This address will increment by 1 after
    //       |     | each access to $2004. The Sprite Memory contains coordinates,
    //       |     | colors, and other attributes of the sprites.
    #[inline(always)]
    fn write_oamaddr(&mut self, val: u8) {
        self.open_bus = val;
        self.oamaddr = val;
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    #[inline(always)]
    fn read_oamdata(&mut self) -> u8 {
        self.open_bus = self.peek_oamdata();
        self.open_bus
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    // Non-mutating version of `read_oamdata`.
    fn peek_oamdata(&self) -> u8 {
        // Reading OAMDATA during rendering will expose OAM accesses during sprite evaluation and loading
        if self.scanline <= scanline::VISIBLE_END
            && self.mask.rendering_enabled
            && cycle::SPR_FETCH_RANGE.contains(&self.cycle)
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
            && (self.is_visible_scanline
                || self.is_prerender_scanline
                || self.is_pal_spr_eval_scanline)
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
    #[inline(always)]
    fn write_scroll(&mut self, val: u8) {
        self.open_bus = val;

        if self.reset_signal {
            return;
        }
        self.scroll.write(val);
    }

    // $2006 | W   | PPUADDR
    #[inline(always)]
    fn write_addr(&mut self, val: u8) {
        self.open_bus = val;

        if self.reset_signal {
            return;
        }
        self.scroll.write_addr(val);
    }

    // $2007 | RW  | PPUDATA
    fn read_data(&mut self) -> u8 {
        let addr = self.scroll.addr();
        self.increment_vram_addr();

        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in $0000 - $3EFF
        let prev_open_bus = self.open_bus;
        let val = self.bus_read(addr);
        // MMC3 clocks using A12
        self.mapper.ppu_read(self.scroll.addr());
        self.open_bus = if addr < addr::PALETTE_START {
            let buffer = self.vram_buffer;
            self.vram_buffer = val;
            buffer
        } else {
            // Set internal buffer with mirrors of nametable when reading palettes
            // Since we're reading from > $3EFF subtract $1000 to fill
            // buffer with nametable mirror data
            self.vram_buffer = self.bus_read(addr - 0x1000);
            // Hi 2 bits of palette should be open bus
            val | (prev_open_bus & 0xC0)
        };

        trace!(
            "PPU $2007 read: {:02X} - PPU:{:3},{:3}",
            self.open_bus, self.cycle, self.scanline
        );

        self.open_bus
    }

    // $2007 | RW  | PPUDATA
    //
    // Non-mutating version of `read_data`.
    fn peek_data(&self) -> u8 {
        let addr = self.scroll.addr();
        if addr < addr::PALETTE_START {
            self.vram_buffer
        } else {
            // Since we're reading from > $3EFF subtract $1000
            // Hi 2 bits of palette should be open bus
            self.bus_peek(addr - 0x1000) | (self.open_bus & 0xC0)
        }
    }

    // $2007 | RW  | PPUDATA
    fn write_data(&mut self, val: u8) {
        let addr = self.scroll.addr();
        trace!(
            "PPU $2007 write: ${addr:04X} -> {val:02X} - PPU:{:3},{:3}",
            self.cycle, self.scanline
        );
        self.increment_vram_addr();
        self.bus_write(addr, val);
        // MMC3 clocks using A12
        self.mapper.ppu_read(self.scroll.addr());
    }
}

impl Clock for Ppu {
    fn clock(&mut self) {
        // === SCANLINE TRANSITION (cycle 340) ===
        if self.cycle >= cycle::END {
            self.cycle = 0;
            self.scanline += 1;
            // === POST-RENDER (240/261) ===
            match self.scanline {
                s if s == self.vblank_scanline - 1 => {
                    self.frame.increment();
                }
                s if s > self.prerender_scanline => {
                    // Wrap scanline back to 0
                    self.scanline = 0;
                    // Force prerender scanline sprite fetches to load the dummy $FF tiles (fixes
                    // shaking in Ninja Gaiden 3 stage 1 after beating boss)
                    self.spr_count = 0;
                }
                _ => (),
            }

            self.is_visible_scanline = self.scanline <= scanline::VISIBLE_END;
            self.is_prerender_scanline = self.scanline == self.prerender_scanline;
            self.is_render_scanline = self.is_visible_scanline | self.is_prerender_scanline;
            // PAL refreshes OAM later due to extended vblank to avoid OAM decay
            self.is_pal_spr_eval_scanline =
                self.region.is_pal() && self.scanline >= self.vblank_scanline + 24;

            if self.scanline == self.debugger.scanline && self.cycle == self.debugger.cycle {
                (*self.debugger.callback)(self.snapshot());
            }

            return;
        }

        self.cycle += 1;

        // === RENDER LINE (scanlins 0-239, 261) ===
        if self.mask.rendering_enabled {
            if self.is_render_scanline {
                if self.cycle <= cycle::VISIBLE_END {
                    if self.is_visible_scanline {
                        self.spr_eval_cycle();
                    }

                    self.bg_fetch_cycle();

                    if self.is_prerender_scanline && self.cycle <= 8 && self.oamaddr >= 0x08 {
                        // If OAMADDR is not less than eight when rendering starts, the eight bytes
                        // starting at OAMADDR & 0xF8 are copied to the first eight bytes of OAM
                        let addr = (self.cycle as usize) - 1;
                        let oamindex = (self.oamaddr as usize & 0xF8) + addr;
                        self.oamdata[addr] = self.oamdata[oamindex];
                    }
                } else if self.cycle <= cycle::SPR_FETCH_END {
                    if self.mask.prev_rendering_enabled && self.cycle == cycle::SPR_FETCH_START {
                        // Copy X bits at the start of a new line since we're going to start writing
                        // new x values to t
                        self.scroll.copy_x();
                        self.spr_present = ConstArray::new();
                    }
                    // 280..=304
                    if self.is_prerender_scanline && cycle::COPY_Y_RANGE.contains(&self.cycle) {
                        // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
                        // if rendering is enabled
                        // https://wiki.nesdev.org/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
                        self.scroll.copy_y();
                    }
                    self.spr_fetch_cycle();
                } else {
                    // 336
                    if self.cycle <= cycle::BG_PREFETCH_END {
                        self.bg_fetch_cycle();
                    } else {
                        // 337..=340
                        self.fetch_bg_nt_byte();
                    }

                    self.oam_fetch = self.secondary_oamdata[0];

                    if self.region.is_ntsc()
                        && self.is_prerender_scanline
                        && self.cycle == cycle::ODD_SKIP
                        && self.frame.is_odd()
                    {
                        // NTSC behavior while rendering - each odd PPU frame is one clock shorter
                        // (skipping from 339 over 340 to 0)
                        trace!(
                            "Skipped odd frame cycle: {} - PPU:{:3},{:3}",
                            self.frame_number(),
                            self.cycle,
                            self.scanline
                        );
                        self.cycle = cycle::END;
                    }
                }
            } else if self.is_pal_spr_eval_scanline {
                self.spr_eval_cycle();
                // 257..=320
                if cycle::SPR_FETCH_RANGE.contains(&self.cycle) {
                    self.write_oamaddr(0x00);
                }
            }
        }

        self.mask.clock();
        if self.scroll.delayed_update()
            && (!self.mask.rendering_enabled || self.scanline > scanline::VISIBLE_END)
        {
            // MMC3 clocks using A12
            self.mapper.ppu_read(self.scroll.addr());
        }

        // Pixels should be put even if rendering is disabled, as this is what blanks out the
        // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
        if self.is_visible_scanline && self.cycle <= cycle::VISIBLE_END {
            if self.skip_rendering {
                self.headless_sprite_zero_hit();
            } else {
                self.render_pixel();
            }
        }

        if self.cycle <= cycle::VISIBLE_END || cycle::BG_PREFETCH_RANGE.contains(&self.cycle) {
            self.tile_shift_lo <<= 1;
            self.tile_shift_hi <<= 1;
        }

        // === VBLANK / IDLE ===
        if self.scanline == self.vblank_scanline && self.cycle == cycle::VBLANK {
            self.start_vblank();
        } else if self.is_prerender_scanline && self.cycle == cycle::VBLANK {
            self.stop_vblank();
        }

        if self.scanline == self.debugger.scanline && self.cycle == self.debugger.cycle {
            (*self.debugger.callback)(self.snapshot());
        }
    }
}

impl ClockTo for Ppu {
    #[inline(always)]
    fn clock_to(&mut self, clock: u32) {
        let divider = u32::from(self.clock_divider);
        while self.master_clock + divider <= clock {
            self.clock();
            self.master_clock += divider;
        }
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
                cycle::DIVIDER_NTSC,
                scanline::VBLANK_NTSC,
                scanline::PRERENDER_NTSC,
            ),
            NesRegion::Pal => (
                cycle::DIVIDER_PAL,
                scanline::VBLANK_PAL,
                scanline::PRERENDER_PAL,
            ),
            NesRegion::Dendy => (
                cycle::DIVIDER_DENDY,
                scanline::VBLANK_DENDY,
                scanline::PRERENDER_DENDY,
            ),
        };
        self.region = region;
        self.clock_divider = clock_divider;
        self.vblank_scanline = vblank_scanline;
        self.prerender_scanline = prerender_scanline;
        self.mask.set_region(region);
    }
}

impl Reset for Ppu {
    fn reset(&mut self, kind: ResetKind) {
        self.master_clock = 0;
        self.cycle = 0;
        self.scanline = 0;
        self.is_visible_scanline = true;
        self.is_prerender_scanline = false;
        self.is_render_scanline = true;
        self.is_pal_spr_eval_scanline = false;
        self.open_bus = 0x00;

        self.mask.reset(kind);
        self.scroll.reset(kind);
        self.ctrl.reset(kind);

        self.mapper.reset(kind);

        self.status.reset(kind);

        self.oam_fetch = 0x00;
        self.oam_eval_done = false;
        self.secondary_oamaddr = 0x0000;
        self.overflow_count = 0;
        self.spr_in_range = false;
        self.spr_zero_in_range = false;
        self.spr_zero_visible = false;
        self.spr_count = 0;
        self.vram_buffer = 0x00;

        if kind == ResetKind::Hard {
            self.oamaddr = 0x0000;
            self.oamdata = ConstArray::new();
        } else {
            self.reset_signal = self.emulate_warmup;
        }
        *self.sprites = [Sprite::new(); 8];
        self.spr_present = ConstArray::new();
        self.prevent_vbl = false;
        self.frame.reset(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::Cart,
        mapper::{Mmc1Revision, Sxrom},
        mem::Memory,
    };

    #[test]
    fn ciram_mirror_horizontal() {
        assert_eq!(CIRam::mirror(0x2000, Mirroring::Horizontal), 0x0000);
        assert_eq!(CIRam::mirror(0x2005, Mirroring::Horizontal), 0x0005);
        assert_eq!(CIRam::mirror(0x23FF, Mirroring::Horizontal), 0x03FF);
        assert_eq!(CIRam::mirror(0x2400, Mirroring::Horizontal), 0x0000);
        assert_eq!(CIRam::mirror(0x2405, Mirroring::Horizontal), 0x0005);
        assert_eq!(CIRam::mirror(0x27FF, Mirroring::Horizontal), 0x03FF);
        assert_eq!(CIRam::mirror(0x2800, Mirroring::Horizontal), 0x0400);
        assert_eq!(CIRam::mirror(0x2805, Mirroring::Horizontal), 0x0405);
        assert_eq!(CIRam::mirror(0x2BFF, Mirroring::Horizontal), 0x07FF);
        assert_eq!(CIRam::mirror(0x2C00, Mirroring::Horizontal), 0x0400);
        assert_eq!(CIRam::mirror(0x2C05, Mirroring::Horizontal), 0x0405);
        assert_eq!(CIRam::mirror(0x2FFF, Mirroring::Horizontal), 0x07FF);
    }

    #[test]
    fn ciram_mirror_vertical() {
        assert_eq!(CIRam::mirror(0x2000, Mirroring::Vertical), 0x0000);
        assert_eq!(CIRam::mirror(0x2005, Mirroring::Vertical), 0x0005);
        assert_eq!(CIRam::mirror(0x23FF, Mirroring::Vertical), 0x03FF);
        assert_eq!(CIRam::mirror(0x2800, Mirroring::Vertical), 0x0000);
        assert_eq!(CIRam::mirror(0x2805, Mirroring::Vertical), 0x0005);
        assert_eq!(CIRam::mirror(0x2BFF, Mirroring::Vertical), 0x03FF);
        assert_eq!(CIRam::mirror(0x2400, Mirroring::Vertical), 0x0400);
        assert_eq!(CIRam::mirror(0x2405, Mirroring::Vertical), 0x0405);
        assert_eq!(CIRam::mirror(0x27FF, Mirroring::Vertical), 0x07FF);
        assert_eq!(CIRam::mirror(0x2C00, Mirroring::Vertical), 0x0400);
        assert_eq!(CIRam::mirror(0x2C05, Mirroring::Vertical), 0x0405);
        assert_eq!(CIRam::mirror(0x2FFF, Mirroring::Vertical), 0x07FF);
    }

    #[test]
    fn ciram_mirror_single_screen_a() {
        assert_eq!(CIRam::mirror(0x2000, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(CIRam::mirror(0x2005, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(CIRam::mirror(0x23FF, Mirroring::SingleScreenA), 0x03FF);
        assert_eq!(CIRam::mirror(0x2800, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(CIRam::mirror(0x2805, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(CIRam::mirror(0x2BFF, Mirroring::SingleScreenA), 0x03FF);
        assert_eq!(CIRam::mirror(0x2400, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(CIRam::mirror(0x2405, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(CIRam::mirror(0x27FF, Mirroring::SingleScreenA), 0x03FF);
        assert_eq!(CIRam::mirror(0x2C00, Mirroring::SingleScreenA), 0x0000);
        assert_eq!(CIRam::mirror(0x2C05, Mirroring::SingleScreenA), 0x0005);
        assert_eq!(CIRam::mirror(0x2FFF, Mirroring::SingleScreenA), 0x03FF);
    }

    #[test]
    fn ciram_mirror_single_screen_b() {
        assert_eq!(CIRam::mirror(0x2000, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(CIRam::mirror(0x2005, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(CIRam::mirror(0x23FF, Mirroring::SingleScreenB), 0x07FF);
        assert_eq!(CIRam::mirror(0x2800, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(CIRam::mirror(0x2805, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(CIRam::mirror(0x2BFF, Mirroring::SingleScreenB), 0x07FF);
        assert_eq!(CIRam::mirror(0x2400, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(CIRam::mirror(0x2405, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(CIRam::mirror(0x27FF, Mirroring::SingleScreenB), 0x07FF);
        assert_eq!(CIRam::mirror(0x2C00, Mirroring::SingleScreenB), 0x0400);
        assert_eq!(CIRam::mirror(0x2C05, Mirroring::SingleScreenB), 0x0405);
        assert_eq!(CIRam::mirror(0x2FFF, Mirroring::SingleScreenB), 0x07FF);
    }

    #[test]
    fn vram_writes() {
        let mut ppu = Ppu::default();
        ppu.write_addr(0x23);
        ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        ppu.clock();
        ppu.clock();
        ppu.write_data(0x66); // write to $2305

        assert_eq!(ppu.chr_read(0x2305), 0x66);
    }

    #[test]
    fn vram_reads() {
        let mut ppu = Ppu::default();
        ppu.write_ctrl(0x00);
        ppu.bus_write(0x2305, 0x66);

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
        ppu.bus_write(0x21FF, 0x66);
        ppu.bus_write(0x2200, 0x77);

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
        ppu.bus_write(0x21FF, 0x66);
        ppu.bus_write(0x21FF + 32, 0x77);
        ppu.bus_write(0x21FF + 64, 0x88);

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
        cart.mapper = Sxrom::load(
            &cart,
            Memory::new(0x2000),
            Memory::new(0x4000),
            Mmc1Revision::BC,
        )
        .unwrap();
        // Set vertical mirroring mode via 5 writes
        let mut val = 0b00_00_00_01_00;
        for _ in 0..5 {
            cart.mapper.prg_write(0x8000, val & 0b11);
            cart.mapper.clock();
            cart.mapper.clock();
            val >>= 2;
        }
        ppu.load_mapper(cart.mapper);

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
        ppu.bus_write(0x2305, 0x66);

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
        ppu.bus_write(0x2305, 0x66);

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
        ppu.spr_present[9..17].fill(true);

        ppu.sprites[0].x = 8;
        ppu.sprites[0].tile_lo = 0b0100;
        ppu.sprites[0].tile_hi = 0b0000;
        ppu.sprites[0].flip_horizontal = true;
        ppu.sprites[0].bg_priority = false;

        ppu.clock();

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
