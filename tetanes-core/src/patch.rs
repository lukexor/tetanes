//! Read substitution: what a cheat is, whichever way the user expressed it.
//!
//! A Game Genie sits between the CPU and the cartridge and rewrites the byte coming back. It never
//! writes memory, which is why it can patch ROM at all, and it is also why one mechanism covers
//! every kind of cheat: a RAM "cheat" that substitutes on read holds its value against the game
//! rather than flickering as a once-a-frame poke would.
//!
//! So [`GenieCode`] is a codec rather than a mechanism - six or eight letters in, one [`Patch`]
//! out - and [`Patches`] is the table the bus consults.

use crate::genie::GenieCode;
use std::collections::HashMap;

/// A value substituted for whatever the bus would otherwise return at an address.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct Patch {
    /// CPU address whose reads are substituted.
    pub addr: u16,
    /// Value to return.
    pub data: u8,
    /// Substitute only when the byte already there matches - the Game Genie's compare byte.
    pub compare: Option<u8>,
}

impl Patch {
    /// A patch that returns `data` at `addr`, or only where the byte there is `compare`.
    pub const fn new(addr: u16, data: u8, compare: Option<u8>) -> Self {
        Self {
            addr,
            data,
            compare,
        }
    }

    /// Applies this patch to a value read from its address.
    #[inline(always)]
    pub const fn read(&self, val: u8) -> u8 {
        match self.compare {
            Some(compare) if val != compare => val,
            _ => self.data,
        }
    }

    /// The Game Genie code for this patch, where one exists.
    ///
    /// `None` below `$8000`, which the Game Genie's 15-bit address field cannot reach.
    #[must_use]
    pub fn genie_code(&self) -> Option<GenieCode> {
        GenieCode::encode(self.addr, self.data, self.compare)
    }
}

impl From<&GenieCode> for Patch {
    fn from(code: &GenieCode) -> Self {
        Self::new(code.addr(), code.data(), code.compare())
    }
}

/// Every patch currently applied, keyed by the address it substitutes.
///
/// One patch per address: a second at the same address replaces the first, as two Game Genie codes
/// for the same byte would on real hardware.
#[derive(Default, Debug, Clone)]
#[must_use]
pub struct Patches {
    /// One bit per 1 KiB of the CPU address space: is there any patch in this page?
    //
    // Every CPU read consults this, opcode and operand fetches included, so the common answer has
    // to be cheap to reach: eight bytes and a shift-and-mask, rather than hashing the address into
    // `by_addr` on reads that will miss. Cheats cluster into a handful of pages, so most addresses
    // never reach the map at all.
    page_mask: u64,
    by_addr: HashMap<u16, Patch>,
}

impl Patches {
    /// Address bits that select a page of [`Patches::page_mask`]. 1 KiB granularity, so the 64
    /// KiB CPU address space fits one `u64`.
    const PAGE_SHIFT: u16 = 10;

    /// Applies whichever patch covers `addr`, or returns `val` unchanged.
    #[inline(always)]
    pub fn read(&self, addr: u16, val: u8) -> u8 {
        if self.page_mask & (1 << (addr >> Self::PAGE_SHIFT)) == 0 {
            return val;
        }
        self.by_addr.get(&addr).map_or(val, |patch| patch.read(val))
    }

    /// Applies a patch, replacing any already at its address.
    pub fn insert(&mut self, patch: Patch) {
        self.page_mask |= 1 << (patch.addr >> Self::PAGE_SHIFT);
        self.by_addr.insert(patch.addr, patch);
    }

    /// Removes the patch at `addr`, if any.
    pub fn remove(&mut self, addr: u16) {
        if self.by_addr.remove(&addr).is_some() {
            // Another patch may share the page, so the bit cannot simply be cleared.
            self.rebuild_page_mask();
        }
    }

    /// Removes every patch.
    pub fn clear(&mut self) {
        self.by_addr.clear();
        self.page_mask = 0;
    }

    /// Whether any patch is applied.
    pub fn is_empty(&self) -> bool {
        self.by_addr.is_empty()
    }

    /// How many patches are applied.
    pub fn len(&self) -> usize {
        self.by_addr.len()
    }

    /// Every patch, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &Patch> {
        self.by_addr.values()
    }

    fn rebuild_page_mask(&mut self) {
        self.page_mask = self
            .by_addr
            .keys()
            .fold(0, |mask, addr| mask | 1 << (addr >> Self::PAGE_SHIFT));
    }
}

impl Extend<Patch> for Patches {
    fn extend<T: IntoIterator<Item = Patch>>(&mut self, iter: T) {
        for patch in iter {
            self.insert(patch);
        }
    }
}

impl FromIterator<Patch> for Patches {
    fn from_iter<T: IntoIterator<Item = Patch>>(iter: T) -> Self {
        let mut patches = Self::default();
        patches.extend(iter);
        patches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_substitutes_and_a_compare_patch_only_on_a_match() {
        let always = Patch::new(0x8000, 0xAA, None);
        assert_eq!(always.read(0x00), 0xAA);
        assert_eq!(always.read(0xFF), 0xAA);

        let only_if = Patch::new(0x8000, 0xAA, Some(0x42));
        assert_eq!(only_if.read(0x42), 0xAA, "the byte it was told to replace");
        assert_eq!(only_if.read(0x43), 0x43, "and nothing else");
    }

    /// The page mask is what keeps a loaded cheat off every other read, so it has to be right in
    /// both directions - a page it clears is a page the map is never consulted for.
    #[test]
    fn the_page_mask_tracks_which_pages_hold_patches() {
        let mut patches = Patches::default();
        assert_eq!(
            patches.read(0x0010, 0x11),
            0x11,
            "empty patches substitute nothing"
        );

        patches.insert(Patch::new(0x0010, 0xAA, None));
        patches.insert(Patch::new(0x0020, 0xBB, None));
        patches.insert(Patch::new(0x8000, 0xCC, None));
        assert_eq!(patches.read(0x0010, 0x11), 0xAA);
        assert_eq!(patches.read(0x8000, 0x11), 0xCC);
        assert_eq!(patches.read(0x0011, 0x11), 0x11, "same page, no patch");
        assert_eq!(patches.read(0x4000, 0x11), 0x11, "page with nothing in it");

        // Its page still holds `0x0020`, so the mask must not have cleared it.
        patches.remove(0x0010);
        assert_eq!(patches.read(0x0010, 0x11), 0x11);
        assert_eq!(patches.read(0x0020, 0x11), 0xBB);

        patches.remove(0x0020);
        assert_eq!(patches.read(0x0020, 0x11), 0x11);
        assert_eq!(
            patches.read(0x8000, 0x11),
            0xCC,
            "a different page is untouched"
        );

        patches.clear();
        assert!(patches.is_empty());
        assert_eq!(patches.read(0x8000, 0x11), 0x11);
    }

    #[test]
    fn a_second_patch_at_one_address_replaces_the_first() {
        let mut patches = Patches::default();
        patches.insert(Patch::new(0x8000, 0xAA, None));
        patches.insert(Patch::new(0x8000, 0xBB, None));
        assert_eq!(patches.len(), 1);
        assert_eq!(patches.read(0x8000, 0x11), 0xBB);
    }

    /// A patch in ROM space carries a code; one in RAM is why `Patch` exists separately.
    #[test]
    fn only_a_rom_address_has_a_genie_code() {
        assert!(Patch::new(0x8000, 0xAA, None).genie_code().is_some());
        assert!(Patch::new(0x7FFF, 0xAA, None).genie_code().is_none());
        assert!(Patch::new(0x0010, 0xAA, None).genie_code().is_none());
    }
}
