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
    memory::{Memory, Src},
    ppu::Mirroring,
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
    pub mmc1: Mmc1,
    pub bank_switching_lock: BankSwitchingLock,
    pub timer: Timer,
    /// The two 16K PRG-ROM banks at $8000 and $C000.
    pub prg_banks: [u8; 2],
}

impl NesEvent {
    const PRG_WINDOW: usize = 16 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const INNER_BANK_MASK: u8 = 0b111;
    const OUTER_BANK_MASK: u8 = 0b1000;

    // PPU $0000..=$1FFF 8K Fixed CHR-RAM
    // CPU $6000..=$7FFF 8K PRG-RAM
    // CPU $8000..=$FFFF 2x 16K PRG-ROM Banks, selected through the MMC1 shift register and gated
    //                   by the tournament timer's bank-switching lock
    pub fn load(cart: &mut Cart, switches: [bool; 4]) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mmc1: Mmc1::new(mmc1::Revision::BC),
            bank_switching_lock: BankSwitchingLock::new(),
            timer: Timer::new(switches),
            prg_banks: [0; 2],
        };
        board.update_state();
        board.sync(&mut cart.memory);
        Ok(board.into())
    }

    /// Recompute the PRG bank selections from the MMC1 registers and the tournament lock.
    pub const fn update_state(&mut self) {
        let timer_control = self.mmc1.chr0 & 0b10000 != 0;
        if timer_control {
            self.timer.stop();
        } else {
            self.timer.start();
        }
        self.bank_switching_lock.write(timer_control);

        if self.bank_switching_lock.locked() {
            // The first 32K, i.e. two consecutive 16K banks - not bank 0 twice, which would put
            // the wrong half at $C000 and so the wrong reset vectors.
            self.prg_banks = [0, 1];
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
                self.prg_banks = [inner_bank | outer_bank, Self::INNER_BANK_MASK | outer_bank];
            } else {
                self.prg_banks = [outer_bank, inner_bank | outer_bank];
            }
        } else {
            // 32K mode ignores the low bank bit.
            let bank = (inner_bank & !0b1) | outer_bank;
            self.prg_banks = [bank, bank + 1];
        }
    }
}

impl Map for NesEvent {

    fn mirroring(&self) -> Mirroring {
        self.mmc1.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.timer.irq_pending()
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 && self.mmc1.process_shift_register_write(addr, val) {
            self.update_state();
            self.sync(memory);
        }
    }

    fn sync(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_banks[0]),
            Src::PrgRom,
        );
        memory.map_prg(
            0xC000,
            Self::PRG_WINDOW,
            i32::from(self.prg_banks[1]),
            Src::PrgRom,
        );
        memory.map_chr(0x0000, Self::CHR_WINDOW, 0, Src::Chr);
        memory.set_mirroring(self.mmc1.mirroring);
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
