//! Unified cartridge memory and page-based address translation.
//!
//! Every byte a mapper can expose - PRG-ROM, PRG-RAM, CHR-ROM/RAM, nametable CIRAM and MMC5 ExRAM -
//! lives in one contiguous allocation. Address translation goes through two small page tables of
//! 1 KiB entries, so a read is a table lookup and an indexed load with no branch on the memory
//! source and no dispatch on the mapper:
//!
//! ```ignore
//! let page = self.chr_pages[(addr as usize >> PAGE_SHIFT) & CHR_PAGE_MASK];
//! self.data[page.offset() | (addr as usize & PAGE_OFFSET_MASK)]
//! ```
//!
//! Boards become pure register state: on a register write they call [`Memory::map_prg`] /
//! [`Memory::map_chr`] to rewrite a few page entries, and the read path never consults them again.
//!
//! 1 KiB is the finest granularity any real board uses (MMC3 and MMC5 CHR, MMC5 PRG), so the CHR
//! table is 16 entries covering `$0000-$3FFF` - a single cache line - and the PRG table is 64
//! entries covering the full 64 KiB CPU address space.
//!
//! Nametable mirroring is expressed as page entries pointing into the CIRAM region rather than as
//! an address-munging function, which makes four-screen, MMC5 ExRAM-as-nametable, and boards that
//! map CHR-ROM into the nametable range fall out for free.
//!
//! Alongside the arena this module holds the small memory primitives the rest of the emulator
//! shares: [`ConstArray`] for plain byte storage, and [`RamState`] for power-on fill.
//!
//! # Stability
//!
//! [`Memory`]'s fields are private, unlike the emulated components', because they hold an
//! invariant between them rather than each standing alone: `data`, `ram_start` and the region
//! ranges have to agree, and the page tables are derived from the loaded board's registers. So
//! [`Memory::prg_pages`] and [`Memory::chr_pages`] are read-only - a board rewrites entries
//! through [`Memory::map_prg`] and [`Memory::map_chr`] - and [`Memory::sram`] computes a span
//! rather than returning a field. See the crate-level [stability](crate#stability) note for the
//! tier this belongs to and why.

use crate::ppu::Mirroring;
use rand::Rng;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{SeqAccess, Visitor},
    ser::SerializeTuple,
};
use std::{
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, IndexMut, Range, RangeInclusive},
    str::FromStr,
};

/// Address bits translated within a single page. 1 KiB granularity.
pub const PAGE_SHIFT: usize = 10;
/// Size of a single translation page.
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
/// Mask selecting the offset within a page.
pub const PAGE_OFFSET_MASK: usize = PAGE_SIZE - 1;

/// Number of CHR page entries, covering `$0000-$3FFF`.
pub const CHR_PAGES: usize = 0x4000 >> PAGE_SHIFT;
/// Number of PRG page entries, covering the full CPU address space.
pub const PRG_PAGES: usize = 0x10000 >> PAGE_SHIFT;

const CHR_PAGE_MASK: usize = CHR_PAGES - 1;
const PRG_PAGE_MASK: usize = PRG_PAGES - 1;

/// First nametable page in the CHR table, i.e. `$2000`.
const NAMETABLE_PAGE: usize = 0x2000 >> PAGE_SHIFT;

/// Which backing region a mapping refers to.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum Src {
    /// Program ROM.
    PrgRom,
    /// Program RAM, battery-backed or not.
    PrgRam,
    /// Battery-backed state a board keeps outside PRG-RAM, staged here so that
    /// [`Memory::sram`] is the whole battery as one slice.
    ///
    /// Never mapped into a page table: nothing on the CPU or PPU bus addresses it. The board that
    /// owns it copies in and out through
    /// [`Map::sync_battery`](crate::mapper::Map::sync_battery).
    BatteryExt,
    /// CHR-ROM or CHR-RAM, whichever the cart provides.
    Chr,
    /// Nametable RAM.
    CiRam,
    /// MMC5 expansion RAM.
    ExRam,
}

/// A page table entry: a 1 KiB-aligned byte offset into [`Memory`] plus access flags.
///
/// An unmapped entry is all zeroes, which points at the reserved zero-filled page at offset 0 and
/// is not writable. That keeps reads branchless - an unmapped read yields 0, matching what the
/// per-mapper `_ => 0` fallbacks returned - while writes still have to test the writable flag.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct Page(u32);

impl Page {
    const OFFSET_MASK: u32 = 0x3FFF_FFFF;
    const WRITABLE: u32 = 1 << 31;

    /// An unmapped page: reads as zero, ignores writes.
    pub const UNMAPPED: Self = Self(0);

    const fn new(offset: usize, writable: bool) -> Self {
        let offset = (offset as u32) & Self::OFFSET_MASK;
        Self(if writable {
            offset | Self::WRITABLE
        } else {
            offset
        })
    }

    /// Byte offset of this page within [`Memory`].
    #[inline(always)]
    pub const fn offset(self) -> usize {
        (self.0 & Self::OFFSET_MASK) as usize
    }

    /// Whether writes to this page are stored.
    #[inline(always)]
    pub const fn is_writable(self) -> bool {
        self.0 & Self::WRITABLE != 0
    }
}

/// Cartridge and console memory, plus the PRG and CHR page tables that address it.
///
/// `Serialize`/`Deserialize` are hand-written so that a save state carries only the mutable
/// tail of `data` - see the private `MemoryState` for what is stored and why.
#[derive(Clone)]
#[must_use]
pub struct Memory {
    data: Box<[u8]>,
    /// Offset at which mutable regions begin. Everything below this is ROM and never changes, so
    /// save states only need `data[ram_start..]`.
    ram_start: usize,
    /// CRC32 of the cart's ROM - the same one the game database is keyed by. It says *which game*
    /// this arena belongs to, which a state that carries no ROM otherwise has no way to record.
    rom_crc32: u32,
    /// Whether the cart's RAM is battery-backed, and so survives a power cycle.
    battery_backed: bool,
    /// Whether `data[..ram_start]` holds the cart's ROM.
    ///
    /// A deserialized arena does not - a save state carries only the mutable tail - until
    /// [`Memory::restore_rom_from`] puts it back. A live or cloned one always does. It describes
    /// this copy of the arena rather than the console, so it is not serialized.
    rom_present: bool,
    prg_rom: Range<usize>,
    prg_ram: Range<usize>,
    /// Reserved span for [`Src::BatteryExt`], a whole number of pages like every other region.
    battery_ext: Range<usize>,
    /// How much of `battery_ext` the loaded board actually uses, which is what [`Memory::sram`]
    /// exposes. Zero for every board whose battery is PRG-RAM alone.
    battery_ext_len: usize,
    chr: Range<usize>,
    chr_writable: bool,
    ciram: Range<usize>,
    ex_ram: Range<usize>,
    // Page tables are derived state, rebuilt by replaying mapper register state on load, so they
    // are not serialized. Serde also has no derive for arrays longer than 32.
    prg_pages: [Page; PRG_PAGES],
    chr_pages: [Page; CHR_PAGES],
}

/// What a save state actually stores of a [`Memory`]: the layout, and the *mutable tail only*.
///
/// Everything below `ram_start` is ROM: it comes from the cart and cannot change, so it is left
/// out and put back from the running console in `Bus::swap_state`, which every restore path ends
/// in - a save state, rewind, and run-ahead alike. For Super Mario Bros. 3 that is 394 KiB of a
/// 408 KiB state, and rewind keeps ~900 of them. What does travel is the ROM's CRC32, so that the
/// console can tell a state of its own from one recorded against a different game with the same
/// memory layout.
///
/// The page tables are absent for a second reason: they are derived state, rebuilt from the
/// mapper's registers by `Map::update_banks`.
#[derive(Serialize)]
struct MemoryState<'a> {
    len: usize,
    ram_start: usize,
    rom_crc32: u32,
    prg_rom: &'a Range<usize>,
    prg_ram: &'a Range<usize>,
    battery_ext: &'a Range<usize>,
    battery_ext_len: usize,
    chr: &'a Range<usize>,
    chr_writable: bool,
    ciram: &'a Range<usize>,
    ex_ram: &'a Range<usize>,
    ram: &'a [u8],
}

/// Owned counterpart of [`MemoryState`], for deserialization.
#[derive(Deserialize)]
struct MemoryStateOwned {
    len: usize,
    ram_start: usize,
    rom_crc32: u32,
    prg_rom: Range<usize>,
    prg_ram: Range<usize>,
    battery_ext: Range<usize>,
    battery_ext_len: usize,
    chr: Range<usize>,
    chr_writable: bool,
    ciram: Range<usize>,
    ex_ram: Range<usize>,
    ram: Vec<u8>,
}

impl Serialize for Memory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MemoryState {
            len: self.data.len(),
            ram_start: self.ram_start,
            rom_crc32: self.rom_crc32,
            prg_rom: &self.prg_rom,
            prg_ram: &self.prg_ram,
            battery_ext: &self.battery_ext,
            battery_ext_len: self.battery_ext_len,
            chr: &self.chr,
            chr_writable: self.chr_writable,
            ciram: &self.ciram,
            ex_ram: &self.ex_ram,
            ram: &self.data[self.ram_start..],
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Memory {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let state = MemoryStateOwned::deserialize(deserializer)?;
        // Corrupt or truncated input must be an error, not a panic on the slice write below.
        if state.ram_start > state.len || state.ram.len() != state.len - state.ram_start {
            return Err(serde::de::Error::custom(format!(
                "memory state is inconsistent: len {}, ram_start {}, ram {} bytes",
                state.len,
                state.ram_start,
                state.ram.len()
            )));
        }
        let mut data = vec![0u8; state.len].into_boxed_slice();
        data[state.ram_start..].copy_from_slice(&state.ram);
        Ok(Self {
            data,
            ram_start: state.ram_start,
            rom_crc32: state.rom_crc32,
            // Cart-derived, like the ROM itself, and put back from the running console by
            // `Memory::restore_rom_from`.
            battery_backed: false,
            rom_present: false,
            prg_rom: state.prg_rom,
            prg_ram: state.prg_ram,
            battery_ext: state.battery_ext,
            battery_ext_len: state.battery_ext_len,
            chr: state.chr,
            chr_writable: state.chr_writable,
            ciram: state.ciram,
            ex_ram: state.ex_ram,
            prg_pages: Self::unmapped_prg_pages(),
            chr_pages: Self::unmapped_chr_pages(),
        })
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new(MemoryLayout::default())
    }
}

/// Reports region sizes rather than their contents, which would be megabytes of ROM.
impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Memory")
            .field("len", &self.data.len())
            .field("ram_start", &self.ram_start)
            .field("prg_rom", &self.prg_rom.len())
            .field("prg_ram", &self.prg_ram.len())
            .field("chr", &self.chr.len())
            .field("chr_writable", &self.chr_writable)
            .field("ciram", &self.ciram.len())
            .field("ex_ram", &self.ex_ram.len())
            .finish_non_exhaustive()
    }
}

/// Sizes of each backing region, in bytes.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub struct MemoryLayout {
    /// Bytes of PRG-ROM.
    pub prg_rom: usize,
    /// Bytes of PRG-RAM.
    pub prg_ram: usize,
    /// Bytes to reserve for battery state a board keeps outside PRG-RAM.
    ///
    /// An upper bound, since the arena is laid out before the board exists; the board narrows it
    /// to what it uses with [`Memory::set_battery_ext_len`].
    pub battery_ext: usize,
    /// Bytes of CHR, ROM or RAM per `chr_writable`.
    pub chr: usize,
    /// Whether the CHR region is RAM. CHR-ROM is placed with the other immutable regions and is
    /// excluded from save states.
    pub chr_writable: bool,
    /// Nametable RAM. Defaults to 2 KiB when zero; four-screen boards supply 4 KiB.
    pub ciram: usize,
    /// Extra RAM on the board itself; only MMC5 has any.
    pub ex_ram: usize,
}

impl Memory {
    const fn unmapped_prg_pages() -> [Page; PRG_PAGES] {
        [Page::UNMAPPED; PRG_PAGES]
    }

    const fn unmapped_chr_pages() -> [Page; CHR_PAGES] {
        [Page::UNMAPPED; CHR_PAGES]
    }

    /// Allocate memory for the given layout, with all pages unmapped.
    pub fn new(layout: MemoryLayout) -> Self {
        let ciram = if layout.ciram == 0 {
            2 * 1024
        } else {
            layout.ciram
        };

        // Reserve page 0 as the zero-filled unmapped page so that `Page::UNMAPPED` needs no
        // special case on the read path.
        let mut offset = PAGE_SIZE;

        // Every region is a whole number of pages. Wrapping an offset within a region then always
        // leaves a full page behind it, which is what lets the read path index without bounds
        // masking while still tolerating games that read past the end of their own banks.
        let alloc = |size: usize, offset: &mut usize| {
            let start = *offset;
            *offset += size.div_ceil(PAGE_SIZE) * PAGE_SIZE;
            start..*offset
        };

        // Immutable regions first so the mutable tail is contiguous and can be serialized alone.
        let prg_rom = alloc(layout.prg_rom, &mut offset);
        let chr_rom = (!layout.chr_writable).then(|| alloc(layout.chr, &mut offset));

        let ram_start = offset;

        // Immediately after PRG-RAM, and before anything else mutable, so that `sram` is one
        // contiguous span rather than two.
        let prg_ram = alloc(layout.prg_ram, &mut offset);
        let battery_ext = alloc(layout.battery_ext, &mut offset);
        let chr_ram = layout.chr_writable.then(|| alloc(layout.chr, &mut offset));
        let ciram = alloc(ciram, &mut offset);
        let ex_ram = alloc(layout.ex_ram, &mut offset);

        let mut memory = Self {
            data: vec![0; offset].into_boxed_slice(),
            ram_start,
            rom_crc32: 0,
            battery_backed: false,
            rom_present: true,
            prg_rom,
            prg_ram,
            battery_ext,
            battery_ext_len: 0,
            chr: chr_rom.or(chr_ram).unwrap_or(0..0),
            chr_writable: layout.chr_writable,
            ciram,
            ex_ram,
            prg_pages: [Page::UNMAPPED; PRG_PAGES],
            chr_pages: [Page::UNMAPPED; CHR_PAGES],
        };
        // CIRAM is console-internal, not part of the cart, so the nametables are readable before
        // any board has mapped anything. Boards that route them elsewhere - MMC5 picks a source
        // per nametable - overwrite these entries in their `update_banks`.
        memory.set_mirroring(Mirroring::default());
        memory
    }

    /// Read a byte from the CPU address space.
    #[inline(always)]
    pub fn prg_peek(&self, addr: u16) -> u8 {
        let page = self.prg_pages[(addr as usize >> PAGE_SHIFT) & PRG_PAGE_MASK];
        self.data[page.offset() | (addr as usize & PAGE_OFFSET_MASK)]
    }

    /// Write a byte to the CPU address space. Writes to ROM or unmapped pages are discarded.
    #[inline(always)]
    pub fn prg_write(&mut self, addr: u16, val: u8) {
        let page = self.prg_pages[(addr as usize >> PAGE_SHIFT) & PRG_PAGE_MASK];
        if page.is_writable() {
            self.data[page.offset() | (addr as usize & PAGE_OFFSET_MASK)] = val;
        }
    }

    /// Read a byte from the PPU address space, including the nametable range.
    #[inline(always)]
    pub fn chr_peek(&self, addr: u16) -> u8 {
        let page = self.chr_pages[(addr as usize >> PAGE_SHIFT) & CHR_PAGE_MASK];
        self.data[page.offset() | (addr as usize & PAGE_OFFSET_MASK)]
    }

    /// Write a byte to the PPU address space. Writes to CHR-ROM are discarded.
    #[inline(always)]
    pub fn chr_write(&mut self, addr: u16, val: u8) {
        let page = self.chr_pages[(addr as usize >> PAGE_SHIFT) & CHR_PAGE_MASK];
        if page.is_writable() {
            self.data[page.offset() | (addr as usize & PAGE_OFFSET_MASK)] = val;
        }
    }

    /// Map a window of the CPU address space to `bank` of `src`.
    ///
    /// `bank` is signed and wraps: `-1` selects the last bank, which is what most boards hard-wire
    /// at `$C000`. `size` is rounded down to a whole number of pages.
    pub fn map_prg(&mut self, addr: u16, size: usize, bank: i32, src: Src) {
        let slot = (addr as usize >> PAGE_SHIFT) & PRG_PAGE_MASK;
        self.map_pages(slot, size, bank, src, true);
    }

    /// Map a window of the PPU address space to `bank` of `src`.
    ///
    /// Also used for the nametable range; see [`Memory::set_mirroring`] for the common cases.
    pub fn map_chr(&mut self, addr: u16, size: usize, bank: i32, src: Src) {
        let slot = (addr as usize >> PAGE_SHIFT) & CHR_PAGE_MASK;
        self.map_pages(slot, size, bank, src, false);
    }

    /// Unmap a window of the CPU address space, so it reads as zero and ignores writes.
    pub fn unmap_prg(&mut self, addr: u16, size: usize) {
        let slot = (addr as usize >> PAGE_SHIFT) & PRG_PAGE_MASK;
        for i in 0..(size >> PAGE_SHIFT).max(1) {
            self.prg_pages[(slot + i) & PRG_PAGE_MASK] = Page::UNMAPPED;
        }
    }

    /// Unmap a window of the PPU address space, so it reads as zero and ignores writes.
    pub fn unmap_chr(&mut self, addr: u16, size: usize) {
        let slot = (addr as usize >> PAGE_SHIFT) & CHR_PAGE_MASK;
        for i in 0..(size >> PAGE_SHIFT).max(1) {
            self.chr_pages[(slot + i) & CHR_PAGE_MASK] = Page::UNMAPPED;
        }
    }

    /// Override whether a mapped CPU window accepts writes, for boards that can write-protect
    /// PRG-RAM.
    pub fn set_prg_writable(&mut self, addr: u16, size: usize, writable: bool) {
        let slot = (addr as usize >> PAGE_SHIFT) & PRG_PAGE_MASK;
        for i in 0..(size >> PAGE_SHIFT) {
            let Some(page) = self.prg_pages.get_mut((slot + i) & PRG_PAGE_MASK) else {
                break;
            };
            *page = Page::new(page.offset(), writable);
        }
    }

    /// Point the nametable range at CIRAM according to `mirroring`.
    ///
    /// `$3000-$3EFF` mirrors `$2000-$2EFF`, so all eight pages of the range are written.
    pub fn set_mirroring(&mut self, mirroring: Mirroring) {
        // Which 1 KiB CIRAM bank each of the four nametables selects.
        let banks: [usize; 4] = match mirroring {
            Mirroring::Vertical => [0, 1, 0, 1],
            Mirroring::Horizontal => [0, 0, 1, 1],
            Mirroring::SingleScreenA => [0; 4],
            Mirroring::SingleScreenB => [1; 4],
            Mirroring::FourScreen => [0, 1, 2, 3],
        };

        let ciram = self.ciram.clone();
        let ciram_pages = (ciram.len() >> PAGE_SHIFT).max(1);
        for (i, bank) in banks.into_iter().enumerate() {
            let offset = ciram.start + (bank % ciram_pages) * PAGE_SIZE;
            let page = Page::new(offset, true);
            self.chr_pages[NAMETABLE_PAGE + i] = page;
            // $3000-$3FFF mirrors $2000-$2FFF.
            self.chr_pages[NAMETABLE_PAGE + 4 + i] = page;
        }
    }

    /// Fill `size` bytes of a page table starting at `slot` with `bank` of `src`.
    ///
    /// Every page offset is wrapped within its region. Regions are allocated in whole pages, so a
    /// wrapped offset always has a full page behind it and the read path can never index outside
    /// `data`. This matters beyond tidiness: several games read past the end of their own banks and
    /// have to come back with a wrapped byte rather than a panic. Wrapping at map time gets that
    /// without a mask on every read.
    fn map_pages(&mut self, slot: usize, size: usize, bank: i32, src: Src, prg: bool) {
        let region = self.region(src);
        let writable = self.is_writable(src);
        let len = region.len();
        let count = (size >> PAGE_SHIFT).max(1);

        // Number of whole `size` banks available. A window at least as large as the region leaves
        // one bank, so the window repeats the region instead of running off its end.
        let banks = (len / size.max(1)).max(1) as i32;
        let base = bank.rem_euclid(banks) as usize * size;

        for i in 0..count {
            let page = if len == 0 {
                Page::UNMAPPED
            } else {
                Page::new(region.start + ((base + (i * PAGE_SIZE)) % len), writable)
            };
            let (table, mask): (&mut [Page], usize) = if prg {
                (&mut self.prg_pages, PRG_PAGE_MASK)
            } else {
                (&mut self.chr_pages, CHR_PAGE_MASK)
            };
            table[(slot + i) & mask] = page;
        }
    }

    fn region(&self, src: Src) -> Range<usize> {
        match src {
            Src::PrgRom => self.prg_rom.clone(),
            Src::PrgRam => self.prg_ram.clone(),
            // The logical length, not the reserved span: the padding up to a page boundary is not
            // the board's and must not reach a `.sram` file.
            Src::BatteryExt => {
                self.battery_ext.start..self.battery_ext.start + self.battery_ext_len
            }
            Src::Chr => self.chr.clone(),
            Src::CiRam => self.ciram.clone(),
            Src::ExRam => self.ex_ram.clone(),
        }
    }

    const fn is_writable(&self, src: Src) -> bool {
        match src {
            Src::PrgRom => false,
            Src::Chr => self.chr_writable,
            Src::PrgRam | Src::BatteryExt | Src::CiRam | Src::ExRam => true,
        }
    }

    /// Read a byte from a region by raw offset, wrapping within the region.
    ///
    /// For the rare access a page entry cannot express: MMC5's extended-attribute mode picks a
    /// 4 KiB CHR bank per *tile* from a byte of ExRAM, so the bank is not known until the fetch.
    //
    // The remainder stays, though it is a divide. Wrapping with a mask needs a power-of-two test
    // first, because a region is a whole number of pages and not necessarily a power of two, and
    // that test measured worth ~2% on the extended-attribute games while costing about 1% on
    // everything else - a shared function grown for one board's benefit, paid for by every game
    // that never calls it.
    #[must_use]
    pub fn region_peek(&self, src: Src, offset: usize) -> u8 {
        let region = self.region(src);
        if region.is_empty() {
            return 0;
        }
        self.data[region.start + offset % region.len()]
    }

    /// Write a byte to a region by raw offset, wrapping within the region.
    pub fn region_write(&mut self, src: Src, offset: usize, val: u8) {
        let region = self.region(src);
        if region.is_empty() {
            return;
        }
        self.data[region.start + offset % region.len()] = val;
    }

    /// Bytes of a region, for loading ROM contents and for save-state and debugger access.
    pub fn region_mut(&mut self, src: Src) -> &mut [u8] {
        let range = self.region(src);
        &mut self.data[range]
    }

    /// Bytes of a region.
    pub fn region_ref(&self, src: Src) -> &[u8] {
        &self.data[self.region(src)]
    }

    /// Declare how much of the reserved [`Src::BatteryExt`] span the loaded board uses.
    ///
    /// Called by a board whose battery covers something other than PRG-RAM, once, while loading.
    /// A board that never calls it contributes nothing to [`Memory::sram`].
    ///
    /// # Panics
    ///
    /// If `len` exceeds [`MemoryLayout::battery_ext`], which means the reservation keyed by mapper
    /// number is too small - a bug in that table rather than anything a ROM can cause.
    pub fn set_battery_ext_len(&mut self, len: usize) {
        assert!(
            len <= self.battery_ext.len(),
            "battery_ext is {} bytes; the board asked for {len}",
            self.battery_ext.len(),
        );
        self.battery_ext_len = len;
    }

    /// The cart's whole battery as one slice: PRG-RAM, then whatever the board staged in
    /// [`Src::BatteryExt`].
    ///
    /// The two are adjacent by construction, so this borrows rather than assembling a copy. Board
    /// state staged in `BatteryExt` is only as fresh as the last
    /// [`Map::sync_battery`](crate::mapper::Map::sync_battery); go through
    /// [`Bus::sram`](crate::bus::Bus::sram) to be sure of it.
    pub fn sram(&self) -> &[u8] {
        &self.data[self.sram_range()]
    }

    /// The cart's whole battery, for restoring a save.
    pub fn sram_mut(&mut self) -> &mut [u8] {
        let range = self.sram_range();
        &mut self.data[range]
    }

    /// PRG-RAM and the used part of `battery_ext`, which `Memory::new` lays out back to back.
    //
    // Every region is a whole number of pages, so this is only gapless while PRG-RAM is a page
    // multiple - which every real size (2K, 8K, 32K, 64K) is. `sram_is_contiguous` pins it.
    const fn sram_range(&self) -> Range<usize> {
        let end = if self.battery_ext_len == 0 {
            self.prg_ram.end
        } else {
            self.battery_ext.start + self.battery_ext_len
        };
        self.prg_ram.start..end
    }

    /// The mutable tail of memory: PRG-RAM, CHR-RAM, CIRAM and ExRAM.
    ///
    /// Save states only need this; ROM is reattached from the loaded cart.
    pub fn ram(&self) -> &[u8] {
        &self.data[self.ram_start..]
    }

    /// The mutable tail of memory, for restoring a save state.
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.data[self.ram_start..]
    }

    /// Record which cart's ROM this arena holds. See [`Memory::restore_rom_from`].
    pub const fn set_rom_crc32(&mut self, crc32: u32) {
        self.rom_crc32 = crc32;
    }

    /// CRC32 of the cart's ROM, the same one the game database is keyed by.
    ///
    /// Says which game the arena belongs to, which offsets into it are only meaningful against.
    #[must_use]
    pub const fn rom_crc32(&self) -> u32 {
        self.rom_crc32
    }

    /// Whether the cart's RAM is battery-backed, and so survives a power cycle.
    #[must_use]
    pub const fn battery_backed(&self) -> bool {
        self.battery_backed
    }

    /// Record whether the cart's RAM is battery-backed.
    pub const fn set_battery_backed(&mut self, battery_backed: bool) {
        self.battery_backed = battery_backed;
    }

    /// Fill the cart's RAM as if the console had just been powered on: PRG-RAM, and CHR when the
    /// cart provides CHR-RAM rather than CHR-ROM.
    pub fn fill_ram(&mut self, state: RamState) {
        state.fill(self.region_mut(Src::PrgRam));
        if self.chr_writable {
            state.fill(self.region_mut(Src::Chr));
        }
    }

    /// Overwrite this arena with `src`, reusing the allocation it already has.
    ///
    /// The ROM half is left where it is when both arenas hold the same cart's - so a run-ahead
    /// snapshot, which is taken every frame and restored moments later, copies its game's RAM
    /// rather than its whole cartridge. Anything else falls back to a plain clone.
    pub fn snapshot_from(&mut self, src: &Self) {
        if !self.rom_present || !self.is_same_cart(src) {
            *self = src.clone();
            return;
        }
        self.data[self.ram_start..].copy_from_slice(&src.data[src.ram_start..]);
        self.prg_ram = src.prg_ram.clone();
        self.ciram = src.ciram.clone();
        self.ex_ram = src.ex_ram.clone();
        self.chr_writable = src.chr_writable;
        self.battery_backed = src.battery_backed;
        self.prg_pages = src.prg_pages;
        self.chr_pages = src.chr_pages;
    }

    /// Whether `other` was built from the same cart as this arena.
    ///
    /// The ROM's CRC is what says which *game* it is; the geometry is checked as well because it
    /// is what makes [`Memory::restore_rom_from`]'s copy sound, and because an arena built with no
    /// ROM to hash - `Cart::empty_sized`, and the tests on it - has nothing else to go on.
    #[must_use]
    pub fn is_same_cart(&self, other: &Self) -> bool {
        self.rom_crc32 == other.rom_crc32
            && self.data.len() == other.data.len()
            && self.ram_start == other.ram_start
            && self.prg_rom == other.prg_rom
            && self.chr == other.chr
    }

    /// Copy the immutable ROM half, and the rest of what the cart rather than the console
    /// decides, in from the running console's memory.
    ///
    /// Save states carry only the mutable tail, so a freshly deserialized `Memory` has a
    /// zero-filled ROM region until this puts it back. An arena that already has its ROM - a
    /// clone rather than a state read back - is left as it is.
    ///
    /// Returns `false` when `src` was not built from the same cart, in which case nothing is
    /// copied: applying the state would leave the console running one game's RAM against
    /// another's ROM.
    pub fn restore_rom_from(&mut self, src: &Self) -> bool {
        if !self.is_same_cart(src) {
            return false;
        }
        // Skipped for an arena that already holds the ROM, so that run-ahead - which restores a
        // clone of the running console every frame - does not pay a whole-cart memcpy for it.
        if !self.rom_present {
            self.data[..self.ram_start].copy_from_slice(&src.data[..src.ram_start]);
            self.rom_present = true;
        }
        self.battery_backed = src.battery_backed;
        true
    }

    /// How many bytes the arena holds, which is the range [`Memory::prg_offset`] returns.
    pub const fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the arena is empty, which is what a console with no cart has.
    pub const fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// PRG page table, for debuggers.
    pub const fn prg_pages(&self) -> &[Page; PRG_PAGES] {
        &self.prg_pages
    }

    /// CHR page table, for debuggers.
    pub const fn chr_pages(&self) -> &[Page; CHR_PAGES] {
        &self.chr_pages
    }

    /// Where the CPU `addr` reads from within mapped memory, or `None` if its page is unmapped. A
    /// 64K CPU address may map to different memory during an execution while a memory offset always
    /// maps to the same byte.
    ///
    /// Only valid until the board switches banks.
    pub const fn prg_offset(&self, addr: u16) -> Option<usize> {
        let page = self.prg_pages[(addr as usize >> PAGE_SHIFT) & PRG_PAGE_MASK];
        // Page 0 is the reserved zero-filled page, so an offset of 0 is unmapped rather than the
        // first byte of the arena.
        if page.offset() == 0 {
            None
        } else {
            Some(page.offset() | (addr as usize & PAGE_OFFSET_MASK))
        }
    }
}

/// A plain byte buffer with a `Debug` impl that reports its length instead of its contents, and
/// an `Index` impl that masks rather than panics.
///
/// Distinct from [`Memory`]: this is a container, not an address space. It backs the odd
/// board-private buffer that is not reachable through the page tables - so far only the Bandai
/// FCG EEPROMs.
#[derive(Default, Copy, Clone, Serialize, Deserialize)]
pub(crate) struct Buffer<D> {
    data: D,
}

impl Buffer<Box<[u8]>> {
    /// Create a zeroed `Buffer` of `size` bytes.
    //
    // `size` is expected to be a power of two: `Index` masks the index with `len - 1` rather than
    // bounds-checking it, so any other length wraps to the wrong byte. The one caller asks for
    // 128 or 256.
    pub(crate) fn new(size: usize) -> Self {
        debug_assert!(size.is_power_of_two(), "buffer size must be a power of two");
        Self {
            data: vec![0; size].into_boxed_slice(),
        }
    }
}

impl fmt::Debug for Buffer<Box<[u8]>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Buffer")
            .field("len", &self.data.len())
            .finish()
    }
}

impl<D> Deref for Buffer<D> {
    type Target = D;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<D: DerefMut> DerefMut for Buffer<D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T, D: AsRef<[T]>> AsRef<[T]> for Buffer<D> {
    fn as_ref(&self) -> &[T] {
        self.data.as_ref()
    }
}

impl<T, D: AsMut<[T]>> AsMut<[T]> for Buffer<D> {
    fn as_mut(&mut self) -> &mut [T] {
        self.data.as_mut()
    }
}

impl<T> Index<usize> for Buffer<Box<[T]>> {
    type Output = T;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        self.data.index(index & (self.data.len() - 1))
    }
}

impl<T> IndexMut<usize> for Buffer<Box<[T]>> {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.data.index_mut(index & (self.data.len() - 1))
    }
}

impl<T> Index<Range<usize>> for Buffer<Box<[T]>> {
    type Output = [T];

    #[inline]
    fn index(&self, range: Range<usize>) -> &Self::Output {
        self.data
            .index((range.start & (self.data.len() - 1))..range.end.min(self.len()))
    }
}

impl<T> IndexMut<Range<usize>> for Buffer<Box<[T]>> {
    #[inline]
    fn index_mut(&mut self, range: Range<usize>) -> &mut Self::Output {
        self.data
            .index_mut((range.start & (self.data.len() - 1))..range.end.min(self.len()))
    }
}

impl<T> Index<RangeInclusive<usize>> for Buffer<Box<[T]>> {
    type Output = [T];

    #[inline]
    fn index(&self, range: RangeInclusive<usize>) -> &Self::Output {
        self.data.index(
            (range.start() & (self.data.len() - 1))..=*range.end().min(&(self.data.len() - 1)),
        )
    }
}

impl<T> IndexMut<RangeInclusive<usize>> for Buffer<Box<[T]>> {
    #[inline]
    fn index_mut(&mut self, range: RangeInclusive<usize>) -> &mut Self::Output {
        self.data.index_mut(
            (range.start() & (self.data.len() - 1))..=*range.end().min(&(self.data.len() - 1)),
        )
    }
}

/// A fixed-size array that serializes as a slice, so a save state does not need `serde`'s
/// const-generic array support.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct ConstArray<T, const N: usize> {
    data: [T; N],
}

impl<T, const N: usize> ConstArray<T, N> {
    /// Create a new `ConstSlice` instance.
    pub fn new() -> Self
    where
        T: Default + Copy,
    {
        Self::default()
    }

    /// Create a new `ConstSlice` instance filled with `val`.
    pub const fn filled(val: T) -> Self
    where
        T: Copy,
    {
        Self { data: [val; N] }
    }
}

impl<T, const N: usize> fmt::Debug for ConstArray<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConstArray")
            .field("len", &self.data.len())
            .finish()
    }
}

impl<T: Default + Copy, const N: usize> Default for ConstArray<T, N> {
    fn default() -> Self {
        Self {
            data: [T::default(); N],
        }
    }
}

impl<T, const N: usize> From<[T; N]> for ConstArray<T, N> {
    fn from(data: [T; N]) -> Self {
        Self { data }
    }
}

impl<T, const N: usize> Deref for ConstArray<T, N> {
    type Target = [T; N];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T, const N: usize> DerefMut for ConstArray<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T, const N: usize> AsRef<[T]> for ConstArray<T, N> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        self.data.as_ref()
    }
}

impl<T, const N: usize> AsMut<[T]> for ConstArray<T, N> {
    #[inline]
    fn as_mut(&mut self) -> &mut [T] {
        self.data.as_mut()
    }
}

impl<T, const N: usize> Index<usize> for ConstArray<T, N> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.data.index(index & (N - 1))
    }
}

impl<T, const N: usize> IndexMut<usize> for ConstArray<T, N> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.data.index_mut(index & (N - 1))
    }
}

impl<T, const N: usize> Index<Range<usize>> for ConstArray<T, N> {
    type Output = [T];

    #[inline]
    fn index(&self, range: Range<usize>) -> &Self::Output {
        self.data.index(range.start & (N - 1)..range.end.min(N))
    }
}

impl<T, const N: usize> IndexMut<Range<usize>> for ConstArray<T, N> {
    #[inline]
    fn index_mut(&mut self, range: Range<usize>) -> &mut Self::Output {
        self.data.index_mut(range.start & (N - 1)..range.end.min(N))
    }
}

impl<T, const N: usize> Index<RangeInclusive<usize>> for ConstArray<T, N> {
    type Output = [T];

    #[inline]
    fn index(&self, range: RangeInclusive<usize>) -> &Self::Output {
        self.data
            .index(range.start() & (N - 1)..=*range.end().min(&(N - 1)))
    }
}

impl<T, const N: usize> IndexMut<RangeInclusive<usize>> for ConstArray<T, N> {
    #[inline]
    fn index_mut(&mut self, range: RangeInclusive<usize>) -> &mut Self::Output {
        self.data
            .index_mut(range.start() & (N - 1)..=*range.end().min(&(N - 1)))
    }
}

impl<T: Serialize, const N: usize> Serialize for ConstArray<T, N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_tuple(N)?;
        for item in &self.data {
            s.serialize_element(item)?;
        }
        s.end()
    }
}

impl<'de, T, const N: usize> Deserialize<'de> for ConstArray<T, N>
where
    T: Deserialize<'de> + Default + Copy,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ArrayVisitor<T, const N: usize>(PhantomData<T>);

        impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
        where
            T: Deserialize<'de> + Default + Copy,
        {
            type Value = [T; N];

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&format!("an array of length {N}"))
            }

            #[inline]
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut data = [T::default(); N];
                for data in &mut data {
                    match (seq.next_element())? {
                        Some(val) => *data = val,
                        None => return Err(serde::de::Error::invalid_length(N, &self)),
                    }
                }
                Ok(data)
            }
        }

        deserializer
            .deserialize_tuple(N, ArrayVisitor(PhantomData))
            .map(|data| Self { data })
    }
}

/// RAM in a given state on startup.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum RamState {
    /// Every byte zero, which is what most emulators do.
    #[default]
    AllZeros,
    /// Every byte $FF.
    AllOnes,
    /// Pseudo-random bytes, closest to a real console's power-on state.
    Random,
}

impl RamState {
    /// Return `RamState` options as a slice.
    pub const fn as_slice() -> &'static [Self] {
        &[Self::AllZeros, Self::AllOnes, Self::Random]
    }

    /// Return `RamState` as a `str`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AllZeros => "all-zeros",
            Self::AllOnes => "all-ones",
            Self::Random => "random",
        }
    }

    /// Fills data slice based on `RamState`.
    pub fn fill(&self, data: &mut [u8]) {
        match self {
            RamState::AllZeros => data.fill(0x00),
            RamState::AllOnes => data.fill(0xFF),
            RamState::Random => {
                rand::rng().fill_bytes(data);
            }
        }
    }
}

impl From<usize> for RamState {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::AllZeros,
            1 => Self::AllOnes,
            _ => Self::Random,
        }
    }
}

impl AsRef<str> for RamState {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for RamState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::AllZeros => "All $00",
            Self::AllOnes => "All $FF",
            Self::Random => "Random",
        };
        write!(f, "{s}")
    }
}

impl FromStr for RamState {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all-zeros" => Ok(Self::AllZeros),
            "all-ones" => Ok(Self::AllOnes),
            "random" => Ok(Self::Random),
            _ => Err("invalid RamState value. valid options: `all-zeros`, `all-ones`, or `random`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 64 KiB PRG-ROM, 8 KiB PRG-RAM, 32 KiB CHR-ROM, with each 1 KiB page filled with its own
    /// index so a read identifies which bank it came from.
    fn test_memory() -> Memory {
        let mut memory = Memory::new(MemoryLayout {
            prg_rom: 64 * 1024,
            prg_ram: 8 * 1024,
            chr: 32 * 1024,
            chr_writable: false,
            ..Default::default()
        });
        for (i, page) in memory
            .region_mut(Src::PrgRom)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            page.fill(i as u8);
        }
        for (i, page) in memory
            .region_mut(Src::Chr)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            page.fill(0x80 | i as u8);
        }
        memory
    }

    #[test]
    fn an_unmapped_address_has_no_arena_offset() {
        let memory = test_memory();
        assert_eq!(memory.prg_offset(0x8000), None);
    }

    #[test]
    fn an_offset_follows_the_bank_its_address_is_mapped_to() {
        let mut memory = test_memory();

        // An 8 KiB window, so the 64 KiB of PRG-ROM holds several banks to switch between.
        memory.map_prg(0x8000, 0x2000, 0, Src::PrgRom);
        let first = memory.prg_offset(0x8000).expect("mapped");
        // Within a page, the offset tracks the address.
        assert_eq!(memory.prg_offset(0x8001), Some(first + 1));

        // The same address after a bank switch is a different byte of the arena, which is why a
        // debugger keys what it knows to the offset rather than to the address.
        memory.map_prg(0x8000, 0x2000, 1, Src::PrgRom);
        let second = memory.prg_offset(0x8000).expect("mapped");
        assert_ne!(first, second);
    }

    #[test]
    fn unmapped_reads_zero_and_ignores_writes() {
        let mut memory = test_memory();
        assert_eq!(memory.prg_peek(0x8000), 0);
        assert_eq!(memory.chr_peek(0x0000), 0);

        memory.prg_write(0x8000, 0x42);
        memory.chr_write(0x0000, 0x42);
        assert_eq!(memory.prg_peek(0x8000), 0);
        assert_eq!(memory.chr_peek(0x0000), 0);
    }

    #[test]
    fn maps_prg_window_across_all_its_pages() {
        let mut memory = test_memory();
        // 16 KiB window at $8000 = PRG pages 0..16.
        memory.map_prg(0x8000, 16 * 1024, 0, Src::PrgRom);
        for i in 0..16 {
            let addr = 0x8000 + (i * PAGE_SIZE) as u16;
            assert_eq!(memory.prg_peek(addr), i as u8, "page {i}");
        }
    }

    #[test]
    fn negative_bank_selects_from_the_end() {
        let mut memory = test_memory();
        // 64 KiB of PRG in 16 KiB banks = 4 banks; -1 is bank 3, starting at page 48.
        memory.map_prg(0xC000, 16 * 1024, -1, Src::PrgRom);
        assert_eq!(memory.prg_peek(0xC000), 48);

        memory.map_prg(0xC000, 16 * 1024, -2, Src::PrgRom);
        assert_eq!(memory.prg_peek(0xC000), 32);
    }

    #[test]
    fn bank_index_wraps_within_the_region() {
        let mut memory = test_memory();
        // Only 4 banks exist, so bank 4 wraps to bank 0 and bank 5 to bank 1.
        memory.map_prg(0x8000, 16 * 1024, 4, Src::PrgRom);
        assert_eq!(memory.prg_peek(0x8000), 0);
        memory.map_prg(0x8000, 16 * 1024, 5, Src::PrgRom);
        assert_eq!(memory.prg_peek(0x8000), 16);
    }

    #[test]
    fn window_sizes_from_1k_to_32k() {
        let mut memory = test_memory();
        for size in [1, 2, 4, 8, 16, 32] {
            let size = size * 1024;
            memory.map_prg(0x8000, size, 1, Src::PrgRom);
            // Bank 1 of a `size` window starts at byte offset `size`.
            assert_eq!(
                memory.prg_peek(0x8000),
                (size >> PAGE_SHIFT) as u8,
                "{size} byte window"
            );
        }
    }

    #[test]
    fn chr_rom_is_not_writable_but_chr_ram_is() {
        let mut memory = test_memory();
        memory.map_chr(0x0000, 8 * 1024, 0, Src::Chr);
        memory.chr_write(0x0000, 0x42);
        assert_eq!(memory.chr_peek(0x0000), 0x80, "CHR-ROM must ignore writes");

        let mut memory = Memory::new(MemoryLayout {
            chr: 8 * 1024,
            chr_writable: true,
            ..Default::default()
        });
        memory.map_chr(0x0000, 8 * 1024, 0, Src::Chr);
        memory.chr_write(0x0000, 0x42);
        assert_eq!(memory.chr_peek(0x0000), 0x42, "CHR-RAM must accept writes");
    }

    #[test]
    fn prg_ram_write_protection() {
        let mut memory = test_memory();
        memory.map_prg(0x6000, 8 * 1024, 0, Src::PrgRam);
        memory.prg_write(0x6000, 0x42);
        assert_eq!(memory.prg_peek(0x6000), 0x42);

        memory.set_prg_writable(0x6000, 8 * 1024, false);
        memory.prg_write(0x6000, 0x99);
        assert_eq!(memory.prg_peek(0x6000), 0x42, "writes must be discarded");
        assert_eq!(memory.prg_peek(0x6000), 0x42, "reads must still work");
    }

    /// Write a distinct value into each nametable slot, then check which slots alias.
    fn nametable_aliases(mirroring: Mirroring) -> [u8; 4] {
        let mut memory = Memory::new(MemoryLayout {
            ciram: 4 * 1024,
            ..Default::default()
        });
        memory.set_mirroring(mirroring);
        for i in 0..4u16 {
            memory.chr_write(0x2000 + i * 0x400, i as u8 + 1);
        }
        [0, 1, 2, 3].map(|i| memory.chr_peek(0x2000 + i * 0x400))
    }

    #[test]
    fn mirroring_modes() {
        // Vertical: NT0/NT2 alias, NT1/NT3 alias. Writing 1,2,3,4 leaves 3,4,3,4.
        assert_eq!(nametable_aliases(Mirroring::Vertical), [3, 4, 3, 4]);
        // Horizontal: NT0/NT1 alias, NT2/NT3 alias.
        assert_eq!(nametable_aliases(Mirroring::Horizontal), [2, 2, 4, 4]);
        // Single screen: all four alias the same 1 KiB.
        assert_eq!(nametable_aliases(Mirroring::SingleScreenA), [4; 4]);
        assert_eq!(nametable_aliases(Mirroring::SingleScreenB), [4; 4]);
        // Four screen: all distinct.
        assert_eq!(nametable_aliases(Mirroring::FourScreen), [1, 2, 3, 4]);
    }

    #[test]
    fn single_screen_a_and_b_use_different_banks() {
        let mut memory = Memory::new(MemoryLayout::default());
        memory.set_mirroring(Mirroring::SingleScreenA);
        memory.chr_write(0x2000, 0xAA);
        memory.set_mirroring(Mirroring::SingleScreenB);
        memory.chr_write(0x2000, 0xBB);

        memory.set_mirroring(Mirroring::SingleScreenA);
        assert_eq!(memory.chr_peek(0x2000), 0xAA);
        memory.set_mirroring(Mirroring::SingleScreenB);
        assert_eq!(memory.chr_peek(0x2000), 0xBB);
    }

    #[test]
    fn nametables_mirror_into_3000_range() {
        let mut memory = Memory::new(MemoryLayout::default());
        memory.set_mirroring(Mirroring::Horizontal);
        memory.chr_write(0x2000, 0x5A);
        assert_eq!(memory.chr_peek(0x3000), 0x5A, "$3000 mirrors $2000");
        memory.chr_write(0x2800, 0xA5);
        assert_eq!(memory.chr_peek(0x3800), 0xA5, "$3800 mirrors $2800");
    }

    /// The whole point of Phase 5: a save state must not carry the cart's ROM.
    #[test]
    fn a_serialized_memory_carries_only_the_mutable_tail() {
        let mut memory = Memory::new(MemoryLayout {
            prg_rom: 256 * 1024,
            prg_ram: 8 * 1024,
            chr: 128 * 1024,
            chr_writable: false,
            ciram: 2 * 1024,
            ex_ram: 8 * 1024,
            ..Default::default()
        });
        memory.region_mut(Src::PrgRom).fill(0xAA);
        memory.region_mut(Src::PrgRam).fill(0x5A);
        memory.set_rom_crc32(0xDEAD_BEEF);

        let config = bincode::config::legacy();
        let bytes = bincode::serde::encode_to_vec(&memory, config).expect("serializes");
        assert!(
            bytes.len() < memory.data.len() / 4,
            "384 KiB of ROM must not be in the {} byte state",
            bytes.len()
        );

        let (mut restored, _) =
            bincode::serde::decode_from_slice::<Memory, _>(&bytes, config).expect("deserializes");
        assert!(
            restored.region_ref(Src::PrgRom).iter().all(|&b| b == 0),
            "ROM comes back empty"
        );
        assert_eq!(
            restored.region_ref(Src::PrgRam),
            memory.region_ref(Src::PrgRam)
        );

        assert!(restored.restore_rom_from(&memory), "same cart");
        assert_eq!(
            restored.region_ref(Src::PrgRom),
            memory.region_ref(Src::PrgRom)
        );
    }

    /// A state from another game must be refused, not left running one game's RAM on another's ROM.
    #[test]
    fn restoring_rom_from_a_different_cart_is_refused() {
        let small = Memory::new(MemoryLayout {
            prg_rom: 32 * 1024,
            prg_ram: 8 * 1024,
            chr: 8 * 1024,
            chr_writable: false,
            ciram: 2 * 1024,
            ex_ram: 8 * 1024,
            ..Default::default()
        });
        let mut large = Memory::new(MemoryLayout {
            prg_rom: 256 * 1024,
            prg_ram: 8 * 1024,
            chr: 8 * 1024,
            chr_writable: false,
            ciram: 2 * 1024,
            ex_ram: 8 * 1024,
            ..Default::default()
        });
        assert!(!large.restore_rom_from(&small), "different cart is refused");
    }

    /// Two games can want exactly the same allocation, so the ROM's CRC is what a state is
    /// matched by. Without it a state loads as a hybrid console: one game's RAM, another's ROM.
    #[test]
    fn restoring_rom_from_another_cart_of_the_same_shape_is_refused() {
        let layout = MemoryLayout {
            prg_rom: 32 * 1024,
            prg_ram: 8 * 1024,
            chr: 8 * 1024,
            chr_writable: false,
            ciram: 2 * 1024,
            ex_ram: 8 * 1024,
            ..Default::default()
        };
        let mut running = Memory::new(layout);
        running.set_rom_crc32(0x1111_1111);
        running.region_mut(Src::PrgRom).fill(0xAA);

        // Through serde, because that is what leaves a state with no ROM of its own.
        let config = bincode::config::legacy();
        let bytes = bincode::serde::encode_to_vec(&running, config).expect("serializes");
        let (mut state, _) =
            bincode::serde::decode_from_slice::<Memory, _>(&bytes, config).expect("deserializes");

        state.set_rom_crc32(0x2222_2222);
        assert!(
            !state.restore_rom_from(&running),
            "another game's state is refused"
        );
        assert!(
            state.region_ref(Src::PrgRom).iter().all(|&b| b == 0),
            "and nothing is copied into it"
        );

        state.set_rom_crc32(0x1111_1111);
        assert!(state.restore_rom_from(&running), "the same game's is not");
        assert!(state.region_ref(Src::PrgRom).iter().all(|&b| b == 0xAA));
    }

    /// A snapshot copies the RAM half and leaves the ROM half where it already is, so it has to
    /// come out equal to a clone - and fall back to one when there is nothing to reuse.
    #[test]
    fn a_snapshot_copies_the_ram_and_keeps_the_rom_it_already_has() {
        let mut running = Memory::new(MemoryLayout {
            prg_rom: 64 * 1024,
            prg_ram: 8 * 1024,
            chr: 8 * 1024,
            ..Default::default()
        });
        running.set_rom_crc32(0x1234_5678);
        running.region_mut(Src::PrgRom).fill(0xAA);
        running.region_mut(Src::PrgRam).fill(0x11);
        running.map_prg(0x8000, 32 * 1024, 1, Src::PrgRom);

        // A console that has already run this cart, one frame behind.
        let mut snapshot = running.clone();
        running.region_mut(Src::PrgRam).fill(0x22);
        running.map_prg(0x8000, 32 * 1024, 0, Src::PrgRom);
        snapshot.snapshot_from(&running);

        assert_eq!(
            snapshot.region_ref(Src::PrgRam),
            running.region_ref(Src::PrgRam),
            "the game's RAM comes across"
        );
        assert_eq!(
            snapshot.region_ref(Src::PrgRom),
            running.region_ref(Src::PrgRom),
            "the ROM it already had is still there"
        );
        assert_eq!(
            snapshot.prg_pages(),
            running.prg_pages(),
            "with the page tables that address it"
        );

        // A cart it has never run has nothing to reuse.
        let mut other = Memory::new(MemoryLayout {
            prg_rom: 32 * 1024,
            ..Default::default()
        });
        other.snapshot_from(&running);
        assert!(other.is_same_cart(&running), "so it takes a whole copy");
        assert_eq!(
            other.region_ref(Src::PrgRom),
            running.region_ref(Src::PrgRom)
        );
    }

    /// Truncated or corrupt input must be an error rather than a panic writing the RAM tail.
    #[test]
    fn an_inconsistent_memory_state_fails_to_deserialize() {
        let memory = Memory::new(MemoryLayout {
            prg_rom: 32 * 1024,
            prg_ram: 8 * 1024,
            chr: 8 * 1024,
            chr_writable: false,
            ciram: 2 * 1024,
            ex_ram: 8 * 1024,
            ..Default::default()
        });
        let config = bincode::config::legacy();
        let mut bytes = bincode::serde::encode_to_vec(&memory, config).expect("serializes");
        // Claim a larger allocation than the RAM tail that follows.
        bytes[..8].copy_from_slice(&(memory.data.len() as u64 * 2).to_le_bytes());
        assert!(
            bincode::serde::decode_from_slice::<Memory, _>(&bytes, config).is_err(),
            "an inconsistent state must not decode"
        );
    }

    #[test]
    fn rom_is_below_ram_start_so_save_states_can_skip_it() {
        let mut memory = test_memory();
        memory.map_prg(0x6000, 8 * 1024, 0, Src::PrgRam);
        memory.prg_write(0x6000, 0x42);

        // PRG-RAM is in the mutable tail, PRG-ROM is not.
        assert!(memory.ram().contains(&0x42));
        assert_eq!(memory.ram().len(), memory.data.len() - memory.ram_start);
        assert!(memory.prg_rom.end <= memory.ram_start);
        assert!(memory.prg_ram.start >= memory.ram_start);
    }

    /// Games that read past the end of their own banks must wrap rather than panic. This is the
    /// behaviour `mem::Memory<D>`'s masking `Index` impl provided, reproduced here by wrapping
    /// page offsets at map time so the read path stays free of bounds masking.
    #[test]
    fn window_larger_than_region_repeats_instead_of_overrunning() {
        let mut memory = Memory::new(MemoryLayout {
            chr: 2 * 1024,
            ..Default::default()
        });
        for (i, page) in memory
            .region_mut(Src::Chr)
            .chunks_mut(PAGE_SIZE)
            .enumerate()
        {
            page.fill(i as u8 + 1);
        }

        // An 8 KiB window over a 2 KiB region: must repeat the region four times, and critically
        // must not index past it.
        memory.map_chr(0x0000, 8 * 1024, 0, Src::Chr);
        let read: Vec<u8> = (0..8).map(|i| memory.chr_peek(i * 1024)).collect();
        assert_eq!(read, vec![1, 2, 1, 2, 1, 2, 1, 2]);
    }

    #[test]
    fn every_addressable_byte_is_in_bounds_for_odd_region_sizes() {
        // Deliberately awkward sizes: not powers of two, not multiples of the page size.
        for size in [1, 100, 1024, 1500, 3 * 1024, 5 * 1024, 40 * 1024] {
            let mut memory = Memory::new(MemoryLayout {
                prg_rom: size,
                chr: size,
                ..Default::default()
            });
            for bank in [-2, -1, 0, 1, 7, 1000] {
                for window in [1024, 8 * 1024, 16 * 1024, 32 * 1024] {
                    memory.map_prg(0x8000, window, bank, Src::PrgRom);
                    memory.map_chr(0x0000, window.min(8 * 1024), bank, Src::Chr);
                    // Sweeping the whole address space must never panic.
                    for addr in (0..=0xFFFFu32).step_by(64) {
                        let _ = memory.prg_peek(addr as u16);
                    }
                    for addr in (0..0x4000u32).step_by(64) {
                        let _ = memory.chr_peek(addr as u16);
                    }
                }
            }
        }
    }

    #[test]
    fn regions_are_whole_pages_so_offsets_always_have_a_full_page_behind_them() {
        let memory = Memory::new(MemoryLayout {
            prg_rom: 1500,
            prg_ram: 100,
            chr: 1,
            ex_ram: 1023,
            ..Default::default()
        });
        for src in [Src::PrgRom, Src::PrgRam, Src::Chr, Src::CiRam, Src::ExRam] {
            let len = memory.region_ref(src).len();
            assert_eq!(len % PAGE_SIZE, 0, "{src:?} region must be whole pages");
            assert!(len > 0, "{src:?} region must be at least one page");
        }
    }

    #[test]
    fn debug_does_not_print_contents() {
        let memory = Memory::new(MemoryLayout {
            prg_rom: 64 * 1024,
            ..Default::default()
        });
        let debug = format!("{memory:?}");
        assert!(debug.len() < 512, "Debug must summarize, not dump: {debug}");
        assert!(debug.contains("prg_rom"));
    }

    #[test]
    fn empty_region_maps_as_unmapped() {
        let mut memory = Memory::new(MemoryLayout::default());
        // No ExRAM allocated, so mapping it must not panic and must read as unmapped.
        memory.map_prg(0x8000, 8 * 1024, 0, Src::ExRam);
        assert_eq!(memory.prg_peek(0x8000), 0);
        memory.prg_write(0x8000, 0x42);
        assert_eq!(memory.prg_peek(0x8000), 0);
    }

    /// `sram` borrows one span rather than assembling a copy, which only holds while PRG-RAM and
    /// `BatteryExt` are adjacent with no page padding between them. Every real PRG-RAM size is a
    /// page multiple, so this is a claim about the allocator rather than about any one board.
    #[test]
    fn sram_is_contiguous() {
        for prg_ram in [2 * 1024, 8 * 1024, 32 * 1024, 64 * 1024] {
            let mut memory = Memory::new(MemoryLayout {
                prg_ram,
                battery_ext: 0x80,
                ..Default::default()
            });
            memory.set_battery_ext_len(0x80);

            memory.region_mut(Src::PrgRam).fill(0xAA);
            memory.region_mut(Src::BatteryExt).fill(0xBB);

            let sram = memory.sram();
            assert_eq!(sram.len(), prg_ram + 0x80, "prg_ram {prg_ram}");
            assert_eq!(sram[prg_ram - 1], 0xAA, "PRG-RAM runs to its end");
            assert_eq!(sram[prg_ram], 0xBB, "and the board's tail starts there");
        }
    }

    /// A board that never declares a tail gets PRG-RAM alone, which is what a `.srm` from any
    /// other emulator holds.
    #[test]
    fn sram_without_a_battery_tail_is_just_prg_ram() {
        let memory = Memory::new(MemoryLayout {
            prg_ram: 8 * 1024,
            battery_ext: 0x80,
            ..Default::default()
        });
        assert_eq!(memory.sram().len(), 8 * 1024);
    }
}
