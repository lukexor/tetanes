//! Picture Processing Unit
//!
//! http://wiki.nesdev.com/w/index.php/PPU

use crate::console::cartridge::Board;
use crate::console::cpu::Cycle;
use crate::console::memory::{Addr, Byte, Memory, Ram, Word, KILOBYTE};
use std::fmt;
use std::sync::{Arc, Mutex};

const SCREEN_HEIGHT: usize = 240;
const SCREEN_WIDTH: usize = 256;
const RENDER_SIZE: usize = SCREEN_HEIGHT * SCREEN_WIDTH * 3; // height * width 3 rgb pixels

const NAMETABLE_SIZE: usize = 2 * KILOBYTE; // two 1K nametables
const PALETTE_SIZE: usize = 32;
const OAM_SIZE: usize = 256;

const CYCLES_PER_SCANLINE: Cycle = 114; // 341 PPU cycles per scanline - 3 PPU cycles per 1 CPU cycle
const POSTRENDER_SCANLINE: Word = 240;
const VBLANK_SCANLINE: Word = 241;
const PRERENDER_SCANLINE: Word = 261;

pub struct Ppu {
    cycles: Cycle,
    vram: Vram,
    oam: Ram, // $2004 read/write
    image: [Byte; RENDER_SIZE],
    scanline: Word,
    ppudata_buffer: Byte,
    scroll: Scroll,

    // Registers
    ppuctrl: PpuCtrl,     // $2000 write-only
    ppumask: PpuMask,     // $2001 write-only
    ppustatus: PpuStatus, // $2002 read-only
    oamaddr: Byte,        // $2003 write-only
    ppuaddr: PpuAddr,     // $2006 write-only 2x
    ppuscroll: PpuScroll, // $2005 write-only 2x
}

struct Vram {
    board: Option<Arc<Mutex<Board>>>,
    nametable: Nametable,
    palette: Palette,
}

struct PpuCtrl(Byte);
struct PpuMask(Byte);
struct PpuStatus(Byte);

struct Scroll {
    x: Word,
    y: Word,
}

// Because PPUSCROLL is a 2x write, first write writes X, next writes Y
enum PpuScrollDir {
    X,
    Y,
}
struct PpuScroll {
    x: Byte,
    y: Byte,
    next: PpuScrollDir,
}

// Because PPUADDR is a 2x write, first write writes Hi, next writes Lo
enum PpuAddrByte {
    Hi,
    Lo,
}
struct PpuAddr {
    addr: Addr,
    next: PpuAddrByte,
}

struct Nametable([Byte; NAMETABLE_SIZE]);
struct Palette([Byte; PALETTE_SIZE]);

enum SpriteSize {
    Sprite8x8,
    Sprite8x16,
}

#[derive(Copy, Clone)]
struct Rgb(Byte, Byte, Byte);

pub struct StepResult {
    new_frame: bool,
    vblank_nmi: bool,
    scanline_irq: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            cycles: 0,
            vram: Vram::new(),
            oam: Ram::with_capacity(OAM_SIZE),
            image: [0; RENDER_SIZE],
            scanline: 0,
            ppudata_buffer: 0,
            scroll: Scroll { x: 0, y: 0 },
            ppuctrl: PpuCtrl(0),
            ppumask: PpuMask(0),
            ppustatus: PpuStatus(0),
            oamaddr: 0,
            ppuscroll: PpuScroll::new(),
            ppuaddr: PpuAddr::new(),
        }
    }

    pub fn set_board(&mut self, board: Arc<Mutex<Board>>) {
        self.vram.set_board(board);
    }

    // Step ticks as many cycles as needed to reach
    // target cycle to syncronize with the CPU
    pub fn step(&mut self, target_cycle: Cycle) -> StepResult {
        let mut step_result = StepResult {
            new_frame: false,
            vblank_nmi: false,
            scanline_irq: false,
        };
        loop {
            let next_cycle = self.cycles + CYCLES_PER_SCANLINE;
            if next_cycle > target_cycle {
                break;
            }

            // If we're still within visible scanlines, render
            if self.scanline < POSTRENDER_SCANLINE {
                self.render_scanline();
            }

            // Check if the mapper wants to send a scanline IRQ
            if let Some(b) = &self.vram.board {
                let mut board = b.lock().unwrap();
                step_result.scanline_irq = board.scanline_irq();
            }

            if self.scanline == PRERENDER_SCANLINE {
                step_result.new_frame = true;
                self.scanline = 0;
                self.ppustatus.set_sprite_zero_hit(false);
                self.ppustatus.set_vblank(false);
            } else if self.scanline == VBLANK_SCANLINE {
                self.start_vblank(&mut step_result);
            }

            self.cycles += CYCLES_PER_SCANLINE;
        }
        step_result
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
}

impl Memory for Ppu {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr & 0x2007 {
            0x2000 => 0, // PPUCTRL is write-only
            0x2001 => 0, // PPUMASK is write-only
            0x2002 => self.read_ppustatus(),
            0x2003 => 0, // OAMADDR is write-only
            0x2004 => self.oam.readb(Addr::from(self.oamaddr)),
            0x2005 => 0, // PPUSCROLL is write-only
            0x2006 => 0, // PPUADDR is write-only
            0x2007 => self.read_ppudata(),
            _ => panic!("impossible"),
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        match addr & 0x2007 {
            0x2000 => self.write_ppuctrl(val),
            0x2001 => self.ppumask.write(val),
            0x2002 => (), // PPUSTATUS is read-only
            0x2003 => self.oamaddr = val,
            0x2004 => {
                self.oam.writeb(Addr::from(self.oamaddr), val);
                self.oamaddr = self.oamaddr.wrapping_add(1);
            }
            0x2005 => self.write_ppuscroll(val),
            0x2006 => self.write_ppuaddr(val),
            0x2007 => self.write_ppudata(val),
            _ => panic!("impossible"),
        }
    }
}

impl Memory for Vram {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr {
            0x0000..=0x1FFF => {
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
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        match addr {
            0x0000..=0x1FFF => {
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
        self.0[addr as usize & (PALETTE_SIZE - 1)]
    }
    fn writeb(&mut self, addr: Addr, val: Byte) {
        let mut index = addr as usize & (PALETTE_SIZE - 1);
        if index == 16 {
            index = 0; // Mirrors sprite background into universal background
        }
        self.0[index] = val;
    }
}

impl Ppu {
    fn ppuaddr(&self) -> Addr {
        self.ppuaddr.read()
    }

    fn write_ppuctrl(&mut self, val: Byte) {
        let ctrl = &mut self.ppuctrl;
        ctrl.write(val);
        self.scroll.x = (self.scroll.x & 0xFF) | ctrl.x_scroll_offset();
        self.scroll.y = (self.scroll.y & 0xFF) | ctrl.y_scroll_offset();
    }

    fn write_ppuscroll(&mut self, val: Byte) {
        match self.ppuscroll.next {
            PpuScrollDir::X => {
                self.scroll.x = (self.scroll.x & 0xFF) | Word::from(val);
                self.ppuscroll.write_x(val);
            }
            PpuScrollDir::Y => {
                self.scroll.y = (self.scroll.y & 0xFF) | Word::from(val);
                self.ppuscroll.write_y(val);
            }
        }
    }

    fn read_ppustatus(&mut self) -> Byte {
        self.ppuscroll.reset();
        self.ppuaddr.reset();
        self.ppustatus.read()
    }

    fn write_ppuaddr(&mut self, val: Byte) {
        self.ppuaddr.write(val);

        // TODO something about scrolling
    }

    fn get_color(&self, palette_idx: u8) -> Rgb {
        PALETTE[palette_idx as usize]
    }

    fn read_ppudata(&mut self) -> Byte {
        let val = self.vram.readb(self.ppuaddr());
        self.ppuaddr.increment(self.ppuctrl.vram_increment());

        // Buffering quirk
        if self.ppuaddr() < 0x3F00 {
            let buffer = self.ppudata_buffer;
            self.ppudata_buffer = val;
            buffer
        } else {
            val
        }
    }

    fn write_ppudata(&mut self, val: Byte) {
        self.vram.writeb(self.ppuaddr(), val);
    }

    fn render_scanline(&mut self) {
        self.scanline += 1;
    }

    fn start_vblank(&mut self, step_result: &mut StepResult) {
        self.ppustatus.set_vblank(true);
        if self.ppuctrl.vblank_nmi() {
            step_result.vblank_nmi = true;
        }
    }
}

// VPHB SINN
// |||| ||++- Nametable Select: 0 = $2000 (upper-left); 1 = $2400 (upper-right);
// |||| ||                      2 = $2800 (lower-left); 3 = $2C00 (lower-right)
// |||| |||+-   Also For PPUSCROLL: 1 = Add 256 to X scroll
// |||| ||+--   Also For PPUSCROLL: 1 = Add 240 to Y scroll
// |||| |+--- VRAM Increment Mode: 0 = add 1, going across; 1 = add 32, going down
// |||| +---- Sprite Select for 8x8: 0 = $0000, 1 = $1000, ignored in 8x16 mode
// |||+------ Background Select: 0 = $0000, 1 = $1000
// ||+------- Sprite Height: 0 = 8x8, 1 = 8x16
// |+-------- PPU Master/Slave: 0 = read from EXT, 1 = write to EXT
// +--------- NMI Enable: NMI at next vblank: 0 = off, 1: on
impl PpuCtrl {
    pub fn write(&mut self, val: Byte) {
        self.0 = val;
    }

    fn x_scroll_offset(&self) -> Word {
        if self.0 & 0x01 > 0 {
            256
        } else {
            0
        }
    }
    fn y_scroll_offset(&self) -> Word {
        if self.0 & 0x02 > 0 {
            240
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
    fn sprite_size(&self) -> SpriteSize {
        if self.0 & 0x20 > 0 {
            SpriteSize::Sprite8x16
        } else {
            SpriteSize::Sprite8x8
        }
    }
    fn vblank_nmi(&self) -> bool {
        self.0 & 0x80 > 0
    }
}

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
    pub fn read(&self) -> Byte {
        0
    }

    pub fn write(&mut self, val: Byte) {
        unimplemented!()
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

// VSO. ....
// |||+-++++- Least significant bits previously written into a PPU register
// ||+------- Sprite overflow.
// |+-------- Sprite 0 Hit.
// +--------- Vertical blank has started (0: not in vblank; 1: in vblank)
impl PpuStatus {
    pub fn read(&mut self) -> Byte {
        self.0
    }

    pub fn write(&mut self, val: Byte) {
        unimplemented!()
    }

    fn set_sprite_overflow(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x20 } else { self.0 & 0x20 }
    }
    fn set_sprite_zero_hit(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x40 } else { self.0 & 0x40 }
    }
    fn set_vblank(&mut self, val: bool) {
        self.0 = if val { self.0 | 0x80 } else { self.0 & 0x80 }
    }
}

impl PpuScroll {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            next: PpuScrollDir::X,
        }
    }

    pub fn reset(&mut self) {
        self.next = PpuScrollDir::X;
    }

    pub fn write_x(&mut self, val: Byte) {
        self.x = val;
        self.next = PpuScrollDir::Y;
    }

    pub fn write_y(&mut self, val: Byte) {
        self.y = val;
        self.next = PpuScrollDir::X;
    }
}

impl PpuAddr {
    pub fn new() -> Self {
        Self {
            addr: 0,
            next: PpuAddrByte::Hi,
        }
    }

    pub fn reset(&mut self) {
        self.next = PpuAddrByte::Hi;
    }

    pub fn increment(&mut self, val: Word) {
        self.addr += val;
    }

    pub fn read(&self) -> Addr {
        self.addr
    }

    pub fn write(&mut self, val: Byte) {
        match self.next {
            PpuAddrByte::Hi => {
                self.write_hi(val);
            }
            PpuAddrByte::Lo => {
                self.write_lo(val);
            }
        }
    }

    pub fn write_hi(&mut self, val: Byte) {
        self.addr = (self.addr & 0x00FF) | (Addr::from(val) << 8);
        self.next = PpuAddrByte::Lo;
    }

    pub fn write_lo(&mut self, val: Byte) {
        self.addr = (self.addr & 0xFF00) | Addr::from(val);
        self.next = PpuAddrByte::Hi;
    }
}

impl fmt::Debug for Ppu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Ppu {{ \
             Cycles: {}, \
             vram: {:?}, \
             oam: {:?}, \
             scanline: {} \
             ppuctrl: 0x{:04X}, \
             ppumask: 0x{:04X}, \
             ppustatus: 0x{:04X}, \
             oamaddr: 0x{:04X}, \
             ppuscroll: {:?}, \
             ppuaddr: 0x{:04X} }}",
            self.cycles,
            self.vram,
            self.oam,
            self.scanline,
            self.ppuctrl.0,
            self.ppumask.0,
            self.ppustatus.0,
            self.oamaddr,
            self.ppuscroll,
            self.ppuaddr.addr,
        )
    }
}

impl fmt::Debug for Vram {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Vram {{ board: {:?}, nametable: {}KB, palette: {} }}",
            self.board,
            NAMETABLE_SIZE / KILOBYTE,
            PALETTE_SIZE,
        )
    }
}

impl fmt::Debug for PpuScroll {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PpuScroll {{ x: {}, y: {}}}", self.x, self.y)
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

#[rustfmt::skip]
const PALETTE: [Rgb; 64] = [
    Rgb(124, 124, 124), Rgb(0, 0, 252),     Rgb(0, 0, 188),     Rgb(68, 40, 188),
    Rgb(148, 0, 132),   Rgb(168, 0, 32),    Rgb(168, 16, 0),    Rgb(136, 20, 0),
    Rgb(80, 48, 0),     Rgb(0, 120, 0),     Rgb(0, 104, 0),     Rgb(0, 88, 0),
    Rgb(0, 64, 88),     Rgb(0, 0, 0),       Rgb(0, 0, 0),       Rgb(0, 0, 0),
    Rgb(188, 188, 188), Rgb(0, 120, 248),   Rgb(0, 88, 248),    Rgb(104, 68, 252),
    Rgb(216, 0, 204),   Rgb(228, 0, 88),    Rgb(248, 56, 0),    Rgb(228, 92, 16),
    Rgb(172, 124, 0),   Rgb(0, 184, 0),     Rgb(0, 168, 0),     Rgb(0, 168, 68),
    Rgb(0, 136, 136),   Rgb(0, 0, 0),       Rgb(0, 0, 0),       Rgb(0, 0, 0),
    Rgb(248, 248, 248), Rgb(60,  188, 252), Rgb(104, 136, 252), Rgb(152, 120, 248),
    Rgb(248, 120, 248), Rgb(248, 88, 152),  Rgb(248, 120, 88),  Rgb(252, 160, 68),
    Rgb(248, 184, 0),   Rgb(184, 248, 24),  Rgb(88, 216, 84),   Rgb(88, 248, 152),
    Rgb(0, 232, 216),   Rgb(120, 120, 120), Rgb(0, 0, 0),       Rgb(0, 0, 0),
    Rgb(252, 252, 252), Rgb(164, 228, 252), Rgb(184, 184, 248), Rgb(216, 184, 248),
    Rgb(248, 184, 248), Rgb(248, 164, 192), Rgb(240, 208, 176), Rgb(252, 224, 168),
    Rgb(248, 216, 120), Rgb(216, 248, 120), Rgb(184, 248, 184), Rgb(184, 248, 216),
    Rgb(0, 252, 252),   Rgb(216, 216, 216), Rgb(0, 0, 0),       Rgb(0, 0, 0),
];
