//! Picture Processing Unit
//!
//! http://wiki.nesdev.com/w/index.php/PPU

use crate::console::cartridge::{Board, Mirroring};
use crate::console::cpu::Cycle;
use crate::console::memory::{Addr, Byte, Memory, Ram, Word, KILOBYTE};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

// Screen/Render
pub const SCREEN_WIDTH: usize = 256;
pub const SCREEN_HEIGHT: usize = 240;
pub const RENDER_SIZE: usize = PIXEL_COUNT * 3;
const PIXEL_COUNT: usize = SCREEN_HEIGHT * SCREEN_WIDTH;

// Sizes
const NAMETABLE_SIZE: usize = 2 * KILOBYTE; // two 1K nametables
const PALETTE_SIZE: usize = 32;
const SYSTEM_PALETTE_SIZE: usize = 64;
const OAM_SIZE: usize = 64 * 4; // 64 entries * 4 bytes each

// Cycles
const VISIBLE_CYCLE_START: Cycle = 1;
const VISIBLE_CYCLE_END: Cycle = 256;
const SPRITE_PREFETCH_CYCLE_START: Cycle = 257;
const COPY_Y_CYCLE_START: Cycle = 280;
const COPY_Y_CYCLE_END: Cycle = 304;
const SPRITE_PREFETCH_CYCLE_END: Cycle = 320;
const PREFETCH_CYCLE_START: Cycle = 321;
const PREFETCH_CYCLE_END: Cycle = 336;
const PRERENDER_CYCLE_END: Cycle = 340;
const VISIBLE_SCANLINE_CYCLE_END: Cycle = 340;

// Scanlines
const VISIBLE_SCANLINE_START: Word = 0;
const VISIBLE_SCANLINE_END: Word = 239;
const POSTRENDER_SCANLINE: Word = 240;
const VBLANK_SCANLINE: Word = 241;
const PRERENDER_SCANLINE: Word = 261;

// PPUSCROLL masks
// yyy NN YYYYY XXXXX
// ||| || ||||| +++++- 5 bit coarse X
// ||| || +++++------- 5 bit coarse Y
// ||| |+------------- Nametable X offset
// ||| +-------------- Nametable Y offset
// +++---------------- 3 bit fine Y
const COARSE_X_MASK: Word = 0x001F;
const COARSE_Y_MASK: Word = 0x03E0;
const NAMETABLE_X_MASK: Word = 0x0400;
const NAMETABLE_Y_MASK: Word = 0x0800;
const FINE_Y_MASK: Word = 0x7000;
const VRAM_ADDR_SIZE_MASK: Word = 0x7FFF; // 15 bits
const X_MAX_COL: Word = 31; // last column of tiles - 255 pixel width / 8 pixel wide tiles
const Y_MAX_COL: Word = 29; // last row of tiles - (240 pixel height / 8 pixel tall tiles) - 1
const Y_OVER_COL: Word = 31; // overscan row

// Nametable ranges
// $2000 upper-left corner, $2400 upper-right, $2800 lower-left, $2C00 lower-right
const NAMETABLE_START: Addr = 0x2000;
const NAMETABLE_END: Addr = 0x2FBF;
const ATTRIBUTE_START: Addr = 0x23C0; // Attributes for NAMETABLEs
const ATTRIBUTE_END: Addr = 0x2FFF;
const PALETTE_START: Addr = 0x3F00;

#[derive(Debug)]
pub struct Ppu {
    cycle: Cycle,   // (0, 340) 341 cycles happen per scanline
    scanline: Word, // (0, 261) 262 total scanlines per frame
    regs: PpuRegs,  // Registers
    oamdata: Oam,   // $2004 OAMDATA read/write - Object Attribute Memory for Sprites
    vram: Vram,     // $2007 PPUDATA
    frame: Frame,   // Frame data keeps track of data and shift registers between frames
    screen: Screen, // The main screen holding pixel data
}

#[derive(Debug)]
pub struct PpuRegs {
    open_bus: Byte,     // This open bus gets set during any write to PPU registers
    ctrl: PpuCtrl,      // $2000 PPUCTRL write-only
    mask: PpuMask,      // $2001 PPUMASK write-only
    status: PpuStatus,  // $2002 PPUSTATUS read-only
    oamaddr: Byte,      // $2003 OAMADDR write-only
    nmi_delay: Byte,    // Some games need a delay after vblank before nmi is triggered
    nmi_previous: bool, // Keeps track of repeated nmi to handle delay timing
    buffer: Byte,       // PPUDATA buffer
    v: Addr,            // $2006 PPUADDR write-only 2x 15 bits: yyy NN YYYYY XXXXX
    t: Addr,            // Temporary v - Also the addr of top-left onscreen tile
    x: Byte,            // Fine X
    w: bool,            // 1st or 2nd write toggle
}

#[derive(Debug)]
struct Vram {
    board: Option<Arc<Mutex<Board>>>,
    nametable: Nametable, // Used to layout backgrounds on the screen
    palette: Palette,     // Background/Sprite color palettes
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
    tile_lo: Byte,
    tile_hi: Byte,
    // tile data - stored in cycles 0 mod 8
    nametable: Byte,
    attribute: Byte,
    tile_data: u64,
    // sprite data
    sprite_count: Byte,
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
    pattern: u32,
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
    pub trigger_nmi: bool,
    pub trigger_irq: bool,
}

impl Ppu {
    pub fn new() -> Self {
        let mut ppu = Self {
            cycle: 0,
            scanline: 0,
            regs: PpuRegs::new(),
            oamdata: Oam::new(),
            vram: Vram::new(),
            frame: Frame::new(),
            screen: Screen::new(),
        };
        ppu.reset();
        ppu
    }

    pub fn reset(&mut self) {
        self.cycle = 340;
        self.scanline = 240;
        self.frame.num = 0;
        self.write_ppuctrl(0);
        self.write_ppumask(0);
        self.write_oamaddr(0);
    }

    // Puts the Cartridge board into VRAM
    pub fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.vram.set_board(board);
    }

    // Step ticks as many cycles as needed to reach
    // target cycle to syncronize with the CPU
    // http://wiki.nesdev.com/w/index.php/PPU_rendering
    pub fn step(&mut self, cycles_to_run: Cycle) -> StepResult {
        let mut step_result = StepResult::new();
        for _ in 0..cycles_to_run {
            if (self.regs.nmi_delay > 0) {
                self.regs.nmi_delay -= 1;
                if self.regs.nmi_delay == 0 && self.nmi_enable() && self.vblank_started() {
                    step_result.trigger_nmi = true;
                }
            }

            self.tick();
            self.render_scanline();
            if self.cycle == 1 {
                if self.scanline == PRERENDER_SCANLINE {
                    // Dummy scanline - set up tiles for next scanline
                    step_result.new_frame = true;
                    self.stop_vblank();
                    self.set_sprite_zero_hit(false);
                    self.set_sprite_overflow(false);
                } else if self.scanline == VBLANK_SCANLINE {
                    self.start_vblank();
                }
            }
        }
        step_result
    }

    // Returns a fully rendered frame of RENDER_SIZE RGB colors
    pub fn render(&self) -> [Byte; RENDER_SIZE] {
        self.screen.render()
    }

    // Render a single frame scanline
    fn render_scanline(&mut self) {
        if self.rendering_enabled() {
            let visible_scanline = self.scanline <= VISIBLE_SCANLINE_END;
            let visible_cycle =
                self.cycle >= VISIBLE_CYCLE_START && self.cycle <= VISIBLE_CYCLE_END;
            let prerender_scanline = self.scanline == PRERENDER_SCANLINE;
            let render_scanline = prerender_scanline || visible_scanline;
            let prefetch_cycle =
                self.cycle >= PREFETCH_CYCLE_START && self.cycle <= PREFETCH_CYCLE_END;
            let fetch_cycle = prefetch_cycle || visible_cycle;

            // evaluate background
            let should_render = visible_scanline && visible_cycle;
            if should_render {
                self.render_pixel();
            }

            let should_fetch = render_scanline && fetch_cycle;
            if should_fetch {
                self.frame.tile_data <<= 4;
                // Fetch 4 tiles and write out shift registers every 8th cycle
                // Each tile fetch takes 2 cycles
                match self.cycle % 8 {
                    0 => self.store_tile(),
                    1 => self.fetch_bg_nametable(),
                    3 => self.fetch_bg_attribute(),
                    5 => self.fetch_bg_tile_lo(),
                    7 => self.fetch_bg_tile_hi(),
                    _ => (),
                }
            }
            // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
            // if rendering is enabled
            // http://wiki.nesdev.com/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
            if prerender_scanline
                && self.cycle >= COPY_Y_CYCLE_START
                && self.cycle <= COPY_Y_CYCLE_END
            {
                self.regs.copy_y();
            }

            if render_scanline {
                // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
                if fetch_cycle && self.cycle % 8 == 0 {
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
                if visible_scanline {
                    self.evaluate_sprites();
                } else {
                    self.frame.sprite_count = 0;
                }
            }
        }
    }

    fn evaluate_sprites(&mut self) {
        let mut count = 0;
        for i in 0..OAM_SIZE / 4 {
            let mut sprite = self.get_sprite(i * 4);
            let row = self.scanline as i16 - i16::from(sprite.y);
            let sprite_height = i16::from(self.regs.ctrl.sprite_height());

            // Sprite is outside of our range for evaluation
            if row < 0 || row >= sprite_height {
                continue;
            }
            if count < 8 {
                sprite.pattern = self.get_sprite_pattern(&sprite, row);
                self.frame.sprites[count] = sprite;
            }
            count += 1;
        }
        if count > 8 {
            count = 0;
            self.set_sprite_overflow(true);
        }
        self.frame.sprite_count = count as Byte;
    }

    fn render_pixel(&mut self) {
        let x = (self.cycle - 1) as Byte; // Because we called tick() before this
        let y = self.scanline as Byte;

        let mut bg_color = self.background_color(x);
        let (i, mut sprite_color) = self.sprite_color(x);
        // if sprite_color > 0 || bg_color > 0 {
        //     eprintln!("bg: {}, sp: {}", bg_color, sprite_color);
        // }

        if x < 8 && !self.regs.mask.show_background() {
            bg_color = 0;
        }
        if x < 8 && !self.regs.mask.show_sprites() {
            sprite_color = 0;
        }
        let bg_opaque = bg_color % 4 != 0;
        let sprite_opaque = sprite_color % 4 != 0;
        let color = if !bg_opaque && !sprite_opaque {
            0
        } else if sprite_opaque && !bg_opaque {
            sprite_color | 0x10
        } else if bg_opaque && !sprite_opaque {
            bg_color
        } else {
            if self.is_sprite_zero(i) && x < 255 {
                self.set_sprite_zero_hit(true);
            }
            if self.frame.sprites[i].has_priority {
                sprite_color | 0x10
            } else {
                bg_color
            }
        };
        let system_palette_idx =
            self.vram.readb(Addr::from(color) + PALETTE_START) % (SYSTEM_PALETTE_SIZE as Byte);
        // if color > 0 {
        //     eprintln!("color: {}, idx: {}", Addr::from(color), system_palette_idx);
        // }
        self.screen
            .put_pixel(x as usize, y as usize, system_palette_idx);
    }

    fn is_sprite_zero(&self, index: usize) -> bool {
        self.frame.sprites[index].index == 0
    }

    fn background_color(&mut self, x: Byte) -> Byte {
        if !self.regs.mask.show_background() {
            return 0;
        }
        // 43210
        // |||||
        // |||++- Pixel value from tile data
        // |++--- Palette number from attribute table or OAM
        // +----- Background/Sprite select

        // TODO Explain the bit shifting here more clearly
        let data = (self.frame.tile_data >> 32) >> ((7 - self.regs.x) * 4);
        (data & 0x0F) as Byte
    }

    fn sprite_color(&mut self, x: Byte) -> (usize, Byte) {
        if !self.regs.mask.show_sprites() {
            return (0, 0);
        }
        for i in 0..self.frame.sprite_count as usize {
            let offset = (self.cycle - 1) as i16 - i16::from(self.frame.sprites[i].x);
            if offset < 0 || offset > 7 {
                continue;
            }
            let offset = 7 - offset;
            let color = ((self.frame.sprites[i].pattern >> (offset * 4) as Byte) & 0x0F) as Byte;
            // eprintln!(
            //     "{}, {}, {}, {}",
            //     self.frame.sprites[i].x, self.frame.sprites[i].pattern, offset, color
            // );
            if color % 4 == 0 {
                continue;
            }
            return (i, color);
        }
        (0, 0)
    }

    fn store_tile(&mut self) {
        let mut data = 0u32;
        for i in 0..8 {
            let a = self.frame.attribute;
            let p1 = (self.frame.tile_lo & 0x80) >> 7;
            let p2 = (self.frame.tile_hi & 0x80) >> 6;
            self.frame.tile_lo <<= 1;
            self.frame.tile_hi <<= 1;
            data <<= 4;
            data |= u32::from(a | p1 | p2);
        }
        self.frame.tile_data = u64::from(data);
    }

    fn fetch_bg_nametable(&mut self) {
        let addr = NAMETABLE_START | (self.regs.v & 0x0FFF);
        self.frame.nametable = self.vram.readb(addr);
    }

    fn fetch_bg_attribute(&mut self) {
        let v = self.regs.v;
        let addr = ATTRIBUTE_START | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
        let shift = ((v >> 4) & 4) | (v & 2);
        self.frame.attribute = ((self.vram.readb(addr) >> shift) & 3) << 2;
    }

    fn fetch_bg_tile_lo(&mut self) {
        let fine_y = self.regs.fine_y();
        let bg_select = self.regs.ctrl.background_select();
        let tile = self.frame.nametable;
        let addr = bg_select + Addr::from(tile) * 16 + Addr::from(fine_y);
        self.frame.tile_lo = self.vram.readb(addr);
    }

    fn fetch_bg_tile_hi(&mut self) {
        let fine_y = self.regs.fine_y();
        let bg_select = self.regs.ctrl.background_select();
        let tile = self.frame.nametable;
        let addr = bg_select + Addr::from(tile) * 16 + Addr::from(fine_y);
        self.frame.tile_hi = self.vram.readb(addr + 8);
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
            nmi_delay: 0,
            nmi_previous: false,
            buffer: 0,
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
        let nn_mask = NAMETABLE_Y_MASK | NAMETABLE_X_MASK;
        // val: ......BA
        // t: ....BA.. ........
        self.t |= (self.t & !nn_mask) | (Addr::from(val) & 0x03) << 10; // take lo 2 bits and set NN
        self.ctrl.write(val);
        self.nmi_change();
    }

    fn nmi_change(&mut self) {
        let nmi = self.ctrl.nmi_enable() && self.status.vblank_started();
        if nmi && !self.nmi_previous {
            self.nmi_delay = 15;
        }
        self.nmi_previous = nmi;
    }

    /*
     * PPUSTATUS
     */

    fn read_status(&mut self) -> Byte {
        self.reset_rw();
        // Include garbage from open bus
        let status = self.status.read() | (self.open_bus & 0x1F);
        self.nmi_change();
        status
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
        let lo_5_bit_mask: Addr = 0x1F;
        let fine_mask: Addr = 0x07;
        let fine_rshift = 3;
        if !self.w {
            // Write X on first write
            // lo 3 bits goes into fine x, remaining 5 bits go into t for coarse x
            // val: HGFEDCBA
            // t: ........ ...HGFED
            // x:               CBA
            self.t &= !COARSE_X_MASK; // Empty coarse X
            self.t |= (val >> fine_rshift) & lo_5_bit_mask; // Set coarse X
            self.x = (val & fine_mask) as Byte; // Set fine X
        } else {
            // Write Y on second write
            // lo 3 bits goes into fine y, remaining 5 bits go into t for coarse y
            // val: HGFEDCBA
            // t: .CBA..HG FED.....
            let coarse_y_lshift = 5;
            let fine_y_lshift = 12;
            self.t &= !(FINE_Y_MASK | COARSE_Y_MASK); // Empty Y
            self.t |= (val & fine_mask) << fine_y_lshift; // Set fine Y
            self.t |= ((val >> fine_rshift) & lo_5_bit_mask) << coarse_y_lshift; // Set coarse Y
        }
        self.w = !self.w;
    }

    // Copy Coarse X from register t and add it to PPUADDR v
    fn copy_x(&mut self) {
        //    .....N.. ...XXXXX
        // t: .....F.. ...EDCBA
        // v: .....F.. ...EDCBA
        let x_mask = NAMETABLE_X_MASK | COARSE_X_MASK;
        self.v = (self.v & !x_mask) | (self.t & x_mask);
    }

    // Copy Fine y and Coarse Y from register t and add it to PPUADDR v
    fn copy_y(&mut self) {
        //    .yyyN.YY YYY.....
        // t: .IHGF.ED CBA.....
        // v: .IHGF.ED CBA.....
        let y_mask = FINE_Y_MASK | NAMETABLE_Y_MASK | COARSE_Y_MASK;
        self.v = (self.v & !y_mask) | (self.t & y_mask);
    }

    // Increment Coarse X
    // 0-4 bits are incremented, with overflow toggling bit 10 which switches the horizontal
    // nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    fn increment_x(&mut self) {
        // let v = self.v;
        // If we've reached the last column, toggle horizontal nametable
        if (self.v & COARSE_X_MASK) == X_MAX_COL {
            self.v = (self.v & !COARSE_X_MASK) ^ NAMETABLE_X_MASK; // toggles X nametable
        } else {
            self.v += 1;
            assert!(self.v <= VRAM_ADDR_SIZE_MASK); // TODO should be able to remove this
        }
        // eprintln!("DEBUG - COARSE-X {:x} {:x}", v, self.v);
    }

    // Increment Fine Y
    // Bits 12-14 are incremented for Fine Y, with overflow incrementing coarse Y in bits 5-9 with
    // overflow toggling bit 11 which switches the vertical nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    fn increment_y(&mut self) {
        if (self.v & FINE_Y_MASK) != FINE_Y_MASK {
            // If fine y < 7 (0b111), increment
            self.v += 0x1000;
        } else {
            self.v &= !FINE_Y_MASK; // set fine y = 0 and overflow into coarse y
            let mut y = (self.v & COARSE_Y_MASK) >> 5; // Get 5 bits of coarse y
            if y == Y_MAX_COL {
                y = 0;
                // switches vertical nametable
                self.v ^= NAMETABLE_Y_MASK;
            } else if y == Y_OVER_COL {
                // Out of bounds. Does not switch nametable
                // Some games use this
                y = 0;
            } else {
                y += 1; // increment coarse y
            }
            self.v = (self.v & !COARSE_Y_MASK) | (y << 5); // put coarse y back into v
        }
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
        let hi_bits_mask = 0x80FF;
        let lo_bits_mask = 0xFF00;
        let six_bits_mask = 0x003F;
        let hi_lshift = 8;
        if !self.w {
            // Write hi address on first write
            // val: ..FEDCBA
            //    FEDCBA98 76543210
            // t: .0FEDCBA ........
            self.t &= hi_bits_mask; // Empty bits 8-E
            self.t |= (val & six_bits_mask) << hi_lshift; // Set hi 6 bits 8-E
        } else {
            // Write lo address on second write
            // val: HGFEDCBA
            // t: ........ HGFEDCBA
            // v: t
            self.t = (self.t & lo_bits_mask) | val;
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
        }
    }

    fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.board = Some(board);
    }

    fn nametable_mirror_addr(&self, addr: Addr) -> Addr {
        let mirroring = if let Some(b) = &self.board {
            let board = b.lock().unwrap();
            board.mirroring()
        } else {
            Mirroring::Horizontal
        };

        let table_size = 0x0400; // Each nametable quandrant is 1K
        let mirror_lookup = match mirroring {
            Mirroring::Horizontal => [0, 0, 1, 1],
            Mirroring::Vertical => [0, 1, 0, 1],
            Mirroring::SingleScreenA => [0, 0, 0, 0],
            Mirroring::SingleScreenB => [1, 1, 1, 1],
            Mirroring::FourScreen => [1, 2, 3, 4],
        };

        let addr = (addr - NAMETABLE_START) % (NAMETABLE_SIZE as Addr);
        let table = addr / table_size;
        let offset = addr % table_size;

        NAMETABLE_START + mirror_lookup[table as usize] * table_size + offset
    }
}

impl Frame {
    fn new() -> Self {
        Self {
            num: 0,
            parity: false,
            nametable: 0,
            attribute: 0,
            tile_lo: 0,
            tile_hi: 0,
            tile_data: 0,
            sprite_count: 0,
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
            // [index * RGB size + color offset
            output[i * 3] = p.r();
            output[i * 3 + 1] = p.g();
            output[i * 3 + 2] = p.b();
        }
        output
    }

    fn put_pixel(&mut self, x: usize, y: usize, system_palette_idx: Byte) {
        if x < SCREEN_WIDTH && y < SCREEN_HEIGHT {
            let i = x + (y * SCREEN_WIDTH);
            self.pixels[i] = SYSTEM_PALETTE[system_palette_idx as usize];
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
    pub fn new() -> Self {
        Self {
            new_frame: false,
            trigger_nmi: false,
            trigger_irq: false,
        }
    }
}

impl Memory for Ppu {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr {
            0x2000 => self.regs.open_bus,    // PPUCTRL is write-only
            0x2001 => self.regs.open_bus,    // PPUMASK is write-only
            0x2002 => self.read_ppustatus(), // PPUSTATUS
            0x2003 => self.regs.open_bus,    // OAMADDR is write-only
            0x2004 => self.read_oamdata(),   // OAMDATA
            0x2005 => self.regs.open_bus,    // PPUSCROLL is write-only
            0x2006 => self.regs.open_bus,    // PPUADDR is write-only
            0x2007 => self.read_ppudata(),   // PPUDATA
            _ => {
                eprintln!("unhandled Ppu readb at 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        self.regs.open_bus = val;
        // Write least sig bits to ppustatus since they're not written to
        *self.regs.status |= val & 0x1F;
        match addr {
            0x2000 => self.write_ppuctrl(val),   // PPUCTRL
            0x2001 => self.write_ppumask(val),   // PPUMASK
            0x2002 => (),                        // PPUSTATUS is read-only
            0x2003 => self.write_oamaddr(val),   // OAMADDR
            0x2004 => self.write_oamdata(val),   // OAMDATA
            0x2005 => self.write_ppuscroll(val), // PPUSCROLL
            0x2006 => self.write_ppuaddr(val),   // PPUADDR
            0x2007 => self.write_ppudata(val),   // PPUDATA
            _ => eprintln!("unhandled Ppu readb at 0x{:04X}", addr),
        }
    }
}

impl Memory for Vram {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr {
            0x0000..=0x1FFF => {
                // CHR-ROM
                if let Some(b) = &self.board {
                    let mut board = b.lock().unwrap();
                    board.readb(addr)
                } else {
                    0
                }
            }
            0x2000..=0x3EFF => {
                let addr = self.nametable_mirror_addr(addr);
                self.nametable.readb(addr % 2048)
            }
            0x3F00..=0x3FFF => self.palette.readb(addr % 32),
            _ => panic!("invalid Vram readb at 0x{:04X}", addr),
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
            0x2000..=0x3EFF => {
                let addr = self.nametable_mirror_addr(addr);
                self.nametable.writeb(addr % 2048, val)
            }
            0x3F00..=0x3FFF => self.palette.writeb(addr % 32, val),
            _ => panic!("invalid Vram readb at 0x{:04X}", addr),
        }
    }
}

impl Memory for Oam {
    fn readb(&mut self, addr: Addr) -> Byte {
        self.entries[addr as usize]
    }
    fn writeb(&mut self, addr: Addr, val: Byte) {
        self.entries[addr as usize] = val;
    }
}

impl Memory for Nametable {
    fn readb(&mut self, addr: Addr) -> Byte {
        self.0[addr as usize]
    }
    fn writeb(&mut self, addr: Addr, val: Byte) {
        self.0[addr as usize] = val;
    }
}

impl Memory for Palette {
    fn readb(&mut self, mut addr: Addr) -> Byte {
        if addr >= 16 && addr % 4 == 0 {
            addr -= 16;
        }
        self.0[addr as usize]
    }
    fn writeb(&mut self, mut addr: Addr, val: Byte) {
        if addr >= 16 && addr % 4 == 0 {
            addr -= 16;
        }
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
        if self.cycle > VISIBLE_SCANLINE_CYCLE_END {
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
        let addr = i as Addr;
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

    fn get_sprite_pattern(&mut self, sprite: &Sprite, mut row: i16) -> u32 {
        // TODO explain these steps better
        let sprite_height = i16::from(self.regs.ctrl.sprite_height());
        if sprite.flip_vertical {
            row = sprite_height - 1 - row;
        }
        let addr = if sprite_height == 8 {
            let pattern_table = self.regs.ctrl.sprite_select();
            pattern_table + Word::from(sprite.tile_index) * 16 + row as Word
        } else {
            let pattern_table = 0x1000 * (Word::from(sprite.tile_index) & 0x01); // use bit 1 of tile index
            let mut tile_index = sprite.tile_index & 0xFE;
            if row >= 8 {
                tile_index += 1;
                row -= 8;
            }
            pattern_table + Word::from(tile_index) * 16 + row as Word
        };

        // Flip bits for horizontal flipping
        let a = (sprite.palette - 4) << 2;
        let mut lo_tile = self.vram.readb(addr);
        let mut hi_tile = self.vram.readb(addr + 8);
        // eprintln!("{}, {} => ({}, {})", a, addr, lo_tile, hi_tile);
        let mut pattern = 0u32;
        for i in 0..8 {
            let (mut p1, mut p2) = (0u8, 0u8);
            if sprite.flip_horizontal {
                p1 = lo_tile & 1;
                p2 = (hi_tile & 1) << 1;
                lo_tile >>= 1;
                hi_tile >>= 1;
            } else {
                p1 = (lo_tile & 0x80) >> 7;
                p2 = (hi_tile & 0x80) >> 6;
                lo_tile <<= 1;
                hi_tile <<= 1;
            }
            pattern <<= 4;
            pattern |= u32::from(a | p1 | p2);
        }
        pattern
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

    fn nmi_enable(&self) -> bool {
        self.regs.ctrl.nmi_enable()
    }
    fn write_ppuctrl(&mut self, val: Byte) {
        // Read PPUSTATUS to clear vblank before setting vblank again
        // FIXME: Is this the correct thing to do?
        // http://wiki.nesdev.com/w/index.php/PPU_programmer_reference#PPUCTRL
        if val & 0x80 > 0 {
            self.read_ppustatus();
        }
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
    fn start_vblank(&mut self) {
        self.regs.status.start_vblank();
        self.regs.nmi_change();
    }
    fn stop_vblank(&mut self) {
        self.regs.status.stop_vblank();
        self.regs.nmi_change();
    }
    fn vblank_started(&mut self) -> bool {
        self.regs.status.vblank_started()
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
        let val = self.vram.readb(self.regs.v);
        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in 0 - $3EFF
        // Keep addr within 15 bits
        let val = if self.regs.v <= 0x3EFF {
            let buffer = self.regs.buffer;
            self.regs.buffer = val;
            buffer
        } else {
            // TODO explain this
            self.regs.buffer = self.vram.readb(self.regs.v - 0x1000);
            val
        };
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

    fn nametable_select(&self) -> Byte {
        self.0 & 0x03
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
    fn nmi_enable(&self) -> bool {
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
    fn emphasize_green(&self) -> bool {
        self.0 & 0x40 > 0
    }
    fn emphasize_blue(&self) -> bool {
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
        let vblank_started = self.0 & 0x80;
        self.0 &= !0x80; // Set vblank to 0
        self.0 | vblank_started // return status with original vblank
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

    fn vblank_started(&mut self) -> bool {
        self.0 & 0x80 > 0
    }
    fn start_vblank(&mut self) {
        self.0 |= 0x80;
    }
    fn stop_vblank(&mut self) {
        self.0 |= !0x80;
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
