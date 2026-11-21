//! `Taito X1-017` (Mapper 082).
//!
//! <https://www.nesdev.org/wiki/Taito_X1-017>

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

// The chip also has a CPU-cycle IRQ at $7EFD-$7EFF. No game uses it, so it is left out.
/// `Taito X1-017` (Mapper 082).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct TaitoX1017 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// `$7EF6` bit 1: the 2K CHR banks sit at `$1000` instead of `$0000`.
    pub chr_inverted: bool,
    /// `$7EF0-$7EF5`: two 2K CHR banks, then four 1K banks, all in 1K units.
    pub chr: [u8; 6],
    /// `$7EFA-$7EFC`: 8K PRG banks at `$8000`, `$A000` and `$C000`, before the shift.
    pub prg: [u8; 3],
    /// `$7EF7-$7EF9`: each PRG-RAM window answers only while its register has its magic value.
    pub ram_enable: [u8; 3],
}

impl TaitoX1017 {
    const PRG_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    /// Each PRG-RAM window's CPU address, size and offset in PRG-RAM, and the value that enables
    /// it.
    const RAM_WINDOWS: [(u16, usize, usize, u8); 3] = [
        (0x6000, 2 * 1024, 0, 0xCA),
        (0x6800, 2 * 1024, 2 * 1024, 0x69),
        (0x7000, 1024, 4 * 1024, 0x84),
    ];

    // PPU $0000..=$1FFF two 2K and four 1K switchable CHR-ROM banks, in either order
    // CPU $6000..=$73FF 5K PRG-RAM in three separately enabled windows
    // CPU $7EF0..=$7EFF registers
    // CPU $8000..=$DFFF three 8K switchable PRG-ROM banks
    // CPU $E000..=$FFFF 8K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            mirroring: cart.mirroring(),
            chr_inverted: false,
            chr: [0; 6],
            prg: [0; 3],
            ram_enable: [0; 3],
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    fn update_ram_windows(&self, memory: &mut Memory) {
        for ((addr, size, offset, magic), enable) in
            Self::RAM_WINDOWS.into_iter().zip(self.ram_enable)
        {
            // The chip pulls the data bus low, so a disabled window reads as zero. An unmapped page
            // returns zero.
            if enable == magic {
                memory.map_prg(addr, size, (offset / size) as i32, Src::PrgRam);
            } else {
                memory.unmap_prg(addr, size);
            }
        }
    }

    fn update_chr_banks(&self, memory: &mut Memory) {
        let (two_k, one_k) = if self.chr_inverted {
            (0x1000, 0x0000)
        } else {
            (0x0000, 0x1000)
        };
        for (slot, reg) in self.chr[..2].iter().enumerate() {
            let addr = two_k + (slot * 2 * Self::CHR_WINDOW) as u16;
            let bank = i32::from(reg & 0xFE);
            memory.map_chr(addr, Self::CHR_WINDOW, bank, Src::Chr);
            memory.map_chr(addr + 0x0400, Self::CHR_WINDOW, bank + 1, Src::Chr);
        }
        for (slot, reg) in self.chr[2..].iter().enumerate() {
            let addr = one_k + (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*reg), Src::Chr);
        }
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        for (slot, reg) in self.prg.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            // Bits 2-5 are PRG A13-A16.
            let bank = i32::from((reg >> 2) & 0x0F);
            memory.map_prg(addr, Self::PRG_WINDOW, bank, Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }
}

impl Map for TaitoX1017 {
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (name, reg) in [
            "CHR 2K 0", "CHR 2K 1", "CHR 1K 0", "CHR 1K 1", "CHR 1K 2", "CHR 1K 3",
        ]
        .into_iter()
        .zip(self.chr)
        {
            out.push((name, u32::from(reg)));
        }
        for (name, reg) in ["PRG $8000", "PRG $A000", "PRG $C000"]
            .into_iter()
            .zip(self.prg)
        {
            out.push((name, u32::from((reg >> 2) & 0x0F)));
        }
        out.push(("CHR inverted", u32::from(self.chr_inverted)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        match addr {
            0x7EF0..=0x7EF5 => {
                self.chr[usize::from(addr - 0x7EF0)] = val;
                self.update_chr_banks(memory);
            }
            0x7EF6 => {
                self.mirroring = if val & 0x01 == 0x01 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
                self.chr_inverted = val & 0x02 == 0x02;
                memory.set_mirroring(self.mirroring);
                self.update_chr_banks(memory);
            }
            0x7EF7..=0x7EF9 => {
                self.ram_enable[usize::from(addr - 0x7EF7)] = val;
                self.update_ram_windows(memory);
            }
            0x7EFA..=0x7EFC => {
                self.prg[usize::from(addr - 0x7EFA)] = val;
                self.update_prg_banks(memory);
            }
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        self.update_ram_windows(memory);
        self.update_prg_banks(memory);
        self.update_chr_banks(memory);
        memory.set_mirroring(self.mirroring);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 64K PRG-ROM (8 8K banks), 8K PRG-RAM and 64K CHR-ROM.
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(64 * 1024, 8 * 1024, 64 * 1024);
        let mapper = TaitoX1017::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// PRG registers sit at consecutive addresses and use bits 2-5.
    #[test]
    fn prg_registers_use_bits_2_to_5() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x7EFA, 2 << 2);
        write(&mut mapper, &mut cart, 0x7EFB, (3 << 2) | 0x03);
        write(&mut mapper, &mut cart, 0x7EFC, 4 << 2);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 2 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 4 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 7 * 8);
    }

    /// `$7EF6` bit 1 swaps the 2K and 1K halves of the pattern tables.
    #[test]
    fn chr_mode_swaps_the_halves() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0x7EF0, 4);
        write(&mut mapper, &mut cart, 0x7EF2, 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 4);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | 9);
        write(&mut mapper, &mut cart, 0x7EF6, 0x02);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 9);
        assert_eq!(chr_peek(&mapper, &cart, 0x1000), 0x80 | 4);
    }

    /// Each PRG-RAM window reads as zero until its register gets its own magic value.
    #[test]
    fn ram_windows_need_their_own_enable_value() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x00, "disabled");
        write(&mut mapper, &mut cart, 0x7EF7, 0xCA);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "enabled");
        assert_eq!(prg_peek(&mapper, &cart, 0x6800), 0x00, "its own enable");
        write(&mut mapper, &mut cart, 0x7EF9, 0x84);
        write(&mut mapper, &mut cart, 0x7000, 0x33);
        assert_eq!(cart.memory.region_ref(Src::PrgRam)[4 * 1024], 0x33);
    }
}
