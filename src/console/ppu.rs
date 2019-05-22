//! Picture Processing Unit
//!
//! http://wiki.nesdev.com/w/index.php/PPU

#![allow(clippy::new_without_default)]

use crate::mapper::{MapperRef, Mirroring};
use crate::memory::Memory;
use std::fmt;
use std::ops::{Deref, DerefMut};

// Screen/Render
pub type Image = [u8; RENDER_SIZE];
pub const SCREEN_WIDTH: usize = 256;
pub const SCREEN_HEIGHT: usize = 240;
pub const RENDER_SIZE: usize = PIXEL_COUNT * 3;
const PIXEL_COUNT: usize = SCREEN_HEIGHT * SCREEN_WIDTH;

// Sizes
const NAMETABLE_SIZE: usize = 2 * 1024; // two 1K nametables
const PALETTE_SIZE: usize = 32;
const SYSTEM_PALETTE_SIZE: usize = 64;
const OAM_SIZE: usize = 64 * 4; // 64 entries * 4 bytes each

// Cycles
const VISIBLE_CYCLE_START: u64 = 1;
const VISIBLE_CYCLE_END: u64 = 256;
const SPRITE_PREFETCH_CYCLE_START: u64 = 257;
const COPY_Y_CYCLE_START: u64 = 280;
const COPY_Y_CYCLE_END: u64 = 304;
const PREFETCH_CYCLE_START: u64 = 321;
const PREFETCH_CYCLE_END: u64 = 336;
const PRERENDER_CYCLE_END: u64 = 340;
const VISIBLE_SCANLINE_CYCLE_END: u64 = 340;

// Scanlines
const VISIBLE_SCANLINE_END: u16 = 239;
const VBLANK_SCANLINE: u16 = 241;
const PRERENDER_SCANLINE: u16 = 261;

// PPUSCROLL masks
// yyy NN YYYYY XXXXX
// ||| || ||||| +++++- 5 bit coarse X
// ||| || +++++------- 5 bit coarse Y
// ||| |+------------- Nametable X offset
// ||| +-------------- Nametable Y offset
// +++---------------- 3 bit fine Y
const COARSE_X_MASK: u16 = 0x001F;
const COARSE_Y_MASK: u16 = 0x03E0;
const NAMETABLE_X_MASK: u16 = 0x0400;
const NAMETABLE_Y_MASK: u16 = 0x0800;
const FINE_Y_MASK: u16 = 0x7000;
const VRAM_ADDR_SIZE_MASK: u16 = 0x7FFF; // 15 bits
const X_MAX_COL: u16 = 31; // last column of tiles - 255 pixel width / 8 pixel wide tiles
const Y_MAX_COL: u16 = 29; // last row of tiles - (240 pixel height / 8 pixel tall tiles) - 1
const Y_OVER_COL: u16 = 31; // overscan row

// Nametable ranges
// $2000 upper-left corner, $2400 upper-right, $2800 lower-left, $2C00 lower-right
const NAMETABLE_START: u16 = 0x2000;
const ATTRIBUTE_START: u16 = 0x23C0; // Attributes for NAMETABLEs
const PALETTE_START: u16 = 0x3F00;

#[derive(Debug)]
pub struct Ppu {
    pub cycle: u64,    // (0, 340) 341 cycles happen per scanline
    pub scanline: u16, // (0, 261) 262 total scanlines per frame
    regs: PpuRegs,     // Registers
    oamdata: Oam,      // $2004 OAMDATA read/write - Object Attribute Memory for Sprites
    vram: Vram,        // $2007 PPUDATA
    frame: Frame,      // Frame data keeps track of data and shift registers between frames
    screen: Screen,    // The main screen holding pixel data
}

#[derive(Default, Debug)]
pub struct PpuRegs {
    open_bus: u8,       // This open bus gets set during any write to PPU registers
    ctrl: PpuCtrl,      // $2000 PPUCTRL write-only
    mask: PpuMask,      // $2001 PPUMASK write-only
    status: PpuStatus,  // $2002 PPUSTATUS read-only
    oamaddr: u8,        // $2003 OAMADDR write-only
    nmi_delay: u8,      // Some games need a delay after vblank before nmi is triggered
    nmi_previous: bool, // Keeps track of repeated nmi to handle delay timing
    v: u16,             // $2006 PPUADDR write-only 2x 15 bits: yyy NN YYYYY XXXXX
    t: u16,             // Temporary v - Also the addr of top-left onscreen tile
    x: u8,              // Fine X
    w: bool,            // 1st or 2nd write toggle
}

struct Vram {
    mapper: MapperRef,
    buffer: u8,           // PPUDATA buffer
    nametable: Nametable, // Used to layout backgrounds on the screen
    palette: Palette,     // Background/Sprite color palettes
}

// Addr Low Nibble
// $00, $04, $08, $0C   Sprite Y coord
// $01, $05, $09, $0D   Sprite tile #
// $02, $06, $0A, $0E   Sprite attribute
// $03, $07, $0B, $0F   Sprite X coord
struct Oam {
    entries: [u8; OAM_SIZE],
}

#[derive(Debug)]
struct Frame {
    num: u32,
    parity: bool,
    // Shift registers
    tile_lo: u8,
    tile_hi: u8,
    // tile data - stored in cycles 0 mod 8
    nametable: u8,
    attribute: u8,
    tile_data: u64,
    // sprite data
    sprite_count: u8,
    sprites: [Sprite; 8], // Each frame can only hold 8 sprites at a time
}

struct Screen {
    pixels: [Rgb; PIXEL_COUNT],
}

#[derive(Default, Debug, Copy, Clone)]
struct Sprite {
    index: u8,
    x: u8,
    y: u8,
    tile_index: u8,
    palette: u8,
    pattern: u32,
    has_priority: bool,
    flip_horizontal: bool,
    flip_vertical: bool,
}

#[derive(Default, Debug)]
struct PpuCtrl(u8);
#[derive(Default, Debug)]
struct PpuMask(u8);
#[derive(Default, Debug)]
struct PpuStatus(u8);

// http://wiki.nesdev.com/w/index.php/PPU_nametables
// http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
struct Nametable([u8; NAMETABLE_SIZE]);
// http://wiki.nesdev.com/w/index.php/PPU_palettes
struct Palette([u8; PALETTE_SIZE]);

#[derive(Default, Debug, Copy, Clone)]
struct Rgb(u8, u8, u8);

#[derive(Default, Debug)]
pub struct StepResult {
    pub new_frame: bool,
    pub trigger_nmi: bool,
    pub trigger_irq: bool,
}

impl Ppu {
    pub fn init(mapper: MapperRef) -> Self {
        Self {
            cycle: 0,
            scanline: 0,
            regs: PpuRegs::new(),
            oamdata: Oam::new(),
            vram: Vram::init(mapper),
            frame: Frame::new(),
            screen: Screen::new(),
        }
    }

    pub fn reset(&mut self) {
        self.cycle = 0;
        self.scanline = 0;
        self.frame.num = 0;
        self.write_ppuctrl(0);
        self.write_ppumask(0);
        self.write_oamaddr(0);
    }

    // Step ticks as many cycles as needed to reach
    // target cycle to syncronize with the CPU
    // http://wiki.nesdev.com/w/index.php/PPU_rendering
    pub fn clock(&mut self) -> StepResult {
        let mut step_result = StepResult::new();
        if self.regs.nmi_delay > 0 {
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
        step_result
    }

    // Returns a fully rendered frame of RENDER_SIZE RGB colors
    pub fn render(&self) -> Image {
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
                match self.cycle & 0x07 {
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
                if fetch_cycle && self.cycle.trailing_zeros() >= 3 {
                    self.regs.increment_x();
                }
                // Increment Fine Y when we reach the end of the screen
                if self.cycle == SCREEN_WIDTH as u64 {
                    self.regs.increment_y();
                }
                // Copy X bits at the start of a new line since we're going to start writing
                // new x values to t
                if self.cycle == (SCREEN_WIDTH + 1) as u64 {
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
        let sprite_height = i16::from(self.regs.ctrl.sprite_height());
        for i in 0..OAM_SIZE / 4 {
            let mut sprite = self.get_sprite(i * 4);
            let row = self.scanline as i16 - i16::from(sprite.y);
            // Sprite is outside of our scanline range for evaluation
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
            count = 8;
            self.set_sprite_overflow(true);
        }
        self.frame.sprite_count = count as u8;
    }

    fn render_pixel(&mut self) {
        let x = (self.cycle - 1) as u8; // Because we called tick() before this
        let y = self.scanline as u8;

        let mut bg_color = self.background_color();
        let (i, mut sprite_color) = self.sprite_color(x);

        if x < 8 && !self.regs.mask.show_background() {
            bg_color = 0;
        }
        if x < 8 && !self.regs.mask.show_sprites() {
            sprite_color = 0;
        }
        let bg_opaque = bg_color & 0x03 != 0;
        let sprite_opaque = sprite_color & 0x03 != 0;
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
            self.vram.readb(u16::from(color) + PALETTE_START) & ((SYSTEM_PALETTE_SIZE as u8) - 1);
        self.screen
            .put_pixel(x as usize, y as usize, system_palette_idx);
    }

    fn is_sprite_zero(&self, index: usize) -> bool {
        self.frame.sprites[index].index == 0
    }

    fn background_color(&mut self) -> u8 {
        if !self.regs.mask.show_background() {
            return 0;
        }
        // 43210
        // |||||
        // |||++- Pixel value from tile data
        // |++--- Palette number from attribute table or OAM
        // +----- Background/Sprite select

        // TODO Explain the bit shifting here more clearly
        let data = (self.frame.tile_data >> 32) as u32 >> ((7 - self.regs.fine_x()) * 4);
        (data & 0x0F) as u8
    }

    fn sprite_color(&mut self, x: u8) -> (usize, u8) {
        if !self.regs.mask.show_sprites() {
            return (0, 0);
        }
        for i in 0..self.frame.sprite_count as usize {
            let offset = i16::from(x) - i16::from(self.frame.sprites[i].x);
            if offset < 0 || offset > 7 {
                continue;
            }
            let offset = 7 - offset;
            let color = ((self.frame.sprites[i].pattern >> (offset * 4) as u8) & 0x0F) as u8;
            if color.trailing_zeros() >= 2 {
                continue;
            }
            return (i, color);
        }
        (0, 0)
    }

    fn store_tile(&mut self) {
        let mut data = 0u32;
        for _ in 0..8 {
            let a = self.frame.attribute;
            let p1 = (self.frame.tile_lo & 0x80) >> 7;
            let p2 = (self.frame.tile_hi & 0x80) >> 6;
            self.frame.tile_lo <<= 1;
            self.frame.tile_hi <<= 1;
            data <<= 4;
            data |= u32::from(a | p1 | p2);
        }
        self.frame.tile_data |= u64::from(data);
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
        let addr = bg_select + u16::from(tile) * 16 + u16::from(fine_y);
        self.frame.tile_lo = self.vram.readb(addr);
    }

    fn fetch_bg_tile_hi(&mut self) {
        let fine_y = self.regs.fine_y();
        let bg_select = self.regs.ctrl.background_select();
        let tile = self.frame.nametable;
        let addr = bg_select + u16::from(tile) * 16 + u16::from(fine_y);
        self.frame.tile_hi = self.vram.readb(addr + 8);
    }
}

impl PpuRegs {
    fn new() -> Self {
        Self {
            open_bus: 0,
            ctrl: PpuCtrl(0),
            mask: PpuMask(0),
            status: PpuStatus(0x80),
            oamaddr: 0,
            nmi_delay: 0,
            nmi_previous: false,
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

    fn write_ctrl(&mut self, val: u8) {
        let nn_mask = NAMETABLE_Y_MASK | NAMETABLE_X_MASK;
        // val: ......BA
        // t: ....BA.. ........
        self.t = (self.t & !nn_mask) | (u16::from(val) & 0x03) << 10; // take lo 2 bits and set NN
        self.ctrl.write(val);
        self.nmi_change();
    }

    fn nmi_change(&mut self) {
        let nmi = self.ctrl.nmi_enable() && self.status.vblank_started();
        if nmi && !self.nmi_previous {
            self.nmi_delay = 12;
        }
        self.nmi_previous = nmi;
    }

    /*
     * PPUSTATUS
     */

    fn read_status(&mut self) -> u8 {
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
    fn fine_x(&self) -> u8 {
        self.x
    }

    // Returns Fine Y: yyy from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn fine_y(&self) -> u8 {
        // Shift yyy over nametable, coarse y and x and return 3 bits
        ((self.v >> 12) & 0x7) as u8
    }

    // Writes val to PPUSCROLL
    // 1st write writes X
    // 2nd write writes Y
    fn write_scroll(&mut self, val: u8) {
        let val = u16::from(val);
        let lo_5_bit_mask: u16 = 0x1F;
        let fine_mask: u16 = 0x07;
        let fine_rshift = 3;
        if !self.w {
            // Write X on first write
            // lo 3 bits goes into fine x, remaining 5 bits go into t for coarse x
            // val: HGFEDCBA
            // t: ........ ...HGFED
            // x:               CBA
            self.t &= !COARSE_X_MASK; // Empty coarse X
            self.t |= (val >> fine_rshift) & lo_5_bit_mask; // Set coarse X
            self.x = (val & fine_mask) as u8; // Set fine X
        } else {
            // Write Y on second write
            // lo 3 bits goes into fine y, remaining 5 bits go into t for coarse y
            // val: HGFEDCBA
            // t: .CBA..HG FED.....
            let coarse_y_lshift = 5;
            let fine_y_lshift = 12;
            self.t &= !(FINE_Y_MASK | COARSE_Y_MASK); // Empty Y
            self.t |= ((val >> fine_rshift) & lo_5_bit_mask) << coarse_y_lshift; // Set coarse Y
            self.t |= (val & fine_mask) << fine_y_lshift; // Set fine Y
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

    fn read_addr(&self) -> u16 {
        self.v & 0x3FFF // Bits 0-14
    }

    // Write val to PPUADDR v
    // 1st write writes hi 6 bits
    // 2nd write writes lo 8 bits
    // Total size is a 14 bit addr
    fn write_addr(&mut self, val: u8) {
        let val = u16::from(val);
        if !self.w {
            // Write hi address on first write
            let hi_bits_mask = 0xC0FF;
            let hi_lshift = 8;
            let six_bits_mask = 0x003F;
            // val: ..FEDCBA
            //    FEDCBA98 76543210
            // t: 00FEDCBA ........
            self.t &= hi_bits_mask; // Empty bits 8-F
            self.t |= (val & six_bits_mask) << hi_lshift; // Set hi 6 bits 8-E
        } else {
            // Write lo address on second write
            let lo_bits_mask = 0xFF00;
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

impl Vram {
    fn init(mapper: MapperRef) -> Self {
        Self {
            mapper,
            buffer: 0,
            nametable: Nametable([0; NAMETABLE_SIZE]),
            palette: Palette([0; PALETTE_SIZE]),
        }
    }

    fn nametable_mirror_addr(&self, addr: u16) -> u16 {
        let mapper = self.mapper.borrow();
        let mirroring = mapper.mirroring();

        let table_size = 0x0400; // Each nametable quandrant is 1K
        let mirror_lookup = match mirroring {
            Mirroring::Horizontal => [0, 0, 1, 1],
            Mirroring::Vertical => [0, 1, 0, 1],
            Mirroring::SingleScreen0 => [0, 0, 0, 0],
            Mirroring::SingleScreen1 => [1, 1, 1, 1],
            Mirroring::FourScreen => [1, 2, 3, 4],
        };

        let addr = (addr - NAMETABLE_START) & ((NAMETABLE_SIZE as u16) - 1);
        let table = addr / table_size;
        let offset = addr & (table_size - 1);

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
    pub fn render(&self) -> Image {
        let mut output = [0; RENDER_SIZE];
        for i in 0..PIXEL_COUNT {
            let p = self.pixels[i];
            // index * RGB size + color offset
            output[i * 3] = p.r();
            output[i * 3 + 1] = p.g();
            output[i * 3 + 2] = p.b();
        }
        output
    }

    fn put_pixel(&mut self, x: usize, y: usize, system_palette_idx: u8) {
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
    fn r(self) -> u8 {
        self.0
    }
    fn g(self) -> u8 {
        self.1
    }
    fn b(self) -> u8 {
        self.2
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
    fn readb(&mut self, addr: u16) -> u8 {
        // TODO emulate decay of open bus bits
        let val = match addr {
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
        };
        self.regs.open_bus = val;
        val
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        // TODO emulate decay of open bus bits
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
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // CHR-ROM
                let mut mapper = self.mapper.borrow_mut();
                mapper.readb(addr)
            }
            0x2000..=0x3EFF => {
                let addr = self.nametable_mirror_addr(addr);
                self.nametable.readb(addr & 2047)
            }
            0x3F00..=0x3FFF => self.palette.readb(addr & 31),
            _ => {
                eprintln!("invalid Vram readb at 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                // CHR-ROM
                let mut mapper = self.mapper.borrow_mut();
                mapper.writeb(addr, val);
            }
            0x2000..=0x3EFF => {
                let addr = self.nametable_mirror_addr(addr);
                self.nametable.writeb(addr & 2047, val)
            }
            0x3F00..=0x3FFF => self.palette.writeb(addr & 31, val),
            _ => eprintln!("invalid Vram readb at 0x{:04X}", addr),
        }
    }
}

impl Memory for Oam {
    fn readb(&mut self, addr: u16) -> u8 {
        self.entries[addr as usize]
    }
    fn writeb(&mut self, addr: u16, val: u8) {
        self.entries[addr as usize] = val;
    }
}

impl Memory for Nametable {
    fn readb(&mut self, addr: u16) -> u8 {
        self.0[addr as usize]
    }
    fn writeb(&mut self, addr: u16, val: u8) {
        self.0[addr as usize] = val;
    }
}

impl Memory for Palette {
    fn readb(&mut self, mut addr: u16) -> u8 {
        if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr -= 16;
        }
        self.0[addr as usize]
    }
    fn writeb(&mut self, mut addr: u16, val: u8) {
        if addr >= 16 && addr.trailing_zeros() >= 2 {
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
        let addr = i as u16;
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
            index: i as u8,
            x: d.readb(addr + 3),
            y: d.readb(addr),
            tile_index: d.readb(addr + 1),
            palette: (attr & 3) + 4, // range 4 to 7
            pattern: 0,
            has_priority: (attr & 0x20) == 0,   // bit 5
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
            pattern_table + u16::from(sprite.tile_index) * 16 + row as u16
        } else {
            let pattern_table = 0x1000 * (u16::from(sprite.tile_index) & 0x01); // use bit 1 of tile index
            let mut tile_index = sprite.tile_index & 0xFE;
            if row >= 8 {
                tile_index += 1;
                row -= 8;
            }
            pattern_table + u16::from(tile_index) * 16 + row as u16
        };

        // Flip bits for horizontal flipping
        let a = (sprite.palette - 4) << 2;
        let mut lo_tile = self.vram.readb(addr);
        let mut hi_tile = self.vram.readb(addr + 8);
        let mut pattern = 0u32;
        for _ in 0..8 {
            let (p1, p2);
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

    fn rendering_enabled(&self) -> bool {
        self.regs.mask.show_background() || self.regs.mask.show_sprites()
    }

    // Register read/writes

    /*
     * PPUCTRL
     */

    fn nmi_enable(&self) -> bool {
        self.regs.ctrl.nmi_enable()
    }
    fn write_ppuctrl(&mut self, val: u8) {
        // Read PPUSTATUS to clear vblank before setting vblank again
        // FIXME: Is this the correct thing to do?
        // http://wiki.nesdev.com/w/index.php/PPU_programmer_reference#PPUCTRL
        if val & 0x80 == 0x80 {
            self.read_ppustatus();
        }
        self.regs.write_ctrl(val);
    }

    /*
     * PPUMASK
     */

    fn write_ppumask(&mut self, val: u8) {
        self.regs.mask.write(val);
    }

    /*
     * PPUSTATUS
     */

    fn read_ppustatus(&mut self) -> u8 {
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

    fn write_oamaddr(&mut self, val: u8) {
        self.regs.oamaddr = val;
    }

    /*
     * OAMDATA
     */

    fn read_oamdata(&mut self) -> u8 {
        self.oamdata.readb(u16::from(self.regs.oamaddr))
    }
    fn write_oamdata(&mut self, val: u8) {
        self.oamdata.writeb(u16::from(self.regs.oamaddr), val);
        self.regs.oamaddr = self.regs.oamaddr.wrapping_add(1);
    }

    /*
     * PPUSCROLL
     */

    fn write_ppuscroll(&mut self, val: u8) {
        self.regs.write_scroll(val);
    }

    /*
     * PPUADDR
     */

    fn read_ppuaddr(&self) -> u16 {
        self.regs.read_addr()
    }
    fn write_ppuaddr(&mut self, val: u8) {
        self.regs.write_addr(val);
    }

    /*
     * PPUDATA
     */

    fn read_ppudata(&mut self) -> u8 {
        let val = self.vram.readb(self.read_ppuaddr());
        // Buffering quirk resulting in a dummy read for the CPU
        // for reading pre-palette data in 0 - $3EFF
        // Keep addr within 15 bits
        let val = if self.read_ppuaddr() <= 0x3EFF {
            let buffer = self.vram.buffer;
            self.vram.buffer = val;
            buffer
        } else {
            // Set internal buffer with mirrors of nametable when reading palettes
            self.vram.buffer = self.vram.readb(self.read_ppuaddr() - 0x1000);
            val
        };
        self.regs.increment_v();
        val
    }
    fn write_ppudata(&mut self, val: u8) {
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
    fn write(&mut self, val: u8) {
        self.0 = val;
    }

    fn vram_increment(&self) -> u16 {
        if self.0 & 0x04 > 0 {
            32
        } else {
            1
        }
    }
    fn sprite_select(&self) -> u16 {
        if self.0 & 0x08 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    fn background_select(&self) -> u16 {
        if self.0 & 0x10 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    fn sprite_height(&self) -> u8 {
        if self.0 & 0x20 > 0 {
            16
        } else {
            8
        }
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
    fn write(&mut self, val: u8) {
        self.0 = val;
    }

    fn show_background(&self) -> bool {
        self.0 & 0x08 > 0
    }
    fn show_sprites(&self) -> bool {
        self.0 & 0x10 > 0
    }
}

// http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS
// VSO. ....
// |||+-++++- Least significant bits previously written into a PPU register
// ||+------- Sprite overflow.
// |+-------- Sprite 0 Hit.
// +--------- Vertical blank has started (0: not in vblank; 1: in vblank)
impl PpuStatus {
    pub fn read(&mut self) -> u8 {
        let vblank_started = self.0 & 0x80;
        self.0 &= !0x80; // Set vblank to 0
        self.0 | vblank_started // return status with original vblank
    }

    fn set_sprite_overflow(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x20 } else { self.0 & !0x20 };
    }

    fn set_sprite_zero_hit(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x40 } else { self.0 & !0x40 };
    }

    fn vblank_started(&mut self) -> bool {
        self.0 & 0x80 > 0
    }
    fn start_vblank(&mut self) {
        self.0 |= 0x80;
    }
    fn stop_vblank(&mut self) {
        self.0 &= !0x80;
    }
}

impl fmt::Debug for Oam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Oam {{ entries: {} bytes }}", OAM_SIZE)
    }
}

impl fmt::Debug for Vram {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Vram {{ }}")
    }
}

impl fmt::Debug for Screen {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Screen {{ pixels: {} bytes }}", PIXEL_COUNT)
    }
}

impl fmt::Debug for Nametable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Nametable {{ size: {}KB }}", NAMETABLE_SIZE / 1024)
    }
}

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Palette {{ size: {} }}", PALETTE_SIZE)
    }
}

impl Deref for PpuStatus {
    type Target = u8;
    fn deref(&self) -> &u8 {
        &self.0
    }
}
impl DerefMut for PpuStatus {
    fn deref_mut(&mut self) -> &mut u8 {
        &mut self.0
    }
}

// 64 total possible colors, though only 32 can be loaded at a time
#[rustfmt::skip]
const SYSTEM_PALETTE: [Rgb; SYSTEM_PALETTE_SIZE] = [
    // 0x00
    Rgb(84, 84, 84),    Rgb(0, 30, 116),    Rgb(8, 16, 144),    Rgb(48, 0, 136),    // $00-$04
    Rgb(68, 0, 100),    Rgb(92, 0, 48),     Rgb(84, 4, 0),      Rgb(60, 24, 0),     // $05-$08
    Rgb(32, 42, 0),     Rgb(8, 58, 0),      Rgb(0, 64, 0),      Rgb(0, 60, 0),      // $09-$0B
    Rgb(0, 50, 60),     Rgb(0, 0, 0),       Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $0C-$0F
    // 0x10                                                                                   
    Rgb(152, 150, 152), Rgb(8, 76, 196),    Rgb(48, 50, 236),   Rgb(92, 30, 228),   // $10-$14
    Rgb(136, 20, 176),  Rgb(160, 20, 100),  Rgb(152, 34, 32),   Rgb(120, 60, 0),    // $15-$18
    Rgb(84, 90, 0),     Rgb(40, 114, 0),    Rgb(8, 124, 0),     Rgb(0, 118, 40),    // $19-$1B
    Rgb(0, 102, 120),   Rgb(0, 0, 0),       Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $1C-$1F
    // 0x20                                                                                   
    Rgb(236, 238, 236), Rgb(76, 154, 236),  Rgb(120, 124, 236), Rgb(176, 98, 236),  // $20-$24
    Rgb(228, 84, 236),  Rgb(236, 88, 180),  Rgb(236, 106, 100), Rgb(212, 136, 32),  // $25-$28
    Rgb(160, 170, 0),   Rgb(116, 196, 0),   Rgb(76, 208, 32),   Rgb(56, 204, 108),  // $29-$2B
    Rgb(56, 180, 204),  Rgb(60, 60, 60),    Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $2C-$2F
    // 0x30                                                                                   
    Rgb(236, 238, 236), Rgb(168, 204, 236), Rgb(188, 188, 236), Rgb(212, 178, 236), // $30-$34
    Rgb(236, 174, 236), Rgb(236, 174, 212), Rgb(236, 180, 176), Rgb(228, 196, 144), // $35-$38
    Rgb(204, 210, 120), Rgb(180, 222, 120), Rgb(168, 226, 144), Rgb(152, 226, 180), // $39-$3B
    Rgb(160, 214, 228), Rgb(160, 162, 160), Rgb(0, 0, 0),       Rgb(0, 0, 0),       // $3C-$3F
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper;
    use std::path::PathBuf;

    #[test]
    fn test_ppu_scrolling_registers() {
        // Dummy rom just to get cartridge vram loaded
        let rom = PathBuf::from("roms/Zelda II - The Adventure of Link (USA).nes");
        let mapper = mapper::load_rom(rom).expect("loaded mapper");
        let mut ppu = Ppu::init(mapper);

        let ppuctrl = 0x2000;
        let ppustatus = 0x2002;
        let ppuscroll = 0x2005;
        let ppuaddr = 0x2006;

        // Test write to ppuctrl
        let ctrl_write: u8 = 0b11; // Write two 1 bits
        let t_result: u16 = 0b11 << 10; // Make sure they're in the NN place of t
        ppu.writeb(ppuctrl, ctrl_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, 0);

        // Test read to ppustatus
        ppu.readb(ppustatus);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuscroll
        let scroll_write: u8 = 0b0111_1101;
        let t_result: u16 = 0b000_11_00000_01111;
        let x_result: u8 = 0b101;
        ppu.writeb(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuscroll
        let scroll_write: u8 = 0b0101_1110;
        let t_result: u16 = 0b110_11_01011_01111;
        ppu.writeb(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuaddr
        let addr_write: u8 = 0b0011_1101;
        let t_result: u16 = 0b111_11_01011_01111;
        ppu.writeb(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuaddr
        let addr_write: u8 = 0b1111_0000;
        let t_result: u16 = 0b111_11_01111_10000;
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
        let t_result: u16 = 0b101_10_01100_10110;
        assert_eq!(ppu.regs.v, t_result);
    }
}
