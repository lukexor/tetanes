//! `VRC7` (Mapper 085).
//!
//! <https://www.nesdev.org/wiki/VRC7>

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

/// `VRC7` (Mapper 085).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Vrc7 {
    /// `$8000`, `$8010` and `$9000`: 8K PRG banks.
    pub prg: [u8; 3],
    /// `$A000-$D010`: 1K CHR banks.
    pub chr: [u8; 8],
    /// `$E000`: bit 7 enables PRG-RAM, bit 6 silences the audio, and bits 0-1 pick mirroring.
    pub control: u8,
    /// The scanline and CPU-cycle IRQ counter shared with VRC4 and VRC6.
    pub irq: VrcIrq,
}

impl Vrc7 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;

    // PPU $0000..=$1FFF eight 1K switchable CHR banks, ROM or RAM
    // CPU $6000..=$7FFF 8K PRG-RAM, behind an enable bit
    // CPU $8000..=$DFFF three 8K switchable PRG-ROM banks
    // CPU $E000..=$FFFF 8K PRG-ROM fixed to the last bank
    /// Load the board for `cart`.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            prg: [0; 3],
            chr: [0; 8],
            control: 0,
            irq: VrcIrq::default(),
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// Fold the two board wirings onto one register map. VRC7a (Lagrange Point) puts the second
    /// register of each pair at `$x010`, and VRC7b (Tiny Toon Adventures 2) at `$x008`. `$9010`
    /// is the audio address port on both, so it is left alone.
    const fn decode(addr: u16) -> u16 {
        let addr = if addr & 0x10 == 0x10 && addr & 0xF010 != 0x9010 {
            (addr | 0x08) & !0x10
        } else {
            addr
        };
        addr & 0xF038
    }

    fn update_prg_banks(&self, memory: &mut Memory) {
        for (slot, bank) in self.prg.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);
    }

    fn update_prg_ram(&self, memory: &mut Memory) {
        if self.control & 0x80 == 0x80 {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }
    }
}

impl Map for Vrc7 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ
    }

    fn mirroring(&self) -> Mirroring {
        match self.control & 0x03 {
            0 => Mirroring::Vertical,
            1 => Mirroring::Horizontal,
            2 => Mirroring::SingleScreenA,
            _ => Mirroring::SingleScreenB,
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.irq_pending
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        for (name, bank) in ["PRG $8000", "PRG $A000", "PRG $C000"]
            .into_iter()
            .zip(self.prg)
        {
            out.push((name, u32::from(bank)));
        }
        let chr = [
            "CHR 0", "CHR 1", "CHR 2", "CHR 3", "CHR 4", "CHR 5", "CHR 6", "CHR 7",
        ];
        for (name, bank) in chr.into_iter().zip(self.chr) {
            out.push((name, u32::from(bank)));
        }
        out.push(("Control", u32::from(self.control)));
        out.push(("IRQ reload", u32::from(self.irq.reload)));
        out.push(("IRQ counter", u32::from(self.irq.counter)));
        out.push(("IRQ enabled", u32::from(self.irq.enabled)));
        out.push(("IRQ pending", u32::from(self.irq.irq_pending)));
        out.push(("IRQ cycle mode", u32::from(self.irq.cycle_mode)));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        match Self::decode(addr) {
            reg @ (0x8000 | 0x8008 | 0x9000) => {
                let slot = match reg {
                    0x8000 => 0,
                    0x8008 => 1,
                    _ => 2,
                };
                self.prg[slot] = val & 0x3F;
                self.update_prg_banks(memory);
            }
            reg @ (0xA000 | 0xA008 | 0xB000 | 0xB008 | 0xC000 | 0xC008 | 0xD000 | 0xD008) => {
                let slot = usize::from(((reg - 0xA000) >> 11) | ((reg >> 3) & 0x01));
                self.chr[slot] = val;
                let addr = (slot * Self::CHR_WINDOW) as u16;
                memory.map_chr(addr, Self::CHR_WINDOW, i32::from(val), Src::Chr);
            }
            0xE000 => {
                self.control = val;
                self.update_prg_ram(memory);
                memory.set_mirroring(self.mirroring());
            }
            0xE008 => self.irq.write_reload(val),
            0xF000 => self.irq.write_control(val),
            0xF008 => self.irq.acknowledge(),
            // $9010 and $9030 are the FM audio address and data ports.
            _ => (),
        }
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        self.update_prg_ram(memory);
        self.update_prg_banks(memory);
        for (slot, bank) in self.chr.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        memory.set_mirroring(self.mirroring());
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

    /// 256K PRG-ROM (32 8K banks), 8K PRG-RAM and 128K CHR-ROM (128 1K banks).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(256 * 1024, 8 * 1024, 128 * 1024);
        let mapper = Vrc7::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Both wirings reach the same registers: VRC7a's `$x010` and VRC7b's `$x008`.
    #[test]
    fn prg_registers_on_both_wirings() {
        for second in [0x8010, 0x8008] {
            let (mut mapper, mut cart) = load();
            write(&mut mapper, &mut cart, 0x8000, 3);
            write(&mut mapper, &mut cart, second, 4);
            write(&mut mapper, &mut cart, 0x9000, 5);
            for (addr, bank) in [(0x8000, 3u8), (0xA000, 4), (0xC000, 5), (0xE000, 31)] {
                // 8K bank n starts at 1K page 8n, which reads back as its low byte.
                assert_eq!(
                    prg_peek(&mapper, &cart, addr),
                    bank * 8,
                    "${second:04X}: ${addr:04X}"
                );
            }
        }
    }

    /// The eight CHR registers sit in pairs at `$A000`-`$D000`, the second of each pair at `$x010`
    /// or `$x008`.
    #[test]
    fn chr_registers_on_both_wirings() {
        for offset in [0x10, 0x08] {
            let (mut mapper, mut cart) = load();
            for (slot, base) in [0xA000u16, 0xB000, 0xC000, 0xD000].into_iter().enumerate() {
                write(&mut mapper, &mut cart, base, 20 + 2 * slot as u8);
                write(&mut mapper, &mut cart, base + offset, 21 + 2 * slot as u8);
            }
            for slot in 0..8u16 {
                assert_eq!(
                    chr_peek(&mapper, &cart, slot * 0x400),
                    0x80 | (20 + slot as u8),
                    "offset ${offset:02X}, slot {slot}"
                );
            }
        }
    }

    /// `$E000` bit 7 maps PRG-RAM and bits 0-1 pick mirroring. `$9010`, the audio address port,
    /// is not a PRG register.
    #[test]
    fn control_register_and_audio_port() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x00, "PRG-RAM disabled");
        write(&mut mapper, &mut cart, 0xE000, 0x83);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "PRG-RAM enabled");
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
        write(&mut mapper, &mut cart, 0xE000, 0x01);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);

        write(&mut mapper, &mut cart, 0x9000, 2);
        write(&mut mapper, &mut cart, 0x9010, 7);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 16, "$9010 left PRG alone");
    }

    /// `$E010`, `$F000` and `$F010` drive the shared VRC IRQ counter.
    #[test]
    fn irq_registers() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xE010, 0xFE);
        // Cycle mode and enabled, which also loads the counter from the reload value.
        write(&mut mapper, &mut cart, 0xF000, 0x06);
        mapper.clock();
        assert!(!mapper.irq_pending());
        mapper.clock();
        assert!(mapper.irq_pending(), "fires as the counter passes $FF");
        write(&mut mapper, &mut cart, 0xF010, 0);
        assert!(!mapper.irq_pending(), "$F010 acknowledges");
    }
}
