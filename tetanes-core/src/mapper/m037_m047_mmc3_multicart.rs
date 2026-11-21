//! Nintendo's `MMC3` multicarts: `PAL-ZZ` (Mapper 037) and `NES-QJ` (Mapper 047).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_037>
//! <https://www.nesdev.org/wiki/INES_Mapper_047>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps, Mmc3, Mmc3Revision},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// Which multicart board an [`Mmc3Multicart`] is.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Revision {
    /// Mapper 037 (Super Mario Bros. + Tetris + Nintendo World Cup): outer blocks picked by a
    /// 3-bit register.
    PalZz,
    /// Mapper 047 (Super Spike V'Ball + Nintendo World Cup): two 128K blocks picked by a 1-bit
    /// register.
    Qj,
}

/// Nintendo's `MMC3` multicarts: `PAL-ZZ` (Mapper 037) and `NES-QJ` (Mapper 047).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Mmc3Multicart {
    /// The `MMC3` register file and scanline counter.
    pub mmc3: Mmc3,
    /// Which board this is.
    pub revision: Revision,
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// `$A001`: the PRG-RAM enable and write-protect bits, which gate the outer bank register.
    pub prg_ram_protect: u8,
    /// `$6000-$7FFF`: the outer bank register that picks which game's block is visible.
    pub block: u8,
}

impl Mmc3Multicart {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const PRG_MODE_MASK: u8 = 0x40;
    const CHR_INVERSION_MASK: u8 = 0x80;

    // PPU $0000..=$1FFF 2x 2K and 4x 1K CHR-ROM banks within the selected block
    // CPU $6000..=$7FFF outer bank register, with no PRG-RAM
    // CPU $8000..=$FFFF 4x 8K PRG-ROM banks within the selected block, two of them fixed
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart, revision: Revision) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mmc3: Mmc3::default(),
            revision,
            mirroring: cart.mirroring(),
            prg_ram_protect: 0,
            block: 0,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// Set the `MMC3` chip revision, which decides when the scanline IRQ fires.
    pub const fn set_revision(&mut self, rev: Mmc3Revision) {
        self.mmc3.set_revision(rev);
    }

    /// The physical 8K PRG bank for an `MMC3` PRG bank number within the current block.
    const fn prg_bank(&self, bank: u8) -> i32 {
        let bank = match self.revision {
            Revision::PalZz => match self.block {
                0..=2 => bank & 0x07,
                3 => (bank & 0x07) | 0x08,
                // Block bit 2 is PRG A17, and A16 is high when bits 0 and 1 both are, which puts
                // block 7 at $30000.
                7 => (bank & 0x07) | 0x18,
                _ => (bank & 0x0F) | 0x10,
            },
            Revision::Qj => (bank & 0x0F) | ((self.block & 0x01) << 4),
        };
        bank as i32
    }

    /// The physical 1K CHR bank for an `MMC3` CHR bank number within the current block.
    const fn chr_bank(&self, bank: u8) -> i32 {
        let high = match self.revision {
            Revision::PalZz => (self.block & 0x04) << 5,
            Revision::Qj => (self.block & 0x01) << 7,
        };
        ((bank & 0x7F) | high) as i32
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        let regs = self.mmc3.bank_values;
        // $FE and $FF are the block's second-last and last banks once masked.
        let (bank_8000, bank_c000) =
            if self.mmc3.bank_select & Self::PRG_MODE_MASK == Self::PRG_MODE_MASK {
                (0xFE, regs[6])
            } else {
                (regs[6], 0xFE)
            };
        for (addr, bank) in [
            (0x8000, bank_8000),
            (0xA000, regs[7]),
            (0xC000, bank_c000),
            (0xE000, 0xFF),
        ] {
            memory.map_prg(addr, Self::PRG_WINDOW, self.prg_bank(bank), Src::PrgRom);
        }
    }

    fn update_chr_banks(&self, memory: &mut Memory) {
        let chr = self.mmc3.bank_values;
        // R0 and R1 are 2K windows that ignore their low bit, laid out as two 1K pages each.
        let pairs = [chr[0] & 0xFE, chr[0] | 0x01, chr[1] & 0xFE, chr[1] | 0x01];
        let singles = [chr[2], chr[3], chr[4], chr[5]];
        let (low, high) = if self.mmc3.bank_select & Self::CHR_INVERSION_MASK == 0 {
            (pairs, singles)
        } else {
            (singles, pairs)
        };
        for (slot, bank) in low.into_iter().chain(high).enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, self.chr_bank(bank), Src::Chr);
        }
    }
}

impl Map for Mmc3Multicart {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ | MapperOps::WATCHES_PPU_BUS
    }

    /// `MMC3` counts scanlines from A12 rising edges on the PPU bus.
    fn ppu_bus_addr(&mut self, _memory: &mut Memory, addr: u16) {
        self.mmc3.clock_irq(addr);
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("Block", u32::from(self.block)));
        out.push(("Bank select", u32::from(self.mmc3.bank_select)));
        for (name, bank) in ["R0", "R1", "R2", "R3", "R4", "R5", "R6", "R7"]
            .into_iter()
            .zip(self.mmc3.bank_values)
        {
            out.push((name, u32::from(bank)));
        }
        out.push(("IRQ latch", u32::from(self.mmc3.irq_latch)));
        out.push(("IRQ counter", u32::from(self.mmc3.irq_counter)));
        out.push(("IRQ enabled", u32::from(self.mmc3.irq_enabled)));
        out.push(("IRQ pending", u32::from(self.mmc3.irq_pending)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // The register answers only while $A001 enables PRG-RAM and leaves it writable.
                if self.prg_ram_protect & 0xC0 == 0x80 {
                    self.block = match self.revision {
                        Revision::PalZz => val & 0x07,
                        Revision::Qj => val & 0x01,
                    };
                    self.update_banks(memory);
                }
            }
            0x8000..=0xFFFF => match addr & 0xE001 {
                0x8000 => {
                    self.mmc3.write_bank_select(val);
                    self.update_banks(memory);
                }
                0x8001 => {
                    self.mmc3.write_bank_data(val);
                    self.update_banks(memory);
                }
                0xA000 => {
                    self.mirroring = if val & 0x01 == 0x01 {
                        Mirroring::Horizontal
                    } else {
                        Mirroring::Vertical
                    };
                    memory.set_mirroring(self.mirroring);
                }
                0xA001 => self.prg_ram_protect = val,
                0xC000 => self.mmc3.write_irq_latch(val),
                0xC001 => self.mmc3.write_irq_reload(),
                0xE000 => self.mmc3.write_irq_disable(),
                _ => self.mmc3.write_irq_enable(),
            },
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        self.update_prg_banks(memory);
        self.update_chr_banks(memory);
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        self.mmc3.clock();
    }

    fn reset(&mut self, kind: ResetKind) {
        // The caller runs `update_banks` after reset.
        // The console's reset line clears the outer register, and the MMC3 keeps $A001.
        self.mmc3.reset(kind);
        self.block = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// A byte no page-indexed fill produces, so a read of it names exactly one bank.
    const MARK: u8 = 0x5C;

    /// 256K of PRG-ROM and 256K of CHR-ROM, with `MARK` planted at the start of every 8K PRG
    /// bank and 1K CHR bank listed. Page indices pass 127 at this size, so the tests read marks
    /// instead of page numbers.
    fn load(revision: Revision, prg_banks: &[usize], chr_banks: &[usize]) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(256 * 1024, 0, 256 * 1024);
        for bank in prg_banks {
            cart.memory.region_mut(Src::PrgRom)[bank * 8 * 1024] = MARK;
        }
        for bank in chr_banks {
            cart.memory.region_mut(Src::Chr)[bank * 1024] = MARK;
        }
        let mapper = Mmc3Multicart::load(&mut cart, revision).expect("valid mapper");
        (mapper, cart)
    }

    /// Enables PRG-RAM through `$A001` so the outer register takes writes, then selects `block`.
    fn select_block(mapper: &mut Mapper, cart: &mut Cart, block: u8) {
        write(mapper, cart, 0xA001, 0x80);
        write(mapper, cart, 0x6000, block);
    }

    /// The board's outer block register.
    fn block(mapper: &Mapper) -> u8 {
        let Mapper::Mmc3Multicart(board) = mapper else {
            panic!("expected an MMC3 multicart");
        };
        board.block
    }

    /// The outer register ignores writes until `$A001` enables PRG-RAM without write protection.
    #[test]
    fn outer_register_needs_prg_ram_enabled() {
        let (mut mapper, mut cart) = load(Revision::Qj, &[], &[]);
        write(&mut mapper, &mut cart, 0x6000, 1);
        assert_eq!(block(&mapper), 0, "PRG-RAM disabled");
        write(&mut mapper, &mut cart, 0xA001, 0xC0);
        write(&mut mapper, &mut cart, 0x6000, 1);
        assert_eq!(block(&mapper), 0, "write-protected");
        select_block(&mut mapper, &mut cart, 1);
        assert_eq!(block(&mapper), 1, "enabled and writable");
    }

    /// Mapper 047's block bit is PRG A17 and CHR A17, so each game sees its own fixed last bank
    /// and its own CHR.
    #[test]
    fn qj_block_selects_the_upper_128k() {
        let (mut mapper, mut cart) = load(Revision::Qj, &[31], &[133]);
        assert_ne!(
            prg_peek(&mapper, &cart, 0xE000),
            MARK,
            "block 0 is not bank 31"
        );
        select_block(&mut mapper, &mut cart, 1);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xE000),
            MARK,
            "block 1 last bank is 31"
        );
        // R2 is the 1K window at $1000 in the normal CHR mode.
        write(&mut mapper, &mut cart, 0x8000, 0x02);
        write(&mut mapper, &mut cart, 0x8001, 0x05);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), MARK, "CHR 128 + 5");
    }

    /// Mapper 037's blocks 0-2 share the first 64K of PRG, block 3 is the next 64K, blocks 4-6
    /// are the second 128K, and block 7 is its upper 64K. Blocks 4-7 also take the second half
    /// of CHR.
    #[test]
    fn pal_zz_blocks() {
        let (mut mapper, mut cart) = load(Revision::PalZz, &[7], &[5]);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xE000),
            MARK,
            "block 0 last bank is 7"
        );
        write(&mut mapper, &mut cart, 0x8000, 0x02);
        write(&mut mapper, &mut cart, 0x8001, 0x05);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), MARK, "block 0 CHR 5");

        let (mut mapper, mut cart) = load(Revision::PalZz, &[15], &[]);
        select_block(&mut mapper, &mut cart, 3);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xE000),
            MARK,
            "block 3 last bank is 15"
        );

        let (mut mapper, mut cart) = load(Revision::PalZz, &[31], &[133]);
        select_block(&mut mapper, &mut cart, 4);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xE000),
            MARK,
            "block 4 last bank is 31"
        );
        write(&mut mapper, &mut cart, 0x8000, 0x02);
        write(&mut mapper, &mut cart, 0x8001, 0x05);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x1000),
            MARK,
            "block 4 CHR 128 + 5"
        );

        // Block 7's first bank is 24, at $30000, and its last is 31.
        let (mut mapper, mut cart) = load(Revision::PalZz, &[24, 31], &[]);
        select_block(&mut mapper, &mut cart, 7);
        write(&mut mapper, &mut cart, 0x8000, 0x06);
        write(&mut mapper, &mut cart, 0x8001, 0x00);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            MARK,
            "block 7 bank 0 is 24"
        );
        assert_eq!(
            prg_peek(&mapper, &cart, 0xE000),
            MARK,
            "block 7 last bank is 31"
        );
    }

    /// The `$FE`/`$FF` fixed banks land on the block's own second-last and last banks, in both
    /// PRG modes.
    #[test]
    fn fixed_banks_follow_the_block() {
        let (mut mapper, mut cart) = load(Revision::Qj, &[30, 31], &[]);
        select_block(&mut mapper, &mut cart, 1);
        assert_eq!(
            prg_peek(&mapper, &cart, 0xC000),
            MARK,
            "mode 0: $C000 is bank 30"
        );
        write(&mut mapper, &mut cart, 0x8000, 0x40);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8000),
            MARK,
            "mode 1: $8000 is bank 30"
        );
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), MARK, "$E000 is bank 31");
    }
}
