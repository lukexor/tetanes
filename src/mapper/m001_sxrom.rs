//! `SxROM`/`MMC1` (Mapper 001)
//!
//! <http://wiki.nesdev.com/w/index.php/SxROM>
//! <http://wiki.nesdev.com/w/index.php/MMC1>

use crate::{
    cart::Cart,
    common::{Clock, Kind, Regional, Reset},
    mapper::{Mapped, MappedRead, MappedWrite, Mapper, MemMap},
    mem::MemBanks,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Mmc1Revision {
    /// MMC1 Revision A
    A,
    /// MMC1 Revisions B & C
    BC,
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
struct SxRegs {
    write_just_occurred: u8,
    shift_register: u8, // $8000-$FFFF - 5 bit shift register
    control: u8,        // $8000-$9FFF
    chr0: u8,           // $A000-$BFFF
    chr1: u8,           // $C000-$DFFF
    prg: u8,            // $E000-$FFFF
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sxrom {
    regs: SxRegs,
    submapper_num: u8,
    mirroring: Mirroring,
    board: Mmc1Revision,
    chr_select: bool,
    // PPU $0000..=$1FFF 4K CHR-ROM/RAM Bank Switchable
    chr_banks: MemBanks,
    // CPU $6000..=$7FFF 8K PRG-RAM Bank (optional)
    prg_ram_banks: MemBanks,
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable or Fixed to First Bank
    // CPU $C000..=$FFFF 16K PRG-ROM Bank Fixed to Last Bank or Switchable
    prg_rom_banks: MemBanks,
}

impl Sxrom {
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_RAM_SIZE: usize = 32 * 1024; // 32K is safely compatible sans NES 2.0 header
    const CHR_RAM_SIZE: usize = 8 * 1024;

    const SHIFT_REG_RESET: u8 = 0x80; // Reset shift register when bit 7 is set
    const DEFAULT_SHIFT_REGISTER: u8 = 0x10; // 0b10000 the 1 is used to tell when register is full
    const MIRRORING_MASK: u8 = 0x03; // 0b00011
    const PRG_MODE_MASK: u8 = 0x0C; // 0b01100
    const CHR_MODE_MASK: u8 = 0x10; // 0b10000

    const DEFAULT_PRG_MODE: u8 = 0x0C; // Mode 3, 16k Fixed Last
    const PRG_BANK_MASK: u8 = 0x0F;
    const PRG_RAM_DISABLED: u8 = 0x10; // 0b10000

    pub fn load(cart: &mut Cart, board: Mmc1Revision) -> Mapper {
        if !cart.has_prg_ram() {
            cart.add_prg_ram(Self::PRG_RAM_SIZE);
        }
        let chr_len = if cart.has_chr_rom() {
            cart.chr_rom.len()
        } else {
            cart.add_chr_ram(Self::CHR_RAM_SIZE);
            cart.chr_ram.len()
        };
        let mut sxrom = Self {
            regs: SxRegs {
                write_just_occurred: 0x00,
                shift_register: Self::DEFAULT_SHIFT_REGISTER,
                control: Self::DEFAULT_PRG_MODE,
                chr0: 0x00,
                chr1: 0x00,
                prg: 0x00,
            },
            submapper_num: cart.submapper_num(),
            mirroring: Mirroring::SingleScreenA,
            board,
            chr_select: cart.prg_rom.len() == 0x80000,
            chr_banks: MemBanks::new(0x0000, 0x1FFF, chr_len, Self::CHR_WINDOW),
            prg_ram_banks: MemBanks::new(0x6000, 0x7FFF, cart.prg_ram.len(), Self::PRG_RAM_WINDOW),
            prg_rom_banks: MemBanks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_ROM_WINDOW),
        };
        sxrom.update_banks(0x0000);
        sxrom.into()
    }

    /// Writes data into a shift register. At every 5th
    /// write, the data is written out to the `SxROM` registers
    /// and the shift register is cleared
    ///
    /// Load Register $8000-$FFFF
    /// 7654 3210
    /// Rxxx xxxD
    /// |       +- Data bit to be shifted into shift register, LSB first
    /// +--------- 1: Reset shift register and write control with (Control OR $0C),
    ///               locking PRG-ROM at $C000-$FFFF to the last bank.
    ///
    /// Control $8000-$9FFF
    /// 43210
    /// CPPMM
    /// |||++- Mirroring (0: one-screen, lower bank; 1: one-screen, upper bank;
    /// |||               2: vertical; 3: horizontal)
    /// |++--- PRG-ROM bank mode (0, 1: switch 32K at $8000, ignoring low bit of bank number;
    /// |                         2: fix first bank at $8000 and switch 16K bank at $C000;
    /// |                         3: fix last bank at $C000 and switch 16K bank at $8000)
    /// +----- CHR-ROM bank mode (0: switch 8K at a time; 1: switch two separate 4K banks)
    ///
    /// CHR bank 0 $A000-$BFFF
    /// 42310
    /// CCCCC
    /// +++++- Select 4K or 8K CHR bank at PPU $0000 (low bit ignored in 8K mode)
    ///
    /// CHR bank 1 $C000-$DFFF
    /// 43210
    /// CCCCC
    /// +++++- Select 4K CHR bank at PPU $1000 (ignored in 8K mode)
    ///
    /// For Mapper001
    /// $A000 and $C000:
    /// 43210
    /// EDCBA
    /// |||||
    /// ||||+- CHR A12
    /// |||+-- CHR A13, if extant (CHR >= 16k)
    /// ||+--- CHR A14, if extant; and PRG-RAM A14, if extant (PRG-RAM = 32k)
    /// |+---- CHR A15, if extant; and PRG-RAM A13, if extant (PRG-RAM >= 16k)
    /// +----- CHR A16, if extant; and PRG-ROM A18, if extant (PRG-ROM = 512k)
    ///
    /// PRG bank $E000-$FFFF
    /// 43210
    /// RPPPP
    /// |++++- Select 16K PRG-ROM bank (low bit ignored in 32K mode)
    /// +----- PRG-RAM chip enable (0: enabled; 1: disabled; ignored on MMC1A)
    fn write_registers(&mut self, addr: u16, val: u8) {
        if self.regs.write_just_occurred > 0 {
            return;
        }
        self.regs.write_just_occurred = 2;
        if val & Self::SHIFT_REG_RESET > 0 {
            self.regs.shift_register = Self::DEFAULT_SHIFT_REGISTER;
            self.regs.control |= Self::PRG_MODE_MASK;
        } else {
            // Check if its time to write
            let write = self.regs.shift_register & 1 == 1;
            // Move shift register and write lowest bit of val
            self.regs.shift_register >>= 1;
            self.regs.shift_register |= (val & 1) << 4;
            if write {
                match addr {
                    0x8000..=0x9FFF => self.regs.control = self.regs.shift_register,
                    0xA000..=0xBFFF => self.regs.chr0 = self.regs.shift_register & 0x1F,
                    0xC000..=0xDFFF => self.regs.chr1 = self.regs.shift_register & 0x1F,
                    0xE000..=0xFFFF => self.regs.prg = self.regs.shift_register & 0x1F,
                    _ => unreachable!("impossible write"),
                }
                self.regs.shift_register = Self::DEFAULT_SHIFT_REGISTER;
                self.update_banks(addr);
            }
        }
    }

    fn update_banks(&mut self, addr: u16) {
        self.mirroring = match self.regs.control & Self::MIRRORING_MASK {
            0 => Mirroring::SingleScreenA,
            1 => Mirroring::SingleScreenB,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!("impossible mirroring mode"),
        };

        let chr0 = self.regs.chr0.into();
        let chr1 = self.regs.chr1.into();
        let chr4k = self.regs.control & Self::CHR_MODE_MASK == Self::CHR_MODE_MASK;
        if chr4k {
            self.chr_banks.set(0, chr0);
            self.chr_banks.set(1, chr1);
        } else {
            self.chr_banks.set_range(0, 1, chr0 & 0x1E); // ignore low bit
        }

        if self.submapper_num == 5 {
            // Fixed PRG SEROM, SHROM, SH1ROM use a fixed 32k PRG-ROM with no banking support.
            self.prg_rom_banks.set_range(0, 1, 0);
        } else {
            let extra_reg = if matches!(addr, 0xC000..=0xDFFF) && chr4k {
                self.regs.chr1
            } else {
                self.regs.chr0
            };

            let bank_select = if self.chr_select {
                (extra_reg & Self::CHR_MODE_MASK).into()
            } else {
                0x00
            };

            let prg_bank = (self.regs.prg & Self::PRG_BANK_MASK) as usize;
            let prg_mode = (self.regs.control & Self::PRG_MODE_MASK) >> 2;
            match prg_mode {
                0 | 1 => {
                    self.prg_rom_banks
                        .set_range(0, 1, bank_select | (prg_bank & 0x1E)); // ignore low bit
                }
                2 => {
                    self.prg_rom_banks.set(0, bank_select);
                    self.prg_rom_banks.set(1, bank_select | prg_bank);
                }
                3 => {
                    let last = self.prg_rom_banks.last();
                    self.prg_rom_banks.set(0, bank_select | prg_bank);
                    self.prg_rom_banks.set(1, bank_select | last);
                }
                _ => unreachable!("impossible prg mode"),
            }
        }
    }

    #[inline]
    fn prg_ram_enabled(&self) -> bool {
        self.board == Mmc1Revision::A || self.regs.prg & Self::PRG_RAM_DISABLED == 0
    }
}

impl Mapped for Sxrom {
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    #[inline]
    fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }
}

impl MemMap for Sxrom {
    fn map_peek(&self, addr: u16) -> MappedRead {
        match addr {
            0x0000..=0x1FFF => MappedRead::Chr(self.chr_banks.translate(addr)),
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                MappedRead::PrgRam(self.prg_ram_banks.translate(addr))
            }
            0x8000..=0xFFFF => MappedRead::PrgRom(self.prg_rom_banks.translate(addr)),
            _ => MappedRead::Default,
        }
    }

    fn map_write(&mut self, addr: u16, val: u8) -> MappedWrite {
        match addr {
            0x0000..=0x1FFF => MappedWrite::Chr(self.chr_banks.translate(addr), val),
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                MappedWrite::PrgRam(self.prg_ram_banks.translate(addr), val)
            }
            0x8000..=0xFFFF => {
                self.write_registers(addr, val);
                MappedWrite::Default
            }
            _ => MappedWrite::Default,
        }
    }
}

impl Clock for Sxrom {
    fn clock(&mut self) -> usize {
        if self.regs.write_just_occurred > 0 {
            self.regs.write_just_occurred -= 1;
        }
        1
    }
}

impl Reset for Sxrom {
    fn reset(&mut self, kind: Kind) {
        self.regs.shift_register = Self::DEFAULT_SHIFT_REGISTER;
        self.regs.control = Self::DEFAULT_PRG_MODE;
        self.regs.prg = Self::PRG_RAM_DISABLED;
        self.update_banks(0x0000);
        if kind == Kind::Hard {
            self.regs.write_just_occurred = 0;
        }
    }
}

impl Regional for Sxrom {}

impl std::fmt::Debug for Sxrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SxRom")
            .field("regs", &self.regs)
            .field("submapper_num", &self.submapper_num)
            .field("mirroring", &self.mirroring)
            .field("board", &self.board)
            .field("chr_select", &self.chr_select)
            .field("chr_banks", &self.chr_banks)
            .field("prg_ram_banks", &self.prg_ram_banks)
            .field("prg_ram_enabled", &self.prg_ram_enabled())
            .field("prg_rom_banks", &self.prg_rom_banks)
            .finish()
    }
}

impl std::fmt::Debug for SxRegs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SxRegs")
            .field("write_just_occurred", &self.write_just_occurred)
            .field(
                "shift_register",
                &format_args!("0b{:08b}", self.shift_register),
            )
            .field("control", &format_args!("0x{:02X}", self.control))
            .field("chr0", &format_args!("0x{:02X}", self.chr0))
            .field("chr1", &format_args!("0x{:02X}", self.chr1))
            .field("prg", &format_args!("0x{:02X}", self.prg))
            .finish()
    }
}
