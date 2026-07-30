//! `Taito TC0190`/`TC0350` (Mapper 033) and `TC0690` (Mapper 048).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_033>
//! <https://www.nesdev.org/wiki/INES_Mapper_048>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, Mmc3},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// Taito board revision.
///
/// Both bank identically. What the TC0690 adds is an MMC3 scanline IRQ and a mirroring register of
/// its own, which is why it decodes twice as much of the address space.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// TC0190/TC0350, i.e. mapper 033: no IRQ, and mirroring in the PRG register.
    #[default]
    Tc0190,
    /// TC0690, i.e. mapper 048: the same board plus an MMC3 IRQ counter and a mirroring register.
    Tc0690,
}

impl Revision {
    /// The register a CPU address selects, or `None` for an address this board does not answer.
    ///
    /// The range matters as much as the mask. The TC0190 answers `$8000-$BFFF` only, selecting one
    /// of eight registers from four address lines; the TC0690 answers all of `$8000-$FFFF`, and
    /// spends the extra registers that gives it on its IRQ counter and a mirroring register. Mask
    /// an address without bounding it and a write to the TC0690's IRQ enable at `$C002` reads as a
    /// CHR bank write on a TC0190.
    const fn register_addr(self, addr: u16) -> Option<u16> {
        match self {
            Self::Tc0190 if matches!(addr, 0x8000..=0xBFFF) => Some(addr & 0xA003),
            Self::Tc0690 if addr >= 0x8000 => Some(addr & 0xE003),
            _ => None,
        }
    }
}

/// `Taito TC0190`/`TC0350` (Mapper 033) and `TC0690` (Mapper 048).
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct TaitoTc0190 {
    pub revision: Revision,
    pub mirroring: Mirroring,
    /// 8K banks at $8000 and $A000; $C000 and $E000 are fixed to the last two.
    pub prg_banks: [u8; 2],
    /// Two 2K selects covering $0000-$0FFF, then four 1K selects covering $1000-$1FFF.
    pub chr_banks: [u8; 6],
    /// The TC0690's counter, which is MMC3's: same A12 edge detection, same reload semantics.
    ///
    /// Only the IRQ half is used - this board has its own register layout and never touches the
    /// MMC3 bank registers.
    pub mmc3: Mmc3,
    /// CPU cycles left before a counter hit reaches the CPU, or 0 when nothing is in flight.
    pub irq_delay: u8,
    /// Whether the IRQ line is asserted, as opposed to `mmc3.irq_pending`'s "the counter has hit".
    ///
    /// The two differ for the few cycles of `irq_delay`, which is the point of keeping both.
    pub irq_pending: bool,
}

impl TaitoTc0190 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW_2K: usize = 2 * 1024;
    const CHR_WINDOW_1K: usize = 1024;
    /// Six bits of PRG select, because bit 6 is the TC0190's mirroring bit.
    const PRG_BANK_MASK: u8 = 0x3F;
    /// Bit 6 of the TC0190's $8000 and of the TC0690's $E000: set is horizontal, clear vertical.
    const MIRRORING_MASK: u8 = 0x40;
    /// How long a counter hit takes to reach the CPU on a TC0690.
    ///
    /// MMC3 asserts immediately; this board trips "about a 4 CPU cycle delay from the normal MMC3
    /// IRQ time", and without the delay the games that split the screen with it shake.
    const IRQ_DELAY: u8 = 4;

    // PPU $0000..=$0FFF 2x 2K CHR Banks
    // PPU $1000..=$1FFF 4x 1K CHR Banks
    // CPU $8000..=$9FFF 8K PRG-ROM Bank Switchable
    // CPU $A000..=$BFFF 8K PRG-ROM Bank Switchable
    // CPU $C000..=$DFFF 8K PRG-ROM Fixed to the second-to-last bank
    // CPU $E000..=$FFFF 8K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart, revision: Revision) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            revision,
            mirroring: cart.mirroring(),
            prg_banks: [0; 2],
            chr_banks: [0; 6],
            mmc3: Mmc3::default(),
            irq_delay: 0,
            irq_pending: false,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    const fn is_tc0690(&self) -> bool {
        matches!(self.revision, Revision::Tc0690)
    }

    const fn mirroring_from(val: u8) -> Mirroring {
        if val & Self::MIRRORING_MASK == Self::MIRRORING_MASK {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        }
    }
}

impl Map for TaitoTc0190 {
    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            out.push((["PRG $8000", "PRG $A000"][slot], u32::from(*bank)));
        }
        for (slot, bank) in self.chr_banks.iter().enumerate() {
            out.push((
                [
                    "CHR $0000 (2K)",
                    "CHR $0800 (2K)",
                    "CHR $1000",
                    "CHR $1400",
                    "CHR $1800",
                    "CHR $1C00",
                ][slot],
                u32::from(*bank),
            ));
        }
        if self.is_tc0690() {
            out.push(("IRQ latch", u32::from(self.mmc3.irq_latch)));
            out.push(("IRQ counter", u32::from(self.mmc3.irq_counter)));
            out.push(("IRQ enabled", u32::from(self.mmc3.irq_enabled)));
            out.push(("IRQ delay", u32::from(self.irq_delay)));
            out.push(("IRQ pending", u32::from(self.irq_pending)));
        }
    }

    fn mapper_ops(&self) -> MapperOps {
        if self.is_tc0690() {
            MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::WATCHES_PPU_BUS
        } else {
            MapperOps::empty()
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    /// The TC0690 counts scanlines from A12 rising edges, exactly as MMC3 does.
    fn ppu_bus_addr(&mut self, _memory: &mut Memory, addr: u16) {
        let was_pending = self.mmc3.irq_pending();
        self.mmc3.clock_irq(addr);
        // Start the delay on the hit itself, not on every edge that leaves the counter pending -
        // an unacknowledged IRQ is already asserted and has nothing left to wait for.
        if !was_pending && self.mmc3.irq_pending() {
            self.irq_delay = Self::IRQ_DELAY;
        }
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        let Some(reg) = self.revision.register_addr(addr) else {
            return;
        };
        match reg {
            0x8000 => {
                self.prg_banks[0] = val & Self::PRG_BANK_MASK;
                // The TC0190 has nowhere else to put mirroring; the TC0690 uses $E000 and leaves
                // this bit alone.
                if !self.is_tc0690() {
                    self.mirroring = Self::mirroring_from(val);
                }
            }
            0x8001 => self.prg_banks[1] = val & Self::PRG_BANK_MASK,
            0x8002 => self.chr_banks[0] = val,
            0x8003 => self.chr_banks[1] = val,
            reg @ 0xA000..=0xA003 => self.chr_banks[2 + (reg & 0x03) as usize] = val,
            // The four IRQ registers are MMC3's, reached through addresses of this board's own and
            // with the reload value inverted: writing $06 here is writing $F9 on an MMC3.
            0xC000 => self.mmc3.write_irq_latch(val ^ 0xFF),
            0xC001 => self.mmc3.write_irq_reload(),
            0xC002 => self.mmc3.write_irq_enable(),
            0xC003 => {
                self.mmc3.write_irq_disable();
                self.irq_delay = 0;
                self.irq_pending = false;
            }
            0xE000 => self.mirroring = Self::mirroring_from(val),
            _ => return,
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_chr(
            0x0000,
            Self::CHR_WINDOW_2K,
            i32::from(self.chr_banks[0]),
            Src::Chr,
        );
        memory.map_chr(
            0x0800,
            Self::CHR_WINDOW_2K,
            i32::from(self.chr_banks[1]),
            Src::Chr,
        );
        for (slot, bank) in self.chr_banks[2..].iter().enumerate() {
            let addr = 0x1000 + (slot * Self::CHR_WINDOW_1K) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW_1K, i32::from(*bank), Src::Chr);
        }

        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_banks[0]),
            Src::PrgRom,
        );
        memory.map_prg(
            0xA000,
            Self::PRG_WINDOW,
            i32::from(self.prg_banks[1]),
            Src::PrgRom,
        );
        // -1 is the last bank, -2 the one before it.
        memory.map_prg(0xC000, Self::PRG_WINDOW, -2, Src::PrgRom);
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);

        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        self.mmc3.clock();
        if self.irq_delay > 0 {
            self.irq_delay -= 1;
            if self.irq_delay == 0 {
                self.irq_pending = true;
            }
        }
    }

    fn reset(&mut self, kind: ResetKind) {
        self.mmc3.reset(kind);
        self.irq_delay = 0;
        self.irq_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (16 8K banks), no PRG-RAM - neither board has any - and 64K CHR-ROM.
    fn load(mapper_num: u16) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 64 * 1024);
        cart.header.mapper_num = mapper_num;
        let mapper = Mapper::from_cart(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    fn tc0190() -> (Mapper, Cart) {
        load(33)
    }

    fn tc0690() -> (Mapper, Cart) {
        load(48)
    }

    fn board(mapper: &Mapper) -> &TaitoTc0190 {
        match mapper {
            Mapper::TaitoTc0190(board) => board,
            _ => unreachable!("mapper is a TaitoTc0190"),
        }
    }

    fn clock(mapper: &mut Mapper, cycles: usize) {
        for _ in 0..cycles {
            mapper.clock();
        }
    }

    /// One A12 rising edge, which is what clocks the counter.
    ///
    /// The board's filter times the low phase from its own cycle counter and ignores an edge that
    /// arrives too soon after the last one, so this clocks either side of the transition - and
    /// clocks before the low read too, since a low phase timed at cycle zero reads as "no low
    /// phase at all".
    fn a12_edge(mapper: &mut Mapper, cart: &mut Cart) {
        clock(mapper, 8);
        mapper.ppu_bus_addr(&mut cart.memory, 0x0000);
        clock(mapper, 8);
        mapper.ppu_bus_addr(&mut cart.memory, 0x1000);
    }

    /// Both mapper numbers have to reach this board through `Mapper::from_cart` - the path a ROM
    /// actually loads through - and pick up the revision the number names.
    #[test]
    fn both_mapper_numbers_load_this_board() {
        for (mapper_num, expected) in [(33, Revision::Tc0190), (48, Revision::Tc0690)] {
            let (mapper, _cart) = load(mapper_num);
            assert_eq!(board(&mapper).revision, expected, "mapper {mapper_num}");
        }
    }

    /// $E000 is hard-wired to the last 8K, which is where the reset vector lives, and $C000 to the
    /// one before it - if either is wrong nothing boots at all.
    #[test]
    fn powers_on_with_the_last_two_prg_banks_fixed() {
        for (mapper, cart) in [tc0190(), tc0690()] {
            // 128K in 1K pages is 128; the last 8K window starts at page 120, the one before at 112.
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), 112);
            assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120);
            assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 127);
        }
    }

    /// Two 8K windows, each with its own register, and the fixed pair does not move.
    #[test]
    fn prg_selects_two_8k_windows() {
        for (mut mapper, mut cart) in [tc0190(), tc0690()] {
            write(&mut mapper, &mut cart, 0x8000, 3);
            write(&mut mapper, &mut cart, 0x8001, 5);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
            assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
            assert_eq!(prg_peek(&mapper, &cart, 0xC000), 112, "still fixed");
            assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "still fixed");
        }
    }

    /// The low half of CHR is two 2K windows and the high half four 1K ones, so a 2K register
    /// selects in units of 2K: bank 3 is pages 6 and 7, not page 3.
    #[test]
    fn chr_maps_two_2k_windows_then_four_1k_ones() {
        for (mut mapper, mut cart) in [tc0190(), tc0690()] {
            write(&mut mapper, &mut cart, 0x8002, 3);
            write(&mut mapper, &mut cart, 0x8003, 5);
            for (slot, bank) in [0xA000, 0xA001, 0xA002, 0xA003].into_iter().zip(9..) {
                write(&mut mapper, &mut cart, slot, bank);
            }

            assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 6);
            assert_eq!(chr_peek(&mapper, &cart, 0x0400), 0x80 | 7, "second half");
            assert_eq!(chr_peek(&mapper, &cart, 0x0800), 0x80 | 10);
            assert_eq!(chr_peek(&mapper, &cart, 0x0C00), 0x80 | 11, "second half");
            for (slot, bank) in (9..13).enumerate() {
                let addr = 0x1000 + (slot * 1024) as u16;
                assert_eq!(
                    chr_peek(&mapper, &cart, addr),
                    0x80 | bank,
                    "1K slot {slot}"
                );
            }
        }
    }

    /// The TC0190 carries mirroring in bit 6 of the same register as its first PRG bank, so that
    /// bit must not reach the bank select either.
    #[test]
    fn tc0190_carries_mirroring_in_its_prg_register() {
        let (mut mapper, mut cart) = tc0190();
        write(&mut mapper, &mut cart, 0x8000, 0x40 | 3);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            3 * 8,
            "bank 3, not bank 67"
        );

        write(&mut mapper, &mut cart, 0x8000, 3);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }

    /// The TC0690 has a register of its own for mirroring, and bit 6 of $8000 means nothing to it.
    #[test]
    fn tc0690_has_a_mirroring_register_of_its_own() {
        let (mut mapper, mut cart) = tc0690();
        write(&mut mapper, &mut cart, 0xE000, 0x40);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        write(&mut mapper, &mut cart, 0xE000, 0x00);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);

        write(&mut mapper, &mut cart, 0x8000, 0x40 | 3);
        assert_eq!(
            mapper.mirroring(),
            Mirroring::Vertical,
            "$8000 does not carry mirroring on a TC0690"
        );
    }

    /// The TC0190 answers $8000-$BFFF only. Its four registers are then reached by two address
    /// lines, so a game writing $8802 still lands on the first CHR register.
    #[test]
    fn tc0190_decodes_only_the_lower_half_of_the_register_space() {
        let (mut mapper, mut cart) = tc0190();
        write(&mut mapper, &mut cart, 0x8802, 3);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 6, "$8802 is $8002");

        // $C002 is the TC0690's IRQ enable. A TC0190 does not decode it at all - under its own
        // mask alone the address would read as $8002 and re-bank CHR.
        write(&mut mapper, &mut cart, 0xC002, 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 6, "unchanged");
    }

    /// A TC0190 has no counter, so the TC0690's IRQ writes must leave the line alone rather than
    /// firing an interrupt no game is waiting for.
    #[test]
    fn tc0190_has_no_irq() {
        let (mut mapper, mut cart) = tc0190();
        assert!(
            !mapper.mapper_ops().contains(MapperOps::IRQ),
            "a TC0190 must not claim an IRQ"
        );
        for addr in [0xC000, 0xC001, 0xC002, 0xC003] {
            write(&mut mapper, &mut cart, addr, 0x01);
        }
        for _ in 0..1000 {
            a12_edge(&mut mapper, &mut cart);
        }
        assert!(!mapper.irq_pending());
    }

    /// The reload value arrives inverted, so writing $06 counts 249 scanlines and not 6. Getting
    /// this backwards puts every split in the wrong place.
    #[test]
    fn tc0690_inverts_the_irq_reload_value() {
        let (mut mapper, mut cart) = tc0690();
        write(&mut mapper, &mut cart, 0xC000, 0x06);
        assert_eq!(board(&mapper).mmc3.irq_latch, 0xF9);
    }

    /// A counter hit reaches the CPU four cycles late on this board, which is what games splitting
    /// the screen with it are written against.
    #[test]
    fn tc0690_irq_arrives_a_few_cycles_after_the_counter_hits() {
        let (mut mapper, mut cart) = tc0690();
        // Reload 1, inverted, so the counter hits on the second edge: the first reloads it.
        write(&mut mapper, &mut cart, 0xC000, 0x01 ^ 0xFF);
        write(&mut mapper, &mut cart, 0xC001, 0x00);
        write(&mut mapper, &mut cart, 0xC002, 0x00);

        a12_edge(&mut mapper, &mut cart);
        assert!(!mapper.irq_pending(), "the first edge only reloads");

        a12_edge(&mut mapper, &mut cart);
        assert!(
            board(&mapper).mmc3.irq_pending(),
            "the counter has hit zero"
        );
        assert!(!mapper.irq_pending(), "but the CPU has not seen it yet");

        clock(&mut mapper, usize::from(TaitoTc0190::IRQ_DELAY) - 1);
        assert!(!mapper.irq_pending(), "still waiting out the delay");
        mapper.clock();
        assert!(mapper.irq_pending());

        write(&mut mapper, &mut cart, 0xC003, 0x00);
        assert!(!mapper.irq_pending(), "acknowledged");
    }

    /// Acknowledging during the delay has to cancel the interrupt outright, or the board asserts an
    /// IRQ the game has already told it to drop.
    #[test]
    fn tc0690_acknowledging_cancels_an_irq_still_in_flight() {
        let (mut mapper, mut cart) = tc0690();
        write(&mut mapper, &mut cart, 0xC000, 0x01 ^ 0xFF);
        write(&mut mapper, &mut cart, 0xC001, 0x00);
        write(&mut mapper, &mut cart, 0xC002, 0x00);
        a12_edge(&mut mapper, &mut cart);
        a12_edge(&mut mapper, &mut cart);

        write(&mut mapper, &mut cart, 0xC003, 0x00);
        clock(&mut mapper, 16);
        assert!(!mapper.irq_pending());
    }

    /// `update_banks` has to rebuild every window from the register state alone - that is what
    /// `Bus::rebuild_mapper_state` calls after loading a save state, which carries no page tables.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = tc0690();
        write(&mut mapper, &mut cart, 0x8000, 3);
        write(&mut mapper, &mut cart, 0x8001, 5);
        write(&mut mapper, &mut cart, 0x8002, 7);
        write(&mut mapper, &mut cart, 0x8003, 9);
        write(&mut mapper, &mut cart, 0xA000, 11);
        write(&mut mapper, &mut cart, 0xE000, 0x40);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain(
                    [0x0000, 0x0400, 0x0800, 0x1000, 0x1C00]
                        .into_iter()
                        .map(|addr| chr_peek(mapper, cart, addr)),
                )
                .collect()
        };
        let before = sample(&mapper, &cart);

        // Wipe every mapping, then rebuild from the registers alone.
        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
    }
}
