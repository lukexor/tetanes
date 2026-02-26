//! `SxROM`/`MMC1` (Mapper 001).
//!
//! <https://wiki.nesdev.org/w/index.php/SxROM>
//! <https://wiki.nesdev.org/w/index.php/MMC1>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// MMC1 Revision.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// MMC1 Revision A
    A,
    /// MMC1 Revisions B & C
    #[default]
    BC,
}

/// `SxROM` registers.
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    write_just_occurred: u8,
    write_buffer: u8,       // $8000-$FFFF - 5 bit shift register
    shift_count: u8,        // How many times write_buffer has shifted
    prg_ram_disabled: bool, // $E000-$FFFF bit 4
    chr_mode: bool,         // $8000-$9FFF bit 4
    prg_mode: bool,         // $8000-$9FFF bits 3
    prg_bank_select: bool,  // $8000-$9FFF bit 2
    last_chr_reg: u16,      // Last chr register written to
    chr0: u8,               // $A000-$BFFF
    chr1: u8,               // $C000-$DFFF
    prg: u8,                // $E000-$FFFF bits 0-3
}

/// `SxROM`/`MMC1` (Mapper 001).
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sxrom {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub chr_banks: Banks,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
    pub regs: Regs,
    pub has_chr_ram: bool,
    pub submapper_num: u8,
    pub mirroring: Mirroring,
    pub revision: Revision,
    pub prg_select: bool,
}

impl Sxrom {
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const PRG_ROM_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_RAM_SIZE: usize = 32 * 1024; // 32K is safely compatible sans NES 2.0 header
    const CHR_RAM_SIZE: usize = 8 * 1024;

    const SHIFT_REG_RESET: u8 = 0x80; // Reset shift register when bit 7 is set
    const MIRRORING_MASK: u8 = 0x03; // 0b00011
    const SLOT_SELECT_MASK: u8 = 0x04; // 0b00100
    const PRG_MODE_MASK: u8 = 0x08; // 0b01000
    const CHR_MODE_MASK: u8 = 0x10; // 0b10000

    const DEFAULT_PRG_MODE: u8 = 0x0C; // Mode 3, 16k Fixed Last
    const CHR_BANK_MASK: u8 = 0x1F;
    const PRG_BANK_MASK: u8 = 0x0F;
    const PRG_RAM_DISABLED: u8 = 0x10; // 0b10000

    /// Load `Sxrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
        revision: Revision,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_ram = cart.prg_ram_or_default(Self::PRG_RAM_SIZE);
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr.len(), Self::CHR_WINDOW)?;
        let prg_ram_banks = Banks::new(0x6000, 0x7FFF, prg_ram.len(), Self::PRG_RAM_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_ROM_WINDOW)?;
        let mut sxrom = Self {
            prg_rom,
            chr,
            prg_ram,
            chr_banks,
            prg_ram_banks,
            prg_rom_banks,
            regs: Regs {
                write_just_occurred: 0x00,
                write_buffer: 0x00,
                shift_count: 0,
                prg_ram_disabled: false,
                chr_mode: false,
                prg_mode: false,
                prg_bank_select: false,
                last_chr_reg: 0xA000,
                chr0: 0x00,
                chr1: 0x00,
                prg: 0x00,
            },
            has_chr_ram,
            submapper_num: cart.submapper_num(),
            mirroring: Mirroring::SingleScreenA,
            revision,
            prg_select: cart.prg_rom_size == 0x80000,
        };
        sxrom.process_register_write(0x8000, Self::DEFAULT_PRG_MODE);
        sxrom.process_register_write(0xA000, 0x00);
        sxrom.process_register_write(0xC000, 0x00);
        sxrom.process_register_write(
            0xE000,
            if revision == Revision::BC {
                0x00
            } else {
                Self::PRG_RAM_DISABLED
            },
        );
        sxrom.regs.last_chr_reg = 0xA000;
        sxrom.update_state();
        Ok(sxrom.into())
    }

    /// Reset the shift register write buffer.
    const fn reset_buffer(&mut self) {
        self.regs.shift_count = 0;
        self.regs.write_buffer = 0;
    }

    /// Process register write, extracting registers into flags.
    const fn process_register_write(&mut self, addr: u16, val: u8) {
        match addr & 0xE000 {
            0x8000 => {
                self.mirroring = match val & Self::MIRRORING_MASK {
                    0b00 => Mirroring::SingleScreenA,
                    0b01 => Mirroring::SingleScreenB,
                    0b10 => Mirroring::Vertical,
                    _ => Mirroring::Horizontal,
                };
                self.regs.prg_bank_select = (val & Self::SLOT_SELECT_MASK) != 0;
                self.regs.prg_mode = (val & Self::PRG_MODE_MASK) != 0;
                self.regs.chr_mode = (val & Self::CHR_MODE_MASK) != 0;
            }
            0xA000 => {
                self.regs.last_chr_reg = addr;
                self.regs.chr0 = val & Self::CHR_BANK_MASK;
            }
            0xC000 => {
                self.regs.last_chr_reg = addr;
                self.regs.chr1 = val & Self::CHR_BANK_MASK;
            }
            0xE000 => {
                self.regs.prg = val & Self::PRG_BANK_MASK;
                self.regs.prg_ram_disabled = (val & Self::PRG_RAM_DISABLED) != 0;
            }
            _ => (),
        }
    }

    /// Update internal state based on register flags.
    pub fn update_state(&mut self) {
        let extra_reg = if self.regs.last_chr_reg == 0xC000 && self.regs.chr_mode {
            self.regs.chr1
        } else {
            self.regs.chr0
        };
        let prg_bank_select = if self.prg_select {
            extra_reg & Self::CHR_MODE_MASK
        } else {
            0x00
        };

        if self.submapper_num == 5 {
            // Fixed PRG SEROM, SHROM, SH1ROM use a fixed 32k PRG-ROM with no banking support.
            self.prg_rom_banks.set_range(0, 1, 0);
        } else if self.regs.prg_mode {
            if self.regs.prg_bank_select {
                self.prg_rom_banks
                    .set(0, (self.regs.prg | prg_bank_select).into());
                self.prg_rom_banks
                    .set(1, (Self::PRG_BANK_MASK | prg_bank_select).into());
            } else {
                self.prg_rom_banks.set(1, prg_bank_select.into());
                self.prg_rom_banks
                    .set(1, (self.regs.prg | prg_bank_select).into());
            }
        } else {
            self.prg_rom_banks
                .set_range(0, 1, ((self.regs.prg & 0xFE) | prg_bank_select).into()); // ignore low bit
        }

        if self.regs.chr_mode {
            self.chr_banks.set(0, self.regs.chr0.into());
            self.chr_banks.set(1, self.regs.chr1.into());
        } else {
            self.chr_banks.set(0, (self.regs.chr0 & 0x1E).into()); // ignore low bit
            self.chr_banks.set(1, ((self.regs.chr0 & 0x1E) + 1).into()); // ignore low bit
        }
    }

    #[inline(always)]
    pub fn prg_ram_enabled(&self) -> bool {
        self.revision == Revision::A || !self.regs.prg_ram_disabled
    }

    pub const fn set_revision(&mut self, revision: Revision) {
        self.revision = revision;
    }
}

impl Map for Sxrom {
    // PPU $0000..=$1FFF 4K CHR-ROM/RAM Bank Switchable
    // CPU $6000..=$7FFF 8K PRG-RAM Bank (optional)
    // CPU $8000..=$BFFF 16K PRG-ROM Bank Switchable or Fixed to First Bank
    // CPU $C000..=$FFFF 16K PRG-ROM Bank Fixed to Last Bank or Switchable

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[self.chr_banks.translate(addr)],
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring),
            _ => 0,
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                self.prg_ram[self.prg_ram_banks.translate(addr)]
            }
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF if self.has_chr_ram => self.chr[self.chr_banks.translate(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                self.prg_ram[self.prg_ram_banks.translate(addr)] = val;
            }
            0x8000..=0xFFFF => {
                // Writes data into a shift register. At every 5th
                // write, the data is written out to the `SxROM` registers
                // and the shift register is cleared
                //
                // Load Register $8000-$FFFF
                // 7654 3210
                // Rxxx xxxD
                // |       +- Data bit to be shifted into shift register, LSB first
                // +--------- 1: Reset shift register and write control with (Control OR $0C),
                //               locking PRG-ROM at $C000-$FFFF to the last bank.
                //
                // Control $8000-$9FFF
                // 43210
                // CPPMM
                // |||++- Mirroring (0: one-screen, lower bank; 1: one-screen, upper bank;
                // |||               2: vertical; 3: horizontal)
                // |++--- PRG-ROM bank mode (0, 1: switch 32K at $8000, ignoring low bit of bank number;
                // |                         2: fix first bank at $8000 and switch 16K bank at $C000;
                // |                         3: fix last bank at $C000 and switch 16K bank at $8000)
                // +----- CHR-ROM bank mode (0: switch 8K at a time; 1: switch two separate 4K banks)
                //
                // CHR bank 0 $A000-$BFFF
                // 42310
                // CCCCC
                // +++++- Select 4K or 8K CHR bank at PPU $0000 (low bit ignored in 8K mode)
                //
                // CHR bank 1 $C000-$DFFF
                // 43210
                // CCCCC
                // +++++- Select 4K CHR bank at PPU $1000 (ignored in 8K mode)
                //
                // For Mapper001
                // $A000 and $C000:
                // 43210
                // EDCBA
                // |||||
                // ||||+- CHR A12
                // |||+-- CHR A13, if extant (CHR >= 16k)
                // ||+--- CHR A14, if extant; and PRG-RAM A14, if extant (PRG-RAM = 32k)
                // |+---- CHR A15, if extant; and PRG-RAM A13, if extant (PRG-RAM >= 16k)
                // +----- CHR A16, if extant; and PRG-ROM A18, if extant (PRG-ROM = 512k)
                //
                // PRG bank $E000-$FFFF
                // 43210
                // RPPPP
                // |++++- Select 16K PRG-ROM bank (low bit ignored in 32K mode)
                // +----- PRG-RAM chip enable (0: enabled; 1: disabled; ignored on MMC1A)

                if self.regs.write_just_occurred > 0 {
                    return;
                }
                self.regs.write_just_occurred = 2;

                if val & Self::SHIFT_REG_RESET == Self::SHIFT_REG_RESET {
                    self.reset_buffer();
                    self.regs.prg_mode = true;
                    self.regs.prg_bank_select = true;
                    self.update_state();
                } else {
                    // Move shift register and write lowest bit of val
                    self.regs.write_buffer >>= 1;
                    self.regs.write_buffer |= (val << 4) & 0x10;

                    self.regs.shift_count += 1;
                    // Check if its time to write
                    if self.regs.shift_count == 5 {
                        self.process_register_write(addr, self.regs.write_buffer);
                        self.update_state();
                        self.reset_buffer();
                    }
                }
            }
            _ => (),
        }
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Sxrom {
    fn reset(&mut self, kind: ResetKind) {
        self.reset_buffer();
        self.regs.prg_mode = true;
        self.regs.prg_bank_select = true;
        self.update_state();
        if kind == ResetKind::Hard {
            self.regs.write_just_occurred = 0;
            self.regs.prg_ram_disabled = false;
        }
    }
}

impl Clock for Sxrom {
    fn clock(&mut self) {
        if self.regs.write_just_occurred > 0 {
            self.regs.write_just_occurred -= 1;
        }
    }
}

impl Regional for Sxrom {}
impl Sram for Sxrom {}

impl std::fmt::Debug for Sxrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SxRom")
            .field("regs", &self.regs)
            .field("submapper_num", &self.submapper_num)
            .field("mirroring", &self.mirroring)
            .field("revision", &self.revision)
            .field("prg_select", &self.prg_select)
            .field("chr_banks", &self.chr_banks)
            .field("prg_ram_banks", &self.prg_ram_banks)
            .field("prg_ram_enabled", &self.prg_ram_enabled())
            .field("prg_rom_banks", &self.prg_rom_banks)
            .finish()
    }
}

impl std::fmt::Debug for Regs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SxRegs")
            .field("write_just_occurred", &self.write_just_occurred)
            .field("write_buffer", &format_args!("0b{:08b}", self.write_buffer))
            .field("shift_count", &self.shift_count)
            .field("prg_ram_disabled", &self.prg_ram_disabled)
            .field("chr_mode", &self.chr_mode)
            .field("prg_mode", &self.prg_mode)
            .field("prg_bank_select", &self.prg_bank_select)
            .field("last_chr_reg", &self.last_chr_reg)
            .field("chr0", &format_args!("${:02X}", self.chr0))
            .field("chr1", &format_args!("${:02X}", self.chr1))
            .field("prg", &format_args!("${:02X}", self.prg))
            .finish()
    }
}
