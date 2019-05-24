use crate::cartridge::Cartridge;
use crate::console::ppu::{Ppu, VISIBLE_SCANLINE_END};
use crate::mapper::Mirroring;
use crate::mapper::{Mapper, MapperRef};
use crate::memory::Memory;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// TxRom (Mapper 4)
///
/// https://wiki.nesdev.com/w/index.php/TxROM

pub struct Txrom {
    cart: Cartridge,
    irq_enable: bool,
    counter: u8,
    reload: u8,
    prg_mode: u8,
    chr_mode: u8,
    bank_select: u8,
    banks: [u8; 8],
    prg_offsets: [i32; 4],
    chr_offsets: [i32; 8],
    mirroring: Mirroring,
}

impl Txrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let mut txrom = Self {
            cart,
            irq_enable: false,
            counter: 0u8,
            reload: 0u8,
            prg_mode: 0u8,
            chr_mode: 0u8,
            bank_select: 0u8,
            banks: [0u8; 8],
            prg_offsets: [0i32; 4],
            chr_offsets: [0i32; 8],
            mirroring: Mirroring::Horizontal,
        };
        txrom.prg_offsets[0] = txrom.prg_bank_offset(0);
        txrom.prg_offsets[1] = txrom.prg_bank_offset(1);
        txrom.prg_offsets[2] = txrom.prg_bank_offset(-2);
        txrom.prg_offsets[3] = txrom.prg_bank_offset(-1);
        Rc::new(RefCell::new(txrom))
    }

    fn write_register(&mut self, addr: u16, val: u8) {
        let addr_even = addr % 2 == 0;
        match addr {
            0x8000..=0x9FFF if addr_even => self.write_bank_select(val),
            0x8000..=0x9FFF if !addr_even => self.write_bank(val),
            0xA000..=0xBFFF if addr_even => self.write_mirror(val),
            0xA000..=0xBFFF if !addr_even => (), // write protect
            0xC000..=0xDFFF if addr_even => self.reload = val,
            0xC000..=0xDFFF if !addr_even => self.counter = 0,
            0xE000..=0xFFFF if addr_even => self.irq_enable = false,
            0xE000..=0xFFFF if !addr_even => self.irq_enable = true,
            _ => (),
        }
    }

    fn write_bank_select(&mut self, val: u8) {
        self.prg_mode = (val >> 6) & 0x01;
        self.chr_mode = (val >> 7) & 0x01;
        self.bank_select = val & 0x07;
        self.update_offsets();
    }

    fn write_bank(&mut self, val: u8) {
        self.banks[self.bank_select as usize] = val;
        self.update_offsets();
    }

    fn write_mirror(&mut self, val: u8) {
        if val & 0x01 == 0x01 {
            self.mirroring = Mirroring::Horizontal;
        } else {
            self.mirroring = Mirroring::Vertical;
        }
    }

    fn prg_bank_offset(&self, mut index: i32) -> i32 {
        if index >= 0x80 {
            index -= 0x100;
        }
        let len = self.cart.prg_rom.len() as i32;
        index %= len / 0x2000;
        let mut offset = index * 0x2000;
        if offset < 0 {
            offset += len;
        }
        offset
    }

    fn chr_bank_offset(&self, mut index: i32) -> i32 {
        if index >= 0x80 {
            index -= 0x100;
        }
        let len = self.cart.chr_rom.len() as i32;
        index %= len / 0x0400;
        let mut offset = index * 0x0400;
        if offset < 0 {
            offset += len;
        }
        offset
    }

    fn update_offsets(&mut self) {
        match self.prg_mode {
            0 => {
                self.prg_offsets[0] = self.prg_bank_offset(i32::from(self.banks[6]));
                self.prg_offsets[1] = self.prg_bank_offset(i32::from(self.banks[7]));
                self.prg_offsets[2] = self.prg_bank_offset(-2);
                self.prg_offsets[3] = self.prg_bank_offset(-1);
            }
            1 => {
                self.prg_offsets[0] = self.prg_bank_offset(-2);
                self.prg_offsets[1] = self.prg_bank_offset(i32::from(self.banks[7]));
                self.prg_offsets[2] = self.prg_bank_offset(i32::from(self.banks[6]));
                self.prg_offsets[3] = self.prg_bank_offset(-1);
            }
            _ => panic!("impossible prg_mode"),
        }

        if self.chr_mode == 0 {
            self.chr_offsets[0] = self.chr_bank_offset(i32::from(self.banks[0] & 0xFE));
            self.chr_offsets[1] = self.chr_bank_offset(i32::from(self.banks[0] | 0x01));
            self.chr_offsets[2] = self.chr_bank_offset(i32::from(self.banks[1] & 0xFE));
            self.chr_offsets[3] = self.chr_bank_offset(i32::from(self.banks[1] | 0x01));
            self.chr_offsets[4] = self.chr_bank_offset(i32::from(self.banks[2]));
            self.chr_offsets[5] = self.chr_bank_offset(i32::from(self.banks[3]));
            self.chr_offsets[6] = self.chr_bank_offset(i32::from(self.banks[4]));
            self.chr_offsets[7] = self.chr_bank_offset(i32::from(self.banks[5]));
        } else {
            self.chr_offsets[0] = self.chr_bank_offset(i32::from(self.banks[2]));
            self.chr_offsets[1] = self.chr_bank_offset(i32::from(self.banks[3]));
            self.chr_offsets[2] = self.chr_bank_offset(i32::from(self.banks[4]));
            self.chr_offsets[3] = self.chr_bank_offset(i32::from(self.banks[5]));
            self.chr_offsets[4] = self.chr_bank_offset(i32::from(self.banks[0] & 0xFE));
            self.chr_offsets[5] = self.chr_bank_offset(i32::from(self.banks[0] | 0x01));
            self.chr_offsets[6] = self.chr_bank_offset(i32::from(self.banks[1] & 0xFE));
            self.chr_offsets[7] = self.chr_bank_offset(i32::from(self.banks[1] | 0x01));
        }
    }
}

impl Memory for Txrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = addr / 0x0400;
                let offset = addr % 0x0400;
                let idx = self.chr_offsets[bank as usize] + i32::from(offset);
                self.cart.chr_rom[idx as usize]
            }
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                let addr = addr - 0x8000;
                let bank = addr / 0x2000;
                let offset = addr % 0x2000;
                let idx = self.prg_offsets[bank as usize] + i32::from(offset);
                self.cart.prg_rom[idx as usize]
            }
            _ => {
                eprintln!("unhandled Uxrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                let bank = addr / 0x0400;
                let offset = addr % 0x0400;
                let idx = self.chr_offsets[bank as usize] + i32::from(offset);
                self.cart.chr_rom[idx as usize] = val;
            }
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize] = val,
            0x8000..=0xFFFF => self.write_register(addr, val),
            _ => {
                eprintln!(
                    "unhandled Sxrom writeb at address: 0x{:04X} - val: 0x{:02X}",
                    addr, val
                );
            }
        }
    }
}

impl Mapper for Txrom {
    fn irq_pending(&self) -> bool {
        if self.counter == 0 && self.irq_enable {
            true
        } else {
            false
        }
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn step(&mut self, ppu: &Ppu) {
        if ppu.cycle == 280 && ppu.scanline <= VISIBLE_SCANLINE_END && ppu.rendering_enabled() {
            if self.counter > 0 {
                self.counter -= 1;
            } else {
                self.counter = self.reload;
            }
        }
    }
    fn cart(&self) -> &Cartridge {
        &self.cart
    }
    fn cart_mut(&mut self) -> &mut Cartridge {
        &mut self.cart
    }
}

impl fmt::Debug for Txrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Uxrom {{ cart: {:?}, mirroring: {:?} }}",
            self.cart,
            self.mirroring()
        )
    }
}
