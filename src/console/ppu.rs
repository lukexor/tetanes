//! Picture Processing Unit
//!
//! http://wiki.nesdev.com/w/index.php/PPU

use crate::console::cartridge::Board;
use crate::console::cpu::Cycle;
use crate::console::memory::{Addr, Byte, Memory, Ram, Word, KILOBYTE};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

// Screen/Render
const SCREEN_HEIGHT: usize = 240;
const SCREEN_WIDTH: usize = 256;
const PIXEL_COUNT: usize = (SCREEN_HEIGHT * SCREEN_WIDTH) as usize;
pub const RENDER_SIZE: usize = PIXEL_COUNT * 3;

// Sizes
const NAMETABLE_SIZE: usize = 2 * KILOBYTE; // two 1K nametables
const PALETTE_SIZE: usize = 32;
const SYSTEM_PALETTE_SIZE: usize = 64;
const OAM_SIZE: usize = 64 * 4; // 64 entries * 4 bytes each

// Cycles
const CYCLES_PER_SCANLINE: Cycle = 114; // 341 PPU cycles per scanline - 3 PPU cycles per 1 CPU cycle
const PRERENDER_CYCLE_END: Cycle = 339;
const TILE_FETCH_CYCLE_END: Cycle = 256;
const SPRITE_PREFETCH_CYCLE_START: Cycle = 257;
const SPRITE_PREFETCH_CYCLE_END: Cycle = 320;
const VISIBLE_CYCLE_END: Cycle = 340;
const COPY_Y_CYCLE_START: Cycle = 280;
const COPY_Y_CYCLE_END: Cycle = 304;

// Scanlines
const PRERENDER_SCANLINE: Word = 261;
const VISIBLE_SCANLINE_START: Word = 0;
const VISIBLE_SCANLINE_END: Word = 239;
const POSTRENDER_SCANLINE: Word = 240;
const VBLANK_SCANLINE: Word = 241;

// PPUSCROLL masks
// yyy NN YYYYY XXXXX
// ||| || ||||| +++++- 5 bit course X
// ||| || +++++------- 5 bit course Y
// ||| |+------------- Nametable X offset
// ||| +-------------- Nametable Y offset
// +++---------------- 3 bit fine Y
const COARSE_X_MASK: Word = 0x001F;
const COARSE_Y_MASK: Word = 0x03e0;
const NAMETABLE_X_MASK: Word = 0x0400;
const NAMETABLE_Y_MASK: Word = 0x0800;
const FINE_Y_MASK: Word = 0x7000;
const VRAM_ADDR_SIZE_MASK: Word = 0x7FFF; // 15 bits
const X_MAX_COL: Word = 31; // last column of tiles
const Y_MAX_COL: Word = 29; // last row of tiles
const Y_OVER_COL: Word = 31; // overscan row

// Nametable ranges
const NAMETABLE_START: Addr = 0x2000; // Upper-left corner
const NAMETABLE_END: Addr = 0x2FBF;
const ATTRIBUTE_START: Addr = 0x23C0; // Attributes for NAMETABLEs
const ATTRIBUTE_END: Addr = 0x2FFF;

#[derive(Debug)]
pub struct Ppu {
    cycle: Cycle,   // 341 cycles happen per scanline
    scanline: Word, // 262 total scanlines per frame
    regs: PpuRegs,  // Registers
    oamdata: Oam,   // $2004 OAMDATA read/write
    vram: Vram,     // $2007 PPUDATA
    frame: Frame,   // Frame data keeps track of data and shift registers between frames
    screen: Screen, // The main screen holding pixel data
}

#[derive(Debug)]
pub struct PpuRegs {
    open_bus: Byte,    // This open bus gets set during any write to PPU registers
    ctrl: PpuCtrl,     // $2000 PPUCTRL write-only
    mask: PpuMask,     // $2001 PPUMASK write-only
    status: PpuStatus, // $2002 PPUSTATUS read-only
    oamaddr: Byte,     // $2003 OAMADDR write-only
    v: Addr,           // $2006 PPUADDR write-only 2x 15 bits: yyy NN YYYYY XXXXX
    t: Addr,           // Temporary v - Also the addr of top-left onscreen tile
    x: Byte,           // Fine X
    w: bool,           // 1st or 2nd write toggle
}

#[derive(Debug)]
struct Vram {
    board: Option<Arc<Mutex<Board>>>,
    buffer: Byte,
    nametable: Nametable,
    palette: Palette,
}

// Addr Low Nibble
// $00, $04, $08, $0C   Sprite Y coord
// $01, $05, $09, $0D   Sprite tile #
// $02, $06, $0A, $0E   Sprite attribute
// $03, $07, $0B, $0F   Sprite X coord
struct Oam {
    entries: [Byte; OAM_SIZE],
}

#[derive(Debug)]
struct Frame {
    num: u32,
    parity: bool,
    // Shift registers
    pattern_shift_lo: Word,
    pattern_shift_hi: Word,
    palette_shift: Word,
    // tile data - stored in cycles 0 mod 8
    nametable: Byte,
    attribute: Byte,
    pattern_lo: Byte,
    pattern_hi: Byte,
    // sprite data
    next_sprite_count: Byte,
    sprites: [Sprite; 8], // Each frame can only hold 8 sprites at a time
}

struct Screen {
    pixels: [Rgb; PIXEL_COUNT],
}

#[derive(Debug, Copy, Clone)]
struct Sprite {
    index: Byte,
    x: Byte,
    y: Byte,
    tile_index: Byte,
    palette: Byte,
    pattern: Word,
    has_priority: bool,
    flip_horizontal: bool,
    flip_vertical: bool,
}

#[derive(Debug)]
struct PpuCtrl(Byte);
#[derive(Debug)]
struct PpuMask(Byte);
#[derive(Debug)]
struct PpuStatus(Byte);

// http://wiki.nesdev.com/w/index.php/PPU_nametables
// http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
struct Nametable([Byte; NAMETABLE_SIZE]);
// http://wiki.nesdev.com/w/index.php/PPU_palettes
struct Palette([Byte; PALETTE_SIZE]);

#[derive(Debug)]
enum SpriteSize {
    Sprite8x8,
    Sprite8x16,
}

#[derive(Debug, Copy, Clone)]
struct Rgb(Byte, Byte, Byte);

#[derive(Copy, Clone)]
struct PaletteColor {
    palette: Byte, // (0, 3)
    pixel: Byte,   // (0, 3)
}

#[derive(Debug)]
pub struct StepResult {
    pub new_frame: bool,
    pub vblank_nmi: bool,
    pub scanline_irq: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycle: 0,
            scanline: 0,
            regs: PpuRegs::new(),
            oamdata: Oam::new(),
            vram: Vram::new(),
            frame: Frame::new(),
            screen: Screen::new(),
        }
    }

    // Puts the Cartridge board into VRAM
    pub fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.vram.set_board(board);
    }

    // Step ticks as many cycles as needed to reach
    // target cycle to syncronize with the CPU
    // http://wiki.nesdev.com/w/index.php/PPU_rendering
    pub fn step(&mut self) -> StepResult {
        let mut step_result = StepResult::new();
        self.tick();
        self.render_scanline();
        if self.cycle == 1 {
            if self.scanline == PRERENDER_SCANLINE {
                // Dummy scanline - set up tiles for next scanline
                step_result.new_frame = true;
                self.set_vblank(false);
                self.set_sprite_zero_hit(false);
                self.set_sprite_overflow(false);
            } else if self.scanline == VBLANK_SCANLINE {
                self.set_vblank(true);
                if self.vblank_nmi() {
                    step_result.vblank_nmi = true;
                }
            }
        }
        step_result
    }

    pub fn render(&self) -> [Byte; RENDER_SIZE] {
        self.screen.render()
    }

    // Execute a visible frame scanline render cycle
    fn render_scanline(&mut self) {
        if self.rendering_enabled() {
            let is_visible_scanline = self.scanline <= VISIBLE_SCANLINE_END;
            let is_tile_cycle = self.cycle >= 1 && self.cycle <= TILE_FETCH_CYCLE_END;
            let is_fetch_scanline = self.scanline == PRERENDER_SCANLINE || is_visible_scanline;
            let is_prefetch_cycle = self.cycle >= 321 && self.cycle <= 336;
            let is_fetch_cycle = is_prefetch_cycle || is_tile_cycle;

            // evaluate background
            let should_render = is_visible_scanline && is_tile_cycle;
            if should_render {
                self.render_pixel();
            }

            let should_fetch = is_fetch_scanline && is_fetch_cycle;
            if should_fetch {
                self.shift_registers();
                // Fetch 4 tiles and write out shift registers every 8th cycle
                // Each tile fetch takes 2 cycles
                match self.cycle & 0x07 {
                    0 => self.shift_new_tile(),
                    1 => self.fetch_bg_nametable(),
                    3 => self.fetch_bg_attribute(),
                    5 => self.fetch_bg_pattern_lo(),
                    7 => self.fetch_bg_pattern_hi(),
                    _ => panic!("invalid cycle"),
                }
            }
            // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
            // if rendering is enabled
            // http://wiki.nesdev.com/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
            if self.scanline == PRERENDER_SCANLINE
                && self.cycle >= COPY_Y_CYCLE_START
                && self.cycle <= COPY_Y_CYCLE_END
            {
                self.regs.copy_y();
            }

            if is_fetch_scanline {
                // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
                if is_fetch_cycle && self.cycle % 8 == 0 {
                    self.regs.increment_x();
                }
                // Increment Fine Y when we reach the end of the screen
                if self.cycle == SCREEN_WIDTH as Cycle {
                    self.regs.increment_y();
                }
                // Copy X bits at the start of a new line since we're going to start writing
                // new x values to t
                if self.cycle == (SCREEN_WIDTH + 1) as Cycle {
                    self.regs.copy_x();
                }
            }

            // evaluate sprites
            if self.cycle == SPRITE_PREFETCH_CYCLE_START {
                if is_visible_scanline {
                    self.evaluate_sprites();
                } else {
                    self.frame.next_sprite_count = 0;
                }
            }
        }
    }

    fn evaluate_sprites(&mut self) {
        let mut count = 0;
        for i in 0..OAM_SIZE / 4 {
            if count == 8 {
                self.set_sprite_overflow(true);
                break;
            }

            let mut sprite = self.get_sprite(i);
            if let Some(pat) = self.get_sprite_pattern(&sprite) {
                if count < 8 {
                    sprite.pattern = pat;
                    self.frame.sprites[i] = sprite;
                    count += 1;
                }
            }
        }
    }

    fn render_pixel(&mut self) {
        eprintln!("Rendering pixel...");
        let x = (self.cycle - 1) as Byte; // Because we called tick() before this
        let y = self.scanline as Byte;

        let bg_color = self.background_color(x);
        let (i, sprite_color) = self.sprite_color(x);

        if self.is_sprite_zero(i) & sprite_color.opaque() && bg_color.opaque() {
            self.set_sprite_zero_hit(true);
        }
        let color = if self.sprite_has_priority(i) && sprite_color.opaque() {
            sprite_color
        } else if bg_color.opaque() {
            bg_color
        } else if sprite_color.opaque() {
            sprite_color
        } else {
            PaletteColor::universal_bg()
        };
        let palette_addr = color.index();
        let system_palette_idx = self.readb(palette_addr);
        self.screen.put_pixel(x, y, system_palette_idx);
    }

    fn is_sprite_zero(&self, index: usize) -> bool {
        false
    }

    fn sprite_has_priority(&self, index: usize) -> bool {
        false
    }

    fn background_color(&mut self, x: Byte) -> PaletteColor {
        if !self.regs.mask.show_background() {
            return PaletteColor::universal_bg();
        }
        // 43210
        // |||||
        // |||++- Pixel value from tile data
        // |++--- Palette number from attribute table or OAM
        // +----- Background/Sprite select

        // TODO Explain the bit shifting here more clearly
        // Get pixel value
        let fine_x = self.regs.fine_x();
        let lo = (self.frame.pattern_shift_lo << fine_x) & 0x8000;
        let hi = (self.frame.pattern_shift_hi << fine_x) & 0x8000;
        let pixel = lo | (hi << 1);

        // Get palette number
        let shift_by = if ((x & 0x07) + fine_x) > 0x07 { 0 } else { 2 };
        let palette = (self.frame.palette_shift >> shift_by) & 0x03;
        PaletteColor::with_parts(palette as Byte, pixel as Byte)
    }

    fn sprite_color(&mut self, x: Byte) -> (usize, PaletteColor) {
        if !self.regs.mask.show_sprites() {
            return (0, PaletteColor::universal_bg());
        }
        for i in 0..self.frame.next_sprite_count as usize {
            let sprite_x = self.frame.sprites[i].x;
            let xsub = i16::from(x) - i16::from(sprite_x);
            if xsub < 0 || xsub > 7 {
                continue;
            }
            let palette = self.frame.sprites[i].palette;
            // TODO explain this better
            let color = (self.frame.sprites[i].pattern >> ((7 - xsub) * 2)) & 0x3;
            let palette_color = PaletteColor::with_parts(palette, color as Byte);
            if palette_color.transparent() {
                continue;
            }
            return (i, palette_color);
        }
        (0, PaletteColor::universal_bg())
    }

    fn shift_registers(&mut self) {
        self.frame.pattern_shift_lo <<= 1;
        self.frame.pattern_shift_hi <<= 1;
    }

    fn shift_new_tile(&mut self) {
        let f = &mut self.frame;
        // Get palette index in our attribute table
        // http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
        let is_left = (self.regs.coarse_x() & 0x03) < 2;
        let is_top = (self.regs.coarse_y() & 0x03) < 2;
        let palette_idx = match (is_left, is_top) {
            (true, true) => f.attribute & 0x03,          // topleft
            (false, true) => (f.attribute >> 2) & 0x03,  // topright
            (true, false) => (f.attribute >> 4) & 0x03,  // bottomleft
            (false, false) => (f.attribute >> 6) & 0x03, // bottomright
        };
        f.pattern_shift_lo |= Word::from(f.pattern_lo);
        f.pattern_shift_hi |= Word::from(f.pattern_hi);
        f.palette_shift <<= 2;
        f.palette_shift |= Word::from(palette_idx);
    }

    fn fetch_bg_nametable(&mut self) {
        let addr = self.regs.nametable_addr();
        self.frame.nametable = self.readb(addr);
    }

    fn fetch_bg_attribute(&mut self) {
        let addr = self.regs.attribute_addr();
        self.frame.attribute = self.readb(addr);
    }

    fn fetch_bg_pattern_lo(&mut self) {
        let is_sprite = false;
        let nametable = self.frame.nametable;
        let fine_y = self.regs.fine_y();
        let (addr_lo, _) = self.get_pattern_rows(is_sprite, nametable, fine_y);
        self.frame.pattern_lo = self.readb(addr_lo);
    }

    fn fetch_bg_pattern_hi(&mut self) {
        let is_sprite = false;
        let nametable = self.frame.nametable;
        let fine_y = self.regs.fine_y();
        let (_, addr_hi) = self.get_pattern_rows(is_sprite, nametable, fine_y);
        self.frame.pattern_hi = self.readb(addr_hi);
    }

    // https://wiki.nesdev.com/w/index.php/PPU_pattern_tables
    // DCBA98 76543210
    // 0HRRRR CCCCPTTT
    // |||||| |||||+++- T: Fine Y offset, the row number within a tile
    // |||||| ||||+---- P: Bit plane (0: "lower"; 1: "upper")
    // |||||| ++++----- C: Tile column (0x10)
    // ||++++---------- R: Tile row
    // |+-------------- H: Pattern Select - Which half of sprite table
    // |                   0: "left" (0x0000)
    // |                   1: "right" (0x1000)
    // +--------------- 0: Unused. Pattern table is at $0000-$1FFF
    fn get_pattern_rows(&self, is_sprite: bool, index: Byte, y_offset: Byte) -> (Word, Word) {
        let pattern_select = if is_sprite {
            self.regs.ctrl.sprite_select()
        } else {
            self.regs.ctrl.background_select()
        };
        let column_size = 0x10;
        let row0 = pattern_select + (column_size * Word::from(index)) + Word::from(y_offset);
        let row1 = row0 + 0x08; // "upper" bit plane
        (row0, row1)
    }
}

impl PpuRegs {
    fn new() -> Self {
        Self {
            open_bus: 0,
            ctrl: PpuCtrl(0),
            mask: PpuMask(0),
            status: PpuStatus(0),
            oamaddr: 0,
            v: 0,
            t: 0,
            x: 0,
            w: false,
        }
    }

    /*
     * PPUCTRL
     */

    // Resets 1st/2nd Write latch for PPUSCROLL and PPUADDR
    fn reset_rw(&mut self) {
        self.w = false;
    }

    fn write_ctrl(&mut self, val: Byte) {
        let valaddr = Addr::from(val);
        let mask = NAMETABLE_X_MASK | NAMETABLE_Y_MASK;
        self.t &= !mask;
        self.t |= (valaddr & 0b11) << 10; // take lo 2 bits and set NN
        self.ctrl.write(val);
    }

    /*
     * PPUSTATUS
     */

    fn read_status(&mut self) -> Byte {
        self.reset_rw();
        // Include garbage from open bus
        self.status.read() | (self.open_bus & 0x10)
    }

    /*
     * PPUSCROLL
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSCROLL
     * http://wiki.nesdev.com/w/index.php/PPU_scrolling
     */

    // Returns Fine X: xxx from x register
    fn fine_x(&self) -> Byte {
        self.x
    }

    // Returns Fine Y: yyy from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn fine_y(&self) -> Byte {
        // Shift yyy over nametable, coarse y and x and return 3 bits
        ((self.v >> 12) & 0x7) as Byte
    }

    // Returns Coarse X: XXXXX from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn coarse_x(&self) -> Byte {
        (self.v & COARSE_X_MASK) as Byte
    }

    // Returns Coarse Y: YYYYY from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn coarse_y(&self) -> Byte {
        // Take coarse y and shift over coase x
        ((self.v & COARSE_Y_MASK) >> 5) as Byte
    }

    // Writes val to PPUSCROLL
    // 1st write writes X
    // 2nd write writes Y
    fn write_scroll(&mut self, val: Byte) {
        let val = Addr::from(val);
        let low_5_bit_mask: Addr = 0x1F;
        let fine_mask: Addr = 0x07;
        let fine_shift = 3;
        if !self.w {
            // Write X on first write
            // lo 3 bits goes into fine x, remaining 5 bits go into t
            self.t |= (val >> fine_shift) & low_5_bit_mask;
            self.x = (val & fine_mask) as Byte; // 3 bits
        } else {
            // Write Y on second write
            let coarse_y_unshift = 5;
            let fine_y_unshift = 12;
            self.t |= ((val >> fine_shift) & low_5_bit_mask) << coarse_y_unshift;
            self.t |= (val & fine_mask) << fine_y_unshift;
        }
        self.w = !self.w;
    }

    // Copy Coarse X from register t and add it to PPUADDR v
    fn copy_x(&mut self) {
        let x_mask = COARSE_X_MASK | NAMETABLE_X_MASK;
        self.v &= !x_mask;
        self.v |= self.t & x_mask;
    }

    // Increment Coarse X
    // 0-4 bits are incremented, with overflow toggling bit 10 which switches the horizontal
    // nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    fn increment_x(&mut self) {
        if (self.v & COARSE_X_MASK) == X_MAX_COL {
            // 255 width / 8x sprite size
            self.v &= !COARSE_X_MASK;
            // XOR with right N which sets X offset to either 0 or 255
            // yyy NN YYYYY XXXXX
            // switches horizontal nametable
            self.v ^= NAMETABLE_X_MASK;
        } else {
            // Otherwise add one and box to our 15 bit addr length
            self.v = (self.v + 1) & VRAM_ADDR_SIZE_MASK;
        }
    }

    // Copy Fine y and Coarse Y from register t and add it to PPUADDR v
    fn copy_y(&mut self) {
        let y_mask = FINE_Y_MASK | NAMETABLE_Y_MASK | COARSE_Y_MASK;
        self.v &= !y_mask;
        self.v |= self.t & y_mask;
    }

    // Increment Fine Y
    // Bits 12-14 are incremented for Fine Y, with overflow incrementing coarse Y in bits 5-9 with
    // overflow toggling bit 11 which switches the vertical nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    fn increment_y(&mut self) {
        if (self.v & FINE_Y_MASK) != FINE_Y_MASK {
            // If fine y < 7 (0b111), increment
            self.v += (1 << 12);
        } else {
            self.v &= !FINE_Y_MASK; // set fine y = 0 and overflow into course y
            let mut y = (self.v & COARSE_Y_MASK) >> 5; // Get 5 bits of course y
            if y == Y_MAX_COL {
                y = 0;
                // switches vertical nametable
                self.v ^= NAMETABLE_Y_MASK;
            } else if y == Y_OVER_COL {
                // Out of bounds. Does not switch nametable when incremented
                // Some games use this
                y = 0;
            } else {
                y += 1; // add one to course y
            }
            self.v = (self.v & !COARSE_Y_MASK) | (y << 5); // put course y back into v
        }
    }

    // Returns the Y value for the current scanline by combining Fine Y and Coarse Y
    fn scanline_y(&self) -> Word {
        (self.v & COARSE_Y_MASK) >> 2 | (self.v & FINE_Y_MASK) >> 12
    }

    /*
     * PPUADDR
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUADDR
     */

    // Write val to PPUADDR v
    // 1st write writes hi 6 bits
    // 2nd write writes lo 8 bits
    // Total size is a 14 bit addr
    fn write_addr(&mut self, val: Byte) {
        let val = Addr::from(val);
        let hi_bits_mask = 0x00FF;
        let lo_bits_mask = 0xFF00;
        let hi_six_bit_mask = 0x003F;
        let hi_unshift = 8;
        if !self.w {
            // Write hi address on first write
            self.t &= hi_bits_mask;
            // Place 6 bits into hi place (8 + 6 = 14 bit address)
            self.t |= (val & hi_six_bit_mask) << hi_unshift;
        } else {
            // Write lo address on second write
            self.t &= lo_bits_mask;
            self.t |= val;
            self.v = self.t;
        }
        self.w = !self.w;
    }

    // Increment PPUADDR v
    // Address wraps and uses vram_increment which is either 1 (going across) or 32 (going down)
    // based on bit 7 in PPUCTRL
    fn increment_v(&mut self) {
        // TODO vram increment is more complex during rendering
        self.v = self.v.wrapping_add(self.ctrl.vram_increment());
    }

    // Gets the nametable address based on PPUADDR v
    fn nametable_addr(&self) -> Addr {
        // TODO explain this bit operation more clearly
        (NAMETABLE_START | (self.v & 0x0FFF)) & (NAMETABLE_END - 1)
    }
    // Gets the attribute table address based on PPUADDR v
    fn attribute_addr(&self) -> Addr {
        let v = self.v;
        // TODO explain this bit operation more clearly
        // Seems to get use the nametable select portion of v and then masks the upper 3 of the
        // lower 6 bits of v shifted to the right 4 times with the lower 3 bits of v shifted to the
        // right twice
        // What does this do??
        (ATTRIBUTE_START | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07))
            & (ATTRIBUTE_END - 1)
    }
}

impl Oam {
    fn new() -> Self {
        Self {
            entries: [0; OAM_SIZE],
        }
    }
}

impl Palette {
    fn mirror_addr(addr: Addr) -> Addr {
        // These addresses are mirrored down
        let addr = addr & (PALETTE_SIZE as Addr - 1);
        match addr {
            0x3F10 => 0x3F00,
            0x3F14 => 0x3F04,
            0x3F18 => 0x3F08,
            0x3F1C => 0x3F0C,
            _ => addr,
        }
    }
}

impl Vram {
    fn new() -> Self {
        Self {
            board: None,
            nametable: Nametable([0; NAMETABLE_SIZE]),
            palette: Palette([0; PALETTE_SIZE]),
            buffer: 0,
        }
    }

    fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.board = Some(board);
    }
}

impl Frame {
    fn new() -> Self {
        Self {
            num: 0,
            parity: false,
            pattern_shift_lo: 0,
            pattern_shift_hi: 0,
            palette_shift: 0,
            nametable: 0,
            attribute: 0,
            pattern_lo: 0,
            pattern_hi: 0,
            next_sprite_count: 0,
            sprites: [Sprite::new(); 8],
        }
    }

    fn increment(&mut self) {
        self.num += 1;
        self.parity = !self.parity;
    }
}

impl Screen {
    fn new() -> Self {
        Self {
            pixels: [Rgb(0, 0, 0); PIXEL_COUNT],
        }
    }

    // Turns a list of pixels into a list of R, G, B
    pub fn render(&self) -> [Byte; RENDER_SIZE] {
        let mut output = [0; RENDER_SIZE];
        for i in 0..PIXEL_COUNT {
            let p = self.pixels[i];
            output[i] = p.r();
            output[i + 1] = p.g();
            output[i + 2] = p.b();
        }
        output
    }

    fn put_pixel(&mut self, x: Byte, y: Byte, system_palette_idx: Byte) {
        if x < SCREEN_WIDTH as Byte && y < SCREEN_HEIGHT as Byte {
            let i = (x + SCREEN_WIDTH as Byte * y) as usize;
            self.pixels[i] = SYSTEM_PALETTE[system_palette_idx as usize];
            eprintln!("Putting pixel: {:?}", self.pixels[i]);
        }
    }
}

impl Sprite {
    fn new() -> Self {
        Self {
            index: 0,
            x: 0,
            y: 0,
            tile_index: 0,
            palette: 0,
            pattern: 0,
            has_priority: false,
            flip_horizontal: false,
            flip_vertical: false,
        }
    }
}

impl Rgb {
    // self is pass by value here because clippy says it's more efficient
    // https://rust-lang.github.io/rust-clippy/master/index.html#trivially_copy_pass_by_ref
    fn r(self) -> Byte {
        self.0
    }
    fn g(self) -> Byte {
        self.1
    }
    fn b(self) -> Byte {
        self.2
    }
}

// https://wiki.nesdev.com/w/index.php/PPU_palettes
impl PaletteColor {
    fn with_parts(palette: Byte, pixel: Byte) -> Self {
        Self { palette, pixel }
    }
    fn universal_bg() -> Self {
        Self {
            palette: 0,
            pixel: 0,
        }
    }

    // self is pass by value here because clippy says it's more efficient
    // https://rust-lang.github.io/rust-clippy/master/index.html#trivially_copy_pass_by_ref
    fn index(self) -> Word {
        let palette_color_size = 4;
        let palette_start = 0x3F00;
        palette_start + palette_color_size * Word::from(self.palette) + Word::from(self.pixel)
    }
    fn transparent(self) -> bool {
        self.pixel == 0
    }
    fn opaque(self) -> bool {
        !self.transparent()
    }
}

impl StepResult {
    fn new() -> Self {
        Self {
            new_frame: false,
            vblank_nmi: false,
            scanline_irq: false,
        }
    }
}

impl Memory for Ppu {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr & 0x2007 {
            0x2000 => self.regs.open_bus,    // PPUCTRL is write-only
            0x2001 => self.regs.open_bus,    // PPUMASK is write-only
            0x2002 => self.read_ppustatus(), // PPUSTATUS
            0x2003 => self.regs.open_bus,    // OAMADDR is write-only
            0x2004 => self.read_oamdata(),   // OAMDATA
            0x2005 => self.regs.open_bus,    // PPUSCROLL is write-only
            0x2006 => self.regs.open_bus,    // PPUADDR is write-only
            0x2007 => self.read_ppudata(),   // PPUDATA
            _ => panic!("impossible"),
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        self.regs.open_bus = val;
        // Write least sig bits to ppustatus since they're not written to
        *self.regs.status |= val & 0x1F;
        match addr & 0x2007 {
            0x2000 => self.write_ppuctrl(val),   // PPUCTRL
            0x2001 => self.write_ppumask(val),   // PPUMASK
            0x2002 => (),                        // PPUSTATUS is read-only
            0x2003 => self.write_oamaddr(val),   // OAMADDR
            0x2004 => self.write_oamdata(val),   // OAMDATA
            0x2005 => self.write_ppuscroll(val), // PPUSCROLL
            0x2006 => self.write_ppuaddr(val),   // PPUADDR
            0x2007 => self.write_ppudata(val),   // PPUDATA
            _ => panic!("impossible"),
        }
    }
}

impl Memory for Vram {
    fn readb(&mut self, addr: Addr) -> Byte {
        let val = match addr {
            0x0000..=0x1FFF => {
                // CHR-ROM
                if let Some(b) = &self.board {
                    let mut board = b.lock().unwrap();
                    board.readb(addr)
                } else {
                    0
                }
            }
            0x2000..=0x3EFF => self.nametable.readb(addr),
            0x3F00..=0x3FFF => self.palette.readb(addr),
            _ => panic!("invalid Vram readb at 0x{:04X}", addr),
        };
        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in 0 - $3EFF
        if addr <= 0x3EFF {
            let buffer = self.buffer;
            self.buffer = val;
            buffer
        } else {
            val
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        match addr {
            0x0000..=0x1FFF => {
                // CHR-ROM
                if let Some(b) = &self.board {
                    let mut board = b.lock().unwrap();
                    board.writeb(addr, val);
                } else {
                    eprintln!(
                        "uninitialized board at Vram writeb at 0x{:04X} - val: 0x{:02x}",
                        addr, val
                    );
                }
            }
            0x2000..=0x3EFF => self.nametable.writeb(addr, val),
            0x3F00..=0x3FFF => self.palette.writeb(addr, val),
            _ => panic!("invalid Vram readb at 0x{:04X}", addr),
        }
    }
}

impl Memory for Oam {
    fn readb(&mut self, addr: Addr) -> Byte {
        self.entries[addr as usize & (OAM_SIZE - 1)]
    }
    fn writeb(&mut self, addr: Addr, val: Byte) {
        self.entries[addr as usize & (OAM_SIZE - 1)] = val;
    }
}

impl Memory for Nametable {
    fn readb(&mut self, addr: Addr) -> Byte {
        self.0[addr as usize & (NAMETABLE_SIZE - 1)]
    }
    fn writeb(&mut self, addr: Addr, val: Byte) {
        self.0[addr as usize & (NAMETABLE_SIZE - 1)] = val;
    }
}

impl Memory for Palette {
    fn readb(&mut self, addr: Addr) -> Byte {
        let addr = Self::mirror_addr(addr);
        self.0[addr as usize]
    }
    fn writeb(&mut self, addr: Addr, val: Byte) {
        let addr = Self::mirror_addr(addr);
        self.0[addr as usize] = val;
    }
}

impl Ppu {
    fn tick(&mut self) {
        if self.rendering_enabled() {
            // Reached the end of a frame cycle
            // Jump to (0, 0) (Cycles, Scanline) and start on the next frame
            if self.frame.parity
                && self.scanline == PRERENDER_SCANLINE
                && self.cycle == PRERENDER_CYCLE_END
            {
                self.cycle = 0;
                self.scanline = 0;
                self.frame.increment();
                return;
            }
        }

        self.cycle += 1;
        if self.cycle > VISIBLE_CYCLE_END {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > PRERENDER_SCANLINE {
                self.scanline = 0;
                self.frame.increment();
            }
        }
    }

    // http://wiki.nesdev.com/w/index.php/PPU_OAM
    fn get_sprite(&mut self, i: usize) -> Sprite {
        // Get sprite info from OAMDATA
        // Each sprite takes 4 bytes
        let d = &mut self.oamdata;
        let addr = (i * 4) as Addr;
        // attribute
        // 76543210
        // ||||||||
        // ||||||++- Palette (4 to 7) of sprite
        // |||+++--- Unimplemented
        // ||+------ Priority (0: in front of background; 1: behind background)
        // |+------- Flip sprite horizontally
        // +-------- Flip sprite vertically
        let attr = d.readb(addr + 2);
        Sprite {
            index: i as Byte,
            x: d.readb(addr + 3),
            y: d.readb(addr),
            tile_index: d.readb(addr + 1),
            palette: (attr & 3) + 4, // range 4 to 7
            pattern: 0,
            has_priority: (attr & 0x20) > 0,    // bit 5
            flip_horizontal: (attr & 0x40) > 0, // bit 6
            flip_vertical: (attr & 0x80) > 0,   // bit 7
        }
    }

    fn get_sprite_pattern(&mut self, sprite: &Sprite) -> Option<Word> {
        // TODO explain these steps better
        let mut row = self.scanline as i16 - i16::from(sprite.y);
        let height = i16::from(self.regs.ctrl.sprite_height());
        // If top of sprite is below our scanline, or scanline is below
        // the bottom of the sprite, just return None since we're not rendering
        // during this cycle
        if row < 0 || row >= height {
            return None;
        }
        if sprite.flip_vertical {
            row = height - 1 - row;
        }
        if row >= 8 {
            row -= 8;
        }
        let is_sprite = true;
        let (addr_lo, addr_hi) = self.get_pattern_rows(is_sprite, sprite.tile_index, row as Byte);
        let mut row0 = self.readb(addr_lo);
        let mut row1 = self.readb(addr_hi);
        if sprite.flip_horizontal {
            row0 = reverse_bits(row0);
            row1 = reverse_bits(row1);
        }
        Some(combine_bitplanes(row0, row1))
    }

    // Returns a system RGB color by index
    fn get_system_color(&self, palette_idx: Byte) -> Rgb {
        SYSTEM_PALETTE[palette_idx as usize & (SYSTEM_PALETTE_SIZE - 1)]
    }

    fn rendering_enabled(&self) -> bool {
        self.regs.mask.show_background() || self.regs.mask.show_sprites()
    }

    fn get_bg_pixel(&mut self, x: Byte) -> Option<Rgb> {
        None
    }

    fn get_sprite_pixel(&mut self, x: Byte) -> Option<Rgb> {
        None
    }

    // Register read/writes

    /*
     * PPUCTRL
     */

    fn vblank_nmi(&self) -> bool {
        self.regs.ctrl.vblank_nmi()
    }
    fn write_ppuctrl(&mut self, val: Byte) {
        // Read PPUSTATUS to clear vblank before setting vblank again
        // FIXME: Is this the correct thing to do?
        // http://wiki.nesdev.com/w/index.php/PPU_programmer_reference#PPUCTRL
        if val & 0x80 > 0 {
            self.read_ppustatus();
        }
        let nn_mask = 0x0C00;
        self.regs.write_ctrl(val);
    }

    /*
     * PPUMASK
     */

    fn write_ppumask(&mut self, val: Byte) {
        self.regs.mask.write(val);
    }

    /*
     * PPUSTATUS
     */

    fn read_ppustatus(&mut self) -> Byte {
        self.regs.read_status()
    }
    fn set_sprite_zero_hit(&mut self, val: bool) {
        self.regs.status.set_sprite_zero_hit(val);
    }
    fn set_sprite_overflow(&mut self, val: bool) {
        self.regs.status.set_sprite_overflow(val);
    }
    fn set_vblank(&mut self, val: bool) {
        self.regs.status.set_vblank(val);
    }

    /*
     * OAMADDR
     */

    fn read_oamaddr(&mut self) -> Byte {
        self.oamdata.readb(Addr::from(self.regs.oamaddr))
    }
    fn write_oamaddr(&mut self, val: Byte) {
        self.regs.oamaddr = val;
    }

    /*
     * OAMDATA
     */

    fn read_oamdata(&mut self) -> Byte {
        self.oamdata.readb(Addr::from(self.regs.oamaddr))
    }
    fn write_oamdata(&mut self, val: Byte) {
        self.oamdata.writeb(Addr::from(self.regs.oamaddr), val);
        self.regs.oamaddr = self.regs.oamaddr.wrapping_add(1);
    }

    /*
     * PPUSCROLL
     */

    fn write_ppuscroll(&mut self, val: Byte) {
        self.regs.write_scroll(val);
    }

    /*
     * PPUADDR
     */

    fn read_ppuaddr(&self) -> Addr {
        self.regs.v
    }
    fn write_ppuaddr(&mut self, val: Byte) {
        self.regs.write_addr(val);
    }

    /*
     * PPUDATA
     */

    fn read_ppudata(&mut self) -> Byte {
        let val = self.vram.readb(self.read_ppuaddr());
        self.regs.increment_v();
        val
    }
    fn write_ppudata(&mut self, val: Byte) {
        self.vram.writeb(self.read_ppuaddr(), val);
        self.regs.increment_v();
    }
}

// http://wiki.nesdev.com/w/index.php/PPU_registers#PPUCTRL
// VPHB SINN
// |||| ||++- Nametable Select: 0 = $2000 (upper-left); 1 = $2400 (upper-right);
// |||| ||                      2 = $2800 (lower-left); 3 = $2C00 (lower-right)
// |||| |||+-   Also For PPUSCROLL: 1 = Add 256 to X scroll
// |||| ||+--   Also For PPUSCROLL: 1 = Add 240 to Y scroll
// |||| |+--- VRAM Increment Mode: 0 = add 1, going across; 1 = add 32, going down
// |||| +---- Sprite Pattern Select for 8x8: 0 = $0000, 1 = $1000, ignored in 8x16 mode
// |||+------ Background Pattern Select: 0 = $0000, 1 = $1000
// ||+------- Sprite Height: 0 = 8x8, 1 = 8x16
// |+-------- PPU Master/Slave: 0 = read from EXT, 1 = write to EXT
// +--------- NMI Enable: NMI at next vblank: 0 = off, 1: on
impl PpuCtrl {
    fn write(&mut self, val: Byte) {
        self.0 = val;
    }

    fn x_scroll_offset(&self) -> Word {
        if self.0 & 0x01 > 0 {
            255
        } else {
            0
        }
    }
    fn y_scroll_offset(&self) -> Word {
        if self.0 & 0x02 > 0 {
            239
        } else {
            0
        }
    }
    fn vram_increment(&self) -> Word {
        if self.0 & 0x04 > 0 {
            32
        } else {
            1
        }
    }
    fn sprite_select(&self) -> Addr {
        if self.0 & 0x08 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    fn background_select(&self) -> Addr {
        if self.0 & 0x10 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    fn sprite_height(&self) -> Byte {
        if self.0 & 0x20 > 0 {
            16
        } else {
            8
        }
    }
    fn master_select(&self) -> bool {
        self.0 & 0x40 > 0
    }
    fn vblank_nmi(&self) -> bool {
        self.0 & 0x80 > 0
    }
}

// http://wiki.nesdev.com/w/index.php/PPU_registers#PPUMASK
// BGRs bMmG
// |||| |||+- Greyscale (0: normal color, 1: produce a greyscale display)
// |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
// |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
// |||| +---- 1: Show background
// |||+------ 1: Show sprites
// ||+------- Emphasize red
// |+-------- Emphasize green
// +--------- Emphasize blue
impl PpuMask {
    fn write(&mut self, val: Byte) {
        self.0 = val;
    }

    fn grayscale(&self) -> bool {
        self.0 & 0x01 > 0
    }
    fn show_left_background(&self) -> bool {
        self.0 & 0x02 > 0
    }
    fn show_left_sprites(&self) -> bool {
        self.0 & 0x04 > 0
    }
    fn show_background(&self) -> bool {
        self.0 & 0x08 > 0
    }
    fn show_sprites(&self) -> bool {
        self.0 & 0x10 > 0
    }
    fn emphasize_red(&self) -> bool {
        self.0 & 0x20 > 0
    }
    fn emphasize_blue(&self) -> bool {
        self.0 & 0x40 > 0
    }
    fn emphasize_green(&self) -> bool {
        self.0 & 0x80 > 0
    }
}

// http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS
// VSO. ....
// |||+-++++- Least significant bits previously written into a PPU register
// ||+------- Sprite overflow.
// |+-------- Sprite 0 Hit.
// +--------- Vertical blank has started (0: not in vblank; 1: in vblank)
impl PpuStatus {
    pub fn read(&mut self) -> Byte {
        let nmi_occurred = self.0 & 0x80;
        self.0 &= !0x80;
        self.0 | nmi_occurred
    }

    pub fn write(&mut self, val: Byte) {
        self.0 = val;
    }

    fn sprite_overflow(&mut self) -> bool {
        self.0 & 0x20 > 0
    }
    fn set_sprite_overflow(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x20 } else { self.0 & 0x20 }
    }

    fn sprite_zero_hit(&mut self) -> bool {
        self.0 & 0x40 > 0
    }
    fn set_sprite_zero_hit(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x40 } else { self.0 & 0x40 }
    }

    fn vblank(&mut self) -> bool {
        self.0 & 0x80 > 0
    }
    fn set_vblank(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x80 } else { self.0 & 0x80 }
    }
}

impl fmt::Debug for Oam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Oam {{ entries: {} bytes }}", OAM_SIZE)
    }
}

impl fmt::Debug for Screen {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Screen {{ pixels: {} bytes }}", PIXEL_COUNT)
    }
}

impl fmt::Debug for Nametable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Nametable {{ size: {}KB }}", NAMETABLE_SIZE / KILOBYTE)
    }
}

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Palette {{ size: {} }}", PALETTE_SIZE)
    }
}

impl Deref for PpuCtrl {
    type Target = Byte;
    fn deref(&self) -> &Byte {
        &self.0
    }
}
impl DerefMut for PpuCtrl {
    fn deref_mut(&mut self) -> &mut Byte {
        &mut self.0
    }
}
impl Deref for PpuMask {
    type Target = Byte;
    fn deref(&self) -> &Byte {
        &self.0
    }
}
impl DerefMut for PpuMask {
    fn deref_mut(&mut self) -> &mut Byte {
        &mut self.0
    }
}
impl Deref for PpuStatus {
    type Target = Byte;
    fn deref(&self) -> &Byte {
        &self.0
    }
}
impl DerefMut for PpuStatus {
    fn deref_mut(&mut self) -> &mut Byte {
        &mut self.0
    }
}

// 64 total possible colors, though only 32 can be loaded at a time
#[rustfmt::skip]
const SYSTEM_PALETTE: [Rgb; SYSTEM_PALETTE_SIZE] = [
    // 0x00
    Rgb(124, 124, 124), Rgb(0, 0, 252),     Rgb(0, 0, 188),     Rgb(68, 40, 188),   // $00-$04
    Rgb(148, 0, 132),   Rgb(168, 0, 32),    Rgb(168, 16, 0),    Rgb(136, 20, 0),    // $05-$08
    Rgb(80, 48, 0),     Rgb(0, 120, 0),     Rgb(0, 104, 0),     Rgb(0, 88, 0),      // $09-$0B
    Rgb(0, 64, 88),     Rgb(0, 0, 0),       Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $0C-$0F
    // 0x10
    Rgb(188, 188, 188), Rgb(0, 120, 248),   Rgb(0, 88, 248),    Rgb(104, 68, 252),  // $10-$14
    Rgb(216, 0, 204),   Rgb(228, 0, 88),    Rgb(248, 56, 0),    Rgb(228, 92, 16),   // $15-$18
    Rgb(172, 124, 0),   Rgb(0, 184, 0),     Rgb(0, 168, 0),     Rgb(0, 168, 68),    // $19-$1B
    Rgb(0, 136, 136),   Rgb(0, 0, 0),       Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $1C-$1F
    // 0x20
    Rgb(248, 248, 248), Rgb(60,  188, 252), Rgb(104, 136, 252), Rgb(152, 120, 248), // $20-$24
    Rgb(248, 120, 248), Rgb(248, 88, 152),  Rgb(248, 120, 88),  Rgb(252, 160, 68),  // $25-$28
    Rgb(248, 184, 0),   Rgb(184, 248, 24),  Rgb(88, 216, 84),   Rgb(88, 248, 152),  // $29-$2B
    Rgb(0, 232, 216),   Rgb(120, 120, 120), Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $2C-$2F
    // 0x30
    Rgb(252, 252, 252), Rgb(164, 228, 252), Rgb(184, 184, 248), Rgb(216, 184, 248), // $30-$34
    Rgb(248, 184, 248), Rgb(248, 164, 192), Rgb(240, 208, 176), Rgb(252, 224, 168), // $35-$38
    Rgb(248, 216, 120), Rgb(216, 248, 120), Rgb(184, 248, 184), Rgb(184, 248, 216), // $39-$3B
    Rgb(0, 252, 252),   Rgb(216, 216, 216), Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $3C-$3F
];

// Zips together two bit strings interlacing them
fn combine_bitplanes(mut a: u8, mut b: u8) -> u16 {
    let mut out = 0u16;
    for i in 0..8 {
        out |= u16::from((a & 1) << 1 | (b & 1)) << (i * 2);
        a >>= 1;
        b >>= 1;
    }
    out
}

fn reverse_bits(mut a: u8) -> u8 {
    let mut out = 0u8;
    for i in 0..8 {
        out <<= 1;
        out |= a & 1;
        a >>= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::cartridge::Cartridge;

    #[test]
    fn test_ppu_scrolling_registers() {
        // Dummy rom just to get cartridge vram loaded
        let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
        let mut ppu = Ppu::new();
        let board = Cartridge::new(rom).unwrap().load_board().unwrap();
        ppu.set_board(board);

        let ppuctrl = 0x2000;
        let ppustatus = 0x2002;
        let ppuscroll = 0x2005;
        let ppuaddr = 0x2006;

        // Test write to ppuctrl
        let ctrl_write: Byte = 0b11; // Write two 1 bits
        let t_result: Addr = 0b11 << 10; // Make sure they're in the NN place of t
        ppu.writeb(ppuctrl, ctrl_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, 0);

        // Test read to ppustatus
        ppu.readb(ppustatus);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuscroll
        let scroll_write: Byte = 0b0111_1101;
        let t_result: Addr = 0b000_11_00000_01111;
        let x_result: Byte = 0b101;
        ppu.writeb(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuscroll
        let scroll_write: Byte = 0b0101_1110;
        let t_result: Addr = 0b110_11_01011_01111;
        ppu.writeb(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuaddr
        let addr_write: Byte = 0b0011_1101;
        let t_result: Addr = 0b011_11_01011_01111;
        ppu.writeb(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuaddr
        let addr_write: Byte = 0b1111_0000;
        let t_result: Addr = 0b011_11_01111_10000;
        ppu.writeb(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, false);

        // Test a 2006/2005/2005/2006 write
        // http://forums.nesdev.com/viewtopic.php?p=78593#p78593
        ppu.writeb(ppuaddr, 0b0000_1000); // nametable select $10
        ppu.writeb(ppuscroll, 0b0100_0101); // $01 hi bits coarse Y scroll, $101 fine Y scroll
        ppu.writeb(ppuscroll, 0b0000_0011); // $011 fine X scroll
        ppu.writeb(ppuaddr, 0b1001_0110); // $100 lo bits coarse Y scroll, $10110 coarse X scroll
        let t_result: Addr = 0b101_10_01100_10110;
        assert_eq!(ppu.regs.v, t_result);
    }
}
