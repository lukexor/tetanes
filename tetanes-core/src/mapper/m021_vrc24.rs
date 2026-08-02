//! `VRC2`/`VRC4` (Mappers 021, 022, 023, 025).
//!
//! <https://www.nesdev.org/wiki/VRC2_and_VRC4>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, vrc_irq::VrcIrq},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `VRC2`/`VRC4` revision.
///
/// Nine boards over four mapper numbers. They bank identically; what differs is which CPU address
/// lines carry the two register-select bits (see [`Revision::select_lines`]), and that VRC2 has no
/// IRQ counter, one mirroring bit instead of two, and no PRG swap mode.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// VRC2a (Mapper 022)
    Vrc2a,
    /// VRC2b (Mapper 023 submapper 3)
    Vrc2b,
    /// VRC2c (Mapper 025 submapper 3)
    Vrc2c,
    /// VRC4a (Mapper 021 submapper 1)
    #[default]
    Vrc4a,
    /// VRC4b (Mapper 025 submapper 1)
    Vrc4b,
    /// VRC4c (Mapper 021 submapper 2)
    Vrc4c,
    /// VRC4d (Mapper 025 submapper 2)
    Vrc4d,
    /// VRC4e (Mapper 023 submapper 2)
    Vrc4e,
    /// VRC4f (Mapper 023 submapper 1)
    Vrc4f,
}

impl Revision {
    /// Which CPU address bit carries each of the two register-select bits, low bit first.
    ///
    /// A register is at `$x000` plus a two-bit index, but the board wires that index to a different
    /// pair of address lines on nearly every revision - which is the whole reason a mapper number
    /// alone does not identify one of these boards.
    pub const fn select_lines(self) -> [u16; 2] {
        match self {
            // A1 and A0, i.e. the pair the register layout suggests, swapped.
            Self::Vrc2a | Self::Vrc2c | Self::Vrc4b => [0x02, 0x01],
            Self::Vrc2b | Self::Vrc4f => [0x01, 0x02],
            Self::Vrc4a => [0x02, 0x04],
            Self::Vrc4c => [0x40, 0x80],
            Self::Vrc4d => [0x08, 0x04],
            Self::Vrc4e => [0x04, 0x08],
        }
    }

    /// Whether this is a VRC2: no IRQ counter, one mirroring bit, no PRG swap mode.
    pub const fn is_vrc2(self) -> bool {
        matches!(self, Self::Vrc2a | Self::Vrc2b | Self::Vrc2c)
    }
}

/// `VRC2`/`VRC4` (Mappers 021, 022, 023, 025).
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc24 {
    pub revision: Revision,
    /// Which CPU address bit carries each of the two register-select bits.
    ///
    /// Taken from the revision when the submapper names one, and otherwise the union of every
    /// candidate for the mapper number - see [`Vrc24::load`]. Not derivable from `revision` alone,
    /// which is why it is stored.
    pub select: [u16; 2],
    pub mirroring: Mirroring,
    pub irq: VrcIrq,
    /// 8K banks at $8000 and $A000; in PRG swap mode the first moves to $C000.
    pub prg_banks: [u8; 2],
    /// Bit 1 of $9002: swaps the switchable and fixed 8K banks at $8000 and $C000.
    pub prg_swap: bool,
    /// Bit 0 of $9002. Reported for a debugger; see `update_banks` for why it gates nothing.
    pub wram_enabled: bool,
    /// 9-bit CHR selects, one per 1K slot, each written as a low nibble and a high half.
    pub chr_banks: [u16; 8],
    /// VRC2's single bit of RAM at $6000, on the boards with no WRAM chip.
    pub latch: bool,
}

impl Vrc24 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    // PPU $0000..=$1FFF 8x 1K CHR Banks
    // CPU $6000..=$7FFF 8K PRG-RAM, or VRC2's one-bit latch
    // CPU $8000..=$9FFF 8K PRG-ROM Bank Switchable, or fixed to the second-to-last bank
    // CPU $A000..=$BFFF 8K PRG-ROM Bank Switchable
    // CPU $C000..=$DFFF 8K PRG-ROM Fixed to the second-to-last bank, or switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        // A submapper names the revision outright. Without one - which is every iNES header, so
        // most ROMs - decode the union of the candidate address lines for this mapper number
        // instead. That works because no game writes an address where two candidates disagree, and
        // it is what makes an unlabelled ROM run at all.
        let (revision, select) = match (cart.mapper_num(), cart.submapper_num()) {
            (22, _) => (Revision::Vrc2a, Revision::Vrc2a.select_lines()),
            (21, 1) => (Revision::Vrc4a, Revision::Vrc4a.select_lines()),
            (21, 2) => (Revision::Vrc4c, Revision::Vrc4c.select_lines()),
            (23, 1) => (Revision::Vrc4f, Revision::Vrc4f.select_lines()),
            (23, 2) => (Revision::Vrc4e, Revision::Vrc4e.select_lines()),
            (23, 3) => (Revision::Vrc2b, Revision::Vrc2b.select_lines()),
            (25, 1) => (Revision::Vrc4b, Revision::Vrc4b.select_lines()),
            (25, 2) => (Revision::Vrc4d, Revision::Vrc4d.select_lines()),
            (25, 3) => (Revision::Vrc2c, Revision::Vrc2c.select_lines()),
            // The VRC4 revisions, since VRC4 is VRC2 plus an IRQ counter and a swap mode: a VRC2
            // game never writes either, so guessing VRC4 costs nothing, while guessing VRC2 loses
            // the IRQ a VRC4 game needs.
            (23, _) => (Revision::Vrc4f, union(&[Revision::Vrc4f, Revision::Vrc4e])),
            (25, _) => (Revision::Vrc4b, union(&[Revision::Vrc4b, Revision::Vrc4d])),
            _ => (Revision::Vrc4a, union(&[Revision::Vrc4a, Revision::Vrc4c])),
        };
        let mut board = Self {
            revision,
            select,
            mirroring: cart.mirroring(),
            irq: VrcIrq::default(),
            prg_banks: [0; 2],
            prg_swap: false,
            wram_enabled: false,
            chr_banks: [0; 8],
            latch: false,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// Whether this board answers $6000 from its one-bit latch rather than from a WRAM chip.
    ///
    /// Only VRC2a, because only mapper 022 identifies a board with no WRAM on the number alone. A
    /// VRC2b or VRC2c cart may have either, and an iNES header cannot say which: mapping RAM when
    /// the board had a latch still round-trips the bit a game writes and reads back, while serving
    /// a latch when the board had RAM would lose 8K of it.
    const fn has_latch(&self) -> bool {
        matches!(self.revision, Revision::Vrc2a)
    }

    /// The 1K CHR bank a slot's register selects.
    fn chr_bank(&self, slot: usize) -> i32 {
        let bank = i32::from(self.chr_banks[slot]);
        // VRC2a's CHR lines sit one place up, so the register's low bit reaches nothing.
        if self.revision == Revision::Vrc2a {
            bank >> 1
        } else {
            bank
        }
    }

    /// The register this CPU address selects, as `$x000` plus a two-bit index.
    fn register_addr(&self, addr: u16) -> u16 {
        let a0 = u16::from(addr & self.select[0] != 0);
        let a1 = u16::from(addr & self.select[1] != 0);
        (addr & 0xF000) | (a1 << 1) | a0
    }
}

/// The address lines every one of `revisions` decodes, for a ROM whose header does not say which
/// board it is.
fn union(revisions: &[Revision]) -> [u16; 2] {
    revisions
        .iter()
        .fold([0; 2], |[lo, hi], revision| match revision.select_lines() {
            [next_lo, next_hi] => [lo | next_lo, hi | next_hi],
        })
}

impl Map for Vrc24 {
    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            out.push((["PRG 0", "PRG 1"][slot], u32::from(*bank)));
        }
        for (slot, bank) in self.chr_banks.iter().enumerate() {
            out.push((
                [
                    "CHR 0", "CHR 1", "CHR 2", "CHR 3", "CHR 4", "CHR 5", "CHR 6", "CHR 7",
                ][slot],
                u32::from(*bank),
            ));
        }
        if self.revision.is_vrc2() {
            out.push(("Latch", u32::from(self.latch)));
        } else {
            out.push(("PRG swap", u32::from(self.prg_swap)));
            out.push(("WRAM enabled", u32::from(self.wram_enabled)));
            out.push(("IRQ reload", u32::from(self.irq.reload)));
            out.push(("IRQ counter", u32::from(self.irq.counter)));
            out.push(("IRQ enabled", u32::from(self.irq.enabled)));
            out.push(("IRQ pending", u32::from(self.irq.irq_pending)));
            out.push(("IRQ cycle mode", u32::from(self.irq.cycle_mode)));
        }
    }

    fn mapper_ops(&self) -> MapperOps {
        if self.revision.is_vrc2() {
            if self.has_latch() {
                MapperOps::SERVES_PRG_READS
            } else {
                MapperOps::empty()
            }
        } else {
            MapperOps::CLOCKED | MapperOps::IRQ
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.irq.irq_pending
    }

    /// VRC2's latch is one bit of RAM, not memory, so no page entry can describe it.
    fn prg_peek(&self, addr: u16) -> Option<u8> {
        // The rest of the bus floats on a read; report it as clear.
        (self.has_latch() && matches!(addr, 0x6000..=0x6FFF)).then_some(u8::from(self.latch))
    }

    fn prg_read(&mut self, addr: u16) -> Option<u8> {
        self.prg_peek(addr)
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            if self.has_latch() && matches!(addr, 0x6000..=0x6FFF) {
                self.latch = val & 0x01 == 0x01;
            }
            return;
        }
        let is_vrc2 = self.revision.is_vrc2();
        match self.register_addr(addr) {
            0x8000..=0x8003 => self.prg_banks[0] = val & 0x1F,
            // VRC2 answers the whole block with its one mirroring register
            // (`docs/mapper/022.txt:71`), and has a single bit: it has no PRG modes and no
            // internal WRAM control, so there is nothing for the upper half to mean.
            0x9000..=0x9003 if is_vrc2 => {
                self.mirroring = if val & 0x01 == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
            0x9000 | 0x9001 => {
                self.mirroring = match val & 0x03 {
                    0 => Mirroring::Vertical,
                    1 => Mirroring::Horizontal,
                    2 => Mirroring::SingleScreenA,
                    _ => Mirroring::SingleScreenB,
                };
            }
            0x9002 | 0x9003 => {
                self.prg_swap = val & 0x02 == 0x02;
                self.wram_enabled = val & 0x01 == 0x01;
            }
            0xA000..=0xA003 => self.prg_banks[1] = val & 0x1F,
            // Two registers per 1K block of the address space, each split into a low nibble at the
            // even address and a high half at the odd one.
            reg @ 0xB000..=0xE003 => {
                let slot = 2 * ((reg >> 12) as usize - 0xB) + ((reg >> 1) & 0x01) as usize;
                let bank = &mut self.chr_banks[slot];
                if reg & 0x01 == 0 {
                    *bank = (*bank & 0x1F0) | u16::from(val & 0x0F);
                } else {
                    // VRC2's CHR select is 8 bits, VRC4's 9.
                    let mask = if is_vrc2 { 0x0F } else { 0x1F };
                    *bank = (*bank & 0x00F) | (u16::from(val & mask) << 4);
                }
            }
            0xF000..=0xF003 if is_vrc2 => return,
            0xF000 => self.irq.write_reload_lo(val),
            0xF001 => self.irq.write_reload_hi(val),
            0xF002 => self.irq.write_control(val),
            0xF003 => self.irq.acknowledge(),
            _ => return,
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        for slot in 0..8 {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, self.chr_bank(slot), Src::Chr);
        }

        // -1 is the last bank, -2 the one before it.
        let prg = i32::from(self.prg_banks[0]);
        if self.prg_swap {
            memory.map_prg(0x8000, Self::PRG_WINDOW, -2, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, prg, Src::PrgRom);
        } else {
            memory.map_prg(0x8000, Self::PRG_WINDOW, prg, Src::PrgRom);
            memory.map_prg(0xC000, Self::PRG_WINDOW, -2, Src::PrgRom);
        }
        memory.map_prg(
            0xA000,
            Self::PRG_WINDOW,
            i32::from(self.prg_banks[1]),
            Src::PrgRom,
        );
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);

        // WRAM stays mapped regardless of `wram_enabled`. What bit 0 of $9002 does is not settled -
        // it is a WRAM enable on some boards and unused on others - and a game that relies on WRAM
        // without ever writing that bit loses its save data if this gates on it.
        if !self.has_latch() {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        }

        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        self.irq.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        self.irq.reset(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (128 1K pages), 8K PRG-RAM, 64K CHR-ROM (64 1K pages).
    fn load(mapper_num: u16, submapper_num: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        cart.header.mapper_num = mapper_num;
        cart.header.submapper_num = submapper_num;
        let mapper = Vrc24::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// VRC4a, the most common of them, and the one an unlabelled mapper 021 ROM gets.
    fn vrc4a() -> (Mapper, Cart) {
        load(21, 1)
    }

    fn board(mapper: &Mapper) -> &Vrc24 {
        match mapper {
            Mapper::Vrc24(board) => board,
            _ => unreachable!("mapper is a Vrc24"),
        }
    }

    /// The register index a revision reads out of an address, for the address-line tests.
    fn reg(revision: Revision, addr: u16) -> u16 {
        let board = Vrc24 {
            revision,
            select: revision.select_lines(),
            ..Vrc24::default()
        };
        board.register_addr(addr)
    }

    /// All four mapper numbers have to reach this board through `Mapper::from_cart` - the path a ROM
    /// actually loads through - and land on the revision the submapper names.
    #[test]
    fn every_mapper_number_loads_this_board() {
        let cases = [
            (21, 0, Revision::Vrc4a),
            (21, 1, Revision::Vrc4a),
            (21, 2, Revision::Vrc4c),
            (22, 0, Revision::Vrc2a),
            (23, 0, Revision::Vrc4f),
            (23, 1, Revision::Vrc4f),
            (23, 2, Revision::Vrc4e),
            (23, 3, Revision::Vrc2b),
            (25, 0, Revision::Vrc4b),
            (25, 1, Revision::Vrc4b),
            (25, 2, Revision::Vrc4d),
            (25, 3, Revision::Vrc2c),
        ];
        for (mapper_num, submapper_num, expected) in cases {
            let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
            cart.header.mapper_num = mapper_num;
            cart.header.submapper_num = submapper_num;
            let mapper = Mapper::from_cart(&mut cart).expect("loads");
            assert_eq!(
                board(&mapper).revision,
                expected,
                "mapper {mapper_num} submapper {submapper_num}"
            );
        }
    }

    /// $E000 is hard-wired to the last 8K, which is where the reset vector lives, and $C000 to the
    /// one before it - if either is wrong nothing boots at all.
    #[test]
    fn powers_on_with_the_last_two_prg_banks_fixed() {
        let (mapper, cart) = vrc4a();
        // 128K in 1K pages is 128; the last 8K window starts at page 120, the one before at 112.
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 112);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120);
        assert_eq!(prg_peek(&mapper, &cart, 0xFFFF), 127);
    }

    /// Both switchable windows are 8K, and $A000's register is a different one from $8000's.
    #[test]
    fn prg_selects_two_8k_windows() {
        let (mut mapper, mut cart) = vrc4a();
        write(&mut mapper, &mut cart, 0x8000, 3);
        write(&mut mapper, &mut cart, 0xA000, 5);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 112, "still fixed");
    }

    /// Bit 1 of $9002 swaps the switchable bank at $8000 with the fixed one at $C000. $A000 and
    /// $E000 do not move.
    #[test]
    fn prg_swap_mode_moves_the_switchable_bank_to_c000() {
        let (mut mapper, mut cart) = vrc4a();
        write(&mut mapper, &mut cart, 0x8000, 3);
        write(&mut mapper, &mut cart, 0xA000, 5);

        // The swap register is index 2, which on a VRC4a is A2 - $9004, not $9002.
        write(&mut mapper, &mut cart, 0x9004, 0x02);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 112, "now fixed");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 3 * 8, "now switchable");
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120);

        write(&mut mapper, &mut cart, 0x9004, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8, "and back");
    }

    /// A VRC2 has no swap mode, so a write that would set it on a VRC4 must leave the banking alone.
    #[test]
    fn vrc2_has_no_prg_swap_mode() {
        let (mut mapper, mut cart) = load(22, 0);
        write(&mut mapper, &mut cart, 0x8000, 3);
        // $9002 on a VRC2a is $9001 with the low lines swapped, so write the swapped address too.
        write(&mut mapper, &mut cart, 0x9001, 0x02);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8, "still switchable");
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 112, "still fixed");
    }

    /// Each CHR register is nine bits written as two halves: the low nibble at the even address and
    /// the high five bits at the odd one. Getting the halves backwards is silently almost right,
    /// since most games write both.
    #[test]
    fn chr_registers_are_written_as_two_nibbles() {
        let (mut mapper, mut cart) = vrc4a();
        // Bank 0x23 into slot 0: low nibble 3 at $B000, high bits 2 at $B002 (VRC4a's A0 is A1).
        write(&mut mapper, &mut cart, 0xB000, 0x03);
        write(&mut mapper, &mut cart, 0xB002, 0x02);
        assert_eq!(board(&mapper).chr_banks[0], 0x23);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 0x23);

        // The low nibble alone must not disturb the high half.
        write(&mut mapper, &mut cart, 0xB000, 0x0F);
        assert_eq!(board(&mapper).chr_banks[0], 0x2F);
    }

    /// Eight 1K slots, two registers per $1000 block of the register space.
    #[test]
    fn chr_maps_eight_independent_1k_banks() {
        let (mut mapper, mut cart) = vrc4a();
        // On a VRC4a the select bits are A1 and A2, so slot 2n is at $x000 and slot 2n+1 at $x004.
        for (slot, block) in [0xB000, 0xC000, 0xD000, 0xE000].into_iter().enumerate() {
            write(&mut mapper, &mut cart, block, 1 + 2 * slot as u8);
            write(&mut mapper, &mut cart, block + 4, 2 + 2 * slot as u8);
        }
        for (slot, bank) in (1..=8).enumerate() {
            let addr = (slot * 1024) as u16;
            assert_eq!(chr_peek(&mapper, &cart, addr), 0x80 | bank, "slot {slot}");
        }
    }

    /// VRC2a's CHR lines are wired one place up, so a register selects `value >> 1` - the only
    /// banking difference between any two of these boards.
    #[test]
    fn vrc2a_ignores_the_low_chr_register_bit() {
        let (mut mapper, mut cart) = load(22, 0);
        // Bank 0x0B: low nibble at $B000, high nibble at $B002 on a VRC2a (A0 is A1).
        write(&mut mapper, &mut cart, 0xB000, 0x0B);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 0x05, "0x0B >> 1");

        let (mut mapper, mut cart) = load(23, 1);
        write(&mut mapper, &mut cart, 0xB000, 0x0B);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x80 | 0x0B,
            "every other board uses the value as-is"
        );
    }

    /// The revisions differ only in which address lines carry the register index, and that is what
    /// makes one board's bank write another's IRQ write. Each pair below is the same register.
    #[test]
    fn each_revision_decodes_its_own_address_lines() {
        // $x001 and $x002, i.e. index 1 and 2, spelled per revision.
        let cases = [
            (Revision::Vrc2a, 0x8002, 0x8001),
            (Revision::Vrc2b, 0x8001, 0x8002),
            (Revision::Vrc2c, 0x8002, 0x8001),
            (Revision::Vrc4a, 0x8002, 0x8004),
            (Revision::Vrc4b, 0x8002, 0x8001),
            (Revision::Vrc4c, 0x8040, 0x8080),
            (Revision::Vrc4d, 0x8008, 0x8004),
            (Revision::Vrc4e, 0x8004, 0x8008),
            (Revision::Vrc4f, 0x8001, 0x8002),
        ];
        for (revision, index_1, index_2) in cases {
            assert_eq!(reg(revision, index_1), 0x8001, "{revision:?} index 1");
            assert_eq!(reg(revision, index_2), 0x8002, "{revision:?} index 2");
            assert_eq!(reg(revision, 0x8000), 0x8000, "{revision:?} index 0");
            assert_eq!(
                reg(revision, index_1 | index_2),
                0x8003,
                "{revision:?} index 3"
            );
        }
    }

    /// Almost every ROM has an iNES header and so names no submapper. Decoding the union of the
    /// candidate lines is what lets those run: each candidate's addresses still reach the register
    /// it meant, because no game writes an address where two of them disagree.
    #[test]
    fn an_unlabelled_rom_decodes_every_candidate_revisions_lines() {
        let cases = [
            (21, [Revision::Vrc4a, Revision::Vrc4c].as_slice()),
            (23, &[Revision::Vrc4f, Revision::Vrc4e, Revision::Vrc2b]),
            (25, &[Revision::Vrc4b, Revision::Vrc4d, Revision::Vrc2c]),
        ];
        for (mapper_num, candidates) in cases {
            let (mapper, _cart) = load(mapper_num, 0);
            let board = board(&mapper);
            for &candidate in candidates {
                let [lo, hi] = candidate.select_lines();
                assert_eq!(
                    board.register_addr(0x8000 | lo),
                    0x8001,
                    "mapper {mapper_num} must decode {candidate:?}'s index 1"
                );
                assert_eq!(
                    board.register_addr(0x8000 | hi),
                    0x8002,
                    "mapper {mapper_num} must decode {candidate:?}'s index 2"
                );
            }
        }
    }

    /// VRC4 has two mirroring bits and all four modes; VRC2 has one bit and the first two.
    #[test]
    fn mirroring_is_two_bits_on_vrc4_and_one_on_vrc2() {
        let (mut mapper, mut cart) = vrc4a();
        for (val, expected) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleScreenA),
            (3, Mirroring::SingleScreenB),
        ] {
            write(&mut mapper, &mut cart, 0x9000, val);
            assert_eq!(mapper.mirroring(), expected, "VRC4 ${val:02X}");
        }

        let (mut mapper, mut cart) = load(22, 0);
        for (val, expected) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            // Bit 1 is not connected, so this is horizontal again rather than single-screen.
            (3, Mirroring::Horizontal),
        ] {
            write(&mut mapper, &mut cart, 0x9000, val);
            assert_eq!(mapper.mirroring(), expected, "VRC2 ${val:02X}");
        }
    }

    /// A VRC2 has no PRG mode and no internal WRAM control, so the upper half of the $9000 block
    /// is not a second register - the one mirroring register answers all four indices
    /// (`docs/mapper/022.txt:71`). No VRC2 cart writes anything but index 0, so this is a decode
    /// that hardware would exercise and software never has.
    #[test]
    fn a_vrc2_answers_the_whole_9000_block_with_mirroring() {
        // VRC2b decodes A0 and A1 in order, so the four indices are $9000-$9003.
        let (mut mapper, mut cart) = load(23, 3);
        for addr in [0x9000, 0x9001, 0x9002, 0x9003] {
            write(&mut mapper, &mut cart, addr, 1);
            assert_eq!(
                mapper.mirroring(),
                Mirroring::Horizontal,
                "${addr:04X} selects mirroring"
            );
            write(&mut mapper, &mut cart, addr, 0);
            assert_eq!(
                mapper.mirroring(),
                Mirroring::Vertical,
                "${addr:04X} selects mirroring"
            );
        }
    }

    /// The same block on a VRC4 is split: mirroring low, PRG mode and WRAM enable high.
    #[test]
    fn a_vrc4_splits_the_9000_block() {
        let (mut mapper, mut cart) = vrc4a();
        // VRC4a decodes A1 and A2, so index 2 is $9004.
        write(&mut mapper, &mut cart, 0x9000, 2);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);

        write(&mut mapper, &mut cart, 0x9004, 0x03);
        assert_eq!(
            mapper.mirroring(),
            Mirroring::SingleScreenA,
            "the control register leaves mirroring alone"
        );
    }

    /// The reload value arrives as two nibbles, and the counter is the shared VRC one: an
    /// up-counter that fires on the wrap out of $FF, so $FF fires on the next clock.
    #[test]
    fn vrc4_irq_reload_is_written_as_two_nibbles() {
        let (mut mapper, mut cart) = vrc4a();
        // Indices 0-3, which on a VRC4a are A1 and A2: $F000, $F002, $F004, $F006.
        write(&mut mapper, &mut cart, 0xF000, 0x0F); // low nibble
        write(&mut mapper, &mut cart, 0xF002, 0x0F); // high nibble
        assert_eq!(board(&mapper).irq.reload, 0xFF);

        write(&mut mapper, &mut cart, 0xF004, 0x06); // enable, cycle mode
        assert!(!mapper.irq_pending());
        mapper.clock();
        assert!(mapper.irq_pending(), "$FF fires on the next clock");

        write(&mut mapper, &mut cart, 0xF006, 0x00); // acknowledge
        assert!(!mapper.irq_pending());
    }

    /// A VRC2 has no counter at all, so the same writes must leave the IRQ line alone rather than
    /// firing an interrupt no game is waiting for.
    #[test]
    fn vrc2_has_no_irq_counter() {
        let (mut mapper, mut cart) = load(22, 0);
        assert!(
            !mapper.mapper_ops().contains(MapperOps::IRQ),
            "a VRC2 must not claim an IRQ"
        );
        for addr in [0xF000, 0xF001, 0xF002, 0xF003] {
            write(&mut mapper, &mut cart, addr, 0xFF);
        }
        for _ in 0..1000 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending());
    }

    /// A VRC2a board has one bit of RAM at $6000 instead of a WRAM chip, which is not memory and so
    /// cannot be a page entry.
    #[test]
    fn vrc2a_serves_its_one_bit_latch_at_6000() {
        let (mut mapper, mut cart) = load(22, 0);
        assert!(
            mapper.mapper_ops().contains(MapperOps::SERVES_PRG_READS),
            "the latch has to be served by the board"
        );
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "clear at power-on");

        write(&mut mapper, &mut cart, 0x6000, 0x01);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 1);
        write(&mut mapper, &mut cart, 0x6000, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0);
    }

    /// Every board that has a WRAM chip keeps it mapped, whatever bit 0 of $9002 says.
    #[test]
    fn wram_is_mapped_at_6000() {
        let (mut mapper, mut cart) = vrc4a();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A);
        write(&mut mapper, &mut cart, 0x9002, 0x00);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x6000),
            0x5A,
            "WRAM does not go away when the enable bit is clear"
        );
    }

    /// `update_banks` has to rebuild every window from the register state alone - that is what
    /// `Bus::rebuild_mapper_state` calls after loading a save state, which carries no page tables.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = vrc4a();
        write(&mut mapper, &mut cart, 0x8000, 3);
        write(&mut mapper, &mut cart, 0xA000, 5);
        write(&mut mapper, &mut cart, 0x9002, 0x02);
        write(&mut mapper, &mut cart, 0x9000, 0x01);
        write(&mut mapper, &mut cart, 0xB000, 0x09);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain(
                    [0x0000, 0x0400, 0x2000]
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
    }
}
