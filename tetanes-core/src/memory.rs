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
//! shares: [`Buffer`] and [`ConstArray`] for plain byte storage, the [`Read`]/[`Write`] bus traits,
//! and [`RamState`] for power-on fill.

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
use tracing::warn;

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
pub enum Src {
    PrgRom,
    PrgRam,
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
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Memory {
    data: Box<[u8]>,
    /// Offset at which mutable regions begin. Everything below this is ROM and never changes, so
    /// save states only need `data[ram_start..]`.
    ram_start: usize,
    prg_rom: Range<usize>,
    prg_ram: Range<usize>,
    chr: Range<usize>,
    chr_writable: bool,
    ciram: Range<usize>,
    ex_ram: Range<usize>,
    // Page tables are derived state, rebuilt by replaying mapper register state on load, so they
    // are not serialized. Serde also has no derive for arrays longer than 32.
    #[serde(skip, default = "Memory::unmapped_prg_pages")]
    prg_pages: [Page; PRG_PAGES],
    #[serde(skip, default = "Memory::unmapped_chr_pages")]
    chr_pages: [Page; CHR_PAGES],
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
pub struct MemoryLayout {
    pub prg_rom: usize,
    pub prg_ram: usize,
    pub chr: usize,
    /// Whether the CHR region is RAM. CHR-ROM is placed with the other immutable regions and is
    /// excluded from save states.
    pub chr_writable: bool,
    /// Nametable RAM. Defaults to 2 KiB when zero; four-screen boards supply 4 KiB.
    pub ciram: usize,
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

        let prg_ram = alloc(layout.prg_ram, &mut offset);
        let chr_ram = layout.chr_writable.then(|| alloc(layout.chr, &mut offset));
        let ciram = alloc(ciram, &mut offset);
        let ex_ram = alloc(layout.ex_ram, &mut offset);

        let mut memory = Self {
            data: vec![0; offset].into_boxed_slice(),
            ram_start,
            prg_rom,
            prg_ram,
            chr: chr_rom.or(chr_ram).unwrap_or(0..0),
            chr_writable: layout.chr_writable,
            ciram,
            ex_ram,
            prg_pages: [Page::UNMAPPED; PRG_PAGES],
            chr_pages: [Page::UNMAPPED; CHR_PAGES],
        };
        // CIRAM is console-internal, not part of the cart, so the nametables are readable before
        // any board has mapped anything. Boards that route them elsewhere - MMC5 picks a source
        // per nametable - overwrite these entries in their `sync`.
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
    /// `data`. This matters beyond tidiness: several games read past the end of their own banks,
    /// and the old `Memory<D>` newtype existed partly to mask those accesses rather than panic.
    /// Wrapping here preserves that behaviour without a mask on every read.
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
            Src::Chr => self.chr.clone(),
            Src::CiRam => self.ciram.clone(),
            Src::ExRam => self.ex_ram.clone(),
        }
    }

    const fn is_writable(&self, src: Src) -> bool {
        match src {
            Src::PrgRom => false,
            Src::Chr => self.chr_writable,
            Src::PrgRam | Src::CiRam | Src::ExRam => true,
        }
    }

    /// Read a byte from a region by raw offset, wrapping within the region.
    ///
    /// For the rare access a page entry cannot express: MMC5's extended-attribute mode picks a
    /// 4 KiB CHR bank per *tile* from a byte of ExRAM, so the bank is not known until the fetch.
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

    /// PRG page table, for debuggers.
    pub const fn prg_pages(&self) -> &[Page; PRG_PAGES] {
        &self.prg_pages
    }

    /// CHR page table, for debuggers.
    pub const fn chr_pages(&self) -> &[Page; CHR_PAGES] {
        &self.chr_pages
    }
}

/// A plain byte buffer with a `Debug` impl that reports its length instead of its contents, and
/// an `Index` impl that masks rather than panics.
///
/// Distinct from [`Memory`]: this is a container, not an address space. It backs the console's own
/// RAM and the odd board-private buffer, neither of which is reachable through the page tables.
#[derive(Default, Copy, Clone, Serialize, Deserialize)]
pub struct Buffer<D> {
    data: D,
}

impl Buffer<Box<[u8]>> {
    /// Create an empty `Buffer`.
    pub fn empty() -> Self {
        Self {
            data: Vec::new().into_boxed_slice(),
        }
    }

    /// Create a zeroed `Buffer` of `size` bytes.
    pub fn new(mut size: usize) -> Self {
        if size > 0 && !size.is_power_of_two() {
            warn!("memory size {size} must be a power of two");
            size = size.next_power_of_two();
        }
        Self {
            data: vec![0; size].into_boxed_slice(),
        }
    }

    pub fn with_ram_state(size: usize, state: RamState) -> Self {
        let mut mem = Self::new(size);
        state.fill(&mut mem.data);
        mem
    }

    /// Shortens the `Buffer` by keeping the first `size` bytes and dropping the rest.
    pub fn truncate(&mut self, size: usize) {
        let mut data = std::mem::take(&mut self.data).to_vec();
        data.truncate(size);
        self.data = data.into_boxed_slice();
    }
}

impl<T, const N: usize> Buffer<ConstArray<T, N>> {
    /// Create a zeroed `Buffer`.
    pub fn new_const() -> Self
    where
        T: Default + Copy,
    {
        Self::default()
    }
}

impl<const N: usize> Buffer<ConstArray<u8, N>> {
    /// Fill memory based on [`RamState`].
    pub fn with_ram_state_const(state: RamState) -> Self {
        let mut mem = Self::default();
        state.fill(&mut *mem.data);
        mem
    }
}

impl fmt::Debug for Buffer<Box<[u8]>> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Buffer")
            .field("len", &self.data.len())
            .finish()
    }
}

impl<T, const N: usize> fmt::Debug for Buffer<ConstArray<T, N>> {
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

/// A trait that represents memory read operations. Reads typically have side-effects.
pub trait Read {
    /// Read from the given address.
    #[inline(always)]
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    /// Peek from the given address.
    fn peek(&self, addr: u16) -> u8;
}

/// A trait that represents memory write operations.
pub trait Write {
    /// Write value to the given address.
    fn write(&mut self, addr: u16, val: u8);
}

/// RAM in a given state on startup.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum RamState {
    #[default]
    AllZeros,
    AllOnes,
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
}
