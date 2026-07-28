//! `NES-EVENT`/`MMC1` (Mapper 105).
//!
//! <https://www.nesdev.org/w/index.php/NES-EVENT>
//! <https://www.nesdev.org/w/index.php/MMC1>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{
        self, Map, Mapper, MapperOps,
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

impl BankSwitchingLock {
    pub const fn reset(&mut self, _kind: ResetKind) {
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

    pub const fn reset(&mut self, _kind: ResetKind) {
        self.started = false;
        self.value = 0;
    }

    pub const fn clock(&mut self) {
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
        board.update_banks(&mut cart.memory);
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
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ
    }

    fn mirroring(&self) -> Mirroring {
        self.mmc1.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.timer.irq_pending()
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr >= 0x8000 && self.mmc1.process_shift_register_write(addr, val) {
            self.update_state();
            self.update_banks(memory);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
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

    fn clock(&mut self) {
        self.mmc1.clock();
        self.timer.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc1.reset(kind);
        self.mmc1.chr0 = 0b10000; // Initially, banking is locked, and the timer does not count
        self.bank_switching_lock.reset(kind);
        self.timer.reset(kind);
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// The dip switches the board ships with, and what `Mapper::from_cart` passes.
    const SWITCHES: [bool; 4] = [false, false, true, false];

    /// 256K PRG-ROM (16 16K banks), 8K PRG-RAM, 8K CHR-RAM - the NWC 1990 cart's layout, with the
    /// two 128K halves the outer bank bit chooses between.
    ///
    /// Loading is followed by a reset because that is what `ControlDeck::load_rom` does, and the
    /// two differ here: `load` leaves the lock one write from opening, `reset` puts it back to
    /// needing the full sequence.
    fn nes_event() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(256 * 1024, 8 * 1024, 0);
        let mut mapper = NesEvent::load(&mut cart, SWITCHES).expect("valid mapper");
        mapper.reset(ResetKind::Hard);
        mapper.update_banks(&mut cart.memory);
        (mapper, cart)
    }

    fn board(mapper: &Mapper) -> &NesEvent {
        match mapper {
            Mapper::NesEvent(board) => board,
            board => unreachable!("expected a NesEvent, got {board:?}"),
        }
    }

    /// Feeds a register through the MMC1 serial port: five writes of one bit each, LSB first, with
    /// the two cycles between them that the chip's write lockout requires.
    fn serial_write(mapper: &mut Mapper, cart: &mut Cart, addr: u16, val: u8) {
        for bit in 0..5 {
            write(mapper, cart, addr, (val >> bit) & 0x01);
            mapper.clock();
            mapper.clock();
        }
    }

    /// The register the board reads as `chr0`: timer control in bit 4, the outer PRG bank bit in 3,
    /// and the inner bank in bits 2-0.
    fn chr0(mapper: &mut Mapper, cart: &mut Cart, val: u8) {
        serial_write(mapper, cart, 0xA000, val);
    }

    /// Opens the bank-switching lock the way the cart's initialisation does: one write with the
    /// timer control clear, then one with it set.
    fn unlock(mapper: &mut Mapper, cart: &mut Cart) {
        chr0(mapper, cart, 0b00000);
        chr0(mapper, cart, 0b10000);
    }

    /// Until the lock opens the board serves the first 32K - two *consecutive* 16K banks, not bank
    /// 0 twice, or $C000 would hold the wrong half and the reset vectors would point into the
    /// middle of the menu code.
    #[test]
    fn powers_on_locked_to_the_first_32k() {
        let (mapper, cart) = nes_event();
        assert!(board(&mapper).bank_switching_lock.locked());
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM");
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "bank 0");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "bank 1");
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 31);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "8K of CHR-RAM");
    }

    /// A locked board ignores every bank the game selects, which is what keeps the menu ROM in
    /// place until the tournament software says otherwise.
    #[test]
    fn banking_does_nothing_while_the_lock_is_shut() {
        let (mut mapper, mut cart) = nes_event();

        // Timer control stays set, so the lock never sees the 0-then-1 sequence.
        chr0(&mut mapper, &mut cart, 0b11100);
        serial_write(&mut mapper, &mut cart, 0xE000, 0b00111);
        assert!(board(&mapper).bank_switching_lock.locked());
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16);
    }

    /// The lock opens on a falling then rising timer-control bit, and never shuts again.
    #[test]
    fn the_lock_opens_on_the_initialisation_sequence_and_stays_open() {
        let (mut mapper, mut cart) = nes_event();

        chr0(&mut mapper, &mut cart, 0b00000);
        assert!(board(&mapper).bank_switching_lock.locked(), "half-way");
        chr0(&mut mapper, &mut cart, 0b10000);
        assert!(!board(&mapper).bank_switching_lock.locked(), "open");

        chr0(&mut mapper, &mut cart, 0b00000);
        assert!(!board(&mapper).bank_switching_lock.locked(), "still open");
    }

    /// With the outer bank bit clear the board banks the first 128K as one 32K window, taking the
    /// bank straight from `chr0` and ignoring both the MMC1 PRG register and its low bank bit.
    #[test]
    fn the_first_128k_banks_32k_at_a_time_from_chr0() {
        let (mut mapper, mut cart) = nes_event();
        unlock(&mut mapper, &mut cart);
        serial_write(&mut mapper, &mut cart, 0xE000, 0b00111);

        chr0(&mut mapper, &mut cart, 0b10100); // timer stopped, outer 0, inner 4
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 4 * 16);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 5 * 16, "one 32K bank");

        chr0(&mut mapper, &mut cart, 0b10101); // inner 5
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 4 * 16, "low bit ignored");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 5 * 16);
    }

    /// With the outer bit set the board switches to the second 128K and hands banking back to the
    /// MMC1: the PRG register supplies the inner bank, and the control register picks which half
    /// moves.
    #[test]
    fn the_second_128k_banks_16k_through_the_mmc1_registers() {
        let (mut mapper, mut cart) = nes_event();
        unlock(&mut mapper, &mut cart);

        // MMC1 mode 3: $8000 switchable, $C000 fixed - here fixed to the last bank of the 128K.
        serial_write(&mut mapper, &mut cart, 0x8000, 0b01100);
        chr0(&mut mapper, &mut cart, 0b11000); // timer stopped, outer 8
        serial_write(&mut mapper, &mut cart, 0xE000, 0b00010);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 10 * 16, "8 | 2");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 15 * 16, "8 | 7");

        // MMC1 mode 2: $8000 fixed to the first bank of the 128K, $C000 switchable.
        serial_write(&mut mapper, &mut cart, 0x8000, 0b01000);
        serial_write(&mut mapper, &mut cart, 0xE000, 0b00010);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 8 * 16, "8 | 0");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 10 * 16, "8 | 2");

        // MMC1 32K mode: the low bank bit is ignored again, but the bank still comes from the PRG
        // register rather than from `chr0`.
        serial_write(&mut mapper, &mut cart, 0x8000, 0b00000);
        serial_write(&mut mapper, &mut cart, 0xE000, 0b00101);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 12 * 16, "8 | 4");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 13 * 16);
    }

    /// The tournament timer only runs while the control bit is clear, and asserts IRQ when its
    /// counter's high byte reaches the value the dip switches encode.
    ///
    /// The switches sit in bits 4-1 of that byte with bit 5 always set, so the shipped setting of
    /// $28 is 0x2800_0000 CPU cycles - about 6 minutes 15 seconds of NTSC play, which is the
    /// tournament's time limit.
    #[test]
    fn the_timer_runs_only_when_started_and_fires_at_the_dip_switch_count() {
        let mut timer = Timer::new(SWITCHES);
        assert_eq!(timer.target_high_byte, 0x28, "switch 3 is the only one set");

        for _ in 0..100 {
            timer.clock();
        }
        assert_eq!(timer.value, 0, "a stopped timer does not count");
        assert!(!timer.irq_pending());

        timer.start();
        timer.clock();
        assert_eq!(timer.value, 1);

        // Fast-forward rather than clocking 671 million times.
        timer.value = 0x27FF_FFFF;
        timer.clock();
        assert_eq!(timer.value, 0x2800_0000);
        assert!(timer.irq_pending(), "the IRQ line asserts");
        assert!(!timer.started, "and the counter stops so it stays asserted");

        timer.clock();
        assert_eq!(timer.value, 0x2800_0000, "a stopped timer holds its count");

        let seconds = f64::from(timer.value) / 1_789_773.0;
        assert!(
            (374.0..376.0).contains(&seconds),
            "{seconds} s is not the ~6:15 time limit"
        );
    }

    /// A restart clears the count, and with it the IRQ - the counter is the only thing holding the
    /// line, since there is no acknowledge register.
    #[test]
    fn restarting_the_timer_clears_the_irq() {
        let mut timer = Timer::new(SWITCHES);
        timer.start();
        timer.value = 0x2800_0000;
        assert!(timer.irq_pending());

        timer.stop();
        timer.start();
        assert_eq!(timer.value, 0);
        assert!(!timer.irq_pending());

        // Reset does the same, and leaves the counter stopped rather than running from zero.
        timer.start();
        timer.value = 0x2800_0000;
        timer.reset(ResetKind::Hard);
        assert_eq!(timer.value, 0);
        assert!(!timer.irq_pending());
        timer.clock();
        assert_eq!(timer.value, 0, "reset stops the counter too");
    }

    /// The board's IRQ reaches the CPU through `Map::irq_pending`, and the timer only runs while
    /// the board is clocked.
    #[test]
    fn the_timer_is_clocked_through_the_board() {
        let (mut mapper, mut cart) = nes_event();
        assert!(!mapper.irq_pending());

        // The reset state has the timer stopped.
        for _ in 0..1000 {
            mapper.clock();
        }
        assert_eq!(board(&mapper).timer.value, 0);

        chr0(&mut mapper, &mut cart, 0b00000); // timer control clear: run
        let started = board(&mapper).timer.value;
        for _ in 0..1000 {
            mapper.clock();
        }
        assert_eq!(board(&mapper).timer.value, started + 1000);

        chr0(&mut mapper, &mut cart, 0b10000); // timer control set: stop
        let stopped = board(&mapper).timer.value;
        for _ in 0..1000 {
            mapper.clock();
        }
        assert_eq!(board(&mapper).timer.value, stopped, "stopped");
    }

    /// Reset puts the cart back to the menu: banking locked to the first 32K and the timer stopped
    /// at zero, whatever the previous game left behind.
    #[test]
    fn reset_relocks_banking_and_stops_the_timer() {
        let (mut mapper, mut cart) = nes_event();
        unlock(&mut mapper, &mut cart);
        chr0(&mut mapper, &mut cart, 0b00010); // timer running, inner bank 2
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 16);
        assert_ne!(board(&mapper).timer.value, 0, "timer running");

        for kind in [ResetKind::Soft, ResetKind::Hard] {
            mapper.reset(kind);
            mapper.update_banks(&mut cart.memory);
            assert!(board(&mapper).bank_switching_lock.locked(), "{kind:?}");
            assert_eq!(board(&mapper).timer.value, 0, "{kind:?}");
            assert!(!mapper.irq_pending(), "{kind:?}");
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "{kind:?}");
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "{kind:?}");
        }
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state - and here that includes the lock,
    /// which is not derivable from the MMC1 registers.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = nes_event();
        unlock(&mut mapper, &mut cart);
        serial_write(&mut mapper, &mut cart, 0x8000, 0b01100);
        chr0(&mut mapper, &mut cart, 0b11000);
        serial_write(&mut mapper, &mut cart, 0xE000, 0b00010);

        let sample = |mapper: &Mapper, cart: &Cart| {
            [
                prg_peek(mapper, cart, 0x6000),
                prg_peek(mapper, cart, 0x8000),
                prg_peek(mapper, cart, 0xC000),
                chr_peek(mapper, cart, 0x0000),
            ]
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
