//! `SxROM`/`MMC1` (Mapper 001).
//!
//! <https://wiki.nesdev.org/w/index.php/MMC1>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, Mmc1, Mmc1Revision},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `SxROM`/`MMC1` (Mapper 001).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sxrom {
    pub mmc1: Mmc1,
    pub submapper_num: u8,
    /// 512K carts use a CHR register bit as the high PRG bank bit.
    pub prg_select: bool,
}

impl Sxrom {
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;
    const PRG_BANK_MASK: u8 = 0x0F;
    const PRG_BANK_SELECT_MASK: u8 = 0x10;

    // PPU $0000..=$0FFF 4K CHR Bank Switchable, or 8K across both in 8K mode
    // PPU $1000..=$1FFF 4K CHR Bank Switchable
    // CPU $6000..=$7FFF 8K PRG-RAM, write-protectable
    // CPU $8000..=$BFFF 16K PRG-ROM, switchable or fixed to first depending on mode
    // CPU $C000..=$FFFF 16K PRG-ROM, fixed to last or switchable depending on mode
    pub fn load(cart: &mut Cart, revision: Mmc1Revision) -> Result<Mapper, mapper::Error> {
        let mut sxrom = Self {
            mmc1: Mmc1::new(revision),
            submapper_num: cart.submapper_num(),
            prg_select: cart.prg_rom_size == 0x80000,
        };
        sxrom.update_banks(&mut cart.memory);
        Ok(sxrom.into())
    }
}

impl Map for Sxrom {
    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("PRG", u32::from(self.mmc1.prg)));
        out.push(("CHR 0", u32::from(self.mmc1.chr0)));
        out.push(("CHR 1", u32::from(self.mmc1.chr1)));
        out.push(("PRG mode 16K", u32::from(self.mmc1.prg_mode)));
        out.push(("PRG bank select", u32::from(self.mmc1.prg_bank_select)));
        out.push(("CHR mode 4K", u32::from(self.mmc1.chr_mode)));
        out.push(("PRG-RAM disabled", u32::from(self.mmc1.prg_ram_disabled)));
        // The 5-bit serial register a game is part-way through filling, which is what to look at
        // when a bank write appears not to have taken effect.
        out.push(("Shift register", u32::from(self.mmc1.write_buffer)));
        out.push(("Shift count", u32::from(self.mmc1.shift_count)));
    }
    fn mapper_ops(&self) -> MapperOps {
        // The busy-cycle counter that ignores rapid consecutive writes needs a per-cycle clock;
        // MMC1 has no IRQ.
        MapperOps::CLOCKED
    }

    fn mirroring(&self) -> Mirroring {
        self.mmc1.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        // $6000..=$7FFF is PRG-RAM, already stored by the caller when the window is writable.
        if addr >= 0x8000 && self.mmc1.process_shift_register_write(addr, val) {
            self.update_banks(memory);
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        let mmc1 = &self.mmc1;
        // In 4K CHR mode the second CHR register supplies the extra PRG bank bit once it has been
        // the most recently written one.
        let extra_reg = if mmc1.last_chr_reg == 0xC000 && mmc1.chr_mode {
            mmc1.chr1
        } else {
            mmc1.chr0
        };
        let prg_high = if self.prg_select {
            extra_reg & Self::PRG_BANK_SELECT_MASK
        } else {
            0x00
        };

        if self.submapper_num == 5 {
            // SUROM variants with fixed 32K PRG.
            memory.map_prg(0x8000, Self::PRG_WINDOW, 0, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, 1, Src::PrgRom);
        } else if mmc1.prg_mode {
            if mmc1.prg_bank_select {
                // $8000 switchable, $C000 fixed to the last bank of the 256K half.
                memory.map_prg(
                    0x8000,
                    Self::PRG_WINDOW,
                    i32::from(mmc1.prg | prg_high),
                    Src::PrgRom,
                );
                memory.map_prg(
                    0xC000,
                    Self::PRG_WINDOW,
                    i32::from(Self::PRG_BANK_MASK | prg_high),
                    Src::PrgRom,
                );
            } else {
                // $8000 fixed to the first bank of the half, $C000 switchable.
                memory.map_prg(0x8000, Self::PRG_WINDOW, i32::from(prg_high), Src::PrgRom);
                memory.map_prg(
                    0xC000,
                    Self::PRG_WINDOW,
                    i32::from(mmc1.prg | prg_high),
                    Src::PrgRom,
                );
            }
        } else {
            // 32K mode ignores the low bank bit.
            let bank = i32::from((mmc1.prg & 0xFE) | prg_high);
            memory.map_prg(0x8000, Self::PRG_WINDOW, bank, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, bank + 1, Src::PrgRom);
        }

        if mmc1.chr_mode {
            memory.map_chr(0x0000, Self::CHR_WINDOW, i32::from(mmc1.chr0), Src::Chr);
            memory.map_chr(0x1000, Self::CHR_WINDOW, i32::from(mmc1.chr1), Src::Chr);
        } else {
            // 8K mode ignores the low bank bit.
            let bank = i32::from(mmc1.chr0 & 0x1E);
            memory.map_chr(0x0000, Self::CHR_WINDOW, bank, Src::Chr);
            memory.map_chr(0x1000, Self::CHR_WINDOW, bank + 1, Src::Chr);
        }

        // Disabled PRG-RAM reads as open bus and ignores writes, which unmapping gives for free.
        if mmc1.prg_ram_enabled() {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }

        memory.set_mirroring(mmc1.mirroring);
    }

    fn clock(&mut self) {
        self.mmc1.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc1.reset(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (8 16K banks), 8K PRG-RAM, 64K CHR-ROM (16 4K banks).
    fn load(revision: Mmc1Revision) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        let mapper = Sxrom::load(&mut cart, revision).expect("valid mapper");
        (mapper, cart)
    }

    fn mmc1() -> (Mapper, Cart) {
        load(Mmc1Revision::BC)
    }

    /// Feeds a register through the serial port: five writes of one bit each, LSB first.
    ///
    /// MMC1 ignores a write that lands within two CPU cycles of the previous one - it is how the
    /// real chip rejects the second half of a read-modify-write - so the clocks between bits are
    /// load-bearing, not padding.
    fn serial_write(mapper: &mut Mapper, cart: &mut Cart, addr: u16, val: u8) {
        for bit in 0..5 {
            write(mapper, cart, addr, (val >> bit) & 0x01);
            mapper.clock();
            mapper.clock();
        }
    }

    /// Sets the control register: mirroring in bits 0-1, PRG slot select in 2, PRG mode in 3, CHR
    /// mode in 4.
    fn control(mapper: &mut Mapper, cart: &mut Cart, val: u8) {
        serial_write(mapper, cart, 0x8000, val);
    }

    /// MMC1 powers on in 16K mode with $C000 fixed to the last bank, which is what puts a reset
    /// vector under the CPU before the game has written a single register.
    #[test]
    fn powers_on_in_16k_mode_with_the_last_bank_at_c000() {
        let (mapper, cart) = mmc1();
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "bank 0 at $8000");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "last bank");
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 127);
    }

    /// A write within two cycles of the previous one is dropped, so a four-bit-then-ignored
    /// sequence must not latch a register.
    #[test]
    fn consecutive_writes_within_two_cycles_are_ignored() {
        let (mut mapper, mut cart) = mmc1();

        // Five bits with no clocks between them: only the first is accepted, so the shift register
        // never reaches its fifth bit and nothing is latched.
        for _ in 0..5 {
            write(&mut mapper, &mut cart, 0xE000, 0x01);
        }
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            0,
            "no register was latched"
        );

        // The same value fed properly does latch.
        serial_write(&mut mapper, &mut cart, 0xE000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16);
    }

    /// The three PRG modes: 32K, 16K with $8000 fixed, and 16K with $C000 fixed.
    #[test]
    fn prg_modes_select_32k_or_either_fixed_16k_half() {
        let (mut mapper, mut cart) = mmc1();

        // Mode 3 (bits 3 and 2 set): $8000 switchable, $C000 fixed last.
        control(&mut mapper, &mut cart, 0b01100);
        serial_write(&mut mapper, &mut cart, 0xE000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16, "switchable");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "fixed last");

        // Mode 2 (bit 3 set, bit 2 clear): $8000 fixed first, $C000 switchable.
        control(&mut mapper, &mut cart, 0b01000);
        serial_write(&mut mapper, &mut cart, 0xE000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "fixed first");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 3 * 16, "switchable");

        // Mode 0/1 (bit 3 clear): one 32K bank, ignoring the low bit of the bank number.
        control(&mut mapper, &mut cart, 0b00000);
        serial_write(&mut mapper, &mut cart, 0xE000, 4);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 4 * 16);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 5 * 16, "the 32K is one bank");
        serial_write(&mut mapper, &mut cart, 0xE000, 5);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            4 * 16,
            "the low bank bit is ignored in 32K mode"
        );
    }

    /// CHR is either one 8K bank or two independent 4K banks.
    #[test]
    fn chr_modes_select_one_8k_bank_or_two_4k_banks() {
        let (mut mapper, mut cart) = mmc1();

        // Bit 4 clear: 8K mode, low bank bit ignored.
        control(&mut mapper, &mut cart, 0b00000);
        serial_write(&mut mapper, &mut cart, 0xA000, 4);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (4 * 4));
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | (5 * 4), "same 8K");
        serial_write(&mut mapper, &mut cart, 0xA000, 5);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x80 | (4 * 4),
            "the low bank bit is ignored in 8K mode"
        );

        // Bit 4 set: two independent 4K banks.
        control(&mut mapper, &mut cart, 0b10000);
        serial_write(&mut mapper, &mut cart, 0xA000, 3);
        serial_write(&mut mapper, &mut cart, 0xC000, 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 4));
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | (9 * 4));
    }

    /// Bit 7 of any load-register write resets the shift register and forces the control register's
    /// PRG bits back to mode 3 - which is how a game recovers a known state at boot.
    #[test]
    fn bit_7_resets_the_shift_register_and_restores_mode_3() {
        let (mut mapper, mut cart) = mmc1();

        control(&mut mapper, &mut cart, 0b00000); // 32K mode
        serial_write(&mut mapper, &mut cart, 0xE000, 4);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 5 * 16, "32K mode");

        write(&mut mapper, &mut cart, 0x8000, 0x80);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xC000),
            7 * 16,
            "back to mode 3, last bank fixed at $C000"
        );
        // The reset write arms the two-cycle lockout like any other, so the next bit needs to wait.
        mapper.clock();
        mapper.clock();

        // The shift register is also empty, so a fresh five-bit sequence latches cleanly.
        serial_write(&mut mapper, &mut cart, 0xE000, 2);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 16);
    }

    /// Mirroring comes from the low two bits of the control register, and MMC1 is the one board
    /// whose one-screen modes come before the usual vertical/horizontal pair.
    #[test]
    fn control_bits_0_and_1_set_mirroring() {
        let (mut mapper, mut cart) = mmc1();

        control(&mut mapper, &mut cart, 0b00010); // vertical
        cart.memory.chr_write(0x2000, 0x11);
        assert_eq!(chr_peek(&mapper, &cart, 0x2800), 0x11, "vertical");

        control(&mut mapper, &mut cart, 0b00011); // horizontal
        cart.memory.chr_write(0x2000, 0x22);
        assert_eq!(chr_peek(&mapper, &cart, 0x2400), 0x22, "horizontal");

        control(&mut mapper, &mut cart, 0b00000); // one-screen, lower
        cart.memory.chr_write(0x2000, 0x33);
        for nt in [0x2400, 0x2800, 0x2C00] {
            assert_eq!(chr_peek(&mapper, &cart, nt), 0x33, "one-screen A");
        }
    }

    /// Bit 4 of the PRG register disables PRG-RAM, which must read as open bus rather than as
    /// stale contents. MMC1A has no such line and ignores the bit.
    #[test]
    fn prg_ram_disable_is_honoured_except_on_mmc1a() {
        let (mut mapper, mut cart) = mmc1();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "enabled at power-on");

        serial_write(&mut mapper, &mut cart, 0xE000, 0x10);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "disabled reads as 0");

        serial_write(&mut mapper, &mut cart, 0xE000, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "re-enabled");

        let (mut mapper, mut cart) = load(Mmc1Revision::A);
        serial_write(&mut mapper, &mut cart, 0xE000, 0x10);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x6000),
            0x5A,
            "MMC1A has no PRG-RAM enable line"
        );
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = mmc1();
        control(&mut mapper, &mut cart, 0b11000);
        serial_write(&mut mapper, &mut cart, 0xA000, 3);
        serial_write(&mut mapper, &mut cart, 0xC000, 9);
        serial_write(&mut mapper, &mut cart, 0xE000, 5);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x6000, 0x8000, 0xC000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain(
                    [0x0000, 0x1000]
                        .into_iter()
                        .map(|addr| chr_peek(mapper, cart, addr)),
                )
                .collect()
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
