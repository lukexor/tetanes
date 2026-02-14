//! `Jaleco SS88006` (Mapper 018).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_018>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    cpu::{Cpu, Irq},
    mapper::{
        self, MapRead, MapWrite, MappedRead, MappedWrite, Mapper, Mirrored, OnBusRead, OnBusWrite,
    },
    mem::{BankAccess, Banks},
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
    const fn page(&self, page: usize, val: u8) -> usize {
        let val = (val as usize) & 0x0F;
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
    pub chr_banks: Banks,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl JalecoSs88006 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    const IRQ_MASKS: [u16; 4] = [0xFFFF, 0x0FFF, 0x00FF, 0x000F];

    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        if !cart.has_prg_ram() {
            cart.add_prg_ram(Self::PRG_RAM_SIZE);
        }
        let mut jalecoss88006 = Self {
            regs: Regs::default(),
            irq_counter: 0,
            mirroring: cart.mirroring(),
            chr_banks: Banks::new(0x0000, 0x1FFF, cart.chr_rom.len(), Self::CHR_WINDOW)?,
            prg_ram_banks: Banks::new(0x6000, 0x7FFF, cart.prg_ram.len(), Self::PRG_WINDOW)?,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW)?,
        };
        jalecoss88006
            .prg_rom_banks
            .set(3, jalecoss88006.prg_rom_banks.last());
        Ok(jalecoss88006.into())
    }

    fn update_prg_bank(&mut self, bank: usize, val: u8, bits: PageBit) {
        self.prg_rom_banks
            .set(bank, bits.page(self.prg_rom_banks.page(bank), val));
    }

    fn update_chr_bank(&mut self, bank: usize, val: u8, bits: PageBit) {
        self.chr_banks
            .set(bank, bits.page(self.chr_banks.page(bank), val));
    }
}

impl Mirrored for JalecoSs88006 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MapRead for JalecoSs88006 {
    // PPU $0000..=$03FF: 1K CHR Bank 1 Switchable
    // PPU $0400..=$07FF: 1K CHR Bank 2 Switchable
    // PPU $0800..=$0BFF: 1K CHR Bank 3 Switchable
    // PPU $0C00..=$0FFF: 1K CHR Bank 4 Switchable
    // PPU $1000..=$13FF: 1K CHR Bank 5 Switchable
    // PPU $1400..=$17FF: 1K CHR Bank 6 Switchable
    // PPU $1800..=$1BFF: 1K CHR Bank 7 Switchable
    // PPU $1C00..=$1FFF: 1K CHR Bank 8 Switchable
    //
    // CPU $6000..=$7FFF: 8K PRG-RAM Bank, if WRAM is present
    // CPU $8000..=$9FFF: 8K PRG-ROM Bank 1 Switchable
    // CPU $A000..=$BFFF: 8K PRG-ROM Bank 2 Switchable
    // CPU $C000..=$DFFF: 8K PRG-ROM Bank 3 Switchable
    // CPU $E000..=$FFFF: 8K PRG-ROM Bank 4 Fixed to last

    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x6000..=0x7FFF if self.prg_ram_banks.readable(addr) => {
                MappedRead::PrgRam(self.prg_ram_banks.translate(addr))
            }
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Bus,
        }
    }
}

impl MapWrite for JalecoSs88006 {
    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_banks.writable(addr) {
                    return MappedWrite::PrgRam(self.prg_ram_banks.translate(addr), val);
                }
            }
            _ => match addr & 0xF003 {
                0x8000 | 0x8001 => self.update_prg_bank(0, val, PageBit::from(addr)),
                0x8002 | 0x8003 => self.update_prg_bank(1, val, PageBit::from(addr)),
                0x9000 | 0x9001 => self.update_prg_bank(2, val, PageBit::from(addr)),
                0x9002 => {
                    let prg_ram_access = if val & 0x01 == 0x01 {
                        if val & 0x02 == 0x02 {
                            BankAccess::ReadWrite
                        } else {
                            BankAccess::Read
                        }
                    } else {
                        BankAccess::None
                    };
                    self.prg_ram_banks.set_access(0, prg_ram_access);
                }
                0xA000 | 0xA001 => self.update_chr_bank(0, val, PageBit::from(addr)),
                0xA002 | 0xA003 => self.update_chr_bank(1, val, PageBit::from(addr)),
                0xB000 | 0xB001 => self.update_chr_bank(2, val, PageBit::from(addr)),
                0xB002 | 0xB003 => self.update_chr_bank(3, val, PageBit::from(addr)),
                0xC000 | 0xC001 => self.update_chr_bank(4, val, PageBit::from(addr)),
                0xC002 | 0xC003 => self.update_chr_bank(5, val, PageBit::from(addr)),
                0xD000 | 0xD001 => self.update_chr_bank(6, val, PageBit::from(addr)),
                0xD002 | 0xD003 => self.update_chr_bank(7, val, PageBit::from(addr)),
                0xE000..=0xE003 => self.regs.irq_reload[(addr & 0x03) as usize] = val,
                0xF000 => {
                    Cpu::clear_irq(Irq::MAPPER);
                    self.irq_counter = u16::from(self.regs.irq_reload[0])
                        | (u16::from(self.regs.irq_reload[1]) << 4)
                        | (u16::from(self.regs.irq_reload[2]) << 8)
                        | (u16::from(self.regs.irq_reload[3]) << 12);
                }
                0xF001 => {
                    Cpu::clear_irq(Irq::MAPPER);
                    self.regs.irq_enabled = val & 0x01 == 0x01;
                    if val & 0x08 == 0x08 {
                        self.regs.irq_counter_size = 3;
                    } else if val & 0x04 == 0x04 {
                        self.regs.irq_counter_size = 2;
                    } else if val & 0x02 == 0x02 {
                        self.regs.irq_counter_size = 1;
                    } else {
                        self.regs.irq_counter_size = 0;
                    }
                }
                0xF002 => self.set_mirroring(match val & 0x03 {
                    0 => Mirroring::Horizontal,
                    1 => Mirroring::Vertical,
                    2 => Mirroring::SingleScreenA,
                    3 => Mirroring::SingleScreenB,
                    _ => unreachable!("invalid mirroring mode: ${val:02X}"),
                }),
                0xF003 => {
                    // TODO: Expansion audio
                }
                _ => (),
            },
        }
        MappedWrite::Bus
    }
}

impl Reset for JalecoSs88006 {
    fn reset(&mut self, kind: ResetKind) {
        self.regs = Regs::default();
        if kind == ResetKind::Hard {
            self.prg_rom_banks.set(3, self.prg_rom_banks.last());
        }
    }
}

impl Clock for JalecoSs88006 {
    fn clock(&mut self) {
        if self.regs.irq_enabled {
            let irq_mask = Self::IRQ_MASKS[self.regs.irq_counter_size as usize];
            let counter = self.irq_counter & irq_mask;
            if counter == 0 {
                Cpu::set_irq(Irq::MAPPER);
            }
            self.irq_counter =
                (self.irq_counter & !irq_mask) | (counter.wrapping_sub(1) & irq_mask);
        }
    }
}

impl OnBusRead for JalecoSs88006 {}
impl OnBusWrite for JalecoSs88006 {}
impl Regional for JalecoSs88006 {}
impl Sram for JalecoSs88006 {}
