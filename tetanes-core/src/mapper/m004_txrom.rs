//! `TxROM`/`MMC3` (Mapper 004).
//!
//! <https://wiki.nesdev.org/w/index.php/TxROM>
//! <https://wiki.nesdev.org/w/index.php/MMC3>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    mapper::{self, Map, Mapper},
    mem::{Banks, Memory},
    ppu::{CIRam, Mirroring},
};
use serde::{Deserialize, Serialize};

/// MMC3 Revision.
///
/// See: <https://forums.nesdev.org/viewtopic.php?p=62546#p62546>
///
/// Known Revisions:
///
/// Conquest of the Crystal Palace (MMC3B S 9039 1 DB)
/// Kickle Cubicle (MMC3B S 9031 3 DA)
/// M.C. Kids (MMC3B S 9152 3 AB)
/// Mega Man 3 (MMC3B S 9046 1 DB)
/// Super Mario Bros. 3 (MMC3B S 9027 5 A)
/// Startropics (MMC6B P 03'5)
/// Batman (MMC3B 9006KP006)
/// Golgo 13: The Mafat Conspiracy (MMC3B 9016KP051)
/// Crystalis (MMC3B 9024KPO53)
/// Legacy of the Wizard (MMC3A 8940EP)
///
/// Only major difference is the IRQ counter
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// MMC3 Revision A
    A,
    /// MMC3 Revisions B & C
    #[default]
    BC,
    /// Acclaims MMC3 clone - clocks on falling edge
    Acc,
}

/// `TxROM` Registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub bank_select: u8,
    pub bank_values: [u8; 8],
    pub irq_latch: u8,
    pub irq_counter: u8,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub irq_reload: bool,
    pub master_clock: u32,
    pub a12_low_clock: u32,
}

/// `TxROM`/`MMC3` (Mapper 004).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Txrom {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub ex_ram: Memory<Box<[u8]>>,
    pub regs: Regs,
    pub has_chr_ram: bool,
    pub mirroring: Mirroring,
    pub mapper_num: u16,
    pub submapper_num: u8,
    pub revision: Revision,
    pub chr_banks: Banks,
    pub prg_ram_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl Txrom {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const CHR_WINDOW_76: usize = 2048;

    const FOUR_SCREEN_RAM_SIZE: usize = 4 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    const PRG_MODE_MASK: u8 = 0x40; // Bit 6 of bank select
    const CHR_INVERSION_MASK: u8 = 0x80; // Bit 7 of bank select

    /// Create `Txrom` from `Cart`.
    pub fn new(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
        chr_window: usize,
    ) -> Result<Self, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_ram = Memory::with_ram_state(Self::PRG_RAM_SIZE, cart.ram_state);
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr.len(), chr_window)?;
        let prg_ram_banks = Banks::new(0x6000, 0x7FFF, prg_ram.len(), Self::PRG_WINDOW)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_WINDOW)?;
        let mut txrom = Self {
            chr,
            prg_rom,
            prg_ram,
            ex_ram: if cart.mirroring() == Mirroring::FourScreen {
                Memory::new(Self::FOUR_SCREEN_RAM_SIZE)
            } else {
                Memory::empty()
            },
            regs: Regs::default(),
            has_chr_ram,
            mirroring: cart.mirroring(),
            mapper_num: cart.mapper_num(),
            submapper_num: cart.submapper_num(),
            revision: Revision::BC, // TODO compare to known games
            chr_banks,
            prg_ram_banks,
            prg_rom_banks,
        };
        let last_bank = txrom.prg_rom_banks.last();
        txrom.prg_rom_banks.set(2, last_bank - 1);
        txrom.prg_rom_banks.set(3, last_bank);
        Ok(txrom)
    }

    /// Load `Txrom` from `Cart`.
    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        Ok(Self::new(
            cart,
            chr_rom,
            prg_rom,
            if cart.mapper_num() == 76 {
                Self::CHR_WINDOW_76
            } else {
                Self::CHR_WINDOW
            },
        )?
        .into())
    }

    pub const fn bank_register(&self, index: usize) -> u8 {
        self.regs.bank_values[index]
    }

    pub const fn set_revision(&mut self, rev: Revision) {
        self.revision = rev;
    }

    #[inline]
    const fn apply_prg_write_masks(&self, addr: &mut u16, val: &mut u8) {
        // Redirects all 0x8000..=0xFFFF writes to 0x8000..=0x8001 as all other features do not
        // exist for the corresponding mappers that call this
        *addr &= 0x8001;
        if *addr == 0x8000 {
            // Disable CHR mode 1 and Prg mode 1
            // PRG has the last two 8K banks fixed to the end.
            // CHR assigns the left pattern table ($0000-$0FFF) two 2K banks, and the right pattern
            // table ($1000-$1FFF) four 1K banks.
            *val &= 0x3F;
        }
    }

    pub fn update_prg_banks(&mut self) {
        let prg_last = self.prg_rom_banks.last();
        let prg_lo = self.regs.bank_values[6] as usize;
        let prg_hi = self.regs.bank_values[7] as usize;
        if self.regs.bank_select & Self::PRG_MODE_MASK == Self::PRG_MODE_MASK {
            self.prg_rom_banks.set(0, prg_last - 1);
            self.prg_rom_banks.set(1, prg_hi);
            self.prg_rom_banks.set(2, prg_lo);
        } else {
            self.prg_rom_banks.set(0, prg_lo);
            self.prg_rom_banks.set(1, prg_hi);
            self.prg_rom_banks.set(2, prg_last - 1);
        }
        self.prg_rom_banks.set(3, prg_last);
    }

    pub fn set_chr_banks(&mut self, f: impl Fn(&mut Banks, &mut [u8])) {
        f(&mut self.chr_banks, &mut self.regs.bank_values)
    }

    pub fn update_chr_banks(&mut self) {
        match self.mapper_num {
            76 => {
                self.set_chr_banks(|banks, regs| {
                    banks.set(0, regs[2] as usize);
                    banks.set(1, regs[3] as usize);
                    banks.set(2, regs[4] as usize);
                    banks.set(3, regs[5] as usize);
                });
                return;
            }
            88 | 154 => {
                self.set_chr_banks(|_, regs| {
                    regs[0] &= 0x3F;
                    regs[1] &= 0x3F;
                    regs[2] |= 0x40;
                    regs[3] |= 0x40;
                    regs[4] |= 0x40;
                    regs[5] |= 0x40;
                });
            }
            _ => (),
        }

        // 1: two 2K banks at $1000-$1FFF, four 1 KB banks at $0000-$0FFF
        // 0: two 2K banks at $0000-$0FFF, four 1 KB banks at $1000-$1FFF
        let chr = self.regs.bank_values;
        if self.regs.bank_select & Self::CHR_INVERSION_MASK == Self::CHR_INVERSION_MASK {
            self.chr_banks.set(0, chr[2] as usize);
            self.chr_banks.set(1, chr[3] as usize);
            self.chr_banks.set(2, chr[4] as usize);
            self.chr_banks.set(3, chr[5] as usize);
            self.chr_banks.set_range(4, 5, (chr[0] & 0xFE) as usize);
            self.chr_banks.set_range(6, 7, (chr[1] & 0xFE) as usize);
        } else {
            self.chr_banks.set_range(0, 1, (chr[0] & 0xFE) as usize);
            self.chr_banks.set_range(2, 3, (chr[1] & 0xFE) as usize);
            self.chr_banks.set(4, chr[2] as usize);
            self.chr_banks.set(5, chr[3] as usize);
            self.chr_banks.set(6, chr[4] as usize);
            self.chr_banks.set(7, chr[5] as usize);
        }
    }

    pub fn update_banks(&mut self) {
        self.update_prg_banks();
        self.update_chr_banks();
    }

    const fn is_a12_rising_edge(&mut self, addr: u16) -> bool {
        if addr & 0x1000 > 0 {
            // NOTE: This is technical 3 falling edges of M2 - but because the mapper doesn't have
            // direct access to the CPUs clock, and is clocked after the PPU runs and calls this
            // method, we're off by 1
            let is_rising_edge = self.regs.a12_low_clock > 0
                && self.regs.master_clock.wrapping_sub(self.regs.a12_low_clock) >= 4;
            self.regs.a12_low_clock = 0;
            return is_rising_edge;
        } else if self.regs.a12_low_clock == 0 {
            self.regs.a12_low_clock = self.regs.master_clock;
        }
        false
    }
}

impl Map for Txrom {
    // PPU $0000..=$07FF (or $1000..=$17FF) 2K CHR-ROM/RAM Bank 1 Switchable --+
    // PPU $0800..=$0FFF (or $1800..=$1FFF) 2K CHR-ROM/RAM Bank 2 Switchable --|-+
    // PPU $1000..=$13FF (or $0000..=$03FF) 1K CHR-ROM/RAM Bank 3 Switchable --+ |
    // PPU $1400..=$17FF (or $0400..=$07FF) 1K CHR-ROM/RAM Bank 4 Switchable --+ |
    // PPU $1800..=$1BFF (or $0800..=$0BFF) 1K CHR-ROM/RAM Bank 5 Switchable ----+
    // PPU $1C00..=$1FFF (or $0C00..=$0FFF) 1K CHR-ROM/RAM Bank 6 Switchable ----+
    // PPU $2000..=$3EFF FourScreen Mirroring (optional)

    // CPU $6000..=$7FFF 8K PRG-RAM Bank (optional)
    // CPU $8000..=$9FFF (or $C000..=$DFFF) 8K PRG-ROM Bank 1 Switchable
    // CPU $A000..=$BFFF 8K PRG-ROM Bank 2 Switchable
    // CPU $C000..=$DFFF (or $8000..=$9FFF) 8K PRG-ROM Bank 3 Fixed to second-to-last Bank
    // CPU $E000..=$FFFF 8K PRG-ROM Bank 4 Fixed to Last

    /// Read a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_read(&mut self, addr: u16, ciram: &CIRam) -> u8 {
        self.ppu_read(addr);
        self.chr_peek(addr, ciram)
    }

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[self.chr_banks.translate(addr)],
            0x2000..=0x3EFF => {
                if self.mirroring == Mirroring::FourScreen {
                    self.ex_ram[usize::from(addr & 0x1FFF)]
                } else {
                    ciram.peek(addr, self.mirroring)
                }
            }
            _ => 0,
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[self.prg_ram_banks.translate(addr)],
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF => self.chr[self.chr_banks.translate(addr)] = val,
            0x2000..=0x3EFF => {
                if self.mirroring == Mirroring::FourScreen {
                    self.ex_ram[usize::from(addr & 0x1FFF)] = val;
                } else {
                    ciram.write(addr, val, self.mirroring);
                }
            }
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    fn prg_write(&mut self, mut addr: u16, mut val: u8) {
        match self.mapper_num {
            76 | 88 | 95 | 206 => self.apply_prg_write_masks(&mut addr, &mut val),
            154 => {
                self.mirroring = if val & 0x40 == 0x40 {
                    Mirroring::SingleScreenB
                } else {
                    Mirroring::SingleScreenA
                };
                self.apply_prg_write_masks(&mut addr, &mut val);
            }
            _ => (),
        }

        match addr {
            0x6000..=0x7FFF => self.prg_ram[self.prg_ram_banks.translate(addr)] = val,
            0x8000..=0xFFFF => {
                //  7654 3210
                // `CPMx xRRR`
                //  |||   +++- Specify which bank register to update on next write to Bank Data register
                //  |||        0: Select 2K CHR bank at PPU $0000-$07FF (or $1000-$17FF);
                //  |||        1: Select 2K CHR bank at PPU $0800-$0FFF (or $1800-$1FFF);
                //  |||        2: Select 1K CHR bank at PPU $1000-$13FF (or $0000-$03FF);
                //  |||        3: Select 1K CHR bank at PPU $1400-$17FF (or $0400-$07FF);
                //  |||        4: Select 1K CHR bank at PPU $1800-$1BFF (or $0800-$0BFF);
                //  |||        5: Select 1K CHR bank at PPU $1C00-$1FFF (or $0C00-$0FFF);
                //  |||        6: Select 8K PRG-ROM bank at $8000-$9FFF (or $C000-$DFFF);
                //  |||        7: Select 8K PRG-ROM bank at $A000-$BFFF
                //  ||+------- Nothing on the MMC3, see MMC6
                //  |+-------- PRG-ROM bank mode (0: $8000-$9FFF swappable,
                //  |                                $C000-$DFFF fixed to second-last bank;
                //  |                             1: $C000-$DFFF swappable,
                //  |                                $8000-$9FFF fixed to second-last bank)
                //  +--------- CHR A12 inversion (0: two 2K banks at $0000-$0FFF,
                //                                   four 1K banks at $1000-$1FFF;
                //                                1: two 2K banks at $1000-$1FFF,
                //                                   four 1K banks at $0000-$0FFF)
                //

                // Match only $8000/1, $A000/1, $C000/1, and $E000/1
                match addr & 0xE001 {
                    0x8000 => {
                        self.regs.bank_select = val;
                        self.update_banks();
                    }
                    0x8001 => {
                        let bank = self.regs.bank_select & 0x07;
                        self.regs.bank_values[bank as usize] = val;
                        self.update_banks();
                    }
                    0xA000 => {
                        if self.mirroring != Mirroring::FourScreen {
                            self.mirroring = if val & 0x01 == 0x01 {
                                Mirroring::Horizontal
                            } else {
                                Mirroring::Vertical
                            };
                            self.update_banks();
                        }
                    }
                    0xA001 => {
                        // TODO RAM protect? Might conflict with MMC6
                    }
                    // IRQ
                    0xC000 => self.regs.irq_latch = val,
                    0xC001 => self.regs.irq_reload = true,
                    0xE000 => {
                        self.regs.irq_enabled = false;
                        self.regs.irq_pending = false;
                    }
                    0xE001 => self.regs.irq_enabled = true,
                    _ => unreachable!("impossible address"),
                }
            }
            _ => (),
        }

        if self.mapper_num == 95 && addr & 0x01 == 0x01 {
            let nametable1 = (self.bank_register(0) >> 5) & 0x01;
            let nametable2 = (self.bank_register(1) >> 5) & 0x01;
            self.mirroring = match (nametable1, nametable2) {
                (0, 0) => Mirroring::SingleScreenA,
                (1, 1) => Mirroring::SingleScreenB,
                _ => Mirroring::Horizontal,
            };
        }
    }

    /// Synchronize a read from a PPU address.
    fn ppu_read(&mut self, addr: u16) {
        // Clock on PPU A12 rising edge
        if self.is_a12_rising_edge(addr) {
            let counter = self.regs.irq_counter;
            if self.regs.irq_counter == 0 || self.regs.irq_reload {
                self.regs.irq_counter = self.regs.irq_latch;
            } else {
                self.regs.irq_counter -= 1;
            }
            if self.revision == Revision::A {
                if (counter > 0 || self.regs.irq_reload)
                    && self.regs.irq_counter == 0
                    && self.regs.irq_enabled
                {
                    self.regs.irq_pending = true;
                }
            } else if self.regs.irq_counter == 0 && self.regs.irq_enabled {
                self.regs.irq_pending = true;
            }
            self.regs.irq_reload = false;
        }
    }

    /// Whether an IRQ is pending acknowledgement.
    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Reset for Txrom {
    fn reset(&mut self, _kind: ResetKind) {
        self.regs = Regs::default();
        self.update_banks();
        self.update_chr_banks();
    }
}

impl Clock for Txrom {
    fn clock(&mut self) {
        self.regs.master_clock = self.regs.master_clock.wrapping_add(1);
    }
}

impl Regional for Txrom {}
impl Sram for Txrom {}
