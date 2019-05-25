use crate::cartridge::Cartridge;
use crate::console::ppu::Ppu;
use crate::mapper::{Mapper, MapperRef, Mirroring};
use crate::memory::Memory;
use crate::serialization::Savable;
use crate::util::Result;
use std::cell::RefCell;
use std::fmt;
use std::io::{Read, Write};
use std::rc::Rc;

/// SxRom (Mapper 1/MMC1)
///
/// http://wiki.nesdev.com/w/index.php/SxROM
/// http://wiki.nesdev.com/w/index.php/MMC1

pub struct Sxrom {
    cart: Cartridge,
    mirroring: Mirroring,
    shift_register: u8, // Write every 5th write
    ctrl: u8,
    prg_mode: u8,
    chr_mode: u8,
    prg_bank: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_offsets: [i32; 2],
    chr_offsets: [i32; 2],
}

impl Sxrom {
    pub fn load(cart: Cartridge) -> MapperRef {
        let mut sxrom = Self {
            cart,
            mirroring: Mirroring::Horizontal,
            shift_register: 0x10,
            ctrl: 0x0C,
            prg_mode: 0u8,
            chr_mode: 0u8,
            prg_bank: 0u8,
            chr_bank0: 0u8,
            chr_bank1: 0u8,
            prg_offsets: [0i32; 2],
            chr_offsets: [0i32; 2],
        };
        sxrom.prg_offsets[1] = sxrom.prg_bank_offset(-1);
        Rc::new(RefCell::new(sxrom))
    }

    // Writes data into a shift register. At every 5th
    // write, the data is written out to the SxRom registers
    // and the shift register is cleared
    fn write_register(&mut self, addr: u16, val: u8) {
        // Check reset
        if val & 0x80 == 0x80 {
            self.shift_register = 0x10;
            self.write_control(self.ctrl | 0x0C);
        } else {
            // Check if its time to write
            let write = self.shift_register & 1 == 1;
            // Move shift register and write lowest bit of val
            self.shift_register >>= 1;
            self.shift_register |= (val & 1) << 4;
            if write {
                match addr {
                    0x8000..=0x9FFF => self.write_control(self.shift_register),
                    0xA000..=0xBFFF => {
                        self.chr_bank0 = self.shift_register;
                        self.update_offsets();
                    }
                    0xC000..=0xDFFF => {
                        self.chr_bank1 = self.shift_register;
                        self.update_offsets();
                    }
                    0xE000..=0xFFFF => {
                        self.prg_bank = self.shift_register;
                        self.update_offsets();
                    }
                    _ => panic!("impossible write"),
                }
                self.shift_register = 0x10;
            }
        }
    }

    fn write_control(&mut self, val: u8) {
        self.ctrl = val;
        self.chr_mode = (val >> 4) & 1;
        self.prg_mode = (val >> 2) & 3;
        self.mirroring = match val & 3 {
            0 => Mirroring::SingleScreen0,
            1 => Mirroring::SingleScreen1,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => panic!("impossible mirroring mode"),
        };
        self.update_offsets();
    }

    fn prg_bank_offset(&self, mut index: i32) -> i32 {
        if index >= 0x80 {
            index -= 0x100;
        }
        let len = self.cart.prg_rom.len() as i32;
        index %= len / 0x4000;
        let mut offset = index * 0x4000;
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
        index %= len / 0x1000;
        let mut offset = index * 0x1000;
        if offset < 0 {
            offset += len;
        }
        offset
    }

    fn update_offsets(&mut self) {
        match self.prg_mode {
            0 | 1 => {
                self.prg_offsets[0] = self.prg_bank_offset(i32::from(self.prg_bank & 0xFE));
                self.prg_offsets[1] = self.prg_bank_offset(i32::from(self.prg_bank | 0x01));
            }
            2 => {
                self.prg_offsets[0] = 0;
                self.prg_offsets[1] = self.prg_bank_offset(i32::from(self.prg_bank));
            }
            3 => {
                self.prg_offsets[0] = self.prg_bank_offset(i32::from(self.prg_bank));
                self.prg_offsets[1] = self.prg_bank_offset(-1);
            }
            _ => panic!("impossible prg_mode"),
        }

        if self.chr_mode == 1 {
            self.chr_offsets[0] = self.chr_bank_offset(i32::from(self.chr_bank0));
            self.chr_offsets[1] = self.chr_bank_offset(i32::from(self.chr_bank1));
        } else {
            self.chr_offsets[0] = self.chr_bank_offset(i32::from(self.chr_bank0 & 0xFE));
            self.chr_offsets[1] = self.chr_bank_offset(i32::from(self.chr_bank1 | 0x01));
        }
    }
}

impl Mapper for Sxrom {
    fn irq_pending(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn step(&mut self, _ppu: &Ppu) {}
    fn cart(&self) -> &Cartridge {
        &self.cart
    }
    fn cart_mut(&mut self) -> &mut Cartridge {
        &mut self.cart
    }
}

impl Memory for Sxrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // PPU 4 KB switchable CHR bank
            0x0000..=0x1FFF => {
                let bank = addr / 0x1000;
                let offset = addr % 0x1000;
                let idx = self.chr_offsets[bank as usize] + i32::from(offset);
                self.cart.chr_rom[idx as usize]
            }
            // CPU 8 KB PRG RAM bank, (optional)
            0x6000..=0x7FFF => self.cart.sram[(addr - 0x6000) as usize],
            // CPU 2x16 KB PRG ROM bank, either switchable or fixed to the first bank
            0x8000..=0xFFFF => {
                let addr = addr - 0x8000;
                let bank = addr / 0x4000;
                let offset = addr % 0x4000;
                let idx = self.prg_offsets[bank as usize] + i32::from(offset);
                self.cart.prg_rom[idx as usize]
            }
            _ => {
                eprintln!("unhandled Sxrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            // PPU 4 KB switchable CHR bank
            0x0000..=0x1FFF => {
                let bank = addr / 0x1000;
                let offset = addr % 0x1000;
                let idx = self.chr_offsets[bank as usize] + i32::from(offset);
                self.cart.chr_rom[idx as usize] = val;
            }
            // CPU 8 KB PRG RAM bank, (optional)
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

impl Savable for Sxrom {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.cart.save(fh)?;
        self.mirroring.save(fh)?;
        self.shift_register.save(fh)?;
        self.ctrl.save(fh)?;
        self.prg_mode.save(fh)?;
        self.chr_mode.save(fh)?;
        self.prg_bank.save(fh)?;
        self.chr_bank0.save(fh)?;
        self.chr_bank1.save(fh)?;
        self.prg_offsets.save(fh)?;
        self.chr_offsets.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        self.cart.load(fh)?;
        self.mirroring.load(fh)?;
        self.shift_register.load(fh)?;
        self.ctrl.load(fh)?;
        self.prg_mode.load(fh)?;
        self.chr_mode.load(fh)?;
        self.prg_bank.load(fh)?;
        self.chr_bank0.load(fh)?;
        self.chr_bank1.load(fh)?;
        self.prg_offsets.load(fh)?;
        self.chr_offsets.load(fh)?;
        Ok(())
    }
}

impl fmt::Debug for Sxrom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Sxrom {{ cart: {:?}, mirroring: {:?} }}",
            self.cart,
            self.mirroring()
        )
    }
}
