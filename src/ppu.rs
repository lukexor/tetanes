//! Picture Processing Unit
//!
//! [http://wiki.nesdev.com/w/index.php/PPU]()

use crate::{
    common::{Clocked, Powered},
    debug,
    logging::{LogLevel, Loggable},
    mapper::{self, MapperRef, Mirroring},
    memory::Memory,
    serialization::Savable,
    NesResult,
};
use std::{
    collections::HashMap,
    f64::consts::PI,
    fmt,
    io::{Read, Write},
};

// Screen/Render
pub const RENDER_WIDTH: u32 = 256;
pub const RENDER_HEIGHT: u32 = 240;
const RENDER_PIXELS: usize = (RENDER_WIDTH * RENDER_HEIGHT) as usize;
const RENDER_SIZE: usize = 3 * RENDER_PIXELS;
const SIGNAL_SIZE: usize = 8 * RENDER_PIXELS;

// Sizes
const NT_SIZE: usize = 2 * 1024; // Two 1K nametables exist in hardware
const PALETTE_SIZE: usize = 32;
const SYSTEM_PALETTE_SIZE: usize = 64;
const OAM_SIZE: usize = 64 * 4; // 64 entries * 4 bytes each

// Cycles
const VISIBLE_CYCLE_START: u16 = 1;
const VISIBLE_CYCLE_END: u16 = 256;
const SPRITE_PREFETCH_CYCLE_START: u16 = 257;
const SPRITE_PREFETCH_CYCLE_END: u16 = 320;
const COPY_Y_CYCLE_START: u16 = 280;
const COPY_Y_CYCLE_END: u16 = 304;
const PREFETCH_CYCLE_START: u16 = 321;
const PREFETCH_CYCLE_END: u16 = 336;
const PRERENDER_CYCLE_END: u16 = 339;
const VISIBLE_SCANLINE_CYCLE_END: u16 = 340;

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
const NT_X_MASK: u16 = 0x0400;
const NT_Y_MASK: u16 = 0x0800;
const FINE_Y_MASK: u16 = 0x7000;
const X_MAX_COL: u16 = 31; // last column of tiles - 255 pixel width / 8 pixel wide tiles
const Y_MAX_COL: u16 = 29; // last row of tiles - (240 pixel height / 8 pixel tall tiles) - 1
const Y_OVER_COL: u16 = 31; // overscan row

// Nametable ranges
// $2000 upper-left corner, $2400 upper-right, $2800 lower-left, $2C00 lower-right
const NT_START: u16 = 0x2000;
const ATTRIBUTE_START: u16 = 0x23C0; // Attributes for NAMETABLEs
const PALETTE_START: u16 = 0x3F00;
const PALETTE_END: u16 = 0x3F20;

#[derive(Clone)]
pub struct Ppu {
    pub cycle: u16, // (0, 340) 341 cycles happen per scanline
    pub cycle_count: usize,
    pub scanline: u16,     // (0, 261) 262 total scanlines per frame
    pub nmi_pending: bool, // Whether the CPU should trigger an NMI next cycle
    pub vram: Vram,        // $2007 PPUDATA
    pub regs: PpuRegs,     // Registers
    oamdata: Oam,          // $2004 OAMDATA read/write - Object Attribute Memory for Sprites
    pub frame: Frame,      // Frame data keeps track of data and shift registers between frames
    pub frame_complete: bool,
    pub ntsc_video: bool,
    debug: bool,
    nt_scanline: u16,
    pat_scanline: u16,
    nametables: Vec<Vec<u8>>,
    pattern_tables: Vec<Vec<u8>>,
    palette: Vec<u8>,
    log_level: LogLevel,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycle: 0u16,
            cycle_count: 0usize,
            scanline: 0u16,
            nmi_pending: false,
            regs: PpuRegs::new(),
            oamdata: Oam::new(),
            vram: Vram::new(),
            frame: Frame::new(),
            frame_complete: false,
            ntsc_video: false,
            debug: false,
            nt_scanline: 0,
            pat_scanline: 0,
            nametables: vec![
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
            ],
            pattern_tables: vec![vec![0; RENDER_SIZE], vec![0; RENDER_SIZE]],
            palette: vec![0; (PALETTE_SIZE + 4) * 3],
            log_level: LogLevel::default(),
        }
    }

    pub fn load_mapper(&mut self, mapper: MapperRef) {
        self.vram.mapper = mapper;
    }

    pub fn set_debug(&mut self, val: bool) {
        self.debug = val;
    }

    pub fn set_nt_scanline(&mut self, scanline: u16) {
        self.nt_scanline = scanline;
    }

    pub fn set_pat_scanline(&mut self, scanline: u16) {
        self.pat_scanline = scanline;
    }

    pub fn update_debug(&mut self) {
        self.nametables = vec![
            self.load_nametable(NT_START),
            self.load_nametable(NT_START + 0x0400),
            self.load_nametable(NT_START + 0x0800),
            self.load_nametable(NT_START + 0x0C00),
        ];
        self.pattern_tables = vec![self.load_pattern_table(0), self.load_pattern_table(1)];
        self.palette = self.load_palette();
    }

    // Returns a fully rendered frame of RENDER_SIZE RGB colors
    pub fn frame(&mut self) -> &Vec<u8> {
        if self.ntsc_video {
            for y in 0..RENDER_HEIGHT as u16 {
                self.frame.decode_ntsc_signal(y);
            }
        }
        &self.frame.pixels
    }

    pub fn nametables(&self) -> &Vec<Vec<u8>> {
        &self.nametables
    }

    fn load_nametable(&self, base_addr: u16) -> Vec<u8> {
        let mut nametable = vec![0u8; RENDER_SIZE];
        for addr in base_addr..(base_addr + 0x0400 - 64) {
            let x_scroll = addr & COARSE_X_MASK;
            let y_scroll = (addr & COARSE_Y_MASK) >> 5;

            let nt_base_addr = NT_START + (addr & (NT_X_MASK | NT_Y_MASK));
            let tile_addr = self.regs.background_select() + u16::from(self.vram.peek(addr)) * 16;
            let supertile_num = (x_scroll / 4) + (y_scroll / 4) * 8;
            let attr = u16::from(self.vram.peek(nt_base_addr + 0x3C0 + supertile_num));
            let corner = ((x_scroll % 4) / 2 + (y_scroll % 4) / 2 * 2) << 1;
            let mask = 0x03 << corner;
            let palette = (attr & mask) >> corner;

            let tile_num = x_scroll + y_scroll * 32;
            let tile_x = (tile_num % 32) * 8;
            let tile_y = (tile_num / 32) * 8;

            self.fetch_and_put_tile(
                tile_addr,
                palette,
                tile_x,
                tile_y,
                RENDER_WIDTH,
                &mut nametable,
            );
        }
        nametable
    }

    pub fn pattern_tables(&self) -> &Vec<Vec<u8>> {
        &self.pattern_tables
    }

    fn load_pattern_table(&self, table: usize) -> Vec<u8> {
        let width = RENDER_WIDTH / 2;
        let height = width;
        let mut pat = vec![0u8; (width * height * 3) as usize];
        let start = table as u16 * 0x1000;
        let end = start + 0x1000;
        for tile_addr in (start..end).step_by(16) {
            let tile_x = ((tile_addr % 0x1000) % 256) / 2;
            let tile_y = ((tile_addr % 0x1000) / 256) * 8;
            self.fetch_and_put_tile(tile_addr, 0, tile_x, tile_y, width, &mut pat);
        }
        pat
    }

    fn fetch_and_put_tile(
        &self,
        addr: u16,
        palette: u16,
        tile_x: u16,
        tile_y: u16,
        width: u32,
        mut pixels: &mut Vec<u8>,
    ) {
        for y in 0..8 {
            let lo = u16::from(self.vram.peek(addr + y));
            let hi = u16::from(self.vram.peek(addr + y + 8));
            for x in 0..8 {
                let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                let palette_idx = self.vram.peek(PALETTE_START + palette * 4 + pix_type) as usize;
                let x = tile_x + (7 - x);
                let y = tile_y + y;
                Self::put_pixel(palette_idx, x.into(), y.into(), width, &mut pixels);
            }
        }
    }

    pub fn palette(&self) -> &Vec<u8> {
        &self.palette
    }

    fn load_palette(&self) -> Vec<u8> {
        // Global  // BG 0 ----------------------------------  // Unused    // SPR 0 -------------------------------
        // 0x3F00: 0,0  0x3F01: 1,0  0x3F02: 2,0  0x3F03: 3,0  0x3F10: 5,0  0x3F11: 6,0  0x3F12: 7,0  0x3F13: 8,0
        // Unused  // BG 1 ----------------------------------  // Unused    // SPR 1 -------------------------------
        // 0x3F04: 0,1  0x3F05: 1,1  0x3F06: 2,1  0x3F07: 3,1  0x3F14: 5,1  0x3F15: 6,1  0x3F16: 7,1  0x3F17: 8,1
        // Unused  // BG 2 ----------------------------------  // Unused    // SPR 2 -------------------------------
        // 0x3F08: 0,2  0x3F09: 1,2  0x3F0A: 2,2  0x3F0B: 3,2  0x3F18: 5,2  0x3F19: 6,2  0x3F1A: 7,2  0x3F1B: 8,2
        // Unused  // BG 3 ----------------------------------  // Unused    // SPR 3 -------------------------------
        // 0x3F0C: 0,3  0x3F0D: 1,3  0x3F0E: 2,3  0x3F0F: 3,3  0x3F1C: 5,3  0x3F1D: 6,3  0x3F1E: 7,3  0x3F1F: 8,3
        let mut palette = vec![0u8; 3 * PALETTE_SIZE];
        let width = 16;
        for addr in PALETTE_START..PALETTE_END {
            let x = (addr - PALETTE_START) % 16;
            let y = (addr - PALETTE_START) / 16;
            let palette_idx = self.vram.peek(addr) as usize;
            Self::put_pixel(palette_idx, x.into(), y.into(), width, &mut palette);
        }
        palette
    }

    fn render_dot(&mut self) {
        let visible_scanline = self.scanline <= VISIBLE_SCANLINE_END;
        let visible_cycle = self.cycle >= VISIBLE_CYCLE_START && self.cycle <= VISIBLE_CYCLE_END;
        let prerender_scanline = self.scanline == PRERENDER_SCANLINE;
        let render_scanline = prerender_scanline || visible_scanline;
        let prefetch_cycle = self.cycle >= PREFETCH_CYCLE_START && self.cycle <= PREFETCH_CYCLE_END;
        let fetch_cycle = prefetch_cycle || visible_cycle;

        // Pixels should be put even if rendering is disabled, as this is what blanks out the
        // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
        let should_render = visible_scanline && visible_cycle;
        if should_render {
            self.render_pixel();
        }

        if self.rendering_enabled() {
            // evaluate background
            let should_fetch = render_scanline && fetch_cycle;
            if should_fetch {
                self.evaluate_background();
            }

            // Two dummy byte fetches
            if render_scanline
                && self.cycle > PREFETCH_CYCLE_END
                && self.cycle <= VISIBLE_SCANLINE_CYCLE_END
                && self.cycle % 2 == 1
            {
                self.fetch_bg_nt_byte();
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
                if self.cycle == RENDER_WIDTH as u16 {
                    self.regs.increment_y();
                }
                // Copy X bits at the start of a new line since we're going to start writing
                // new x values to t
                if self.cycle == (RENDER_WIDTH + 1) as u16 {
                    self.regs.copy_x();
                }

                // TODO - This should be split up instead of being done all at once
                // The code block below this simulates the reads required, but
                // its not ideal
                if self.cycle == SPRITE_PREFETCH_CYCLE_START {
                    self.evaluate_sprites();
                }

                // This gets our IRQ timing properly for certain mappers (MMC3, MMC5)
                // because evaluation is done all on one cycle
                if self.cycle >= SPRITE_PREFETCH_CYCLE_START
                    && self.cycle <= SPRITE_PREFETCH_CYCLE_END
                {
                    let sprite_idx = (self.cycle - SPRITE_PREFETCH_CYCLE_START) / 8;
                    let sprite = self.frame.sprites[sprite_idx as usize];

                    match self.cycle % 8 {
                        1 => self.fetch_bg_nt_byte(),   // Garbage NT fetch
                        3 => self.fetch_bg_attr_byte(), // Garbage attr fetch
                        5 => {
                            let _ = self.vram.read(sprite.tile_addr);
                        }
                        7 => {
                            let _ = self.vram.read(sprite.tile_addr + 8);
                        }
                        _ => (),
                    }
                }
            }
        }
    }

    fn fetch_bg_nt_byte(&mut self) {
        // Fetch BG nametable
        // https://wiki.nesdev.com/w/index.php/PPU_scrolling#Tile_and_attribute_fetching
        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = NT_START | (self.regs.v & nametable_addr_mask);
        self.frame.nametable = u16::from(self.vram.read(addr));
    }

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
        let addr = ATTRIBUTE_START | nametable_select | y_bits | x_bits;
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

    fn evaluate_background(&mut self) {
        self.frame.tile_data <<= 4;
        // Fetch 4 tiles and write out shift registers every 8th cycle
        // Each tile fetch takes 2 cycles
        match self.cycle % 8 {
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
                self.frame.tile_data |= data;
            }
            _ => (),
        }
    }

    fn evaluate_sprites(&mut self) {
        self.frame.sprite_count = 0;
        for i in 0..8 {
            let mut sprite = Sprite::new();
            let sprite_height = self.regs.sprite_height();
            let sprite_table = if sprite_height == 8 {
                self.regs.sprite_select()
            } else {
                // use bit 1 of tile index to determine pattern table
                0x1000 * (sprite.tile_index & 0x01)
            };

            sprite.tile_addr = sprite_table + sprite.tile_index * 16;
            self.frame.sprites[i] = sprite;
        }
        let sprite_height = self.regs.sprite_height();
        for i in 0..OAM_SIZE / 4 {
            let sprite_y = u16::from(self.oamdata.read((i * 4) as u16));
            let sprite_on_line = sprite_y <= self.scanline
                && self.scanline < sprite_y + sprite_height
                && self.scanline < 255;
            if !sprite_on_line {
                continue;
            }
            if i == 0 {
                self.frame.sprite_zero_on_line = true;
            }
            if self.frame.sprite_count < 8 {
                self.frame.sprites[self.frame.sprite_count as usize] = self.get_sprite(i * 4);
            }
            self.frame.sprite_count += 1;
            if self.frame.sprite_count > 8 {
                self.frame.sprite_count = 8;
                self.set_sprite_overflow(true);
            }
        }
    }

    #[allow(clippy::many_single_char_names)]
    fn render_pixel(&mut self) {
        let x = self.cycle - 1; // Because we called tick() before this
        let y = self.scanline;

        let mut bg_color = self.background_color();
        let (i, mut sprite_color) = self.sprite_color();

        let border_pixel = x < 8;
        if border_pixel && !self.regs.show_left_background() {
            bg_color = 0;
        }
        if border_pixel && !self.regs.show_left_sprites() {
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
            if self.is_sprite_zero(i)
                && self.frame.sprite_zero_on_line
                && self.rendering_enabled()
                && !self.sprite_zero_hit()
                && self.frame.sprites[i].x < 255
                && x < 255
                && self.frame.sprites[i].y < 255
                && bg_opaque
                && sprite_opaque
            {
                self.set_sprite_zero_hit(true);
            }
            if !self.frame.sprites[i].has_priority {
                sprite_color | 0x10
            } else {
                bg_color
            }
        };
        let mut palette = self.vram.read(u16::from(color) + PALETTE_START);
        if self.regs.grayscale() {
            palette &= !0x0F; // Remove chroma
        }
        if self.ntsc_video {
            self.frame.render_ntsc_pixel(
                y * RENDER_WIDTH as u16 + x,
                palette & 0x3F,
                self.regs.emphasis(),
                self.cycle_count - 1,
            );
        } else {
            let color_idx = (palette as usize % SYSTEM_PALETTE_SIZE) * 3;
            let r = SYSTEM_PALETTE[color_idx];
            let g = SYSTEM_PALETTE[color_idx + 1];
            let b = SYSTEM_PALETTE[color_idx + 2];
            self.frame.put_pixel(x.into(), y.into(), r, g, b);
        }
    }

    fn put_pixel(palette_idx: usize, x: u32, y: u32, width: u32, pixels: &mut Vec<u8>) {
        let idx = (palette_idx % SYSTEM_PALETTE_SIZE) * 3;
        let red = SYSTEM_PALETTE[idx];
        let green = SYSTEM_PALETTE[idx + 1];
        let blue = SYSTEM_PALETTE[idx + 2];
        let idx = 3 * (x + y * width) as usize;
        pixels[idx] = red;
        pixels[idx + 1] = green;
        pixels[idx + 2] = blue;
    }

    fn is_sprite_zero(&self, index: usize) -> bool {
        self.frame.sprites[index].index == 0
    }

    fn background_color(&self) -> u8 {
        if !self.regs.show_background() {
            return 0;
        }
        // 43210
        // |||||
        // |||++- Pixel value from tile data
        // |++--- Palette number from attribute table or OAM
        // +----- Background/Sprite select

        let tile_data = self.frame.tile_data;
        let data = tile_data >> ((7 - self.regs.fine_x()) * 4);
        (data & 0x0F) as u8
    }

    fn sprite_color(&self) -> (usize, u8) {
        if !self.regs.show_sprites() {
            return (0, 0);
        }
        for i in 0..self.frame.sprite_count as usize {
            let offset = self.cycle as i16 - 1 - self.frame.sprites[i].x as i16;
            if offset < 0 || offset > 7 {
                continue;
            }
            let offset = 7 - offset;
            let color = ((self.frame.sprites[i].pattern >> (offset * 4) as u8) & 0x0F) as u8;
            if color % 4 == 0 {
                continue;
            }
            return (i, color);
        }
        (0, 0)
    }

    fn tick(&mut self) {
        // Clear open bus roughly once every frame
        if self.scanline == 0 {
            self.regs.open_bus = 0x0;
        }

        // Reached the end of a frame cycle
        // Jump to (0, 0) (Cycles, Scanline) and start on the next frame
        let should_skip =
            self.scanline == PRERENDER_SCANLINE && self.rendering_enabled() && self.frame.parity;
        let cycle_end = if should_skip {
            PRERENDER_CYCLE_END
        } else {
            VISIBLE_SCANLINE_CYCLE_END
        };
        self.cycle += 1;
        self.cycle_count += 1;
        if self.cycle > cycle_end {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > PRERENDER_SCANLINE {
                self.scanline = 0;
                self.cycle_count = 0;
                self.frame.increment();
                self.frame_complete = true;
                debug!(
                    self,
                    "{} frame, jumping from {} to 0", self.frame.parity, cycle_end
                );
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
        let attr = d.read(addr + 2);
        let mut sprite = Sprite {
            index: i as u8,
            x: d.read(addr + 3).into(),
            y: d.read(addr).into(),
            tile_index: d.read(addr + 1).into(),
            tile_addr: 0u16,
            palette: (attr & 3) + 4, // range 4 to 7
            pattern: 0,
            has_priority: (attr & 0x20) == 0x20,    // bit 5
            flip_horizontal: (attr & 0x40) == 0x40, // bit 6
            flip_vertical: (attr & 0x80) == 0x80,   // bit 7
        };

        // Now fetch sprite pattern graphics

        // Dummy sprite
        let dummy_sprite =
            sprite.x == 0xFF && sprite.y == 0xFF && sprite.tile_index == 0xFF && attr == 0xFF;

        let sprite_height = self.regs.sprite_height();
        let mut sprite_row = self.scanline - sprite.y;
        if sprite.flip_vertical {
            sprite_row = sprite_height - 1 - sprite_row;
        }
        let sprite_table = if sprite_height == 8 {
            self.regs.sprite_select()
        } else {
            // use bit 1 of tile index to determine pattern table
            0x1000 * (sprite.tile_index & 0x01)
        };
        if sprite_height == 16 {
            sprite.tile_index &= 0xFE;
            // Finished the top half, read from adjacent tile
            if sprite_row > 7 {
                sprite.tile_index += 1;
                sprite_row -= 8;
            }
        }

        if dummy_sprite {
            sprite_row = 0;
        }

        sprite.tile_addr = sprite_table + sprite.tile_index * 16 + sprite_row;

        // Flip bits for horizontal flipping
        let a = (sprite.palette - 4) << 2;
        let mut lo_tile = self.vram.peek(sprite.tile_addr);
        let mut hi_tile = self.vram.peek(sprite.tile_addr + 8);
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
            sprite.pattern <<= 4;
            sprite.pattern |= u32::from(a | p1 | p2);
        }
        sprite
    }

    pub fn rendering_enabled(&self) -> bool {
        self.regs.show_background() || self.regs.show_sprites()
    }

    // Register read/writes

    /*
     * $2000 PPUCTRL
     */

    pub fn nmi_enabled(&self) -> bool {
        self.regs.nmi_enabled()
    }
    fn write_ppuctrl(&mut self, val: u8) {
        let nmi_flag = val & 0x80 > 0;
        if nmi_flag && !self.nmi_enabled() && self.vblank_started()
        // FIXME This is a bit of a hack - VBL should clear on cycle 1,
        // but something is off with timing and cycle 1 causes
        // 03-vbl_clear_time.nes/4.vbl_clear_timing.nes to fail.
        // Changing it to 2 makes them pass, but then causes 07-nmi_on_timing.nes
        // to fail so this condition is added to correct it
        && (self.scanline != PRERENDER_SCANLINE || self.cycle == 0)
        {
            debug!(self, "setting nmi_pending, cycle: {}", self.cycle);
            self.nmi_pending = true;
        }
        // Race condition
        if self.scanline == VBLANK_SCANLINE && !nmi_flag && self.cycle < 4 {
            debug!(self, "nmi pending false, cycle: {}", self.cycle);
            self.nmi_pending = false;
        }
        self.regs.write_ctrl(val);
    }

    /*
     * $2001 PPUMASK
     */

    fn write_ppumask(&mut self, val: u8) {
        if val & 0x08 != self.regs.mask & 0x08 {
            debug!(
                self,
                "setting bg: {}, cycle: {}",
                val & 0x08 > 0,
                self.cycle
            );
        }
        self.regs.write_mask(val);
    }

    /*
     * $2002 PPUSTATUS
     */

    pub fn read_ppustatus(&mut self) -> u8 {
        let mut status = self.regs.read_status();
        // Race conditions
        if self.scanline == VBLANK_SCANLINE {
            debug!(
                self,
                "reading status as ${:04X} cyc: {}", status, self.cycle
            );
            if self.cycle == 1 {
                debug!(self, "cycle matched, returning clear");
                status &= !0x80;
            }
            if self.cycle < 4 {
                debug!(self, "supressing nmi, cycle: {}", self.cycle);
                self.nmi_pending = false;
            }
        }
        // read_status() modifies register, so make sure mapper is aware
        // of new status
        self.vram
            .mapper
            .borrow_mut()
            .ppu_write(0x2002, self.regs.peek_status());
        status
    }
    fn peek_ppustatus(&self) -> u8 {
        self.regs.peek_status()
    }
    fn sprite_zero_hit(&mut self) -> bool {
        self.regs.sprite_zero_hit()
    }
    fn set_sprite_zero_hit(&mut self, val: bool) {
        self.regs.set_sprite_zero_hit(val);
    }
    fn set_sprite_overflow(&mut self, val: bool) {
        self.regs.set_sprite_overflow(val);
    }
    fn start_vblank(&mut self) {
        debug!(self, "started vbl {}", self.cycle);
        self.regs.start_vblank();
        if self.nmi_enabled() {
            debug!(self, "nmi enabled, nmi pending true");
            self.nmi_pending = true;
        }
        // Ensure our mapper knows vbl changed
        self.vram
            .mapper
            .borrow_mut()
            .ppu_write(0x2002, self.regs.peek_status());
    }
    fn stop_vblank(&mut self) {
        self.regs.stop_vblank();
        debug!(self, "Stopping vblank, clearing nmi, cycle: {}", self.cycle);
        // Ensure our mapper knows vbl changed
        self.vram
            .mapper
            .borrow_mut()
            .ppu_write(0x2002, self.regs.peek_status());
    }
    pub fn vblank_started(&self) -> bool {
        self.regs.vblank_started()
    }

    /*
     * $2003 OAMADDR
     */

    pub fn read_oamaddr(&self) -> u8 {
        self.regs.oamaddr
    }

    fn write_oamaddr(&mut self, val: u8) {
        self.regs.oamaddr = val;
    }

    /*
     * $2004 OAMDATA
     */

    fn read_oamdata(&mut self) -> u8 {
        self.oamdata.read(u16::from(self.regs.oamaddr))
    }
    fn peek_oamdata(&self) -> u8 {
        self.oamdata.peek(u16::from(self.regs.oamaddr))
    }
    fn write_oamdata(&mut self, val: u8) {
        self.oamdata.write(u16::from(self.regs.oamaddr), val);
        self.regs.oamaddr = self.regs.oamaddr.wrapping_add(1);
    }

    /*
     * $2005 PPUSCROLL
     */

    fn write_ppuscroll(&mut self, val: u8) {
        self.regs.write_scroll(val);
    }

    /*
     * $2006 PPUADDR
     */

    pub fn read_ppuaddr(&self) -> u16 {
        self.regs.read_addr()
    }
    fn write_ppuaddr(&mut self, val: u16) {
        self.regs.write_addr(val);
        self.vram.mapper.borrow_mut().vram_change(self.regs.v);
    }

    /*
     * $2007 PPUDATA
     */

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
        // During rendering, v increments coarse X and coarse Y at the simultaneously
        if self.rendering_enabled()
            && (self.scanline == PRERENDER_SCANLINE || self.scanline <= VISIBLE_SCANLINE_END)
        {
            self.regs.increment_x();
            self.regs.increment_y();
        } else {
            self.regs.increment_v();
        }
        self.vram.mapper.borrow_mut().vram_change(self.regs.v);
        val
    }
    fn peek_ppudata(&self) -> u8 {
        let val = self.vram.peek(self.read_ppuaddr());
        if self.read_ppuaddr() <= 0x3EFF {
            self.vram.buffer
        } else {
            val
        }
    }
    fn write_ppudata(&mut self, val: u8) {
        self.vram.write(self.read_ppuaddr(), val);
        // During rendering, v increments coarse X and coarse Y simultaneously
        if self.rendering_enabled()
            && (self.scanline == PRERENDER_SCANLINE || self.scanline <= VISIBLE_SCANLINE_END)
        {
            self.regs.increment_x();
            self.regs.increment_y();
        } else {
            self.regs.increment_v();
        }
        self.vram.mapper.borrow_mut().vram_change(self.regs.v);
    }
}

impl Clocked for Ppu {
    // http://wiki.nesdev.com/w/index.php/PPU_rendering
    fn clock(&mut self) -> usize {
        self.tick();
        self.render_dot();
        if self.cycle == 1 && self.scanline == VBLANK_SCANLINE {
            self.start_vblank();
        }
        // FIXME This is a bit of a hack - VBL should clear on cycle 1,
        // but something is off with timing and cycle 1 causes
        // 03-vbl_clear_time.nes/4.vbl_clear_timing.nes to fail.
        // Changing it to 2 makes them pass, but then causes 07-nmi_on_timing.nes
        // to fail so write_ppuctrl is changed as a result
        if self.cycle == 2 && self.scanline == PRERENDER_SCANLINE {
            // Dummy scanline - set up tiles for next scanline
            self.stop_vblank();
            self.set_sprite_zero_hit(false);
            self.set_sprite_overflow(false);
        }

        if self.debug && self.cycle == 0 {
            if self.scanline == self.nt_scanline {
                self.nametables = vec![
                    self.load_nametable(NT_START),
                    self.load_nametable(NT_START + 0x0400),
                    self.load_nametable(NT_START + 0x0800),
                    self.load_nametable(NT_START + 0x0C00),
                ];
            } else if self.scanline == self.pat_scanline {
                self.pattern_tables = vec![self.load_pattern_table(0), self.load_pattern_table(1)];
                self.palette = self.load_palette();
            }
        }
        1
    }
}

impl Memory for Ppu {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x2000 => self.regs.open_bus, // PPUCTRL is write-only
            0x2001 => self.regs.open_bus, // PPUMASK is write-only
            0x2002 => {
                let val = self.read_ppustatus(); // PPUSTATUS
                self.regs.open_bus |= val & !0x1F;
                (val & !0x1F) | (self.regs.open_bus & 0x1F)
            }
            0x2003 => self.regs.open_bus, // OAMADDR is write-only
            0x2004 => {
                let val = self.read_oamdata(); // OAMDATA
                self.regs.open_bus = val;
                val
            }
            0x2005 => self.regs.open_bus, // PPUSCROLL is write-only
            0x2006 => self.regs.open_bus, // PPUADDR is write-only
            0x2007 => {
                let val = self.read_ppudata(); // PPUDATA
                self.regs.open_bus = val;
                val
            }
            _ => 0,
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x2000 => self.regs.open_bus,    // PPUCTRL is write-only
            0x2001 => self.regs.open_bus,    // PPUMASK is write-only
            0x2002 => self.peek_ppustatus(), // PPUSTATUS
            0x2003 => self.regs.open_bus,    // OAMADDR is write-only
            0x2004 => self.peek_oamdata(),   // OAMDATA
            0x2005 => self.regs.open_bus,    // PPUSCROLL is write-only
            0x2006 => self.regs.open_bus,    // PPUADDR is write-only
            0x2007 => self.peek_ppudata(),   // PPUDATA
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.regs.open_bus = val;
        match addr {
            0x2000 => self.write_ppuctrl(val),            // PPUCTRL
            0x2001 => self.write_ppumask(val),            // PPUMASK
            0x2002 => (),                                 // PPUSTATUS is read-only
            0x2003 => self.write_oamaddr(val),            // OAMADDR
            0x2004 => self.write_oamdata(val),            // OAMDATA
            0x2005 => self.write_ppuscroll(val),          // PPUSCROLL
            0x2006 => self.write_ppuaddr(u16::from(val)), // PPUADDR
            0x2007 => self.write_ppudata(val),            // PPUDATA
            _ => (),
        }
    }
}

impl Powered for Ppu {
    fn reset(&mut self) {
        self.cycle = 0;
        self.scanline = 0;
        self.frame.reset();
        self.vram.reset();
        self.set_sprite_zero_hit(false);
        self.set_sprite_overflow(false);
        self.write_ppuctrl(0);
        self.write_ppumask(0);
        self.write_oamaddr(0);
    }
}

impl Loggable for Ppu {
    fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }
    fn log_level(&self) -> LogLevel {
        self.log_level
    }
}

impl Savable for Ppu {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.cycle.save(fh)?;
        self.scanline.save(fh)?;
        self.nmi_pending.save(fh)?;
        self.vram.save(fh)?;
        self.regs.save(fh)?;
        self.oamdata.save(fh)?;
        self.frame.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.cycle.load(fh)?;
        self.scanline.load(fh)?;
        self.nmi_pending.load(fh)?;
        self.vram.load(fh)?;
        self.regs.load(fh)?;
        self.oamdata.load(fh)?;
        self.frame.load(fh)
    }
}

// http://wiki.nesdev.com/w/index.php/PPU_nametables
// http://wiki.nesdev.com/w/index.php/PPU_attribute_tables
#[derive(Clone)]
pub struct Nametable(pub [u8; NT_SIZE]);

impl Memory for Nametable {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, addr: u16) -> u8 {
        self.0[addr as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.0[addr as usize] = val;
    }
}

impl Savable for Nametable {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.0.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.0.load(fh)
    }
}

// http://wiki.nesdev.com/w/index.php/PPU_palettes
#[derive(Clone)]
pub struct Palette(pub [u8; PALETTE_SIZE]);

impl Memory for Palette {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, mut addr: u16) -> u8 {
        if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr -= 16;
        }
        self.0[addr as usize]
    }
    fn write(&mut self, mut addr: u16, val: u8) {
        if addr >= 16 && addr.trailing_zeros() >= 2 {
            addr -= 16;
        }
        self.0[addr as usize] = val;
    }
}

impl Savable for Palette {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.0.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.0.load(fh)
    }
}

#[derive(Debug, Clone)]
pub struct PpuRegs {
    ctrl: u8,     // $2000 PPUCTRL write-only
    mask: u8,     // $2001 PPUMASK write-only
    status: u8,   // $2002 PPUSTATUS read-only
    oamaddr: u8,  // $2003 OAMADDR write-only
    pub v: u16,   // $2006 PPUADDR write-only 2x 15 bits: yyy NN YYYYY XXXXX
    t: u16,       // Temporary v - Also the addr of top-left onscreen tile
    x: u16,       // Fine X
    w: bool,      // 1st or 2nd write toggle
    open_bus: u8, // This open bus gets set during any write to PPU registers
}

impl PpuRegs {
    fn new() -> Self {
        Self {
            ctrl: 0x00,
            mask: 0x00,
            status: 0x00,
            oamaddr: 0x00,
            v: 0x0000,
            t: 0x0000,
            x: 0x0000,
            w: false,
            open_bus: 0x00,
        }
    }

    // Resets 1st/2nd Write latch for PPUSCROLL and PPUADDR
    fn reset_rw(&mut self) {
        self.w = false;
    }

    /*
     * $2000 PPUCTRL
     *
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUCTRL
     * VPHB SINN
     * |||| ||++- Nametable Select: 0 = $2000 (upper-left); 1 = $2400 (upper-right);
     * |||| ||                      2 = $2800 (lower-left); 3 = $2C00 (lower-right)
     * |||| |||+-   Also For PPUSCROLL: 1 = Add 256 to X scroll
     * |||| ||+--   Also For PPUSCROLL: 1 = Add 240 to Y scroll
     * |||| |+--- VRAM Increment Mode: 0 = add 1, going across; 1 = add 32, going down
     * |||| +---- Sprite Pattern Select for 8x8: 0 = $0000, 1 = $1000, ignored in 8x16 mode
     * |||+------ Background Pattern Select: 0 = $0000, 1 = $1000
     * ||+------- Sprite Height: 0 = 8x8, 1 = 8x16
     * |+-------- PPU Master/Slave: 0 = read from EXT, 1 = write to EXT
     * +--------- NMI Enable: NMI at next vblank: 0 = off, 1: on
     */
    fn write_ctrl(&mut self, val: u8) {
        let nn_mask = NT_Y_MASK | NT_X_MASK;
        // val: ......BA
        // t: ....BA.. ........
        self.t = (self.t & !nn_mask) | (u16::from(val) & 0x03) << 10; // take lo 2 bits and set NN
        self.ctrl = val;
    }
    fn vram_increment(&self) -> u16 {
        if self.ctrl & 0x04 > 0 {
            32
        } else {
            1
        }
    }
    fn sprite_select(&self) -> u16 {
        if self.ctrl & 0x08 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    fn background_select(&self) -> u16 {
        if self.ctrl & 0x10 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    pub fn sprite_height(&self) -> u16 {
        if self.ctrl & 0x20 > 0 {
            16
        } else {
            8
        }
    }
    fn nmi_enabled(&self) -> bool {
        self.ctrl & 0x80 > 0
    }

    /*
     * $2001 PPUMASK
     *
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUMASK
     * BGRs bMmG
     * |||| |||+- Grayscale (0: normal color, 1: produce a grayscale display)
     * |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
     * |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
     * |||| +---- 1: Show background
     * |||+------ 1: Show sprites
     * ||+------- Emphasize red
     * |+-------- Emphasize green
     * +--------- Emphasize blue
     */
    fn write_mask(&mut self, val: u8) {
        self.mask = val;
    }
    fn show_left_background(&self) -> bool {
        self.mask & 0x02 > 0
    }
    fn show_left_sprites(&self) -> bool {
        self.mask & 0x04 > 0
    }
    fn show_background(&self) -> bool {
        self.mask & 0x08 > 0
    }
    fn show_sprites(&self) -> bool {
        self.mask & 0x10 > 0
    }
    fn grayscale(&self) -> bool {
        self.mask & 0x01 > 0
    }
    fn emphasis(&self) -> u8 {
        (self.mask & 0xE0) >> 5
    }

    /*
     * $2002 PPUSTATUS
     *
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS
     * VSO. ....
     * |||+-++++- Least significant bits previously written into a PPU register
     * ||+------- Sprite overflow.
     * |+-------- Sprite 0 Hit.
     * +--------- Vertical blank has started (0: not in vblank; 1: in vblank)
     */
    fn read_status(&mut self) -> u8 {
        self.reset_rw();
        let vblank_started = self.status & 0x80;
        self.status &= !0x80; // Set vblank to 0
        self.status | vblank_started // return status with original vblank
    }
    fn peek_status(&self) -> u8 {
        self.status
    }

    fn set_sprite_overflow(&mut self, val: bool) {
        self.status = if val {
            self.status | 0x20
        } else {
            self.status & !0x20
        };
    }
    fn sprite_zero_hit(&self) -> bool {
        self.status & 0x40 == 0x40
    }
    fn set_sprite_zero_hit(&mut self, val: bool) {
        self.status = if val {
            self.status | 0x40
        } else {
            self.status & !0x40
        };
    }
    fn vblank_started(&self) -> bool {
        self.status & 0x80 > 0
    }
    fn start_vblank(&mut self) {
        self.status |= 0x80;
    }
    fn stop_vblank(&mut self) {
        self.status &= !0x80;
    }

    /*
     * $2005 PPUSCROLL
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSCROLL
     * http://wiki.nesdev.com/w/index.php/PPU_scrolling
     */

    // Returns Coarse X: XXXXX from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn coarse_x(&self) -> u16 {
        self.v & COARSE_X_MASK
    }

    // Returns Fine X: xxx from x register
    fn fine_x(&self) -> u16 {
        self.x
    }

    // Returns Coarse Y: YYYYY from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn coarse_y(&self) -> u16 {
        (self.v & COARSE_Y_MASK) >> 5
    }

    // Returns Fine Y: yyy from PPUADDR v
    // yyy NN YYYYY XXXXX
    fn fine_y(&self) -> u16 {
        (self.v & FINE_Y_MASK) >> 12
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
            self.x = val & fine_mask; // Set fine X
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
        let x_mask = NT_X_MASK | COARSE_X_MASK;
        self.v = (self.v & !x_mask) | (self.t & x_mask);
    }

    // Copy Fine y and Coarse Y from register t and add it to PPUADDR v
    fn copy_y(&mut self) {
        //    .yyyN.YY YYY.....
        // t: .IHGF.ED CBA.....
        // v: .IHGF.ED CBA.....
        let y_mask = FINE_Y_MASK | NT_Y_MASK | COARSE_Y_MASK;
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
            self.v = (self.v & !COARSE_X_MASK) ^ NT_X_MASK; // toggles X nametable
        } else {
            self.v += 1;
        }
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
                self.v ^= NT_Y_MASK;
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
     * $2006 PPUADDR
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUADDR
     */

    fn read_addr(&self) -> u16 {
        self.v & 0x3FFF // Bits 0-14
    }

    // Write val to PPUADDR v
    // 1st write writes hi 6 bits
    // 2nd write writes lo 8 bits
    // Total size is a 14 bit addr
    fn write_addr(&mut self, val: u16) {
        if !self.w {
            // Write hi address on first write
            let hi_bits_mask = 0x00FF;
            let hi_lshift = 8;
            let six_bits_mask = 0x003F;
            // val: ..FEDCBA
            //    FEDCBA98 76543210
            // t: 00FEDCBA ........
            self.t = (self.t & hi_bits_mask) | ((val & six_bits_mask) << hi_lshift);
        } else {
            // Write lo address on second write
            let lo_bits_mask = 0x7F00;
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
        self.v = self.v.wrapping_add(self.vram_increment());
    }
}

impl Savable for PpuRegs {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.open_bus.save(fh)?;
        self.ctrl.save(fh)?;
        self.mask.save(fh)?;
        self.status.save(fh)?;
        self.oamaddr.save(fh)?;
        self.v.save(fh)?;
        self.t.save(fh)?;
        self.x.save(fh)?;
        self.w.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.open_bus.load(fh)?;
        self.ctrl.load(fh)?;
        self.mask.load(fh)?;
        self.status.load(fh)?;
        self.oamaddr.load(fh)?;
        self.v.load(fh)?;
        self.t.load(fh)?;
        self.x.load(fh)?;
        self.w.load(fh)
    }
}

impl Default for PpuRegs {
    fn default() -> Self {
        Self::new()
    }
}

// Addr Low Nibble
// $00, $04, $08, $0C   Sprite Y coord
// $01, $05, $09, $0D   Sprite tile #
// $02, $06, $0A, $0E   Sprite attribute
// $03, $07, $0B, $0F   Sprite X coord
#[derive(Clone)]
struct Oam {
    entries: [u8; OAM_SIZE],
}

impl Oam {
    fn new() -> Self {
        Self {
            entries: [0; OAM_SIZE],
        }
    }
}

impl Memory for Oam {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }
    fn peek(&self, addr: u16) -> u8 {
        let val = self.entries[addr as usize];
        // Bits 2-4 of Sprite attribute should always be 0
        if let 0x02 | 0x06 | 0x0A | 0x0E = addr & 0x0F {
            val & 0xE3
        } else {
            val
        }
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.entries[addr as usize] = val;
    }
}

impl Savable for Oam {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.entries.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.entries.load(fh)
    }
}

#[derive(Clone)]
pub struct Vram {
    pub nametable: Nametable, // Used to layout backgrounds on the screen
    pub palette: Palette,     // Background/Sprite color palettes
    mapper: MapperRef,
    buffer: u8, // PPUDATA buffer
    a12: bool,  // Whether address line 12 is low or high
}

impl Vram {
    fn new() -> Self {
        Self {
            nametable: Nametable([0u8; NT_SIZE]),
            palette: Palette([0u8; PALETTE_SIZE]),
            mapper: mapper::null(),
            buffer: 0u8,
            a12: false,
        }
    }

    fn nametable_addr(&self, addr: u16) -> u16 {
        let mirroring = self.mapper.borrow().mirroring();
        // Maps addresses to nametable pages based on mirroring mode
        let mirroring_shift = match mirroring {
            Mirroring::Horizontal => 11,
            Mirroring::Vertical => 10,
            Mirroring::SingleScreenA => 14,
            Mirroring::SingleScreenB => 13,
            _ => panic!("Invalid mirroring mode"),
        };
        let page = (addr >> mirroring_shift) & 1;
        let table_size = 0x0400;
        let offset = addr % table_size;
        NT_START + page * table_size + offset
    }
}

impl Memory for Vram {
    fn read(&mut self, addr: u16) -> u8 {
        if addr < 0x2000 {
            self.a12 = (addr >> 12) & 1 > 0;
        }
        self.mapper.borrow_mut().vram_change(addr);
        match addr {
            0x0000..=0x1FFF => self.mapper.borrow_mut().read(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.mapper.borrow().use_ciram(addr) {
                    let mut mirror_addr = self.mapper.borrow().nametable_addr(addr);
                    if mirror_addr == 0 {
                        mirror_addr = self.nametable_addr(addr);
                    }
                    self.nametable.read(mirror_addr % NT_SIZE as u16)
                } else {
                    self.mapper.borrow_mut().read(addr)
                }
            }
            0x3F00..=0x3FFF => self.palette.read(addr % PALETTE_SIZE as u16),
            _ => 0,
        }
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.mapper.borrow().peek(addr),
            0x2000..=0x3EFF => {
                // Use PPU Nametables or Cartridge RAM
                if self.mapper.borrow().use_ciram(addr) {
                    let mut mirror_addr = self.mapper.borrow().nametable_addr(addr);
                    if mirror_addr == 0 {
                        mirror_addr = self.nametable_addr(addr);
                    }
                    self.nametable.peek(mirror_addr % NT_SIZE as u16)
                } else {
                    self.mapper.borrow().peek(addr)
                }
            }
            0x3F00..=0x3FFF => self.palette.peek(addr % PALETTE_SIZE as u16),
            _ => 0,
        }
    }

    fn write(&mut self, addr: u16, val: u8) {
        if addr < 0x2000 {
            self.a12 = (addr >> 12) & 1 > 0;
        }
        self.mapper.borrow_mut().vram_change(addr);
        match addr {
            0x0000..=0x1FFF => self.mapper.borrow_mut().write(addr, val),
            0x2000..=0x3EFF => {
                if self.mapper.borrow().use_ciram(addr) {
                    let mut mirror_addr = self.mapper.borrow().nametable_addr(addr);
                    if mirror_addr == 0 {
                        mirror_addr = self.nametable_addr(addr);
                    }
                    self.nametable.write(mirror_addr % NT_SIZE as u16, val);
                } else {
                    self.mapper.borrow_mut().write(addr, val);
                }
            }
            0x3F00..=0x3FFF => self.palette.write(addr % PALETTE_SIZE as u16, val),
            _ => (),
        }
    }
}

impl Powered for Vram {
    fn reset(&mut self) {
        self.nametable = Nametable([0u8; NT_SIZE]);
        self.palette = Palette([0u8; PALETTE_SIZE]);
    }
    fn power_cycle(&mut self) {
        self.reset();
    }
}

impl Savable for Vram {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.buffer.save(fh)?;
        self.nametable.save(fh)?;
        self.palette.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.buffer.load(fh)?;
        self.nametable.load(fh)?;
        self.palette.load(fh)
    }
}

#[derive(Clone)]
pub struct Frame {
    num: u32,
    parity: bool,
    // Shift registers
    tile_lo: u8,
    tile_hi: u8,
    // Tile data - stored in cycles 0 mod 8
    nametable: u16,
    attribute: u8,
    tile_data: u32,
    // Sprite data
    sprite_count: u8,
    sprite_zero_on_line: bool,
    sprites: [Sprite; 8], // Each frame can only hold 8 sprites at a time
    signal_levels: Vec<f32>,
    phase_lookup: HashMap<u16, Vec<f32>>,
    pixels: Vec<u8>,
}

impl Frame {
    // Voltage levels, relative to sync voltage
    const BLACK: f32 = 0.518;
    const WHITE: f32 = 1.962;
    const ATTENUATION: f32 = 0.746;
    const LEVELS: [f32; 8] = [
        0.350, 0.518, 0.962, 1.550, // Signal low
        1.094, 1.506, 1.962, 1.962, // Signal high
    ];

    fn new() -> Self {
        let phase_size = (RENDER_WIDTH * 8) as usize;
        let mut phase_lookup = HashMap::new();
        for scanline in 0..RENDER_HEIGHT as u16 {
            let mut phases = vec![0.0; 2 * phase_size];
            let phase = (f32::from(scanline) * 341.0 * 8.0 + 3.9) % 12.0;
            for p in 0..phase_size {
                let phase = (PI as f32 * (phase + p as f32)) / 6.0;
                phases[p as usize] = phase.cos();
                phases[p as usize + phase_size] = phase.sin();
            }
            phase_lookup.insert(scanline, phases);
        }
        Self {
            num: 0u32,
            parity: false,
            nametable: 0u16,
            attribute: 0u8,
            tile_lo: 0u8,
            tile_hi: 0u8,
            tile_data: 0u32,
            sprite_count: 0u8,
            sprite_zero_on_line: false,
            sprites: [Sprite::new(); 8],
            signal_levels: vec![0.0; SIGNAL_SIZE],
            phase_lookup,
            pixels: vec![0u8; RENDER_SIZE],
        }
    }

    fn increment(&mut self) {
        self.num += 1;
        self.parity = !self.parity;
    }

    #[allow(clippy::many_single_char_names)]
    fn put_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        if x > RENDER_WIDTH || y > RENDER_HEIGHT {
            return;
        }
        let idx = (3 * (x + y * RENDER_WIDTH)) as usize;
        self.pixels[idx] = r;
        self.pixels[idx + 1] = g;
        self.pixels[idx + 2] = b;
    }

    fn ntsc_signal(palette: u8, emphasis: u8, phase: usize) -> f32 {
        // Decode the NES color
        let color = u16::from(palette & 0x0F); // 0..15 "cccc"
        let level = if color > 13 {
            1 // For colors 14..15, level 1 is forced.
        } else {
            palette >> 4 & 3 // 0..3  "ll"
        };

        // The square wave for this color alternates between these two voltages:
        let mut low = Self::LEVELS[level as usize] as f32;
        let mut high = Self::LEVELS[4 + level as usize] as f32;
        if color == 0 {
            low = high;
        } // For color 0, only high level is emitted
        if color > 12 {
            high = low;
        } // For colors 13..15, only low level is emitted

        // Generate the square wave
        let in_color_phase = |color| (usize::from(color) + phase) % 12 < 6; // Inline function
        let mut signal = if in_color_phase(color) { high } else { low };

        // When de-emphasis bits are set, some parts of the signal are attenuated:
        if ((emphasis & 1 > 0) && in_color_phase(0))
            || ((emphasis & 2 > 0) && in_color_phase(4))
            || ((emphasis & 4 > 0) && in_color_phase(8))
        {
            signal *= Self::ATTENUATION;
        }

        signal
    }

    fn render_ntsc_pixel(&mut self, x: u16, palette: u8, emphasis: u8, ppu_cycle: usize) {
        let phase = ppu_cycle * 8;
        // Each pixel produces distinct 8 samples of NTSC signal
        for p in 0..8 {
            let mut signal = Self::ntsc_signal(palette, emphasis, (phase + p) % 12);
            // Optionally apply some lowpass-filtering to the signal here
            // Optionally normalize the signal to 0..1 range:
            signal = (signal - Self::BLACK) / (Self::WHITE - Self::BLACK);
            // Save the signal for this pixel.
            self.signal_levels[x as usize * 8 + p as usize] = signal;
        }
    }

    // phase: This should the value that was cycle * 8 + 3.9
    // at the BEGINNING of this scanline. It should be modulo 12.
    // It can additionally include a floating-point hue offset.
    #[allow(clippy::many_single_char_names)]
    fn decode_ntsc_signal(&mut self, scanline: u16) {
        let phase_lookup = self.phase_lookup.get(&scanline).unwrap();
        let mut pixels = Vec::with_capacity(RENDER_WIDTH as usize);
        let samples = 6;
        for x in 0..RENDER_WIDTH {
            // Determine the region of scanline signal to sample. Take 12 samples.
            let center = x * 8;
            let begin = if center > samples {
                center - samples
            } else {
                0
            };
            let end = if center < RENDER_WIDTH * 8 - samples {
                center + samples
            } else {
                RENDER_WIDTH * 8
            };
            // Calculate the color in YIQ
            let mut y = 0.0;
            let mut i = 0.0;
            let mut q = 0.0;
            let signal_idx = (u32::from(scanline) * RENDER_WIDTH * 8) as usize;
            for p in begin..end {
                // Collect and accumulate samples
                let level = self.signal_levels[signal_idx + p as usize] / 12.0;
                y += level;
                i += level * phase_lookup[p as usize];
                q += level * phase_lookup[p as usize + RENDER_WIDTH as usize * 8];
            }
            let (r, g, b) = Self::yiq_to_rgb(y, i, q);
            pixels.push((r, g, b));
        }
        for (x, (r, g, b)) in pixels.iter().enumerate() {
            self.put_pixel(x as u32, u32::from(scanline), *r, *g, *b);
        }
    }

    fn yiq_to_rgb(y: f32, i: f32, q: f32) -> (u8, u8, u8) {
        let gamma_fix = |f: f32| {
            if f <= 0.0 {
                0.0
            } else {
                f.powf(1.1)
            }
        };
        let clamp = |v: f32| {
            if v > 255.0 {
                255
            } else {
                v.floor() as u32
            }
        };
        (
            clamp(255.95 * gamma_fix(y + 0.946_882 * i + 0.623_557 * q)) as u8,
            clamp(255.95 * gamma_fix(y - 0.274_788 * i - 0.635_691 * q)) as u8,
            clamp(255.95 * gamma_fix(y - 1.108_545 * i + 1.709_007 * q)) as u8,
        )
    }
}

impl Powered for Frame {
    fn reset(&mut self) {
        self.num = 0;
        self.parity = false;
    }
}

impl Savable for Frame {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.num.save(fh)?;
        self.parity.save(fh)?;
        self.tile_lo.save(fh)?;
        self.tile_hi.save(fh)?;
        self.nametable.save(fh)?;
        self.attribute.save(fh)?;
        self.tile_data.save(fh)?;
        self.sprite_count.save(fh)?;
        self.sprite_zero_on_line.save(fh)?;
        self.sprites.save(fh)?;
        self.pixels.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.num.load(fh)?;
        self.parity.load(fh)?;
        self.tile_lo.load(fh)?;
        self.tile_hi.load(fh)?;
        self.nametable.load(fh)?;
        self.attribute.load(fh)?;
        self.tile_data.load(fh)?;
        self.sprite_count.load(fh)?;
        self.sprite_zero_on_line.load(fh)?;
        self.sprites.load(fh)?;
        self.pixels.load(fh)
    }
}

#[derive(Default, Debug, Copy, Clone)]
struct Sprite {
    index: u8,
    x: u16,
    y: u16,
    tile_index: u16,
    tile_addr: u16,
    palette: u8,
    pattern: u32,
    has_priority: bool,
    flip_horizontal: bool,
    flip_vertical: bool,
}

impl Sprite {
    fn new() -> Self {
        Self {
            index: 0u8,
            x: 0xFF,
            y: 0xFF,
            tile_index: 0xFF,
            tile_addr: 0xFF,
            palette: 0x07,
            pattern: 0u32,
            has_priority: true,
            flip_horizontal: true,
            flip_vertical: true,
        }
    }
}

impl Savable for Sprite {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.index.save(fh)?;
        self.x.save(fh)?;
        self.y.save(fh)?;
        self.tile_index.save(fh)?;
        self.tile_addr.save(fh)?;
        self.palette.save(fh)?;
        self.pattern.save(fh)?;
        self.has_priority.save(fh)?;
        self.flip_horizontal.save(fh)?;
        self.flip_vertical.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.index.load(fh)?;
        self.x.load(fh)?;
        self.y.load(fh)?;
        self.tile_index.load(fh)?;
        self.tile_addr.load(fh)?;
        self.palette.load(fh)?;
        self.pattern.load(fh)?;
        self.has_priority.load(fh)?;
        self.flip_horizontal.load(fh)?;
        self.flip_vertical.load(fh)
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Ppu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ppu {{ }}")
    }
}

impl fmt::Debug for Oam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Oam {{ entries: {} bytes }}", OAM_SIZE)
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Frame {{ }}")
    }
}

impl fmt::Debug for Vram {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Vram {{ }}")
    }
}

impl fmt::Debug for Nametable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Nametable {{ size: {}KB }}", NT_SIZE / 1024)
    }
}

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Palette {{ size: {} }}", PALETTE_SIZE)
    }
}

// 64 total possible colors, though only 32 can be loaded at a time
#[rustfmt::skip]
const SYSTEM_PALETTE: [u8; SYSTEM_PALETTE_SIZE * 3] = [
    // 0x00
    84, 84, 84,    0, 30, 116,    8, 16, 144,    48, 0, 136,    // $00-$03
    68, 0, 100,    92, 0, 48,     84, 4, 0,      60, 24, 0,     // $04-$07
    32, 42, 0,     8, 58, 0,      0, 64, 0,      0, 60, 0,      // $08-$0B
    0, 50, 60,     0, 0, 0,       0, 0, 0,       0, 0, 0,       // $0C-$0F
    // 0x10
    152, 150, 152, 8, 76, 196,    48, 50, 236,   92, 30, 228,   // $10-$13
    136, 20, 176,  160, 20, 100,  152, 34, 32,   120, 60, 0,    // $14-$17
    84, 90, 0,     40, 114, 0,    8, 124, 0,     0, 118, 40,    // $18-$1B
    0, 102, 120,   0, 0, 0,       0, 0, 0,       0, 0, 0,       // $1C-$1F
    // 0x20
    236, 238, 236, 76, 154, 236,  120, 124, 236, 176, 98, 236,  // $20-$23
    228, 84, 236,  236, 88, 180,  236, 106, 100, 212, 136, 32,  // $24-$27
    160, 170, 0,   116, 196, 0,   76, 208, 32,   56, 204, 108,  // $28-$2B
    56, 180, 204,  60, 60, 60,    0, 0, 0,       0, 0, 0,       // $2C-$2F
    // 0x30
    236, 238, 236, 168, 204, 236, 188, 188, 236, 212, 178, 236, // $30-$33
    236, 174, 236, 236, 174, 212, 236, 180, 176, 228, 196, 144, // $34-$37
    204, 210, 120, 180, 222, 120, 168, 226, 144, 152, 226, 180, // $38-$3B
    160, 214, 228, 160, 162, 160, 0, 0, 0,       0, 0, 0,       // $3C-$3F
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper;

    #[test]
    fn ppu_scrolling_registers() {
        let mut ppu = Ppu::new();
        ppu.load_mapper(mapper::null());

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
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuscroll
        let scroll_write: u8 = 0b0111_1101;
        let t_result: u16 = 0b000_11_00000_01111;
        let x_result: u16 = 0b101;
        ppu.write(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuscroll
        let scroll_write: u8 = 0b0101_1110;
        let t_result: u16 = 0b110_11_01011_01111;
        ppu.write(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuaddr
        let addr_write: u8 = 0b0011_1101;
        let t_result: u16 = 0b011_11_01011_01111;
        ppu.write(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuaddr
        let addr_write: u8 = 0b1111_0000;
        let t_result: u16 = 0b011_11_01111_10000;
        ppu.write(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, false);

        // Test a 2006/2005/2005/2006 write
        // http://forums.nesdev.com/viewtopic.php?p=78593#p78593
        ppu.write(ppuaddr, 0b0000_1000); // nametable select $10
        ppu.write(ppuscroll, 0b0100_0101); // $01 hi bits coarse Y scroll, $101 fine Y scroll
        ppu.write(ppuscroll, 0b0000_0011); // $011 fine X scroll
        ppu.write(ppuaddr, 0b1001_0110); // $100 lo bits coarse Y scroll, $10110 coarse X scroll
        let t_result: u16 = 0b101_10_01100_10110;
        assert_eq!(ppu.regs.v, t_result);
    }

    #[test]
    fn a12_timing() {
        let mut ppu = Ppu::new();
        ppu.load_mapper(mapper::null());

        // Show BG and Spr
        ppu.write_ppumask(3 << 3);
        // BG use 0x0000, Spr use 0x1000
        ppu.write_ppuctrl(1 << 3);

        // Rising edge test
        let mut last_clock = false;
        let mut clocks = 0u16;
        for i in 0..340 * 241 {
            let _ = ppu.clock();
            if !last_clock && ppu.vram.a12 {
                clocks += 1;
            }
            last_clock = ppu.vram.a12;
            if i == 260 {
                assert_eq!(clocks, 1, "a12 rising first clock @ 324");
            }
        }
        assert_eq!(clocks, 240, "a12 rising clocked 241 times");

        ppu = Ppu::new();
        ppu.load_mapper(mapper::null());

        // Show BG and Spr
        ppu.write_ppumask(3 << 3);
        // BG use 0x0000, Spr use 0x1000
        ppu.write_ppuctrl(1 << 3);

        // Failling edge
        last_clock = false;
        clocks = 0u16;
        for i in 0..340 * 241 {
            let _ = ppu.clock();
            if last_clock && !ppu.vram.a12 {
                clocks += 1;
            }
            last_clock = ppu.vram.a12;
            if i == 324 {
                assert_eq!(clocks, 1, "a12 falling first clock @ 324");
            }
        }
        assert_eq!(clocks, 240, "a12 rising clocked 241 times");
    }
}
