//! `Jaleco SS88006` (Mapper 018).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_018>

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// `Jaleco SS88006` page bit.
#[derive(Debug)]
#[must_use]
enum PageBit {
    Low,
    High,
}

impl PageBit {
    /// Merge a 4-bit write into the low or high nibble of an existing bank index.
    const fn page(&self, page: u16, val: u8) -> u16 {
        let val = (val as u16) & 0x0F;
        match self {
            PageBit::Low => (page & 0xF0) | val,
            PageBit::High => (val << 4) | (page & 0x0F),
        }
    }
}

impl From<u16> for PageBit {
    fn from(addr: u16) -> Self {
        if addr & 0x01 == 0x01 {
            Self::High
        } else {
            Self::Low
        }
    }
}

/// `Jaleco SS88006` registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub irq_reload: [u8; 4],
    pub irq_counter_size: u8,
}

/// `Jaleco SS88006` (Mapper 018).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct JalecoSs88006 {
    pub regs: Regs,
    pub irq_counter: u16,
    pub mirroring: Mirroring,
    /// 8x 1K CHR banks, each written as two 4-bit halves.
    pub chr_banks: [u16; 8],
    /// 3x 8K PRG-ROM banks, likewise written in halves. $E000 is fixed to the last bank.
    pub prg_banks: [u16; 3],
    /// PRG-RAM access, from the $9002 register.
    pub prg_ram_readable: bool,
    pub prg_ram_writable: bool,
}

impl JalecoSs88006 {
    const PRG_WINDOW: usize = 8 * 1024;
    const PRG_RAM_WINDOW: usize = 8 * 1024;
    const CHR_WINDOW: usize = 1024;
    const IRQ_MASKS: [u16; 4] = [0xFFFF, 0x0FFF, 0x00FF, 0x000F];

    // PPU $0000..=$1FFF 8x 1K CHR Banks
    // CPU $6000..=$7FFF 8K PRG-RAM, access controlled by $9002
    // CPU $8000..=$DFFF 3x 8K PRG-ROM Banks Switchable
    // CPU $E000..=$FFFF 8K PRG-ROM Fixed to Last Bank
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let mut board = Self {
            regs: Regs::default(),
            irq_counter: 0,
            mirroring: cart.mirroring(),
            chr_banks: [0; 8],
            prg_banks: [0; 3],
            prg_ram_readable: true,
            prg_ram_writable: true,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }
}

impl Map for JalecoSs88006 {
    fn mapper_ops(&self) -> MapperOps {
        MapperOps::CLOCKED | MapperOps::IRQ
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        if addr < 0x8000 {
            return;
        }
        let bits = PageBit::from(addr);
        match addr & 0xF003 {
            0x8000 | 0x8001 => self.prg_banks[0] = bits.page(self.prg_banks[0], val),
            0x8002 | 0x8003 => self.prg_banks[1] = bits.page(self.prg_banks[1], val),
            0x9000 | 0x9001 => self.prg_banks[2] = bits.page(self.prg_banks[2], val),
            0x9002 => {
                self.prg_ram_readable = val & 0x01 == 0x01;
                self.prg_ram_writable = self.prg_ram_readable && val & 0x02 == 0x02;
            }
            0xA000 | 0xA001 => self.chr_banks[0] = bits.page(self.chr_banks[0], val),
            0xA002 | 0xA003 => self.chr_banks[1] = bits.page(self.chr_banks[1], val),
            0xB000 | 0xB001 => self.chr_banks[2] = bits.page(self.chr_banks[2], val),
            0xB002 | 0xB003 => self.chr_banks[3] = bits.page(self.chr_banks[3], val),
            0xC000 | 0xC001 => self.chr_banks[4] = bits.page(self.chr_banks[4], val),
            0xC002 | 0xC003 => self.chr_banks[5] = bits.page(self.chr_banks[5], val),
            0xD000 | 0xD001 => self.chr_banks[6] = bits.page(self.chr_banks[6], val),
            0xD002 | 0xD003 => self.chr_banks[7] = bits.page(self.chr_banks[7], val),
            0xE000..=0xE003 => self.regs.irq_reload[(addr & 0x03) as usize] = val,
            0xF000 => {
                self.regs.irq_pending = false;
                self.irq_counter = u16::from(self.regs.irq_reload[0])
                    | (u16::from(self.regs.irq_reload[1]) << 4)
                    | (u16::from(self.regs.irq_reload[2]) << 8)
                    | (u16::from(self.regs.irq_reload[3]) << 12);
            }
            0xF001 => {
                self.regs.irq_enabled = val & 0x01 == 0x01;
                self.regs.irq_pending = false;
                self.regs.irq_counter_size = if val & 0x08 == 0x08 {
                    3
                } else if val & 0x04 == 0x04 {
                    2
                } else if val & 0x02 == 0x02 {
                    1
                } else {
                    0
                };
            }
            0xF002 => {
                self.mirroring = match val & 0x03 {
                    0b00 => Mirroring::Horizontal,
                    0b01 => Mirroring::Vertical,
                    0b10 => Mirroring::SingleScreenA,
                    _ => Mirroring::SingleScreenB,
                };
            }
            // $F003 selects expansion audio, which is not emulated.
            0xF003 => (),
            _ => (),
        }
        self.update_banks(memory);
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        for (slot, bank) in self.chr_banks.iter().enumerate() {
            let addr = (slot * Self::CHR_WINDOW) as u16;
            memory.map_chr(addr, Self::CHR_WINDOW, i32::from(*bank), Src::Chr);
        }
        for (slot, bank) in self.prg_banks.iter().enumerate() {
            let addr = 0x8000 + (slot * Self::PRG_WINDOW) as u16;
            memory.map_prg(addr, Self::PRG_WINDOW, i32::from(*bank), Src::PrgRom);
        }
        memory.map_prg(0xE000, Self::PRG_WINDOW, -1, Src::PrgRom);

        if self.prg_ram_readable {
            memory.map_prg(0x6000, Self::PRG_RAM_WINDOW, 0, Src::PrgRam);
            memory.set_prg_writable(0x6000, Self::PRG_RAM_WINDOW, self.prg_ram_writable);
        } else {
            memory.unmap_prg(0x6000, Self::PRG_RAM_WINDOW);
        }
        memory.set_mirroring(self.mirroring);
    }

    fn clock(&mut self) {
        if self.regs.irq_enabled {
            let irq_mask = Self::IRQ_MASKS[self.regs.irq_counter_size as usize];
            let counter = self.irq_counter & irq_mask;
            if counter == 0 {
                self.regs.irq_pending = true;
            }
            self.irq_counter =
                (self.irq_counter & !irq_mask) | (counter.wrapping_sub(1) & irq_mask);
        }
    }

    fn reset(&mut self, _kind: ResetKind) {
        // The last PRG slot is fixed in `update_banks`, which the caller runs after reset.
        self.regs = Regs::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};

    /// 128K PRG-ROM (16 8K banks), 8K PRG-RAM, 64K CHR-ROM (64 1K banks).
    fn load() -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 8 * 1024, 64 * 1024);
        let mapper = JalecoSs88006::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Writes a bank register as the two 4-bit halves the board actually takes: the even address
    /// carries the low nibble and the odd one the high nibble.
    fn bank(mapper: &mut Mapper, cart: &mut Cart, addr: u16, bank: u8) {
        write(mapper, cart, addr, bank & 0x0F);
        write(mapper, cart, addr + 1, bank >> 4);
    }

    /// Three switchable 8K windows and a fixed last bank. The registers are at $8000/$8002/$9000 -
    /// not at the window each controls.
    #[test]
    fn prg_windows_are_three_switchable_8k_banks_and_a_fixed_last() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "last bank at $E000");

        bank(&mut mapper, &mut cart, 0x8000, 3);
        bank(&mut mapper, &mut cart, 0x8002, 5);
        bank(&mut mapper, &mut cart, 0x9000, 7);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 5 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 8);
        assert_eq!(prg_peek(&mapper, &cart, 0xE000), 120, "still fixed");
    }

    /// Every bank register arrives as two 4-bit halves, and each half must land in its own nibble
    /// without disturbing the other.
    #[test]
    fn bank_registers_are_written_as_two_independent_nibbles() {
        let (mut mapper, mut cart) = load();

        // Low nibble first: bank $02.
        write(&mut mapper, &mut cart, 0xA000, 0x02);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 0x02);

        // Then the high nibble alone, making it bank $12 - the low nibble must survive.
        write(&mut mapper, &mut cart, 0xA001, 0x01);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 0x12);

        // And rewriting the low nibble alone must leave the high one alone.
        write(&mut mapper, &mut cart, 0xA000, 0x05);
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | 0x15);
    }

    /// Eight 1K CHR windows, spread over $A000-$D003 two nibbles at a time.
    #[test]
    fn chr_registers_map_eight_1k_banks() {
        let (mut mapper, mut cart) = load();
        let regs = [0xA000, 0xA002, 0xB000, 0xB002, 0xC000, 0xC002, 0xD000, 0xD002];
        for (slot, reg) in regs.into_iter().enumerate() {
            bank(&mut mapper, &mut cart, reg, 20 + slot as u8);
        }
        for slot in 0..8u16 {
            assert_eq!(
                chr_peek(&mapper, &cart, slot * 1024),
                0x80 | (20 + slot as u8),
                "slot {slot}"
            );
        }
    }

    /// $9002 controls PRG-RAM: bit 0 maps it at all, bit 1 makes it writable - and write access
    /// requires read access too.
    #[test]
    fn prg_ram_access_is_controlled_by_9002() {
        let (mut mapper, mut cart) = load();
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "mapped at power-on");

        write(&mut mapper, &mut cart, 0x9002, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0, "unmapped reads as 0");

        // Readable but not writable.
        write(&mut mapper, &mut cart, 0x9002, 0x01);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A);
        cart.memory.prg_write(0x6000, 0x77);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x5A, "write-protected");

        write(&mut mapper, &mut cart, 0x9002, 0x03);
        cart.memory.prg_write(0x6000, 0x77);
        assert_eq!(prg_peek(&mapper, &cart, 0x6000), 0x77, "writable");
    }

    /// The IRQ counter is a down-counter reloaded from four 4-bit registers, and it fires when the
    /// counter is already zero at the start of a clock - so a reload of `n` gives `n + 1` clocks.
    #[test]
    fn irq_counter_reloads_from_four_nibbles_and_counts_down() {
        let (mut mapper, mut cart) = load();

        // Reload $0003, full 16-bit width.
        write(&mut mapper, &mut cart, 0xE000, 3);
        write(&mut mapper, &mut cart, 0xF001, 0x01); // enable, 16-bit
        write(&mut mapper, &mut cart, 0xF000, 0); // latch the reload into the counter

        for clock in 0..3 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "fires once the counter reaches zero");
    }

    /// The counter width is selectable, and a narrower width must count only its own low bits while
    /// leaving the rest of the register untouched.
    #[test]
    fn a_narrow_counter_counts_only_its_low_bits() {
        let (mut mapper, mut cart) = load();

        // Reload $1234, then run it as a 4-bit counter: only the $4 counts.
        for (nibble, val) in [4, 3, 2, 1].into_iter().enumerate() {
            write(&mut mapper, &mut cart, 0xE000 + nibble as u16, val);
        }
        write(&mut mapper, &mut cart, 0xF001, 0x09); // enable, 4-bit
        write(&mut mapper, &mut cart, 0xF000, 0);

        for clock in 0..4 {
            mapper.clock();
            assert!(!mapper.irq_pending(), "too early, at clock {clock}");
        }
        mapper.clock();
        assert!(mapper.irq_pending(), "a 4-bit counter fires after 5 clocks");
    }

    /// Writing either IRQ control register acknowledges a pending interrupt, and a disabled counter
    /// never fires.
    #[test]
    fn the_irq_is_acknowledged_by_either_control_register() {
        let (mut mapper, mut cart) = load();
        write(&mut mapper, &mut cart, 0xE000, 1);
        write(&mut mapper, &mut cart, 0xF001, 0x01);
        write(&mut mapper, &mut cart, 0xF000, 0);
        for _ in 0..2 {
            mapper.clock();
        }
        assert!(mapper.irq_pending());

        write(&mut mapper, &mut cart, 0xF000, 0);
        assert!(!mapper.irq_pending(), "reload acknowledges");

        write(&mut mapper, &mut cart, 0xF001, 0x00); // disable
        for _ in 0..1000 {
            mapper.clock();
        }
        assert!(!mapper.irq_pending(), "a disabled counter never fires");
    }

    /// `update_banks` must rebuild every window from the registers alone, which is what
    /// `Ppu::rebuild_mapper_state` relies on after a save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = load();
        bank(&mut mapper, &mut cart, 0x8000, 3);
        bank(&mut mapper, &mut cart, 0x8002, 5);
        bank(&mut mapper, &mut cart, 0x9000, 7);
        bank(&mut mapper, &mut cart, 0xA000, 9);

        let sample = |mapper: &Mapper, cart: &Cart| -> Vec<u8> {
            [0x6000, 0x8000, 0xA000, 0xC000, 0xE000]
                .into_iter()
                .map(|addr| prg_peek(mapper, cart, addr))
                .chain([chr_peek(mapper, cart, 0x0000)])
                .collect()
        };
        let before = sample(&mapper, &cart);

        cart.memory.unmap_prg(0x0000, 0x10000);
        cart.memory.unmap_chr(0x0000, 0x4000);
        mapper.update_banks(&mut cart.memory);

        assert_eq!(before, sample(&mapper, &cart));
    }
}
