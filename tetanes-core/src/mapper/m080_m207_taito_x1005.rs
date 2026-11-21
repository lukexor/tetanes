//! `Taito X1-005` (Mappers 080, 207).
//!
//! <https://www.nesdev.org/wiki/Taito_X1-005>
//! <https://www.nesdev.org/wiki/INES_Mapper_207>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{ConstArray, Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Taito X1-005` (Mappers 080, 207).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct TaitoX1005 {
    /// The current nametable arrangement. Mapper 207 picks nametables from the CHR registers
    /// instead, so this stays at the header's value there.
    pub mirroring: Mirroring,
    /// Mapper 207 wires CHR A17 to CIRAM A10, so bit 7 of `$7EF0`/`$7EF1` selects nametables.
    pub mapper_num: u16,
    /// `$7EF0-$7EF5`: two 2K CHR banks, then four 1K banks, all in 1K units.
    pub chr: [u8; 6],
    /// `$7EFA`, `$7EFC`, `$7EFE`: 8K PRG banks at `$8000`, `$A000` and `$C000`.
    pub prg: [u8; 3],
    /// `$7EF8`: the internal RAM answers only while this is [`TaitoX1005::RAM_ENABLE`].
    pub ram_enable: u8,
    /// The 128 bytes of internal RAM at `$7F00`, mirrored at `$7F80`.
    pub ram: ConstArray<u8, { TaitoX1005::RAM_SIZE }>,
}

impl TaitoX1005 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    /// Bytes of internal RAM, which the battery covers.
    pub const RAM_SIZE: usize = 128;
    /// The `$7EF8` value that enables the internal RAM.
    pub const RAM_ENABLE: u8 = 0xA3;

    // PPU $0000..=$0FFF two 2K switchable CHR-ROM banks
    // PPU $1000..=$1FFF four 1K switchable CHR-ROM banks
    // CPU $7EF0..=$7EFF registers, also at $7E70..=$7E7F
    // CPU $7F00..=$7FFF 128 bytes of internal RAM, mirrored once
    // CPU $8000..=$DFFF three 8K switchable PRG-ROM banks
    // CPU $E000..=$FFFF 8K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            mapper_num: cart.mapper_num(),
            chr: [0; 6],
            prg: [0; 3],
            ram_enable: 0,
            ram: ConstArray::new(),
        };
        cart.memory.set_battery_ext_len(Self::RAM_SIZE);
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    const fn ram_enabled(&self) -> bool {
        self.ram_enable == Self::RAM_ENABLE
    }

    fn update_chr_banks(&self, memory: &mut Memory) {
        // Mapper 207 spends bit 7 of the 2K registers on nametables.
        let mask = if self.mapper_num == 207 { 0x7E } else { 0xFE };
        for (slot, reg) in self.chr[..2].iter().enumerate() {
            let addr = (slot * 2 * Self::CHR_WINDOW) as u16;
            let bank = i32::from(reg & mask);
            memory.map_chr(addr, Self::CHR_WINDOW, bank, Src::Chr);
            memory.map_chr(addr + 0x0400, Self::CHR_WINDOW, bank + 1, Src::Chr);
        }
        for (slot, reg) in self.chr[2..].iter().enumerate() {
            let addr = 0x1000 + (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*reg), Src::Chr);
        }
        if self.mapper_num == 207 {
            // $7EF0 selects CIRAM for the top two nametables, $7EF1 for the bottom two.
            for nametable in 0..4 {
                let addr = 0x2000 + (nametable * Self::CHR_WINDOW) as u16;
                let bank = i32::from(self.chr[nametable / 2] >> 7);
                memory.map_chr(addr, Self::CHR_WINDOW, bank, Src::CiRam);
                // $3000-$3EFF mirrors $2000-$2EFF.
                memory.map_chr(addr + 0x1000, Self::CHR_WINDOW, bank, Src::CiRam);
            }
        } else {
            memory.set_mirroring(self.mirroring);
        }
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        for (slot, reg) in self.prg.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*reg), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }
}

impl Map for TaitoX1005 {
    // The internal RAM shares a 1K page with the registers, so its reads are served here.
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::SERVES_PRG_READS
    }

    fn prg_read(&mut self, addr: u16) -> Option<u8> {
        self.prg_peek(addr)
    }

    fn prg_peek(&self, addr: u16) -> Option<u8> {
        ((0x7F00..=0x7FFF).contains(&addr) && self.ram_enabled())
            .then(|| self.ram[usize::from(addr) % Self::RAM_SIZE])
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        let chr = [
            "CHR $0000",
            "CHR $0800",
            "CHR $1000",
            "CHR $1400",
            "CHR $1800",
            "CHR $1C00",
        ];
        for (name, reg) in chr.into_iter().zip(self.chr) {
            out.push((name, u32::from(reg)));
        }
        for (name, reg) in ["PRG $8000", "PRG $A000", "PRG $C000"]
            .into_iter()
            .zip(self.prg)
        {
            out.push((name, u32::from(reg)));
        }
        out.push(("RAM enable", u32::from(self.ram_enable)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if (0x7F00..=0x7FFF).contains(&addr) {
            if self.ram_enabled() {
                self.ram[usize::from(addr) % Self::RAM_SIZE] = val;
            }
            return;
        }
        // The board ignores A7, so the registers also appear at $7E70-$7E7F.
        if addr & 0xFF70 != 0x7E70 {
            return;
        }
        match addr & 0x0F {
            reg @ 0x00..=0x05 => {
                self.chr[usize::from(reg)] = val;
                self.update_chr_banks(memory);
            }
            0x06 if self.mapper_num != 207 => {
                self.mirroring = if val & 0x01 == 0x01 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
                memory.set_mirroring(self.mirroring);
            }
            0x08 | 0x09 => self.ram_enable = val,
            reg @ 0x0A..=0x0F => {
                self.prg[usize::from((reg - 0x0A) / 2)] = val & 0x3F;
                self.update_prg_banks(memory);
            }
            _ => (),
        }
    }

    fn sync_battery(&self, memory: &mut Memory) {
        memory
            .region_mut(Src::BatteryExt)
            .copy_from_slice(self.ram.as_ref());
    }

    fn restore_battery(&mut self, memory: &Memory) {
        self.ram
            .as_mut()
            .copy_from_slice(memory.region_ref(Src::BatteryExt));
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        self.update_prg_banks(memory);
        self.update_chr_banks(memory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 64K PRG-ROM (8 8K banks) and 64K CHR-ROM, loaded as `mapper_num`.
    fn load(mapper_num: u16) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(64 * 1024, 0, 64 * 1024);
        cart.header.mapper_num = mapper_num;
        let mapper = TaitoX1005::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// The PRG registers come in mirrored pairs, and $E000 is fixed to the last bank.
    #[test]
    fn prg_registers_are_mirrored_pairs() {
        let (mut mapper, mut cart) = load(80);
        write(&mut mapper, &mut cart, 0x7EFB, 2);
        write(&mut mapper, &mut cart, 0x7EFC, 3);
        write(&mut mapper, &mut cart, 0x7EFF, 4);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 4 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 7 * 8);
    }

    /// The 2K CHR registers ignore bit 0, and the board also decodes them with A7 clear.
    #[test]
    fn chr_registers_bank_2k_then_1k() {
        let (mut mapper, mut cart) = load(80);
        write(&mut mapper, &mut cart, 0x7E70, 5);
        write(&mut mapper, &mut cart, 0x7EF5, 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 4);
        assert_eq!(chr_peek(&mapper, &cart, 0x0400), 0x80 | 5);
        assert_eq!(chr_peek(&mapper, &cart, 0x1C00), 0x80 | 9);
    }

    /// The internal RAM answers only after `$A3` goes to `$7EF8`, and mirrors at `$7F80`.
    #[test]
    fn internal_ram_needs_the_enable_value() {
        let (mut mapper, mut cart) = load(80);
        write(&mut mapper, &mut cart, 0x7F05, 0x42);
        assert_eq!(prg_peek(&mapper, &cart, 0x7F05), 0x00, "disabled");

        write(&mut mapper, &mut cart, 0x7EF8, TaitoX1005::RAM_ENABLE);
        write(&mut mapper, &mut cart, 0x7F05, 0x42);
        assert_eq!(prg_peek(&mapper, &cart, 0x7F85), 0x42, "mirrored");

        mapper.sync_battery(&mut cart.memory);
        // The test cart has no PRG-RAM, so the battery is the internal RAM alone.
        assert_eq!(cart.memory.sram()[5], 0x42, "in the battery");
    }

    /// Mapper 207 takes bit 7 of `$7EF0`/`$7EF1` for nametables and ignores `$7EF6`.
    #[test]
    fn mapper_207_selects_nametables_from_chr_bit_7() {
        let (mut mapper, mut cart) = load(207);
        let ciram = cart.memory.region_mut(Src::CiRam);
        ciram[..0x400].fill(0x11);
        ciram[0x400..].fill(0x22);
        write(&mut mapper, &mut cart, 0x7EF0, 0x80 | 6);
        write(&mut mapper, &mut cart, 0x7EF6, 0x01);
        let nametables = [0x2000, 0x2400, 0x2800, 0x2C00].map(|a| chr_peek(&mapper, &cart, a));
        assert_eq!(nametables, [0x22, 0x22, 0x11, 0x11]);
        assert_eq!(
            chr_peek(&mapper, &cart, 0x0000),
            0x80 | 6,
            "bit 7 is not a CHR bit"
        );
    }
}
