//! `Jaleco SS88006` (Mapper 018).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_018>

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Jaleco SS88006` page bit.
#[derive(Debug)]
#[must_use]
enum PageBit {
    Low,
    High,
}

impl PageBit {
    /// Merge a 4-bit write into the low or high nibble of an existing bank index.
    const fn page(&self, page: u16, val: u8) -> u16 {
        let val = (val as u16) & 0x0F;
        match self {
            PageBit::Low => (page & 0xF0) | val,
            PageBit::High => (val << 4) | (page & 0x0F),
        }
    }
}

impl From<u16> for PageBit {
    fn from(addr: u16) -> Self {
        if addr & 0x01 == 0x01 {
            Self::High
        } else {
            Self::Low
        }
    }
}

/// `Jaleco SS88006` registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub irq_reload: [u8; 4],
    pub irq_counter_size: u8,
}

/// `Jaleco SS88006` (Mapper 018).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct JalecoSs88006 {
    pub regs: Regs,
    pub irq_counter: u16,
    pub mirroring: Mirroring,
    /// 8x 1K CHR banks, each written as two 4-bit halves.
    pub chr_banks: [u16; 8],
    /// 3x 8K PRG-ROM banks, likewise written in halves. $E000 is fixed to the last bank.
    pub prg_banks: [u16; 3],
    /// PRG-RAM access, from the $9002 register.
    pub prg_ram_readable: bool,
    pub prg_ram_writable: bool,
}

impl JalecoSs88006 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const IRQ_MASKS: [u16; 4] = [0xFFFF, 0x0FFF, 0x00FF, 0x000F];

    // PPU $0000..=$1FFF 8x 1K CHR Banks
    // CPU $6000..=$7FFF 8K PRG-RAM, access controlled by $9002
    // CPU $8000..=$DFFF 3x 8K PRG-ROM Banks Switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            regs: Regs::default(),
            irq_counter: 0,
            mirroring: cart.mirroring(),
            chr_banks: [0; 8],
            prg_banks: [0; 3],
            prg_ram_readable: true,
            prg_ram_writable: true,
        };
        board.sync(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for JalecoSs88006 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        let bits = PageBit::from(addr);
        match addr & 0xF003 {
            0x8000 | 0x8001 => self.prg_banks[0] = bits.page(self.prg_banks[0], val),
            0x8002 | 0x8003 => self.prg_banks[1] = bits.page(self.prg_banks[1], val),
            0x9000 | 0x9001 => self.prg_banks[2] = bits.page(self.prg_banks[2], val),
            0x9002 => {
                self.prg_ram_readable = val & 0x01 == 0x01;
                self.prg_ram_writable = self.prg_ram_readable && val & 0x02 == 0x02;
            }
            0xA000 | 0xA001 => self.chr_banks[0] = bits.page(self.chr_banks[0], val),
            0xA002 | 0xA003 => self.chr_banks[1] = bits.page(self.chr_banks[1], val),
            0xB000 | 0xB001 => self.chr_banks[2] = bits.page(self.chr_banks[2], val),
            0xB002 | 0xB003 => self.chr_banks[3] = bits.page(self.chr_banks[3], val),
            0xC000 | 0xC001 => self.chr_banks[4] = bits.page(self.chr_banks[4], val),
            0xC002 | 0xC003 => self.chr_banks[5] = bits.page(self.chr_banks[5], val),
            0xD000 | 0xD001 => self.chr_banks[6] = bits.page(self.chr_banks[6], val),
            0xD002 | 0xD003 => self.chr_banks[7] = bits.page(self.chr_banks[7], val),
            0xE000..=0xE003 => self.regs.irq_reload[(addr & 0x03) as usize] = val,
            0xF000 => {
                self.regs.irq_pending = false;
                self.irq_counter = u16::from(self.regs.irq_reload[0])
                    | (u16::from(self.regs.irq_reload[1]) << 4)
                    | (u16::from(self.regs.irq_reload[2]) << 8)
                    | (u16::from(self.regs.irq_reload[3]) << 12);
            }
            0xF001 => {
                self.regs.irq_enabled = val & 0x01 == 0x01;
                self.regs.irq_pending = false;
                self.regs.irq_counter_size = if val & 0x08 == 0x08 {
                    3
                } else if val & 0x04 == 0x04 {
                    2
                } else if val & 0x02 == 0x02 {
                    1
                } else {
                    0
                };
            }
            0xF002 => {
                self.mirroring = match val & 0x03 {
                    0b00 => Mirroring::Horizontal,
                    0b01 => Mirroring::Vertical,
                    0b10 => Mirroring::SingleScreenA,
                    _ => Mirroring::SingleScreenB,
                };
            }
            // $F003 selects expansion audio, which is not emulated.
            0xF003 => (),
            _ => (),
        }
        self.sync(memory);
    }

    fn sync(&mut self, memory: &mut Memory) {
        for (slot, bank) in self.chr_banks.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);

        if self.prg_ram_readable {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
            memory.set_prg_writable(0x6000, Self::PRG_RAM_WINDOW, self.prg_ram_writable);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        if self.regs.irq_enabled {
            let irq_mask = Self::IRQ_MASKS[self.regs.irq_counter_size as usize];
            let counter = self.irq_counter & irq_mask;
            if counter == 0 {
                self.regs.irq_pending = true;
            }
            self.irq_counter =
                (self.irq_counter & !irq_mask) | (counter.wrapping_sub(1) & irq_mask);
        }
    }

    fn reset(&mut self, _kind: ResetKind) {
        // The last PRG slot is fixed in `sync`, which the caller runs after reset.
        self.regs = Regs::default();
    }
}
