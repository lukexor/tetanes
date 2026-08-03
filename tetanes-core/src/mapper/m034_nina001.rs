//! `NINA-001 (Mapper 034)`.
//!
//! <https://wiki.nesdev.org/w/index.php/INES_Mapper_034>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `NINA-001` (Mapper 034).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Nina001 {
    pub mirroring: Mirroring,
    pub prg_bank: u8,
    pub chr_banks: [u8; 2],
}

impl Nina001 {
    const PRG_WINDOW: usize = 32 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 4 * 1024;

    // PPU $0000..=$0FFF 4K CHR-ROM Bank Switchable
    // PPU $1000..=$1FFF 4K CHR-ROM Bank Switchable
    // CPU $6000..=$7FFF 8K PRG-RAM, with registers at $7FFD..=$7FFF
    // CPU $8000..=$FFFF 32K PRG-ROM Bank Switchable
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: Mirroring::Horizontal,
            prg_bank: 0,
            chr_banks: [0, 1],
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for Nina001 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        // The register writes also land in PRG-RAM, which the caller has already stored.
        match addr {
            0x7FFD => {
                self.prg_bank = val & 0x01;
                memory.map_prg(
                    0x8000,
                    Self::PRG_WINDOW,
                    i32::from(self.prg_bank),
                    Src::PrgRom,
                );
            }
            0x7FFE => {
                self.chr_banks[0] = val & 0x0F;
                memory.map_chr(
                    0x0000,
                    Self::CHR_WINDOW,
                    i32::from(self.chr_banks[0]),
                    Src::Chr,
                );
            }
            0x7FFF => {
                self.chr_banks[1] = val & 0x0F;
                memory.map_chr(
                    0x1000,
                    Self::CHR_WINDOW,
                    i32::from(self.chr_banks[1]),
                    Src::Chr,
                );
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.prg_bank),
            Src::PrgRom,
        );
        memory.map_chr(
            0x0000,
            Self::CHR_WINDOW,
            i32::from(self.chr_banks[0]),
            Src::Chr,
        );
        memory.map_chr(
            0x1000,
            Self::CHR_WINDOW,
            i32::from(self.chr_banks[1]),
            Src::Chr,
        );
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM, 8K PRG-RAM, 64K CHR-ROM (16 4K banks). A real NINA-001 holds 64K, i.e. the
    /// two 32K banks its one PRG bit can address; the extra pair here is what makes that width
    /// visible, since with only two banks a masked register and a wrapped one agree.
    fn nina001() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        let mapper = Nina001::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The two CHR halves come up as consecutive banks rather than both at 0, so a cart that never
    /// writes a register still shows the right tiles.
    #[test]
    fn powers_on_with_consecutive_chr_halves() {
        let (mapper, cart) = nina001();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM");
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "one 32K bank");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80, "CHR bank 0");
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x84, "CHR bank 1");
    }

    /// The three registers sit at the very top of PRG-RAM, one per bank.
    #[test]
    fn the_top_three_prg_ram_bytes_are_the_bank_registers() {
        let (mut mapper, mut cart) = nina001();

        write(&mut mapper, &mut cart, 0x7FFD, 1);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 32, "PRG bank 1");

        write(&mut mapper, &mut cart, 0x7FFE, 5);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (5 * 4));
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x84, "high half unmoved");

        write(&mut mapper, &mut cart, 0x7FFF, 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | (9 * 4));
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (5 * 4), "unmoved");
    }

    /// PRG is one bit wide (the board holds 64K) and CHR four.
    #[test]
    fn the_registers_are_masked_to_their_bank_widths() {
        let (mut mapper, mut cart) = nina001();
        write(&mut mapper, &mut cart, 0x7FFD, 0xFE);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0, "PRG is one bit");
        write(&mut mapper, &mut cart, 0x7FFD, 0x03);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 32, "bank 1, not bank 3");
        write(&mut mapper, &mut cart, 0x7FFE, 0xF3);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (3 * 4), "CHR is 4");
    }

    /// The registers are decoded on top of PRG-RAM rather than instead of it, so a game that saves
    /// through $6000-$7FFF still reads back what it wrote to those three bytes.
    #[test]
    fn a_register_write_also_lands_in_prg_ram() {
        let (mut mapper, mut cart) = nina001();
        write(&mut mapper, &mut cart, 0x7FFD, 1);
        assert_eq!(prg_peek(&mapper, &cart, 0x7FFD), 1);
        assert_eq!(prg_peek(&mapper, &cart, 0x7FFC), 0x5A, "not a register");
    }

    /// Only those three addresses are registers; the rest of PRG-RAM is ordinary storage.
    #[test]
    fn writes_elsewhere_are_ignored() {
        let (mut mapper, mut cart) = nina001();
        write(&mut mapper, &mut cart, 0x7FFD, 1);
        for addr in [0x4100, 0x6000, 0x7FFC, 0x8000, 0xFFFF] {
            write(&mut mapper, &mut cart, addr, 0);
            assert_eq!(prg_peek(&mapper, &cart, 0x8000), 32, "${addr:04X}");
        }
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80);
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on after a
    /// save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = nina001();
        write(&mut mapper, &mut cart, 0x7FFD, 1);
        write(&mut mapper, &mut cart, 0x7FFE, 5);
        write(&mut mapper, &mut cart, 0x7FFF, 9);

        let sample = |mapper: &Mapper, cart: &Cart| {
            [
                prg_peek(mapper, cart, 0x6000),
                prg_peek(mapper, cart, 0x8000),
                prg_peek(mapper, cart, 0xC000),
                chr_peek(mapper, cart, 0x0000),
                chr_peek(mapper, cart, 0x1000),
            ]
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
