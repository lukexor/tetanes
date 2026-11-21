//! `UNROM 512` (Mapper 030).
//!
//! <https://www.nesdev.org/wiki/UNROM_512>

// Board register state, whose meaning is the mapper hardware's rather than this crate's. See the
// module docs on `mapper` for what a board is.
#![allow(missing_docs)]

use crate::{
    cart::Cart,
    common::ResetKind,
    mapper::{self, Map, Mapper, MapperOps},
    memory::{Memory, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};

/// Where the SST39SF040 flash chip is in its command sequence.
///
/// <https://www.nesdev.org/wiki/UNROM_512#Flash_Save>
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum FlashState {
    /// Reads return flash contents.
    #[default]
    Read,
    /// `$AA` was written to `$5555`.
    Unlock1,
    /// `$55` followed at `$2AAA`.
    Unlock2,
    /// `$A0` followed, so the next write programs a byte.
    Program,
    /// `$80` followed, so an erase sequence has started.
    Erase,
    /// The erase sequence's own `$AA` to `$5555`.
    EraseUnlock1,
    /// The erase sequence's own `$55` to `$2AAA`.
    EraseUnlock2,
    /// `$90` followed, so reads return the chip's ID until a `$F0` write.
    SoftwareId,
}

/// `UNROM 512` (Mapper 030).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Unrom512 {
    /// The current nametable arrangement.
    pub mirroring: Mirroring,
    /// NES 2.0 submapper, which picks the latch decode, bus conflicts and mirroring control.
    pub submapper_num: u8,
    /// Whether latch bit 7 selects the nametables, which the header asks for with one-screen
    /// mirroring and which submapper 3 always has.
    pub mirroring_select: bool,
    /// Whether the board self-flashes, which the header's battery bit signals.
    pub flashable: bool,
    /// The bank latch: PRG bank in bits 0-4, CHR bank in bits 5-6, nametable select in bit 7.
    pub latch: u8,
    /// Where the flash chip is in its command sequence.
    pub flash: FlashState,
}

impl Unrom512 {
    const PRG_WINDOW: usize = 16 * 1024;
    const CHR_WINDOW: usize = 8 * 1024;
    const SECTOR_SIZE: usize = 4 * 1024;
    const PRG_BANK_MASK: u8 = 0x1F;
    const CHR_BANK_SHIFT: u8 = 5;
    const CHR_BANK_MASK: u8 = 0x03;
    const NAMETABLE_SELECT: u8 = 0x80;
    /// SST's manufacturer ID, read at an even address in software ID mode.
    const MANUFACTURER_ID: u8 = 0xBF;
    /// The SST39SF040's device ID, read at an odd address in software ID mode.
    const DEVICE_ID: u8 = 0xB7;

    // PPU $0000..=$1FFF 8K switchable CHR-RAM bank, of four
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank, and the flash command window when flashable
    // CPU $C000..=$FFFF 16K PRG-ROM fixed to the last bank
    /// Load the board for `cart`, marking PRG-ROM as flash when the header has a battery.
    pub fn load(cart: &mut Cart) -> Result<Mapper, mapper::Error> {
        let submapper_num = cart.submapper_num();
        let flashable = cart.battery_backed();
        // Submapper 3 controls H/V mirroring from the latch and ignores the header bits.
        // Otherwise the header's four-screen and mirroring bits pick one of four arrangements.
        let (mirroring, mirroring_select) = if submapper_num == 3 {
            (Mirroring::Horizontal, true)
        } else {
            match cart.header.flags & 0x09 {
                0x00 => (Mirroring::Horizontal, false),
                0x01 => (Mirroring::Vertical, false),
                0x08 => (Mirroring::SingleScreenA, true),
                _ => (Mirroring::FourScreen, false),
            }
        };
        if flashable {
            cart.memory.set_prg_flash();
        }
        let mut board = Self {
            mirroring,
            submapper_num,
            mirroring_select,
            flashable,
            latch: 0,
            flash: FlashState::Read,
        };
        board.update_banks(&mut cart.memory);
        Ok(board.into())
    }

    /// Whether a write to `addr` reaches the latch.
    ///
    /// A flashable board, and submappers 1, 3 and 4, decode it at `$C000-$FFFF` only. That leaves
    /// `$8000-$BFFF` to the flash chip, or to submapper 4's LED register.
    const fn decodes_latch(&self, addr: u16) -> bool {
        if self.flashable || matches!(self.submapper_num, 1 | 3 | 4) {
            addr >= 0xC000
        } else {
            addr >= 0x8000
        }
    }

    /// Whether a latch write ANDs with the ROM byte the CPU is writing over.
    const fn has_bus_conflicts(&self) -> bool {
        !self.flashable && matches!(self.submapper_num, 0 | 2)
    }

    /// Apply latch bit 7 to the nametables, where the board lets it.
    const fn update_mirroring(&mut self) {
        if !self.mirroring_select {
            return;
        }
        let select = self.latch & Self::NAMETABLE_SELECT != 0;
        self.mirroring = match (self.submapper_num, select) {
            (3, false) => Mirroring::Horizontal,
            (3, true) => Mirroring::Vertical,
            (_, false) => Mirroring::SingleScreenA,
            (_, true) => Mirroring::SingleScreenB,
        };
    }

    /// Step the flash command sequence with a CPU write to `$8000-$BFFF`.
    fn write_flash(&mut self, memory: &mut Memory, addr: u16, val: u8) {
        // The latch's PRG bits drive the chip's upper address lines, so `$C000 = $01` then a write
        // to `$9555` reaches flash address `$5555`.
        let flash_addr = (usize::from(self.latch & Self::PRG_BANK_MASK) * Self::PRG_WINDOW)
            | usize::from(addr & 0x3FFF);
        // The chip decodes A0-A14 of a command address.
        let cmd_addr = flash_addr & 0x7FFF;
        self.flash = match (self.flash, cmd_addr, val) {
            (FlashState::Read, 0x5555, 0xAA) => FlashState::Unlock1,
            (FlashState::Unlock1, 0x2AAA, 0x55) => FlashState::Unlock2,
            (FlashState::Unlock2, 0x5555, 0xA0) => FlashState::Program,
            (FlashState::Unlock2, 0x5555, 0x80) => FlashState::Erase,
            (FlashState::Unlock2, 0x5555, 0x90) => FlashState::SoftwareId,
            (FlashState::Program, ..) => {
                // Programming can only clear bits. Setting them takes an erase.
                let old = memory.region_peek(Src::PrgRom, flash_addr);
                memory.region_write(Src::PrgRom, flash_addr, old & val);
                FlashState::Read
            }
            (FlashState::Erase, 0x5555, 0xAA) => FlashState::EraseUnlock1,
            (FlashState::EraseUnlock1, 0x2AAA, 0x55) => FlashState::EraseUnlock2,
            (FlashState::EraseUnlock2, _, 0x30) => {
                let flash = memory.region_mut(Src::PrgRom);
                let start = (flash_addr % flash.len()) & !(Self::SECTOR_SIZE - 1);
                flash[start..start + Self::SECTOR_SIZE].fill(0xFF);
                FlashState::Read
            }
            (FlashState::EraseUnlock2, 0x5555, 0x10) => {
                memory.region_mut(Src::PrgRom).fill(0xFF);
                FlashState::Read
            }
            (FlashState::SoftwareId, _, 0xF0) => FlashState::Read,
            (FlashState::SoftwareId, ..) => FlashState::SoftwareId,
            _ => FlashState::Read,
        };
    }
}

impl Map for Unrom512 {
    // Only a flashable board answers reads itself, for the chip's software ID mode. Every other
    // board leaves CPU reads to the page tables.
    fn mapper_ops(&self) -> MapperOps {
        if self.flashable {
            MapperOps::SERVES_PRG_READS
        } else {
            MapperOps::empty()
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn registers(&self, out: &mut Vec<(&'static str, u32)>) {
        out.push(("Latch", u32::from(self.latch)));
        out.push(("PRG $8000", u32::from(self.latch & Self::PRG_BANK_MASK)));
        out.push((
            "CHR $0000",
            u32::from((self.latch >> Self::CHR_BANK_SHIFT) & Self::CHR_BANK_MASK),
        ));
    }

    fn write_register(&mut self, memory: &mut Memory, addr: u16, mut val: u8) {
        if self.decodes_latch(addr) {
            if self.has_bus_conflicts() {
                val &= memory.prg_peek(addr);
            }
            self.latch = val;
            self.update_mirroring();
            self.update_banks(memory);
        } else if self.flashable && (0x8000..=0xBFFF).contains(&addr) {
            self.write_flash(memory, addr, val);
        }
    }

    fn prg_read(&mut self, addr: u16) -> Option<u8> {
        self.prg_peek(addr)
    }

    fn prg_peek(&self, addr: u16) -> Option<u8> {
        (self.flash == FlashState::SoftwareId && addr >= 0x8000).then_some(if addr & 0x01 == 0 {
            Self::MANUFACTURER_ID
        } else {
            Self::DEVICE_ID
        })
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.flash = FlashState::Read;
    }

    fn update_banks(&mut self, memory: &mut Memory) {
        memory.map_prg(
            0x8000,
            Self::PRG_WINDOW,
            i32::from(self.latch & Self::PRG_BANK_MASK),
            Src::PrgRom,
        );
        memory.map_prg(0xC000, Self::PRG_WINDOW, -1, Src::PrgRom);
        memory.map_chr(
            0x0000,
            Self::CHR_WINDOW,
            i32::from((self.latch >> Self::CHR_BANK_SHIFT) & Self::CHR_BANK_MASK),
            Src::Chr,
        );
        if self.mirroring == Mirroring::FourScreen {
            // The four-screen board puts the last 8K of CHR-RAM at $2000-$3FFF: four nametables,
            // then scratch RAM under the palette.
            memory.map_chr(0x2000, Self::CHR_WINDOW, -1, Src::Chr);
        } else {
            memory.set_mirroring(self.mirroring);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::test_utils::{chr_peek, page_indexed_cart, prg_peek, write};
    use crate::memory::{MemoryLayout, PAGE_SIZE, RamState};

    /// Header byte 6 battery bit.
    const BATTERY: u8 = 0x02;
    /// Header byte 6 four-screen bit.
    const FOUR_SCREEN: u8 = 0x08;
    /// Header byte 6 vertical-mirroring bit.
    const VERTICAL: u8 = 0x01;

    /// 128K PRG-ROM (8 16K banks) and 32K CHR-RAM, both page-indexed, with header byte 6 set to
    /// `flags`.
    fn unrom512(flags: u8, submapper_num: u8) -> (Mapper, Cart) {
        let mut cart = page_indexed_cart(128 * 1024, 0, 0);
        cart.memory = Memory::new(MemoryLayout {
            prg_rom: 128 * 1024,
            prg_ram: 8 * 1024,
            chr: 32 * 1024,
            chr_writable: true,
            ciram: 2 * 1024,
            ..MemoryLayout::default()
        });
        for (page, data) in cart
            .memory
            .region_mut(Src::PrgRom)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            data.fill(page as u8);
        }
        for (page, data) in cart
            .memory
            .region_mut(Src::Chr)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            data.fill(0x80 | page as u8);
        }
        cart.header.flags = flags;
        cart.header.submapper_num = submapper_num;
        cart.memory.set_battery_backed(flags & BATTERY != 0);
        let mapper = Unrom512::load(&mut cart).expect("valid mapper");
        (mapper, cart)
    }

    /// Writes `val` to flash address `flash_addr` the way a game does: the latch first, for the
    /// upper address lines, then the CPU write in `$8000-$BFFF`.
    fn flash_write(mapper: &mut Mapper, cart: &mut Cart, flash_addr: usize, val: u8) {
        write(mapper, cart, 0xC000, (flash_addr / 0x4000) as u8);
        write(mapper, cart, 0x8000 | (flash_addr & 0x3FFF) as u16, val);
    }

    fn unlock(mapper: &mut Mapper, cart: &mut Cart) {
        flash_write(mapper, cart, 0x5555, 0xAA);
        flash_write(mapper, cart, 0x2AAA, 0x55);
    }

    /// `$C000` is fixed to the last bank from power-on, and bank 0 sits below it.
    #[test]
    fn powers_on_with_bank_0_low_and_the_last_bank_fixed_high() {
        let (mapper, cart) = unrom512(0, 0);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "last bank");
    }

    /// Bits 0-4 pick the PRG bank and bits 5-6 the CHR bank, from the same byte.
    #[test]
    fn the_latch_banks_prg_and_chr_together() {
        let (mut mapper, mut cart) = unrom512(0, 1);
        write(&mut mapper, &mut cart, 0xC000, (2 << 5) | 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16);
        assert_eq!(prg_peek(&mapper, &cart, 0xC000), 7 * 16, "still fixed");
        assert_eq!(chr_peek(&mapper, &cart, 0x0000), 0x80 | (2 * 8));
    }

    /// An iNES 1.0 header has no CHR-RAM size, so a loaded cart gets the board's full 32K. A
    /// battery bit makes it flashable, with all of PRG-ROM in the save.
    #[test]
    fn loads_an_ines_header_with_32k_chr_ram_and_flash() {
        let mut rom = vec![0x4E, 0x45, 0x53, 0x1A, 2, 0, 0xE2, 0x10];
        rom.resize(16, 0);
        rom.resize(16 + 32 * 1024, 0xEA);
        let cart = Cart::from_rom("unrom512", &mut rom.as_slice(), RamState::default())
            .expect("valid rom");
        assert!(matches!(cart.mapper, Mapper::Unrom512(_)));
        assert_eq!(cart.memory.region_ref(Src::Chr).len(), 32 * 1024);
        assert_eq!(cart.memory.sram().len(), (32 + 8) * 1024);
        assert_eq!(cart.memory.sram()[0], 0xEA, "starts with PRG-ROM");
    }

    /// Submapper 0 without a battery has bus conflicts, so the latch gets the AND of the value
    /// and the ROM byte under it.
    #[test]
    fn non_flashable_submapper_0_has_bus_conflicts() {
        let (mut mapper, mut cart) = unrom512(0, 0);
        // $C000 reads $70 from the fixed bank, so $13 latches $10: bank 16, which wraps to 0.
        // Without the conflict it would be bank 3.
        write(&mut mapper, &mut cart, 0xC000, 0x13);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
    }

    /// A flashable board decodes the latch at `$C000-$FFFF` only, so a write to `$8000` does not
    /// bank.
    #[test]
    fn flashable_board_ignores_latch_writes_below_c000() {
        let (mut mapper, mut cart) = unrom512(BATTERY, 0);
        write(&mut mapper, &mut cart, 0x8000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
        write(&mut mapper, &mut cart, 0xC000, 3);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 3 * 16);
    }

    /// Header one-screen mirroring hands latch bit 7 the nametable choice.
    #[test]
    fn one_screen_header_switches_nametables_from_bit_7() {
        let (mut mapper, mut cart) = unrom512(FOUR_SCREEN, 1);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenA);
        write(&mut mapper, &mut cart, 0xC000, 0x80);
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
    }

    /// Submapper 3 uses bit 7 for H/V mirroring and ignores the header bits.
    #[test]
    fn submapper_3_switches_h_and_v_from_bit_7() {
        let (mut mapper, mut cart) = unrom512(VERTICAL, 3);
        assert_eq!(mapper.mirroring(), Mirroring::Horizontal);
        write(&mut mapper, &mut cart, 0xC000, 0x80);
        assert_eq!(mapper.mirroring(), Mirroring::Vertical);
    }

    /// The four-screen board maps the last 8K of CHR-RAM over `$2000-$3EFF`.
    #[test]
    fn four_screen_nametables_are_the_last_chr_bank() {
        let (mapper, cart) = unrom512(FOUR_SCREEN | VERTICAL, 1);
        assert_eq!(mapper.mirroring(), Mirroring::FourScreen);
        for (nametable, addr) in [0x2000, 0x2400, 0x2800, 0x2C00].into_iter().enumerate() {
            assert_eq!(
                chr_peek(&mapper, &cart, addr),
                0x80 | (24 + nametable as u8),
                "${addr:04X}"
            );
        }
    }

    /// Byte program clears bits, and only after the unlock and `$A0` command.
    #[test]
    fn byte_program_writes_flash() {
        let (mut mapper, mut cart) = unrom512(BATTERY, 0);
        flash_write(&mut mapper, &mut cart, 0x4123, 0x00);
        assert_eq!(
            prg_peek(&mapper, &cart, 0x8123),
            0x10,
            "no command, no write"
        );

        unlock(&mut mapper, &mut cart);
        flash_write(&mut mapper, &mut cart, 0x5555, 0xA0);
        flash_write(&mut mapper, &mut cart, 0x4123, 0x31);
        assert_eq!(prg_peek(&mapper, &cart, 0x8123), 0x10, "$10 & $31");
        flash_write(&mut mapper, &mut cart, 0x4123, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x8123), 0x10, "back to read mode");

        unlock(&mut mapper, &mut cart);
        flash_write(&mut mapper, &mut cart, 0x5555, 0xA0);
        flash_write(&mut mapper, &mut cart, 0x4123, 0x00);
        assert_eq!(prg_peek(&mapper, &cart, 0x8123), 0x00);
    }

    /// Sector erase fills one 4K sector with `$FF` and leaves its neighbors.
    #[test]
    fn sector_erase_clears_4k() {
        let (mut mapper, mut cart) = unrom512(BATTERY, 0);
        unlock(&mut mapper, &mut cart);
        flash_write(&mut mapper, &mut cart, 0x5555, 0x80);
        unlock(&mut mapper, &mut cart);
        flash_write(&mut mapper, &mut cart, 0x5000, 0x30);
        write(&mut mapper, &mut cart, 0xC000, 1);
        assert_eq!(prg_peek(&mapper, &cart, 0x9000), 0xFF);
        assert_eq!(prg_peek(&mapper, &cart, 0x9FFF), 0xFF);
        assert_eq!(prg_peek(&mapper, &cart, 0x8FFF), 16 + 3, "previous sector");
        assert_eq!(prg_peek(&mapper, &cart, 0xA000), 16 + 8, "next sector");
    }

    /// Software ID mode answers reads with SST's IDs until a `$F0` write.
    #[test]
    fn software_id_reads_the_chip_ids() {
        let (mut mapper, mut cart) = unrom512(BATTERY, 0);
        unlock(&mut mapper, &mut cart);
        flash_write(&mut mapper, &mut cart, 0x5555, 0x90);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0xBF);
        assert_eq!(prg_peek(&mapper, &cart, 0x8001), 0xB7);
        flash_write(&mut mapper, &mut cart, 0x0000, 0xF0);
        assert_eq!(prg_peek(&mapper, &cart, 0x8000), 0);
    }

    /// Flash is the save memory, so `sram` starts at PRG-ROM and includes what was programmed.
    #[test]
    fn flash_is_in_sram() {
        let (mut mapper, mut cart) = unrom512(BATTERY, 0);
        unlock(&mut mapper, &mut cart);
        flash_write(&mut mapper, &mut cart, 0x5555, 0xA0);
        flash_write(&mut mapper, &mut cart, PAGE_SIZE + 1, 0x00);
        let sram = cart.memory.sram();
        assert_eq!(sram.len(), (128 + 8) * 1024);
        assert_eq!(sram[PAGE_SIZE], 1, "untouched");
        assert_eq!(sram[PAGE_SIZE + 1], 0, "programmed");
    }

    /// A non-flashable board's `sram` is PRG-RAM alone.
    #[test]
    fn non_flashable_sram_is_prg_ram() {
        let (_, cart) = unrom512(0, 0);
        assert_eq!(cart.memory.sram().len(), 8 * 1024);
    }

    /// `update_banks` must rebuild every window from the registers alone.
    /// [`Bus::rebuild_mapper_state`](crate::bus::Bus::rebuild_mapper_state) relies on it after a
    /// save state.
    #[test]
    fn update_banks_rebuilds_every_window_from_register_state() {
        let (mut mapper, mut cart) = unrom512(FOUR_SCREEN, 1);
        write(&mut mapper, &mut cart, 0xC000, 0x80 | (1 << 5) | 5);

        let sample = |mapper: &Mapper, cart: &Cart| {
            [
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
        assert_eq!(mapper.mirroring(), Mirroring::SingleScreenB);
    }
}
