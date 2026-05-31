use serde::{Deserialize, Serialize};

use crate::{
    common::{Clock, Reset, ResetKind},
    ppu::Mirroring,
};

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

/// The `MMC1` chip.
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Mmc1 {
    pub revision: Revision,
    pub write_just_occurred: u8,
    pub write_buffer: u8,       // $8000-$FFFF - 5 bit shift register
    pub shift_count: u8,        // How many times write_buffer has shifted
    pub prg_ram_disabled: bool, // $E000-$FFFF bit 4
    pub chr_mode: bool,         // $8000-$9FFF bit 4
    pub prg_mode: bool,         // $8000-$9FFF bit 3
    pub prg_bank_select: bool,  // $8000-$9FFF bit 2
    pub mirroring: Mirroring,   // $8000-$9FFF bits 0-1
    pub last_chr_reg: u16,      // Last chr register written to
    pub chr0: u8,               // $A000-$BFFF
    pub chr1: u8,               // $C000-$DFFF
    pub prg: u8,                // $E000-$FFFF bits 0-3
}

impl Mmc1 {
    const SHIFT_REG_RESET: u8 = 0x80; // Reset shift register when bit 7 is set
    const MIRRORING_MASK: u8 = 0x03; // 0b00011
    const SLOT_SELECT_MASK: u8 = 0x04; // 0b00100
    const PRG_MODE_MASK: u8 = 0x08; // 0b01000
    const CHR_MODE_MASK: u8 = 0x10; // 0b10000

    const CHR_BANK_MASK: u8 = 0x1F;
    const PRG_BANK_MASK: u8 = 0x0F;
    const PRG_RAM_DISABLED: u8 = 0x10; // 0b10000

    pub fn new(revision: Revision) -> Self {
        Self {
            revision,
            write_just_occurred: 0x00,
            write_buffer: 0x00,
            shift_count: 0,
            prg_ram_disabled: revision == Revision::A,
            chr_mode: false,
            prg_mode: true,
            prg_bank_select: true,
            mirroring: Mirroring::SingleScreenA,
            last_chr_reg: 0xA000,
            chr0: 0x00,
            chr1: 0x00,
            prg: 0x00,
        }
    }

    /// Reset the shift register write buffer.
    const fn reset_buffer(&mut self) {
        self.shift_count = 0;
        self.write_buffer = 0;
    }

    pub const fn process_shift_register_write(&mut self, addr: u16, val: u8) -> bool {
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

        if self.write_just_occurred > 0 {
            return false;
        }
        self.write_just_occurred = 2;

        if val & Self::SHIFT_REG_RESET == Self::SHIFT_REG_RESET {
            self.reset_buffer();
            self.prg_mode = true;
            self.prg_bank_select = true;
            return true;
        } else {
            // Move shift register and write lowest bit of val
            self.write_buffer >>= 1;
            self.write_buffer |= (val << 4) & 0x10;

            self.shift_count += 1;
            // Check if its time to write
            if self.shift_count == 5 {
                self.process_register_write(addr, self.write_buffer);
                self.reset_buffer();
                return true;
            }
        }
        false
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
                self.prg_bank_select = (val & Self::SLOT_SELECT_MASK) != 0;
                self.prg_mode = (val & Self::PRG_MODE_MASK) != 0;
                self.chr_mode = (val & Self::CHR_MODE_MASK) != 0;
            }
            0xA000 => {
                self.last_chr_reg = addr;
                self.chr0 = val & Self::CHR_BANK_MASK;
            }
            0xC000 => {
                self.last_chr_reg = addr;
                self.chr1 = val & Self::CHR_BANK_MASK;
            }
            0xE000 => {
                self.prg = val & Self::PRG_BANK_MASK;
                self.prg_ram_disabled = (val & Self::PRG_RAM_DISABLED) != 0;
            }
            _ => (),
        }
    }

    #[inline(always)]
    pub fn prg_ram_enabled(&self) -> bool {
        self.revision == Revision::A || !self.prg_ram_disabled
    }

    pub const fn set_revision(&mut self, revision: Revision) {
        self.revision = revision;
    }
}

impl Reset for Mmc1 {
    fn reset(&mut self, kind: ResetKind) {
        self.reset_buffer();
        self.prg_mode = true;
        self.prg_bank_select = true;
        if kind == ResetKind::Hard {
            self.write_just_occurred = 0;
            self.prg_ram_disabled = false;
        }
    }
}

impl Clock for Mmc1 {
    fn clock(&mut self) {
        if self.write_just_occurred > 0 {
            self.write_just_occurred -= 1;
        }
    }
}

impl std::fmt::Debug for Mmc1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mmc1")
            .field("revision", &self.revision)
            .field("write_just_occurred", &self.write_just_occurred)
            .field("write_buffer", &format_args!("0b{:08b}", self.write_buffer))
            .field("shift_count", &self.shift_count)
            .field("prg_ram_disabled", &self.prg_ram_disabled)
            .field("chr_mode", &self.chr_mode)
            .field("prg_mode", &self.prg_mode)
            .field("prg_bank_select", &self.prg_bank_select)
            .field("mirroring", &self.mirroring)
            .field("last_chr_reg", &self.last_chr_reg)
            .field("chr0", &format_args!("${:02X}", self.chr0))
            .field("chr1", &format_args!("${:02X}", self.chr1))
            .field("prg", &format_args!("${:02X}", self.prg))
            .field("prg_ram_enabled", &self.prg_ram_enabled())
            .finish()
    }
}
