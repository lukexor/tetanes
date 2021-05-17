//! Picture Processing Unit
//!
//! [http://wiki.nesdev.com/w/index.php/PPU]()

use crate::{
    common::{Addr, Byte, Clocked, NesFormat, Powered},
    mapper::{Mapper, MapperType},
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use frame::Frame;
use nametable::{ATTRIBUTE_START, NT_START};
use oam::{Oam, OAM_SIZE};
use palette::{PALETTE_END, PALETTE_SIZE, PALETTE_START, SYSTEM_PALETTE, SYSTEM_PALETTE_SIZE};
use ppu_regs::{PpuRegs, COARSE_X_MASK, COARSE_Y_MASK, NT_X_MASK, NT_Y_MASK};
use sprite::Sprite;
use std::{
    fmt,
    io::{Read, Write},
};
use vram::Vram;

mod frame;
mod nametable;
mod oam;
mod palette;
mod ppu_regs;
mod sprite;
mod vram;

// Screen/Render
pub const RENDER_WIDTH: u32 = 256;
pub const RENDER_HEIGHT: u32 = 240;
const _TOTAL_CYCLES: u32 = 341;
const _TOTAL_SCANLINES: u32 = 262;
const RENDER_PIXELS: usize = (RENDER_WIDTH * RENDER_HEIGHT) as usize;
const RENDER_SIZE: usize = 3 * RENDER_PIXELS;

// Cycles
const IDLE_CYCLE: u16 = 0; // PPU is idle this cycle
const VISIBLE_CYCLE_START: u16 = 1; // Tile data fetching starts
const VISIBLE_CYCLE_END: u16 = 256; // 2 cycles each for 4 fetches = 32 tiles
const SPRITE_PREFETCH_CYCLE_START: u16 = 257; // Sprites for next scanline fetch starts
const SPRITE_PREFETCH_CYCLE_END: u16 = 320; // 2 cycles each for 4 fetches = 8 sprites
const COPY_Y_CYCLE_START: u16 = 280; // Copy Y scroll start
const COPY_Y_CYCLE_END: u16 = 304; // Copy Y scroll stop
const INC_Y_CYCLE: u16 = 256; // Increase Y scroll when it reaches end of the screen
const COPY_X_CYCLE: u16 = 257; // Copy X scroll when starting a new scanline
const PREFETCH_CYCLE_START: u16 = 321; // Tile data for next scanline fetched
const PREFETCH_CYCLE_END: u16 = 336; // 2 cycles each for 4 fetches = 2 tiles
const DUMMY_CYCLE_START: u16 = 337; // Dummy fetches - use is unknown
const SKIP_CYCLE: u16 = 339; // Odd frames skip the last cycle
const CYCLE_END: u16 = 340; // 2 cycles each for 2 fetches
const POWER_ON_CYCLES: usize = 29658 * 3; // https://wiki.nesdev.com/w/index.php/PPU_power_up_state

// Scanlines
const _VISIBLE_SCANLINE_START: u16 = 0; // Rendering graphics for the screen
const VISIBLE_SCANLINE_END: u16 = 239; // Rendering graphics for the screen
const _POSTRENDER_SCANLINE: u16 = 240; // Idle scanline
const VBLANK_SCANLINE: u16 = 241; // Vblank set at tick 1 (the second tick)
const PRERENDER_SCANLINE: u16 = 261;

#[derive(Clone)]
pub struct Ppu {
    pub cycle: u16,         // (0, 340) 341 cycles happen per scanline
    pub cycle_count: usize, // Total number of PPU cycles run
    frame_cycles: u32,      // Total number of PPU cycles run per frame
    pub scanline: u16,      // (0, 261) 262 total scanlines per frame
    scanline_phase: u32,    // Phase at the start of this scanline
    pub nmi_pending: bool,  // Whether the CPU should trigger an NMI next cycle
    vram: Vram,             // $2007 PPUDATA
    pub regs: PpuRegs,      // Registers
    oamdata: Oam,           // $2004 OAMDATA read/write - Object Attribute Memory for Sprites
    frame: Frame,           // Frame data keeps track of data and shift registers between frames
    pub frame_complete: bool,
    pub ntsc_video: bool,
    nes_format: NesFormat,
    clock_remainder: u8,
    debug: bool,
    nt_scanline: u16,
    pat_scanline: u16,
    pub nametables: Vec<Vec<Byte>>,
    pub nametable_ids: Vec<u8>,
    pub pattern_tables: Vec<Vec<Byte>>,
    pub palette: Vec<Byte>,
    pub palette_ids: Vec<u8>,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycle: 0,
            cycle_count: 0,
            frame_cycles: 0,
            scanline: 0,
            scanline_phase: 0,
            nmi_pending: false,
            regs: PpuRegs::new(),
            oamdata: Oam::new(),
            vram: Vram::new(),
            frame: Frame::new(),
            frame_complete: false,
            ntsc_video: true,
            nes_format: NesFormat::Ntsc,
            clock_remainder: 0,
            debug: false,
            nt_scanline: 0,
            pat_scanline: 0,
            nametables: vec![
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
                vec![0; RENDER_SIZE],
            ],
            nametable_ids: vec![0; 4 * 0x0400],
            pattern_tables: vec![vec![0; RENDER_SIZE / 2], vec![0; RENDER_SIZE / 2]],
            palette: vec![0; (PALETTE_SIZE + 4) * 4],
            palette_ids: vec![0; (PALETTE_SIZE + 4) * 4],
        }
    }

    pub fn load_mapper(&mut self, mapper: &mut MapperType) {
        self.vram.mapper = &mut *mapper as *mut MapperType;
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
        self.load_nametables();
        self.load_pattern_tables();
        self.load_palettes();
    }

    // Returns a fully rendered frame of RENDER_SIZE RGB colors
    pub fn frame(&self) -> &Vec<Byte> {
        &self.frame.pixels
    }

    fn load_nametables(&mut self) {
        for i in 0..4 {
            let base_addr = NT_START + i * 0x0400;
            for addr in base_addr..(base_addr + 0x0400 - 64) {
                let x_scroll = addr & COARSE_X_MASK;
                let y_scroll = (addr & COARSE_Y_MASK) >> 5;

                let nt_base_addr = NT_START + (addr & (NT_X_MASK | NT_Y_MASK));
                let tile = self.vram.peek(addr);
                let tile_addr = self.regs.background_select() + Addr::from(tile) * 16;
                let supertile_num = (x_scroll / 4) + (y_scroll / 4) * 8;
                let attr = Addr::from(self.vram.peek(nt_base_addr + 0x3C0 + supertile_num));
                let corner = ((x_scroll % 4) / 2 + (y_scroll % 4) / 2 * 2) << 1;
                let mask = 0x03 << corner;
                let palette = (attr & mask) >> corner;

                let tile_num = x_scroll + y_scroll * 32;
                let tile_x = (tile_num % 32) * 8;
                let tile_y = (tile_num / 32) * 8;

                self.nametable_ids[(addr - NT_START) as usize] = tile;
                for y in 0..8 {
                    let lo = Addr::from(self.vram.peek(tile_addr + y));
                    let hi = Addr::from(self.vram.peek(tile_addr + y + 8));
                    for x in 0..8 {
                        let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                        let palette_idx =
                            self.vram.peek(PALETTE_START + palette * 4 + pix_type) as usize;
                        let x = tile_x + (7 - x);
                        let y = tile_y + y;
                        Self::put_pixel(
                            palette_idx,
                            x.into(),
                            y.into(),
                            RENDER_WIDTH,
                            &mut self.nametables[i as usize],
                        );
                    }
                }
            }
        }
    }

    fn load_pattern_tables(&mut self) {
        let width = RENDER_WIDTH / 2;
        for table in 0..2 {
            let start = table as Addr * 0x1000;
            let end = start + 0x1000;
            for tile_addr in (start..end).step_by(16) {
                let tile_x = ((tile_addr % 0x1000) % 256) / 2;
                let tile_y = ((tile_addr % 0x1000) / 256) * 8;
                for y in 0..8 {
                    let lo = Addr::from(self.vram.peek(tile_addr + y));
                    let hi = Addr::from(self.vram.peek(tile_addr + y + 8));
                    for x in 0..8 {
                        let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                        let palette_idx = self.vram.peek(PALETTE_START + pix_type) as usize;
                        let x = tile_x + (7 - x);
                        let y = tile_y + y;
                        Self::put_pixel(
                            palette_idx,
                            x.into(),
                            y.into(),
                            width,
                            &mut self.pattern_tables[table as usize],
                        );
                    }
                }
            }
        }
    }

    fn load_palettes(&mut self) {
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
            let x = (addr - PALETTE_START) % 16;
            let y = (addr - PALETTE_START) / 16;
            let palette_idx = self.vram.peek(addr);
            self.palette_ids[y as usize * width + x as usize] = palette_idx;
            Self::put_pixel(
                palette_idx as usize,
                x.into(),
                y.into(),
                width as u32,
                &mut self.palette,
            );
        }
    }

    fn run_cycle(&mut self) {
        self.tick();

        // Idle cycles/scanline
        if self.cycle == IDLE_CYCLE {
            return;
        }

        let visible_cycle = self.cycle >= VISIBLE_CYCLE_START && self.cycle <= VISIBLE_CYCLE_END;
        let prefetch_cycle = self.cycle >= PREFETCH_CYCLE_START && self.cycle <= PREFETCH_CYCLE_END;
        let dummy_cycle = self.cycle >= DUMMY_CYCLE_START && self.cycle <= CYCLE_END;
        let fetch_cycle = prefetch_cycle || visible_cycle;
        let visible_scanline = self.scanline <= VISIBLE_SCANLINE_END;
        let prerender_scanline = self.scanline == PRERENDER_SCANLINE;
        let render_scanline = prerender_scanline || visible_scanline;

        // Pixels should be put even if rendering is disabled, as this is what blanks out the
        // screen. Rendering disabled just means we don't evaluate/read bg/sprite info
        self.render_pixel();

        if self.rendering_enabled() && render_scanline {
            // (1, 0) - (256, 239) - visible cycles/scanlines
            // (1, 261) - (256, 261) - prefetch scanline
            // (321, 0) - (336, 239) - next scanline fetch cycles
            if fetch_cycle {
                self.evaluate_background();
            } else if dummy_cycle {
                // Dummy byte fetches
                // (337, 0) - (337, 239)
                self.fetch_bg_nt_byte();
            }

            // Y scroll bits are supposed to be reloaded during this pixel range of PRERENDER
            // if rendering is enabled
            // http://wiki.nesdev.com/w/index.php/PPU_rendering#Pre-render_scanline_.28-1.2C_261.29
            let copy_y = self.cycle >= COPY_Y_CYCLE_START && self.cycle <= COPY_Y_CYCLE_END;
            if prerender_scanline && copy_y {
                self.regs.copy_y();
            }

            // Increment Coarse X every 8 cycles (e.g. 8 pixels) since sprites are 8x wide
            if fetch_cycle && self.cycle % 8 == 0 {
                self.regs.increment_x();
            }
            // Increment Fine Y when we reach the end of the screen
            if self.cycle == INC_Y_CYCLE {
                self.regs.increment_y();
            }
            // Copy X bits at the start of a new line since we're going to start writing
            // new x values to t
            if self.cycle == COPY_X_CYCLE {
                self.regs.copy_x();
            }

            // TODO - This should be split up instead of being done all at once
            // The code block below this simulates the reads required, but
            // its not ideal
            if self.cycle == SPRITE_PREFETCH_CYCLE_START {
                self.evaluate_sprites();
            }

            // HACK: This gets our IRQ timing properly for certain mappers (MMC3, MMC5)
            // because evaluation is done all on one cycle
            let sprite_prefetch = self.cycle >= SPRITE_PREFETCH_CYCLE_START
                && self.cycle <= SPRITE_PREFETCH_CYCLE_END;
            if sprite_prefetch {
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

    fn fetch_bg_nt_byte(&mut self) {
        // Fetch BG nametable
        // https://wiki.nesdev.com/w/index.php/PPU_scrolling#Tile_and_attribute_fetching
        let nametable_addr_mask = 0x0FFF; // Only need lower 12 bits
        let addr = NT_START | (self.regs.v & nametable_addr_mask);
        self.frame.nametable = Addr::from(self.vram.read(addr));
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
                self.frame.tile_data |= u64::from(data);
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
            let sprite_y = Addr::from(self.oamdata.read((i * 4) as Addr));
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
        let x = self.cycle - 1;
        let y = self.scanline;

        let mut bg_color = self.background_color();
        let (i, mut sprite_color) = self.sprite_color();

        let border_pixel = x < 8;
        let left_clip = !self.regs.show_left_background() && border_pixel;
        if left_clip {
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
                && x > 0
                && x != 255
                && y <= VISIBLE_SCANLINE_END
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
        let mut palette = self.vram.read(Addr::from(color) + PALETTE_START);
        if self.regs.grayscale() {
            palette &= !0x0F; // Remove chroma
        }
        if self.ntsc_video {
            let format = self.nes_format;
            let pixel = ((self.regs.emphasis(format) as u32) << 6) | palette as u32;
            self.frame
                .put_ntsc_pixel(x.into(), self.scanline.into(), pixel, self.frame_cycles);
        } else {
            let color_idx = (palette as usize % SYSTEM_PALETTE_SIZE) * 3;
            let r = SYSTEM_PALETTE[color_idx];
            let g = SYSTEM_PALETTE[color_idx + 1];
            let b = SYSTEM_PALETTE[color_idx + 2];
            self.frame.put_pixel(x.into(), y.into(), r, g, b);
        }
    }

    fn put_pixel(palette_idx: usize, x: u32, y: u32, width: u32, pixels: &mut Vec<Byte>) {
        if x >= RENDER_WIDTH || y >= RENDER_HEIGHT {
            return;
        }
        let idx = (palette_idx % SYSTEM_PALETTE_SIZE) * 3;
        let red = SYSTEM_PALETTE[idx];
        let green = SYSTEM_PALETTE[idx + 1];
        let blue = SYSTEM_PALETTE[idx + 2];
        let idx = 4 * (x + y * width) as usize;
        pixels[idx] = red;
        pixels[idx + 1] = green;
        pixels[idx + 2] = blue;
        pixels[idx + 3] = 255;
    }

    fn is_sprite_zero(&self, index: usize) -> bool {
        self.frame.sprites[index].index == 0
    }

    fn background_color(&self) -> Byte {
        if !self.regs.show_background() {
            return 0;
        }
        // 43210
        // |||||
        // |||++- Pixel value from tile data
        // |++--- Palette number from attribute table or OAM
        // +----- Background/Sprite select

        let tile_data = (self.frame.tile_data >> 32) as u32;
        let data = tile_data >> ((7 - self.regs.fine_x()) * 4);
        (data & 0x0F) as Byte
    }

    fn sprite_color(&self) -> (usize, Byte) {
        if !self.regs.show_sprites() {
            return (0, 0);
        }
        for i in 0..self.frame.sprite_count as usize {
            let offset = self.cycle as i16 - 1 - self.frame.sprites[i].x as i16;
            if offset < 0 || offset > 7 {
                continue;
            }
            let offset = 7 - offset;
            let color = ((self.frame.sprites[i].pattern >> (offset * 4) as Byte) & 0x0F) as Byte;
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
        let cycle_end = if should_skip { SKIP_CYCLE } else { CYCLE_END };
        self.cycle += 1;
        self.cycle_count = self.cycle_count.wrapping_add(1);
        self.frame_cycles = (self.frame_cycles + 1) % 3;
        if self.cycle > cycle_end {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > PRERENDER_SCANLINE {
                self.scanline = 0;
                self.frame.increment();
                self.frame_complete = true;
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
    fn write_ppuctrl(&mut self, val: Byte) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        let nmi_flag = val & 0x80 > 0;
        if nmi_flag && !self.nmi_enabled() && self.vblank_started()
        // FIXME This is a bit of a hack - VBL should clear on cycle 1,
        // but something is off with timing and cycle 1 causes
        // 03-vbl_clear_time.nes/4.vbl_clear_timing.nes to fail.
        // Changing it to 2 makes them pass, but then causes 07-nmi_on_timing.nes
        // to fail so this condition is added to correct it
        && (self.scanline != PRERENDER_SCANLINE || self.cycle == 0)
        {
            self.nmi_pending = true;
        }
        // Race condition
        if self.scanline == VBLANK_SCANLINE && !nmi_flag && self.cycle < 4 {
            self.nmi_pending = false;
        }
        self.regs.write_ctrl(val);
    }

    /*
     * $2001 PPUMASK
     */

    fn write_ppumask(&mut self, val: Byte) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_mask(val);
    }

    /*
     * $2002 PPUSTATUS
     */

    pub fn read_ppustatus(&mut self) -> Byte {
        let mut status = self.regs.read_status();
        // Race conditions
        if self.scanline == VBLANK_SCANLINE {
            if self.cycle == 1 {
                status &= !0x80;
            }
            if self.cycle < 4 {
                self.nmi_pending = false;
            }
        }
        // read_status() modifies register, so make sure mapper is aware
        // of new status
        self.vram
            .mapper_mut()
            .ppu_write(0x2002, self.regs.peek_status());
        status
    }
    fn peek_ppustatus(&self) -> Byte {
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
        self.regs.start_vblank();
        if self.nmi_enabled() {
            self.nmi_pending = true;
        }
        // Ensure our mapper knows vbl changed
        self.vram
            .mapper_mut()
            .ppu_write(0x2002, self.regs.peek_status());
    }
    fn stop_vblank(&mut self) {
        self.regs.stop_vblank();
        // Ensure our mapper knows vbl changed
        self.vram
            .mapper_mut()
            .ppu_write(0x2002, self.regs.peek_status());
    }
    pub fn vblank_started(&self) -> bool {
        self.regs.vblank_started()
    }

    /*
     * $2003 OAMADDR
     */

    pub fn read_oamaddr(&self) -> Byte {
        self.regs.oamaddr
    }

    fn write_oamaddr(&mut self, val: Byte) {
        self.regs.oamaddr = val;
    }

    /*
     * $2004 OAMDATA
     */

    fn read_oamdata(&mut self) -> Byte {
        if self.rendering_enabled() {
            match self.cycle {
                0..=63 => 0xFF,
                64..=255 => 0x00,
                256..=319 => 0xFF,
                _ => 0x00,
            }
        } else {
            self.oamdata.read(Addr::from(self.regs.oamaddr))
        }
    }
    fn peek_oamdata(&self) -> Byte {
        self.oamdata.peek(Addr::from(self.regs.oamaddr))
    }
    fn write_oamdata(&mut self, val: Byte) {
        self.oamdata.write(Addr::from(self.regs.oamaddr), val);
        self.regs.oamaddr = self.regs.oamaddr.wrapping_add(1);
    }

    /*
     * $2005 PPUSCROLL
     */

    fn write_ppuscroll(&mut self, val: Byte) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_scroll(val);
    }

    /*
     * $2006 PPUADDR
     */

    pub fn read_ppuaddr(&self) -> Addr {
        self.regs.read_addr()
    }
    fn write_ppuaddr(&mut self, val: Addr) {
        if self.cycle_count < POWER_ON_CYCLES {
            return;
        }
        self.regs.write_addr(val);
        self.vram.mapper_mut().vram_change(self.regs.v);
    }

    /*
     * $2007 PPUDATA
     */

    fn read_ppudata(&mut self) -> Byte {
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
        self.vram.mapper_mut().vram_change(self.regs.v);
        val
    }
    fn peek_ppudata(&self) -> Byte {
        let val = self.vram.peek(self.read_ppuaddr());
        if self.read_ppuaddr() <= 0x3EFF {
            self.vram.buffer
        } else {
            val
        }
    }
    fn write_ppudata(&mut self, val: Byte) {
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
        self.vram.mapper_mut().vram_change(self.regs.v);
    }
}

impl Clocked for Ppu {
    // http://wiki.nesdev.com/w/index.php/PPU_rendering
    fn clock(&mut self) -> usize {
        let clocks = match self.nes_format {
            NesFormat::Ntsc | NesFormat::Dendy => 3,
            NesFormat::Pal => {
                if self.clock_remainder == 5 {
                    self.clock_remainder = 0;
                    4
                } else {
                    self.clock_remainder += 1;
                    3
                }
            }
        };

        for _ in 0..clocks {
            self.run_cycle();

            if self.cycle == VISIBLE_CYCLE_START && self.scanline == VBLANK_SCANLINE {
                self.start_vblank();
            }
            // FIXME This is a bit of a hack - VBL should clear on cycle 1,
            // but something is off with timing and cycle 1 causes
            // 03-vbl_clear_time.nes/4.vbl_clear_timing.nes to fail.
            // Changing it to 2 makes them pass, but then causes 07-nmi_on_timing.nes
            // to fail so write_ppuctrl is changed as a result
            if self.cycle == VISIBLE_CYCLE_START + 1 && self.scanline == PRERENDER_SCANLINE {
                self.set_sprite_zero_hit(false);
                self.set_sprite_overflow(false);
                self.stop_vblank();
            }
            if self.debug && self.cycle == IDLE_CYCLE {
                if self.scanline == self.nt_scanline {
                    self.load_nametables();
                }
                if self.scanline == self.pat_scanline {
                    self.load_pattern_tables();
                    self.load_palettes();
                }
            }
        }
        clocks
    }
}

impl MemRead for Ppu {
    fn read(&mut self, addr: Addr) -> Byte {
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

    fn peek(&self, addr: Addr) -> Byte {
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
}

impl MemWrite for Ppu {
    fn write(&mut self, addr: Addr, val: Byte) {
        self.regs.open_bus = val;
        match addr {
            0x2000 => self.write_ppuctrl(val),             // PPUCTRL
            0x2001 => self.write_ppumask(val),             // PPUMASK
            0x2002 => (),                                  // PPUSTATUS is read-only
            0x2003 => self.write_oamaddr(val),             // OAMADDR
            0x2004 => self.write_oamdata(val),             // OAMDATA
            0x2005 => self.write_ppuscroll(val),           // PPUSCROLL
            0x2006 => self.write_ppuaddr(Addr::from(val)), // PPUADDR
            0x2007 => self.write_ppudata(val),             // PPUDATA
            _ => (),
        }
    }
}

impl Powered for Ppu {
    fn reset(&mut self) {
        self.cycle = 0;
        self.frame_cycles = 0;
        self.scanline = 0;
        self.scanline_phase = 0;
        self.regs.w = false;
        self.frame.reset();
        self.vram.reset();
        self.set_sprite_zero_hit(false);
        self.set_sprite_overflow(false);
        self.write_ppuctrl(0);
        self.write_ppumask(0);
        self.write_ppuscroll(0);
    }
    fn power_cycle(&mut self) {
        self.cycle = 0;
        self.frame_cycles = 0;
        self.scanline = 0;
        self.scanline_phase = 0;
        self.regs.w = false;
        self.frame.power_cycle();
        self.vram.power_cycle();
        self.set_sprite_zero_hit(false);
        self.set_sprite_overflow(false);
        self.write_ppuctrl(0);
        self.write_ppumask(0);
        self.write_oamaddr(0);
        self.write_ppuscroll(0);
        self.write_ppuaddr(0);
        self.cycle_count = 0; // This has to reset after register writes
    }
}

impl Savable for Ppu {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.cycle.save(fh)?;
        self.cycle_count.save(fh)?;
        self.frame_cycles.save(fh)?;
        self.scanline.save(fh)?;
        self.scanline_phase.save(fh)?;
        self.nmi_pending.save(fh)?;
        self.vram.save(fh)?;
        self.regs.save(fh)?;
        self.oamdata.save(fh)?;
        self.frame.save(fh)?;
        self.frame_complete.save(fh)?;
        self.ntsc_video.save(fh)?;
        self.nes_format.save(fh)?;
        self.clock_remainder.save(fh)?;
        // Ignore
        // debug
        // nt_scanline
        // pat_scanline
        // nametables
        // nametable_ids
        // pattern_tables
        // palette
        // palette_ids
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.cycle.load(fh)?;
        self.cycle_count.load(fh)?;
        self.frame_cycles.load(fh)?;
        self.scanline.load(fh)?;
        self.scanline_phase.load(fh)?;
        self.nmi_pending.load(fh)?;
        self.vram.load(fh)?;
        self.regs.load(fh)?;
        self.oamdata.load(fh)?;
        self.frame.load(fh)?;
        self.frame_complete.load(fh)?;
        self.ntsc_video.load(fh)?;
        self.nes_format.load(fh)?;
        self.clock_remainder.load(fh)?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper;

    #[test]
    fn ppu_scrolling_registers() {
        let mut ppu = Ppu::new();
        let mut mapper = Box::new(mapper::null());
        ppu.load_mapper(&mut mapper);
        while ppu.cycle_count < POWER_ON_CYCLES {
            ppu.clock();
        }

        let ppuctrl = 0x2000;
        let ppustatus = 0x2002;
        let ppuscroll = 0x2005;
        let ppuaddr = 0x2006;

        // Test write to ppuctrl
        let ctrl_write: Byte = 0b11; // Write two 1 bits
        let t_result: Addr = 0b11 << 10; // Make sure they're in the NN place of t
        ppu.write(ppuctrl, ctrl_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.v, 0);

        // Test read to ppustatus
        ppu.read(ppustatus);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuscroll
        let scroll_write: Byte = 0b0111_1101;
        let t_result: Addr = 0b000_11_00000_01111;
        let x_result: Addr = 0b101;
        ppu.write(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuscroll
        let scroll_write: Byte = 0b0101_1110;
        let t_result: Addr = 0b110_11_01011_01111;
        ppu.write(ppuscroll, scroll_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, false);

        // Test 1st write to ppuaddr
        let addr_write: Byte = 0b0011_1101;
        let t_result: Addr = 0b011_11_01011_01111;
        ppu.write(ppuaddr, addr_write);
        assert_eq!(ppu.regs.t, t_result);
        assert_eq!(ppu.regs.x, x_result);
        assert_eq!(ppu.regs.w, true);

        // Test 2nd write to ppuaddr
        let addr_write: Byte = 0b1111_0000;
        let t_result: Addr = 0b011_11_01111_10000;
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
        let t_result: Addr = 0b101_10_01100_10110;
        assert_eq!(ppu.regs.v, t_result);
    }
}
