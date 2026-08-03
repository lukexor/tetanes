//! NES PPU (Picture Processing Unit) implementation.
//!
//! [`Ppu`] is the state a 2C02 keeps - registers, OAM, palette RAM and the frame it is drawing.
//! Anything that reaches the cartridge is an `impl Bus` block further down this file, since a CHR
//! or nametable fetch resolves through the board: [`Bus::ppu_clock`](crate::bus::Bus::ppu_clock)
//! drives a dot and [`Bus::chr_peek`](crate::bus::Bus::chr_peek) resolves an address, while the
//! register reads and the pixel pipeline are here.
//!
//! # Stability
//!
//! [`Ppu`]'s fields are the emulation's internal wiring. They are public so that embedders and
//! debuggers can read them - the PPU viewer in the `tetanes` UI does exactly that - but they track
//! the implementation rather than the crate version, and a release may add, rename or retype any
//! of them. The stable entry point is
//! [`ControlDeck`](crate::control_deck::ControlDeck). Fields documented as *derived* are caches of
//! state that lives elsewhere; writing one from outside desynchronizes the emulator rather than
//! changing what it does.

use crate::{
    bus::Bus,
    common::{NesRegion, ResetKind},
    debug::Debugger,
    mapper::{Mapper, MapperOps},
    memory::ConstArray,
    ppu::frame::Frame,
};
use ctrl::Ctrl;
use mask::Mask;
use scroll::Scroll;
use serde::{Deserialize, Serialize};
use sprite::Sprite;
use status::Status;
use std::cmp::Ordering;
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
    /// Nametables mirrored left-to-right, giving two vertically-scrollable screens.
    Vertical = 0,
    /// Nametables mirrored top-to-bottom, giving two horizontally-scrollable screens.
    #[default]
    Horizontal = 1,
    /// All four nametables show CIRAM's first 1K bank.
    SingleScreenA = 2,
    /// All four nametables show CIRAM's second 1K bank.
    SingleScreenB = 3,
    /// Four distinct nametables, which needs 2K of RAM on the cartridge beyond the console's CIRAM.
    FourScreen = 4,
}

/// Palette RAM which enforces mirroring.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(transparent)]
pub struct PaletteRam(ConstArray<u8, 32>);

impl PaletteRam {
    /// Return palette address, mirrored.
    //
    // Mirroring on read rather than storing both halves of each backdrop mirror at write time:
    // this pays a second, dependent load per pixel and still measured 1.8% faster than a plain
    // indexed load off a write-mirrored array. The pair is 5.4% of frame time and neither load is
    // bounds checked, so there was little to win and code layout swamped it.
    #[inline(always)]
    const fn mirror(addr: u16) -> usize {
        const PALETTE_MIRROR: [u8; 32] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 17, 18, 19, 4, 21, 22, 23, 8,
            25, 26, 27, 12, 29, 30, 31,
        ];
        PALETTE_MIRROR[(addr & 0x1F) as usize] as usize
    }
}

impl PaletteRam {
    /// Read a colour, with the $3F10/$3F14/$3F18/$3F1C backdrop mirrors applied.
    #[inline(always)]
    fn peek(&self, addr: u16) -> u8 {
        self.0[Self::mirror(addr)]
    }

    /// Write a colour, with the backdrop mirrors applied.
    #[inline(always)]
    fn write(&mut self, addr: u16, val: u8) {
        self.0[Self::mirror(addr)] = val;
    }
}

/// Whether a PPU address fetches a nametable attribute byte.
///
/// A free function rather than an extension trait on `u16`: with one implementor and no generic
/// use, the trait only bought an import at each of the four call sites.
#[inline(always)]
#[must_use]
pub const fn is_attr(addr: u16) -> bool {
    (addr & (size::NAMETABLE - 1)) >= addr::ATTR_OFFSET
}

/// Whether a PPU address lands in palette RAM.
#[inline(always)]
#[must_use]
pub const fn is_palette(addr: u16) -> bool {
    addr >= addr::PALETTE_START
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

    /// Scanline is visible. *Derived* from `scanline`; recomputed once per scanline so the
    /// per-dot paths test a bool instead of a range. Writing it does not move the PPU.
    pub is_visible_scanline: bool,
    /// Scanline is a pre-render scanline. *Derived* from `scanline`; see
    /// [`Ppu::is_visible_scanline`].
    pub is_prerender_scanline: bool,
    /// Scanline is a render scanline. *Derived* from `scanline`; see
    /// [`Ppu::is_visible_scanline`].
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
    /// How much of [`Ppu::frame`] has had greyscale and colour emphasis applied.
    //
    // Rendering stores raw palette colours and the $2001 bits are folded in over whole runs of
    // pixels, so a frame that never touches them - which is nearly every frame - pays nothing per
    // pixel. Derived from a frame buffer that is itself not serialized.
    //
    // Cold, but it lives here rather than with the other cold fields: moving it down shifts every
    // field below by eight bytes, and that measured 3.2% slower even though it is what aligns
    // `palette` inside a single cache line. See `benches/README.md`.
    #[serde(skip)]
    pub color_bits_applied: usize,
    // === 104 ===
    /// Palette RAM: the 32 colours currently loaded, at $3F00-$3F1F.
    pub palette: PaletteRam,
    // === 136 ===
    /// Secondary OAM data on a given scanline.
    pub secondary_oamdata: ConstArray<u8, 32>,

    // === 160 ===
    /// Each scanline can hold 8 sprites at a time before the `spr_overflow` flag is set.
    pub sprites: [Sprite; 8],
    /// Which of the scanline's sprites cover each dot, one bit per index into [`Ppu::sprites`].
    ///
    /// A set bit means that sprite's 8-pixel span contains the dot, so the pixel path visits only
    /// the sprites that can contribute instead of scanning all [`Ppu::spr_count`] of them and
    /// range-testing each.
    // Rebuilt every scanline, so there is nothing here worth saving.
    #[serde(skip)]
    pub spr_cover: ConstArray<u8, 256>,
    // === 520 ===
    /// $2004 Object Attribute Memory (OAM) data (read/write).
    pub oamdata: ConstArray<u8, 256>,

    // === 776 ===
    /// NMI pending.
    pub nmi_pending: bool,

    /// $2007 PPUDATA buffer.
    pub vram_buffer: u8,
    /// Prevents VBL from being triggered this frame.
    pub prevent_vbl: bool,
    /// Current NesRegion.
    pub region: NesRegion,
    /// Whether to emulate PPU warmup on power up.
    pub emulate_warmup: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(NesRegion::default())
    }
}

pub mod addr {
    //! Address constants.

    /// First nametable, at the start of the PPU's VRAM window.
    pub const NAMETABLE_START: u16 = 0x2000;
    /// Offset of a nametable's attribute table from its own start.
    pub const ATTR_OFFSET: u16 = 0x03C0;

    /// First palette entry, i.e. the universal background colour.
    pub const PALETTE_START: u16 = 0x3F00;
    /// One past the last palette entry.
    pub const PALETTE_END: u16 = 0x3F20;
}

pub mod size {
    //! Memory size constants.

    /// Visible pixels per scanline.
    pub const WIDTH: u16 = 256;
    /// Visible scanlines per frame.
    pub const HEIGHT: u16 = 240;
    /// Pixels in one frame.
    pub const FRAME: usize = (WIDTH * HEIGHT) as usize;

    /// Bytes in one nametable, attribute table included.
    pub const NAMETABLE: u16 = 0x0400;
    /// Bytes of primary OAM: 64 four-byte sprites, the most a frame can hold.
    pub const OAM: usize = 256;
    /// Bytes of secondary OAM: 8 four-byte sprites, the most a scanline can hold.
    pub const SECONDARY_OAM: usize = 32;

    /// Bytes of CIRAM, i.e. the two 1K nametables on the console itself.
    pub const VRAM: usize = 0x0800;
    /// Bytes of palette RAM: the 32 colours loadable at a time, out of the 64 the PPU can make.
    pub const PALETTE: usize = 32;
}

pub mod cycle {
    //! Cycle constants.
    //! <https://www.nesdev.org/wiki/PPU_rendering>

    use std::ops::RangeInclusive;

    /// First cycle of a scanline, an idle dot.
    pub const START: u16 = 0;
    /// The cycle odd frames skip when rendering is enabled.
    pub const ODD_SKIP: u16 = 339;
    /// Last cycle of a scanline.
    pub const END: u16 = 340;

    /// Tile data fetching starts.
    pub const VISIBLE_START: u16 = 1;
    /// Tile data fetching ends: 2 cycles each for 4 fetches = 32 tiles.
    pub const VISIBLE_END: u16 = 256;

    /// Cycle on which the VBlank flag is set and cleared.
    pub const VBLANK: u16 = VISIBLE_START;

    /// Secondary OAM clear starts.
    pub const OAM_CLEAR_START: u16 = 1;
    /// Secondary OAM clear ends.
    pub const OAM_CLEAR_END: u16 = 64;

    /// Sprite evaluation for the next scanline starts.
    pub const SPR_EVAL_START: u16 = 65;
    /// One past [`SPR_EVAL_START`], to split up match arms.
    pub const SPR_EVAL_START1: u16 = 66;
    /// One before [`SPR_EVAL_END`], to split up match arms.
    pub const SPR_EVAL_END0: u16 = 255;
    /// Sprite evaluation ends.
    pub const SPR_EVAL_END: u16 = 256;
    /// Fetching the next scanline's sprites starts.
    pub const SPR_FETCH_START: u16 = 257;
    /// Sprite fetching ends: 2 cycles each for 4 fetches = 8 sprites.
    pub const SPR_FETCH_END: u16 = 320;
    /// [`SPR_FETCH_START`]..=[`SPR_FETCH_END`].
    pub const SPR_FETCH_RANGE: RangeInclusive<u16> = SPR_FETCH_START..=SPR_FETCH_END;

    /// Prefetching the next scanline's tile data starts.
    pub const BG_PREFETCH_START: u16 = 321;
    /// Background prefetch ends: 2 cycles each for 4 fetches = 2 tiles.
    pub const BG_PREFETCH_END: u16 = 336;
    /// [`BG_PREFETCH_START`]..=[`BG_PREFETCH_END`].
    pub const BG_PREFETCH_RANGE: RangeInclusive<u16> = BG_PREFETCH_START..=BG_PREFETCH_END;

    /// Two dummy nametable fetches start; what the hardware does with them is unknown.
    pub const BG_DUMMY_START: u16 = 337;
    /// Dummy fetches end, at the end of the scanline.
    pub const BG_DUMMY_END: u16 = END;

    /// Increment the Y scroll, the screen's last visible pixel having been reached.
    pub const INC_Y: u16 = 256;
    /// Copying the Y scroll from `t` to `v` starts (pre-render scanline only).
    pub const COPY_Y_START: u16 = 280;
    /// Copying the Y scroll ends.
    pub const COPY_Y_END: u16 = 304;
    /// [`COPY_Y_START`]..=[`COPY_Y_END`].
    pub const COPY_Y_RANGE: RangeInclusive<u16> = COPY_Y_START..=COPY_Y_END;

    /// Master clock cycles per PPU cycle on NTSC.
    pub const DIVIDER_NTSC: u8 = 4;
    /// Master clock cycles per PPU cycle on PAL.
    pub const DIVIDER_PAL: u8 = 5;
    /// Master clock cycles per PPU cycle on Dendy.
    pub const DIVIDER_DENDY: u8 = DIVIDER_PAL;
}

pub mod scanline {
    //! Scanline constants.
    //! <https://www.nesdev.org/wiki/PPU_rendering>

    /// First scanline of a frame.
    pub const START: u16 = 0;

    /// First visible scanline.
    pub const VISIBLE_START: u16 = START;
    /// Last visible scanline.
    pub const VISIBLE_END: u16 = 239;

    /// The idle scanline between the last visible one and VBlank.
    pub const POSTRENDER: u16 = 240;
    /// NTSC pre-render scanline, where the next frame's state is set up.
    pub const PRERENDER_NTSC: u16 = 261;
    /// PAL pre-render scanline.
    pub const PRERENDER_PAL: u16 = 311;
    /// Dendy pre-render scanline.
    pub const PRERENDER_DENDY: u16 = PRERENDER_PAL;

    /// NTSC scanline on which VBlank starts.
    pub const VBLANK_NTSC: u16 = 241;
    /// PAL scanline on which VBlank starts.
    pub const VBLANK_PAL: u16 = VBLANK_NTSC;
    /// Dendy scanline on which VBlank starts; its longer post-render gap is what makes Dendy's
    /// VBlank later than PAL's despite the same frame height.
    pub const VBLANK_DENDY: u16 = 291;
}

impl Ppu {
    /// The NTSC palette the `Ntsc` video filter samples, as raw RGB triples.
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

            prev_palette: 0x00,
            curr_palette: 0x00,
            next_palette: 0x00,
            tile_shift_lo: 0x0000,
            tile_shift_hi: 0x0000,
            tile_lo: 0x00,
            tile_hi: 0x00,
            tile_addr: 0x0000,

            status: Status::new(),
            nmi_pending: false,

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
            sprites: [Sprite::new(); 8],
            spr_cover: ConstArray::new(),

            prevent_vbl: false,
            frame: Frame::new(),
            color_bits_applied: 0,

            region,
            skip_rendering: false,
            reset_signal: false,
            emulate_warmup: false,
        };

        ppu.set_region(ppu.region);

        ppu
    }

    /// Return the current frame buffer.
    #[inline]
    #[must_use]
    pub fn frame_buffer(&self) -> &[u16; size::FRAME] {
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

            curr_palette: self.curr_palette,

            status: self.status,

            secondary_oamaddr: self.secondary_oamaddr,

            oamdata: self.oamdata,
            secondary_oamdata: self.secondary_oamdata,

            sprites: self.sprites,

            ..Default::default()
        }
    }

    /// Load the passed given buffer with RGBA pixels from the current nametables.
    ///
    /// `chr` is `$0000-$2FFF` as the PPU currently sees it - pattern tables and nametables, banked
    /// and mirrored - which [`Bus::copy_ppu_bus`](crate::bus::Bus::copy_ppu_bus) fills. Taking it
    /// as an argument is what lets a debugger render from a snapshot on another thread.
    pub fn load_nametables(&self, chr: &[u8], nametables: &mut [u8]) {
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

                let tile_index = u16::from(chr[usize::from(addr)]);
                let tile_addr = self.ctrl.bg_select | (tile_index << 4);

                let supertile = ((y_scroll & 0xFC) << 1) + (x_scroll >> 2);
                let attr = u16::from(chr[usize::from(base_attr_addr + supertile)]);
                let attr_shift = (x_scroll & 0x02) | ((y_scroll & 0x02) << 1);
                let palette_addr = ((attr >> attr_shift) & 0x03) << 2;

                let tile_num = x_scroll + (y_scroll << 5);
                let tile_x = (tile_num % 32) << 3;
                let tile_y = (tile_num / 32) << 3;

                for y in 0..8 {
                    let tile_addr = tile_addr + y;
                    let tile_lo = chr[usize::from(tile_addr)];
                    let tile_hi = chr[usize::from(tile_addr + 8)];
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
    ///
    /// See [`Ppu::load_nametables`] for what `chr` is.
    pub fn load_pattern_tables(&self, chr: &[u8], pattern_tables: &mut [u8]) {
        for i in 0..2 {
            let start = i * 0x1000;
            let end = start + 0x1000;
            let x_offset = (i % 2) * size::WIDTH / 2;
            for tile_addr in (start..end).step_by(16) {
                let tile_x = ((tile_addr % 0x1000) % 256) / 2;
                let tile_y = ((tile_addr % 0x1000) / 256) * 8;
                for y in 0..8 {
                    let tile_lo = u16::from(chr[usize::from(tile_addr + y)]);
                    let tile_hi = u16::from(chr[usize::from(tile_addr + y + 8)]);
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
        chr: &[u8],
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
                    let tile_lo = chr[usize::from(tile_addr + line_offset)];
                    let tile_hi = chr[usize::from(tile_addr + line_offset + 8)];
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
                self.nmi_pending = true;
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
        self.nmi_pending = false;
        self.reset_signal = false;
        self.open_bus = 0; // Clear open bus every frame
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
                // Hardware clears secondary OAM one byte at a time across cycles 1-64. Nothing
                // reads it until sprite evaluation starts at cycle 65, so filling once on the
                // last cycle of the range leaves identical state - including when rendering is
                // enabled part way through - while avoiding rewriting all 32 bytes on all 64
                // cycles of every scanline.
                if self.cycle == cycle::OAM_CLEAR_END {
                    self.secondary_oamdata = ConstArray::filled(0xFF);
                }
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

    #[inline]
    fn pixel_palette(&mut self) -> u8 {
        let cycle = self.cycle;
        let x = cycle - 1;
        let show_left_bg = self.mask.show_left_bg;
        let show_left_spr = self.mask.show_left_spr;
        let show_bg = self.mask.show_bg;
        let show_spr = self.mask.show_spr;
        let fine_x = self.scroll.fine_x;
        let bg_shift = 15 - fine_x;

        let min_render_x = x >= 8;
        let bg_mask = u8::from(show_bg & (show_left_bg | min_render_x));
        let bg_color = bg_mask
            * ((((self.tile_shift_hi >> bg_shift) & 0x01) << 1)
                | ((self.tile_shift_lo >> bg_shift) & 0x01)) as u8;

        let mut covering = self.spr_cover[usize::from(cycle)];
        if (covering != 0) & (show_spr & (show_left_spr | min_render_x)) {
            while covering != 0 {
                // Lowest set bit first, which is sprite priority order.
                let i = covering.trailing_zeros() as usize;
                covering &= covering - 1;
                let sprite = &self.sprites[i];

                // The cover bits are rebuilt from `sprites` each scanline, but only when rendering
                // was on at dot 257 - toggling it mid-frame can leave a bit set against a sprite
                // that has since moved, so the span is still checked here.
                let spr_shift = x.wrapping_sub(sprite.x);
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

        let palette_mask = u8::from((fine_x + (x & 0x07)) < 8);
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
            | (self.spr_cover[usize::from(cycle)] == 0)
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
        let color = if self.mask.rendering_enabled || !is_palette(addr) {
            let palette = u16::from(self.pixel_palette());
            self.palette
                .peek(addr::PALETTE_START | ((palette & 0x03 > 0) as u16 * palette))
        } else {
            self.palette.peek(addr)
        };

        self.frame
            .set_pixel(self.cycle - 1, self.scanline, u16::from(color));
    }

    /// One past the frame-buffer index of the last pixel rendered.
    #[inline]
    fn rendered_through(&self) -> usize {
        if self.scanline > scanline::VISIBLE_END {
            return size::FRAME;
        }
        (usize::from(self.scanline) << 8) + usize::from(self.cycle).min(256)
    }

    /// Fold greyscale and colour emphasis into every pixel rendered since this last ran.
    ///
    /// Called where the $2001 bits are about to change and once at the end of a frame, so each run
    /// is covered by the settings it was drawn under. Neither bit is set in the overwhelming
    /// majority of frames, which is the case this exists to make free.
    fn apply_color_bits(&mut self, through: usize) {
        let through = through.min(size::FRAME);
        if through <= self.color_bits_applied {
            return;
        }
        if self.mask.grayscale != 0x3F || self.mask.emphasis != 0 {
            let grayscale = u16::from(self.mask.grayscale);
            let emphasis = self.mask.emphasis;
            for pixel in &mut self.frame.buffer[self.color_bits_applied..through] {
                *pixel = (*pixel & grayscale) | emphasis;
            }
        }
        self.color_bits_applied = through;
    }

    // $2002 | R   | PPUSTATUS
    //       | 0-5 | Unknown (???)
    //       |   6 | Sprite0 Hit Flag, 1 = PPU rendering has hit sprite #0
    //       |     | This flag resets to 0 when VBlank starts, or CPU reads $2002
    //       |   7 | VBlank Flag, 1 = PPU is generating a Vertical Blanking Impulse
    //       |     | This flag resets to 0 when VBlank ends, or CPU reads $2002
    /// Reads $2002 PPUSTATUS, which clears the VBlank flag and resets the address latch as a side
    /// effect. Use [`Ppu::peek_status`] to look without disturbing it.
    pub fn read_status(&mut self) -> u8 {
        let status = self.peek_status();
        // Top three bits ignored for open bus
        self.open_bus |= status & 0xE0;

        if self.nmi_pending {
            trace!("$2002 NMI Ack - PPU:{:3},{:3}", self.cycle, self.scanline);
        }
        self.nmi_pending = false;
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
    /// Reads $2002 PPUSTATUS without its side effects.
    #[inline(always)]
    pub const fn peek_status(&self) -> u8 {
        // Only upper 3 bits are connected for this register
        (self.status.read() & 0xE0) | (self.open_bus & 0x1F)
    }

    // $2003 | W   | OAMADDR
    //       |     | Used to set the address in the 256-byte Sprite Memory to be
    //       |     | accessed via $2004. This address will increment by 1 after
    //       |     | each access to $2004. The Sprite Memory contains coordinates,
    //       |     | colors, and other attributes of the sprites.
    /// Writes $2003 OAMADDR, the index the next OAM access starts from.
    #[inline(always)]
    pub const fn write_oamaddr(&mut self, val: u8) {
        self.open_bus = val;
        self.oamaddr = val;
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    /// Reads $2004 OAMDATA at the current OAM address.
    #[inline(always)]
    pub fn read_oamdata(&mut self) -> u8 {
        self.open_bus = self.peek_oamdata();
        self.open_bus
    }

    // $2004 | RW  | OAMDATA
    //       |     | Used to read the Sprite Memory. The address is set via
    //       |     | $2003 and increments after each access. The Sprite Memory
    //       |     | contains coordinates, colors, and other attributes of the
    //       |     | sprites.
    // Non-mutating version of `read_oamdata`.
    /// Reads $2004 OAMDATA without side effects.
    #[inline(always)]
    pub fn peek_oamdata(&self) -> u8 {
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
    /// Writes $2004 OAMDATA at the current OAM address, incrementing it.
    pub fn write_oamdata(&mut self, mut val: u8) {
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
    /// Writes $2005 PPUSCROLL: X then Y, selected by the shared address latch.
    #[inline(always)]
    pub fn write_scroll(&mut self, val: u8) {
        self.open_bus = val;

        if self.reset_signal {
            return;
        }
        self.scroll.write(val);
    }

    // $2006 | W   | PPUADDR
    /// Writes $2006 PPUADDR: high byte then low, selected by the shared address latch.
    #[inline(always)]
    pub fn write_addr(&mut self, val: u8) {
        self.open_bus = val;

        if self.reset_signal {
            return;
        }
        self.scroll.write_addr(val);
    }

    /// Sprite evaluation on PAL's extra vblank scanlines. Never taken on NTSC, so kept out of
    /// line rather than interleaved with the render-scanline path.
    #[cold]
    #[inline(never)]
    fn clock_pal_spr_eval(&mut self) {
        self.spr_eval_cycle();
        // 257..=320
        if cycle::SPR_FETCH_RANGE.contains(&self.cycle) {
            self.write_oamaddr(0x00);
        }
    }

    /// Returns the region the PPU is timed for.
    pub const fn region(&self) -> NesRegion {
        self.region
    }

    /// Sets the region, which re-times the frame and forwards the change to the loaded mapper.
    pub fn set_region(&mut self, region: NesRegion) {
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

    /// Resets the PPU. A soft reset leaves VRAM and palette RAM alone, as the console does.
    pub fn reset(&mut self, kind: ResetKind) {
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

        self.status.reset(kind);
        self.nmi_pending = false;

        self.oam_fetch = 0x00;
        self.oam_eval_done = false;
        self.secondary_oamaddr = 0x0000;
        self.overflow_count = 0;
        self.spr_in_range = false;
        self.spr_zero_in_range = false;
        self.spr_zero_visible = false;
        self.spr_count = 0;
        self.vram_buffer = 0x00;
        self.color_bits_applied = 0;

        if kind == ResetKind::Hard {
            self.oamaddr = 0x0000;
            self.oamdata = ConstArray::new();
        } else {
            self.reset_signal = self.emulate_warmup;
        }
        self.sprites = [Sprite::new(); 8];
        self.spr_cover = ConstArray::new();
        self.prevent_vbl = false;
        self.frame.reset(kind);
    }
}

/// The PPU's view of the console: everything here reaches the cartridge, so it takes the [`Bus`]
/// rather than the [`Ppu`].
impl Bus {
    /// Read a byte from CHR-ROM/RAM/CIRAM at a given address.
    #[inline(always)]
    pub(crate) fn chr_read(&mut self, addr: u16) -> u8 {
        let served = if self.mapper_ops.intersects(MapperOps::SERVES_CHR_READS) {
            let Self { mapper, memory, .. } = self;
            mapper.chr_read(memory, addr)
        } else {
            None
        };
        let val = served.unwrap_or_else(|| self.memory.chr_peek(addr));
        // After the fetch: MMC2/MMC4 flip their CHR latch on certain addresses and the byte being
        // read must come from the pre-flip bank. MMC3's A12 counter does not affect the data, so
        // it is unaffected by the ordering.
        if self.mapper_ops.intersects(MapperOps::WATCHES_PPU_BUS) {
            let Self { mapper, memory, .. } = self;
            mapper.ppu_bus_addr(memory, addr);
        }
        val
    }

    /// Peek a byte from CHR-ROM/RAM/CIRAM at a given address.
    ///
    /// Reads through the same routing the emulation uses; reaching into `mapper` directly bypasses
    /// page-table boards and yields garbage.
    #[inline(always)]
    pub fn chr_peek(&self, addr: u16) -> u8 {
        if self.mapper_ops.intersects(MapperOps::SERVES_CHR_READS)
            && let Some(val) = self.mapper.chr_peek(&self.memory, addr)
        {
            return val;
        }
        self.memory.chr_peek(addr)
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    pub(crate) fn chr_write(&mut self, addr: u16, val: u8) {
        self.memory.chr_write(addr, val);
    }

    /// Route a read on the PPU bus: pattern tables and nametables through the board, palette RAM
    /// on the PPU itself.
    ///
    /// Has side effects - it latches the PPU's open bus, and can move an MMC2 CHR latch or an MMC3
    /// A12 counter. [`Bus::ppu_bus_peek`] observes without them.
    #[inline]
    pub(crate) fn ppu_bus_read(&mut self, addr: u16) -> u8 {
        self.ppu.open_bus = match addr {
            0x0000..=0x3EFF => self.chr_read(addr),
            0x3F00..=0x3FFF => self.ppu.palette.peek(addr),
            _ => {
                error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        };
        self.ppu.open_bus
    }

    /// Route a read on the PPU bus with no side effects at all.
    #[inline]
    #[must_use]
    pub fn ppu_bus_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3EFF => self.chr_peek(addr),
            0x3F00..=0x3FFF => self.ppu.palette.peek(addr),
            _ => {
                error!("unexpected PPU memory access at ${:04X}", addr);
                0x00
            }
        }
    }

    /// Route a write on the PPU bus.
    #[inline]
    pub(crate) fn ppu_bus_write(&mut self, addr: u16, val: u8) {
        self.ppu.open_bus = val;
        match addr {
            0x0000..=0x3EFF => self.chr_write(addr, val),
            0x3F00..=0x3FFF => self.ppu.palette.write(addr, val),
            _ => error!("unexpected PPU memory access at ${:04X}", addr),
        }
    }

    /// Load a Mapper into the PPU.
    #[inline]
    pub fn load_mapper(&mut self, mapper: Mapper) {
        self.mapper_ops = mapper.mapper_ops();
        self.mapper = mapper;
        // `ControlDeck::load_rom` sets the region *before* installing the cart, so a mapper whose
        // timing depends on it (MMC5's expansion audio) would otherwise never be told.
        self.mapper.set_region(self.region);
        #[cfg(debug_assertions)]
        self.mapper.check_mapper_ops(&self.memory);
    }

    /// Notify the mapper of a PPU bus address, for A12 scanline counters and CHR latches.
    ///
    /// Reads made through `chr_read` notify the board themselves; this exists for the sites that
    /// move the PPU address without fetching through it, such as `$2006` writes.
    #[inline(always)]
    pub fn notify_ppu_bus(&mut self, addr: u16) {
        if self.mapper_ops.intersects(MapperOps::WATCHES_PPU_BUS) {
            let Self { mapper, memory, .. } = self;
            mapper.ppu_bus_addr(memory, addr);
        }
    }

    /// Rebuild the page tables from the mapper's register state.
    ///
    /// Required after loading a save state: page tables are derived state and are not serialized,
    /// so without this a restored state would have every page unmapped.
    pub fn rebuild_mapper_state(&mut self) {
        let Self { mapper, memory, .. } = self;
        mapper.update_banks(memory);
        // mapper_ops is #[serde(skip)] - a restored save state replaced the whole `Bus`, so this
        // is the state-load path's chance to recompute it from the (serialized, and thus correct)
        // mapper.
        self.mapper_ops = self.mapper.mapper_ops();
        #[cfg(debug_assertions)]
        self.mapper.check_mapper_ops(&self.memory);
    }

    /// Return the current Nametable mirroring mode.
    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        self.mapper.mirroring()
    }

    /// Attach (or clear, via `Debugger::default()`) a debugger callback.
    //
    // Recomputes the cached `debugger_active` flag so the per-dot path tests one bool instead of
    // touching the cold `debugger` field when nothing is attached.
    #[inline]
    pub fn set_debugger(&mut self, debugger: Debugger) {
        self.debugger_active = debugger != Debugger::default();
        self.debugger = debugger;
    }

    /// Fetch BG nametable byte.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    #[inline]
    fn fetch_bg_nt_byte(&mut self) {
        self.ppu.prev_palette = self.ppu.curr_palette;
        self.ppu.curr_palette = self.ppu.next_palette;

        self.ppu.tile_shift_lo |= u16::from(self.ppu.tile_lo);
        self.ppu.tile_shift_hi |= u16::from(self.ppu.tile_hi);

        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = addr::NAMETABLE_START | (self.ppu.scroll.addr() & nametable_addr_mask);
        let tile_index = u16::from(self.chr_read(addr));
        self.ppu.tile_addr = self.ppu.ctrl.bg_select | (tile_index << 4) | self.ppu.scroll.fine_y;
    }

    /// Fetch BG attribute byte.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    #[inline(always)]
    fn fetch_bg_attr_byte(&mut self) {
        let addr = self.ppu.scroll.attr_addr();
        let shift = self.ppu.scroll.attr_shift();
        self.ppu.next_palette = ((self.chr_read(addr) >> shift) & 0x03) << 2;
    }

    /// Fetch 4 tiles and write out shift registers every 8th cycle.
    /// Each tile fetch takes 2 cycles.
    ///
    /// See: <https://wiki.nesdev.org/w/index.php/PPU_scrolling#Tile_and_attribute_fetching>
    #[inline]
    fn bg_fetch_cycle(&mut self) {
        let phase = self.ppu.cycle & 0x07;
        if self.ppu.mask.prev_rendering_enabled && phase == 0 {
            // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
            self.ppu.scroll.increment_x();
            // 256, Increment Fine Y when we reach the end of the screen
            if self.ppu.cycle == cycle::INC_Y {
                self.ppu.scroll.increment_y();
            }
            return;
        }

        match phase {
            1 => self.fetch_bg_nt_byte(),
            3 => self.fetch_bg_attr_byte(),
            5 => self.ppu.tile_lo = self.chr_read(self.ppu.tile_addr),
            7 => self.ppu.tile_hi = self.chr_read(self.ppu.tile_addr + 8),
            _ => (),
        }
    }

    fn load_sprites(&mut self) {
        // Local variables improve cache locality
        let cycle = self.ppu.cycle;
        let scanline = self.ppu.scanline;
        let spr_count = usize::from(self.ppu.spr_count);

        let idx = (cycle - cycle::SPR_FETCH_START) as usize / 8;
        let oam_idx = idx << 2;

        if let [y, tile_index, attr, x] = self.ppu.secondary_oamdata[oam_idx..=oam_idx + 3] {
            let x = u16::from(x);
            let y = u16::from(y);
            let mut tile_index = u16::from(tile_index);
            let flip_vertical = (attr & 0x80) == 0x80;

            let height = self.ppu.ctrl.spr_height;
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
                self.ppu.ctrl.spr_select | (tile_index << 4) | line_offset
            };

            if idx < spr_count {
                self.ppu.sprites[idx] = Sprite {
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
                let bit = 1 << idx;
                for dot in &mut self.ppu.spr_cover[cycle..(cycle + 8).min(256)] {
                    *dot |= bit;
                }
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
        self.ppu.write_oamaddr(0x00);

        match self.ppu.cycle & 0x07 {
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

    /// The visible and pre-render scanlines: background/sprite fetches for every dot. This is
    /// the hot path - ~92% of scanlines land here.
    #[inline]
    fn clock_render_scanline(&mut self) {
        if self.ppu.cycle <= cycle::VISIBLE_END {
            if self.ppu.is_visible_scanline {
                self.ppu.spr_eval_cycle();
            }

            self.bg_fetch_cycle();

            if self.ppu.is_prerender_scanline && self.ppu.cycle <= 8 && self.ppu.oamaddr >= 0x08 {
                // If OAMADDR is not less than eight when rendering starts, the eight bytes
                // starting at OAMADDR & 0xF8 are copied to the first eight bytes of OAM
                let addr = (self.ppu.cycle as usize) - 1;
                let oamindex = (self.ppu.oamaddr as usize & 0xF8) + addr;
                self.ppu.oamdata[addr] = self.ppu.oamdata[oamindex];
            }
        } else if self.ppu.cycle <= cycle::SPR_FETCH_END {
            if self.ppu.mask.prev_rendering_enabled && self.ppu.cycle == cycle::SPR_FETCH_START {
                // Copy X bits at the start of a new line since we're going to start writing
                // new x values to t
                self.ppu.scroll.copy_x();
                self.ppu.spr_cover = ConstArray::new();
            }
            // 280..=304
            if self.ppu.is_prerender_scanline && cycle::COPY_Y_RANGE.contains(&self.ppu.cycle) {
                // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
                // if rendering is enabled
                // https://wiki.nesdev.org/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
                self.ppu.scroll.copy_y();
            }
            self.spr_fetch_cycle();
        } else {
            // 336
            if self.ppu.cycle <= cycle::BG_PREFETCH_END {
                self.bg_fetch_cycle();
            } else {
                // 337..=340
                self.fetch_bg_nt_byte();
            }

            self.ppu.oam_fetch = self.ppu.secondary_oamdata[0];

            if self.ppu.region.is_ntsc()
                && self.ppu.is_prerender_scanline
                && self.ppu.cycle == cycle::ODD_SKIP
                && self.ppu.frame.is_odd()
            {
                // NTSC behavior while rendering - each odd PPU frame is one clock shorter
                // (skipping from 339 over 340 to 0)
                trace!(
                    "Skipped odd frame cycle: {} - PPU:{:3},{:3}",
                    self.ppu.frame_number(),
                    self.ppu.cycle,
                    self.ppu.scanline
                );
                self.ppu.cycle = cycle::END;
            }
        }
    }

    /// Advance to the next scanline. Taken once every 341 dots, so kept out of line to keep it
    /// out of the hot path's instruction footprint.
    #[cold]
    #[inline(never)]
    fn end_scanline(&mut self) {
        self.ppu.cycle = 0;
        self.ppu.scanline += 1;
        // === POST-RENDER (240/261) ===
        match self.ppu.scanline {
            s if s == self.ppu.vblank_scanline - 1 => {
                // Every visible scanline is done, and this is where the frame counter advances -
                // so it is the last point before a consumer can ask for the buffer. The mark stays
                // at the end of the buffer through vblank, so a $2001 write there finds nothing
                // outstanding rather than folding the bits into the finished frame a second time.
                self.ppu.apply_color_bits(size::FRAME);
                self.ppu.frame.increment();
            }
            s if s > self.ppu.prerender_scanline => {
                // Wrap scanline back to 0
                self.ppu.scanline = 0;
                self.ppu.color_bits_applied = 0;
                // Force prerender scanline sprite fetches to load the dummy $FF tiles (fixes
                // shaking in Ninja Gaiden 3 stage 1 after beating boss)
                self.ppu.spr_count = 0;
            }
            _ => (),
        }

        self.ppu.is_visible_scanline = self.ppu.scanline <= scanline::VISIBLE_END;
        self.ppu.is_prerender_scanline = self.ppu.scanline == self.ppu.prerender_scanline;
        self.ppu.is_render_scanline = self.ppu.is_visible_scanline | self.ppu.is_prerender_scanline;
        // PAL refreshes OAM later due to extended vblank to avoid OAM decay
        self.ppu.is_pal_spr_eval_scanline =
            self.ppu.region.is_pal() && self.ppu.scanline >= self.ppu.vblank_scanline + 24;

        self.check_debugger();
    }

    /// Fire the attached debugger if the PPU has reached its dot.
    #[inline(always)]
    fn check_debugger(&mut self) {
        if self.debugger_active
            && self.ppu.scanline == self.debugger.scanline
            && self.ppu.cycle == self.debugger.cycle
        {
            // Cloned so the callback can borrow the console it is handed. At most once a frame,
            // and only while a debugger is attached.
            let callback = std::sync::Arc::clone(&self.debugger.callback);
            callback(self);
        }
    }

    /// Clocks the PPU a single dot.
    pub fn ppu_clock(&mut self) {
        // === SCANLINE TRANSITION (cycle 340) ===
        if self.ppu.cycle >= cycle::END {
            self.end_scanline();
            return;
        }

        self.ppu.cycle += 1;

        // === RENDER LINE (scanlins 0-239, 261) ===
        if self.ppu.mask.rendering_enabled {
            if self.ppu.is_render_scanline {
                self.clock_render_scanline();
            } else if self.ppu.is_pal_spr_eval_scanline {
                self.ppu.clock_pal_spr_eval();
            }
        }

        self.ppu.mask.clock();
        if self.ppu.scroll.delayed_update()
            && (!self.ppu.mask.rendering_enabled || self.ppu.scanline > scanline::VISIBLE_END)
        {
            // MMC3 clocks using A12
            self.notify_ppu_bus(self.ppu.scroll.addr());
        }

        // The pixel and the shift registers both want the visible dots, so the range is tested
        // once for the two of them rather than once each.
        if self.ppu.cycle <= cycle::VISIBLE_END {
            // Pixels should be put even if rendering is disabled, as this is what blanks out the
            // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
            if self.ppu.is_visible_scanline {
                if self.ppu.skip_rendering {
                    self.ppu.headless_sprite_zero_hit();
                } else {
                    self.ppu.render_pixel();
                }
            }
            self.ppu.tile_shift_lo <<= 1;
            self.ppu.tile_shift_hi <<= 1;
        } else if cycle::BG_PREFETCH_RANGE.contains(&self.ppu.cycle) {
            self.ppu.tile_shift_lo <<= 1;
            self.ppu.tile_shift_hi <<= 1;
        }

        // === VBLANK / IDLE ===
        // Both edges land on dot 1, so that compare goes first and rejects 340 of every 341 dots.
        if self.ppu.cycle == cycle::VBLANK {
            if self.ppu.scanline == self.ppu.vblank_scanline {
                self.ppu.start_vblank();
            } else if self.ppu.is_prerender_scanline {
                self.ppu.stop_vblank();
            }
        }

        self.check_debugger();
    }

    /// Clocks the PPU forward until it catches up to `clock` master cycles.
    #[inline(always)]
    pub fn ppu_clock_to(&mut self, clock: u32) {
        let divider = u32::from(self.ppu.clock_divider);
        while self.ppu.master_clock + divider <= clock {
            self.ppu_clock();
            self.ppu.master_clock += divider;
        }
    }

    // $2007 | RW  | PPUDATA
    /// Reads $2007 PPUDATA. Everything below the palettes returns the *previous* read's buffered
    /// value, and the address is incremented either way.
    pub fn read_data(&mut self) -> u8 {
        let addr = self.ppu.scroll.addr();
        self.ppu.increment_vram_addr();

        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in $0000 - $3EFF
        let prev_open_bus = self.ppu.open_bus;
        let val = self.ppu_bus_read(addr);
        // MMC3 clocks using A12
        self.notify_ppu_bus(self.ppu.scroll.addr());
        self.ppu.open_bus = if addr < addr::PALETTE_START {
            let buffer = self.ppu.vram_buffer;
            self.ppu.vram_buffer = val;
            buffer
        } else {
            // Set internal buffer with mirrors of nametable when reading palettes
            // Since we're reading from > $3EFF subtract $1000 to fill
            // buffer with nametable mirror data
            self.ppu.vram_buffer = self.ppu_bus_read(addr - 0x1000);
            // Hi 2 bits of palette should be open bus
            val | (prev_open_bus & 0xC0)
        };

        trace!(
            "PPU $2007 read: {:02X} - PPU:{:3},{:3}",
            self.ppu.open_bus, self.ppu.cycle, self.ppu.scanline
        );

        self.ppu.open_bus
    }

    // $2007 | RW  | PPUDATA
    //
    // Non-mutating version of `read_data`.
    /// Reads $2007 PPUDATA without side effects.
    pub fn peek_data(&self) -> u8 {
        let addr = self.ppu.scroll.addr();
        if addr < addr::PALETTE_START {
            self.ppu.vram_buffer
        } else {
            // Since we're reading from > $3EFF subtract $1000
            // Hi 2 bits of palette should be open bus
            self.ppu_bus_peek(addr - 0x1000) | (self.ppu.open_bus & 0xC0)
        }
    }

    // $2007 | RW  | PPUDATA
    /// Writes $2007 PPUDATA at the current VRAM address, incrementing it.
    pub fn write_data(&mut self, val: u8) {
        let addr = self.ppu.scroll.addr();
        trace!(
            "PPU $2007 write: ${addr:04X} -> {val:02X} - PPU:{:3},{:3}",
            self.ppu.cycle, self.ppu.scanline
        );
        self.ppu.increment_vram_addr();
        self.ppu_bus_write(addr, val);
        // MMC3 clocks using A12
        self.notify_ppu_bus(self.ppu.scroll.addr());
    }

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
    /// Writes $2000 PPUCTRL: nametable select, VRAM increment, pattern table selects, sprite size
    /// and NMI enable.
    pub fn write_ctrl(&mut self, val: u8) {
        self.ppu.open_bus = val;
        if self.ppu.reset_signal {
            return;
        }
        self.ppu.ctrl.write(val);
        self.ppu.scroll.write_nametable_select(val);
        // MMC5 tracks changes to PPUCTRL
        self.mapper.ppu_write(0x2000, val);

        trace!(
            "$2000 NMI Enabled: {} - PPU:{:3},{:3}",
            self.ppu.ctrl.nmi_enabled, self.ppu.cycle, self.ppu.scanline,
        );

        // By toggling NMI (bit 7) during VBlank without reading $2002, /NMI can be pulled low
        // multiple times, causing multiple NMIs to be generated.
        if !self.ppu.ctrl.nmi_enabled {
            self.ppu.nmi_pending = false;
        } else if self.ppu.status.in_vblank {
            trace!(
                "$2000 NMI During VBL - PPU:{:3},{:3}",
                self.ppu.cycle, self.ppu.scanline
            );
            self.ppu.nmi_pending = true;
        }
    }

    // $2001 | RW  | PPUMASK
    //       |   0 | Unknown (???)
    //       |   1 | BG Mask, 0 = don't show background in left 8 columns
    //       |   2 | Sprite Mask, 0 = don't show sprites in left 8 columns
    //       |   3 | BG Switch, 1 = show background, 0 = hide background
    //       |   4 | Sprites Switch, 1 = show sprites, 0 = hide sprites
    //       | 5-7 | Unknown (???)
    /// Writes $2001 PPUMASK: greyscale, the two left-column clips, the two render enables and
    /// colour emphasis.
    #[inline(always)]
    pub fn write_mask(&mut self, val: u8) {
        self.ppu.open_bus = val;
        if self.ppu.reset_signal {
            return;
        }
        // Settle the pixels drawn under the old greyscale/emphasis before adopting the new ones.
        let through = self.ppu.rendered_through();
        self.ppu.apply_color_bits(through);
        self.ppu.mask.write(val);
        // MMC5 tracks changes to PPUMASK
        self.mapper.ppu_write(0x2001, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cart::Cart,
        mapper::{Mmc1Revision, Sxrom},
    };

    #[test]
    fn vram_writes() {
        let mut bus = Bus::default();
        bus.ppu.write_addr(0x23);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.write_data(0x66); // write to $2305

        assert_eq!(bus.chr_read(0x2305), 0x66);
    }

    #[test]
    fn vram_reads() {
        let mut bus = Bus::default();
        bus.write_ctrl(0x00);
        bus.ppu_bus_write(0x2305, 0x66);

        bus.ppu.write_addr(0x23);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.ppu.scroll.addr(), 0x2306);
        assert_eq!(bus.read_data(), 0x66);
        assert_eq!(bus.ppu.scroll.addr(), 0x2307);
    }

    #[test]
    fn vram_read_pagecross() {
        let mut bus = Bus::default();
        bus.write_ctrl(0x00);
        bus.ppu_bus_write(0x21FF, 0x66);
        bus.ppu_bus_write(0x2200, 0x77);

        bus.ppu.write_addr(0x21);
        bus.ppu.write_addr(0xFF);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x66);
        assert_eq!(bus.read_data(), 0x77);
    }

    #[test]
    fn vram_read_vertical_increment() {
        let mut bus = Bus::default();
        bus.write_ctrl(0b100);
        bus.ppu_bus_write(0x21FF, 0x66);
        bus.ppu_bus_write(0x21FF + 32, 0x77);
        bus.ppu_bus_write(0x21FF + 64, 0x88);

        bus.ppu.write_addr(0x21);
        bus.ppu.write_addr(0xFF);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x66);
        assert_eq!(bus.read_data(), 0x77);
        assert_eq!(bus.read_data(), 0x88);
    }

    // Horizontal: https://wiki.nesdev.org/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 a ]
    //   [0x2800 B ] [0x2C00 b ]
    #[test]
    fn vram_horizontal_mirror() {
        let mut bus = Bus::default();
        bus.ppu.write_addr(0x24);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.write_data(0x66); // write to a at $2405

        bus.ppu.write_addr(0x28);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.write_data(0x77); // write to B at $2805

        bus.ppu.write_addr(0x20);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x66); // read A from $2005

        bus.ppu.write_addr(0x2C);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x77); // read b from $2C05
    }

    // Vertical: https://wiki.nesdev.org/w/index.php/Mirroring
    //   [0x2000 A ] [0x2400 B ]
    //   [0x2800 a ] [0x2C00 b ]
    #[test]
    fn vram_vertical_mirror() {
        let mut bus = Bus::default();
        let mut cart = Cart::default();
        cart.mapper = Sxrom::load(&mut cart, Mmc1Revision::BC).unwrap();
        // Set vertical mirroring mode via 5 writes
        let mut val = 0b00_00_00_01_00;
        for _ in 0..5 {
            cart.mapper
                .write_register(&mut cart.memory, 0x8000, val & 0b11);
            cart.mapper.clock();
            cart.mapper.clock();
            val >>= 2;
        }
        bus.load_cart(cart);

        bus.ppu.write_addr(0x20);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.write_data(0x66); // write to A at $2005

        bus.ppu.write_addr(0x2C);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.write_data(0x77); // write to b at $2C05

        bus.ppu.write_addr(0x28);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x66); // read a from $2805

        bus.ppu.write_addr(0x24);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x77); // read B from $2405
    }

    #[test]
    fn read_status_resets_latch() {
        let mut bus = Bus::default();
        bus.ppu_bus_write(0x2305, 0x66);

        bus.ppu.write_addr(0x21);
        bus.ppu.write_addr(0x23);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.ppu.write_addr(0x05);
        bus.read_data(); // buffer read
        assert_ne!(bus.read_data(), 0x66);

        bus.ppu.read_status();

        bus.ppu.write_addr(0x23);
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.read_data(), 0x66);
    }

    #[test]
    fn vram_mirroring() {
        let mut bus = Bus::default();
        bus.write_ctrl(0);
        bus.ppu_bus_write(0x2305, 0x66);

        bus.ppu.write_addr(0x63); // 0x6305 mirrors to 0x2305
        bus.ppu.write_addr(0x05);
        // PPU writes to $2006 are delayed by 2 PPU clocks
        bus.ppu_clock();
        bus.ppu_clock();
        bus.read_data(); // buffer read
        assert_eq!(bus.ppu.scroll.addr(), 0x2306);
        assert_eq!(bus.read_data(), 0x66);
        assert_eq!(bus.ppu.scroll.addr(), 0x2307);
    }

    #[test]
    fn read_status_resets_vblank() {
        let mut bus = Bus::default();
        bus.ppu.status.set_in_vblank(true);

        let status = bus.ppu.read_status();
        assert_eq!(status >> 7, 1);
        assert_eq!(bus.ppu.status.read() >> 7, 0);
    }

    #[test]
    fn sprite_zero_hit_headless_visible_cycle() {
        let mut bus = Bus::default();
        bus.write_mask(0x18);
        bus.ppu.skip_rendering = true;
        bus.ppu.scanline = 0;
        bus.ppu.cycle = 10;
        bus.ppu.scroll.fine_x = 0;

        bus.ppu.tile_shift_lo = 0x8000;
        bus.ppu.tile_shift_hi = 0x0000;

        bus.ppu.spr_zero_visible = true;
        bus.ppu.spr_cover[9..17].fill(1 << 0);

        bus.ppu.sprites[0].x = 8;
        bus.ppu.sprites[0].tile_lo = 0b0100;
        bus.ppu.sprites[0].tile_hi = 0b0000;
        bus.ppu.sprites[0].flip_horizontal = true;
        bus.ppu.sprites[0].bg_priority = false;

        bus.ppu_clock();

        assert!(bus.ppu.status.spr_zero_hit);
    }

    #[test]
    fn oam_read_write() {
        let mut bus = Bus::default();
        bus.ppu.write_oamaddr(0x10);
        bus.ppu.write_oamdata(0x66);
        bus.ppu.write_oamdata(0x77);

        bus.ppu.write_oamaddr(0x10);
        assert_eq!(bus.ppu.read_oamdata(), 0x66);

        bus.ppu.write_oamaddr(0x11);
        assert_eq!(bus.ppu.read_oamdata(), 0x77);
    }
}
