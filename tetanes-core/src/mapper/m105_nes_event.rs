//! `NES-EVENT`/`MMC1` (Mapper 105).
//!
//! <https://www.nesdev.org/w/index.php/NES-EVENT>
//! <https://www.nesdev.org/w/index.php/MMC1>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, ResetKind, Sram},
    mapper::{
        self, Map, Mapper,
        mmc1::{self, Mmc1},
    },
    mem::{Banks, Memory},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BankSwitchingLock {
    LockedPending0,
    LockedPending1,
    Unlocked,
}

impl BankSwitchingLock {
    const fn new() -> Self {
        Self::LockedPending0
    }

    const fn locked(&self) -> bool {
        !matches!(self, BankSwitchingLock::Unlocked)
    }

    const fn write(&mut self, value: bool) {
        match (&self, value) {
            (&BankSwitchingLock::LockedPending0, false) => {
                *self = BankSwitchingLock::LockedPending1
            }
            (&BankSwitchingLock::LockedPending1, true) => *self = BankSwitchingLock::Unlocked,
            _ => {}
        }
    }
}

impl Default for BankSwitchingLock {
    fn default() -> Self {
        Self::new()
    }
}

impl Reset for BankSwitchingLock {
    fn reset(&mut self, _kind: ResetKind) {
        *self = Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    started: bool,
    value: u32,
    target_high_byte: u8,
}

impl Timer {
    fn new(switches: [bool; 4]) -> Self {
        Self {
            started: false,
            value: 0,
            target_high_byte: (1 << 5)
                | (u8::from(switches[3]) << 4)
                | (u8::from(switches[2]) << 3)
                | (u8::from(switches[1]) << 2)
                | (u8::from(switches[0]) << 1),
        }
    }

    const fn start(&mut self) {
        if !self.started {
            self.started = true;
            self.value = 0;
        }
    }

    const fn stop(&mut self) {
        self.started = false;
    }

    const fn irq_pending(&self) -> bool {
        self.value.to_le_bytes()[3] == self.target_high_byte
    }
}

impl Reset for Timer {
    fn reset(&mut self, _kind: ResetKind) {
        self.started = false;
        self.value = 0;
    }
}

impl Clock for Timer {
    fn clock(&mut self) {
        if !self.started {
            return;
        }

        self.value += 1;
        if self.irq_pending() {
            self.stop();
        }
    }
}

/// `NES-EVENT`/`MMC1` (Mapper 105).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct NesEvent {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub prg_rom_banks: Banks,
    pub mmc1: Mmc1,
    pub bank_switching_lock: BankSwitchingLock,
    pub timer: Timer,
    pub has_chr_ram: bool,
}

impl NesEvent {
    const PRG_WINDOW: usize = 16 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    const INNER_BANK_MASK: u8 = 0b111;
    const OUTER_BANK_MASK: u8 = 0b1000;

    /// Load `NesEvent` from `Cart`.
    pub fn load(
        cart: &mut Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
        switches: [bool; 4],
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let prg_ram = cart.prg_ram_or_default(Self::PRG_RAM_SIZE);
        let mut nes_event = Self {
            chr,
            prg_rom,
            prg_ram,
            prg_rom_banks: Banks::new(0x8000, 0xFFFF, cart.prg_rom.len(), Self::PRG_WINDOW)?,
            mmc1: Mmc1::new(mmc1::Revision::BC),
            bank_switching_lock: BankSwitchingLock::new(),
            timer: Timer::new(switches),
            has_chr_ram,
        };
        nes_event.update_state();
        Ok(nes_event.into())
    }

    /// Update internal state based on register flags.
    pub fn update_state(&mut self) {
        let timer_control = self.mmc1.chr0 & 0b10000 != 0;
        if timer_control {
            self.timer.stop();
        } else {
            self.timer.start();
        }
        self.bank_switching_lock.write(timer_control);
        if self.bank_switching_lock.locked() {
            self.prg_rom_banks.set_range(0, 1, 0);
            return;
        }

        let outer_bank = self.mmc1.chr0 & Self::OUTER_BANK_MASK;

        let inner_bank = if outer_bank == 0 {
            self.mmc1.chr0
        } else {
            self.mmc1.prg
        } & Self::INNER_BANK_MASK;

        if self.mmc1.prg_mode && outer_bank != 0 {
            if self.mmc1.prg_bank_select {
                self.prg_rom_banks.set(0, (inner_bank | outer_bank).into());
                self.prg_rom_banks
                    .set(1, (Self::INNER_BANK_MASK | outer_bank).into());
            } else {
                self.prg_rom_banks.set(0, outer_bank.into());
                self.prg_rom_banks.set(1, (inner_bank | outer_bank).into());
            }
        } else {
            self.prg_rom_banks
                .set_range(0, 1, ((inner_bank & !0b1) | outer_bank).into()); // ignore low bit
        }
    }
}

impl Map for NesEvent {
    fn chr_peek(&self, addr: u16, ciram: &crate::ppu::CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[usize::from(addr)],
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring()),
            _ => 0,
        }
    }

    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.mmc1.prg_ram_enabled() => {
                self.prg_ram[usize::from(addr) & (Self::PRG_RAM_SIZE - 1)]
            }
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    fn mirroring(&self) -> crate::prelude::Mirroring {
        self.mmc1.mirroring
    }

    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut crate::ppu::CIRam) {
        match addr {
            0x0000..=0x1FFF => self.chr[usize::from(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring()),
            _ => (),
        }
    }

    fn prg_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF if self.mmc1.prg_ram_enabled() => {
                self.prg_ram[usize::from(addr) & (Self::PRG_RAM_SIZE - 1)] = val;
            }
            0x8000..=0xFFFF => {
                let written = self.mmc1.process_shift_register_write(addr, val);
                if written {
                    self.update_state();
                }
            }
            _ => (),
        }
    }

    fn irq_pending(&self) -> bool {
        self.timer.irq_pending()
    }
}

impl Reset for NesEvent {
    fn reset(&mut self, kind: ResetKind) {
        self.mmc1.reset(kind);
        self.mmc1.chr0 = 0b10000; // Initially, banking is locked, and the timer does not count 
        self.bank_switching_lock.reset(kind);
        self.timer.reset(kind);
        self.update_state();
    }
}

impl Clock for NesEvent {
    fn clock(&mut self) {
        self.mmc1.clock();
        self.timer.clock();
    }
}

impl Regional for NesEvent {}
impl Sram for NesEvent {}
