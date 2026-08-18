//! The console's memory, as a frontend expects to see it.
//!
//! `retro_get_memory_data` hands back a pointer the frontend keeps: it reads through it every frame
//! for cheat search and achievements, and writes through it to restore a battery file. That pointer
//! therefore has to outlive anything the console does to itself - and it cannot point into the
//! console, because restoring a save state swaps the whole `Bus`, leaving `Memory`'s arena at a
//! different address.
//!
//! So the core owns the buffers and copies. The console's 2 KiB always, plus the cart's battery
//! where it has one - 8 KiB on an ordinary board, and 64 KiB on the largest (MMC5 banks its
//! PRG-RAM and is emulated as one block). Twice a frame, against a frame that is milliseconds
//! long.

use crate::{
    log,
    sys::{
        RETRO_ENVIRONMENT_SET_MEMORY_MAPS, RETRO_MEMDESC_SAVE_RAM, RETRO_MEMDESC_SYSTEM_RAM,
        retro_environment_t, retro_memory_descriptor, retro_memory_map,
    },
};
use std::ffi::c_void;
use tetanes_core::{bus::size, control_deck::ControlDeck};

/// The window each region is decoded in: address bits 15-13, so `(addr & 0xE000) == start`.
///
/// Work RAM is 2 KiB inside an 8 KiB window and so is mirrored four times across `$0000-$1FFF`,
/// which is what the hardware does and what one descriptor can therefore say.
const SELECT: usize = 0xE000;

/// Where the cartridge's battery is decoded, and how much of it the CPU can see.
///
/// A board whose battery is larger than the window is described only as far as `$7FFF`, because
/// the map describes the CPU's address space rather than the save file. That covers the boards
/// which bank PRG-RAM behind `$6000` - MMC5's 64 KiB, SOROM/SXROM's and FK23C's 32 KiB - and the
/// ones whose battery covers something other than PRG-RAM entirely.
const SAVE_RAM_START: usize = 0x6000;
const SAVE_RAM_WINDOW: usize = 0x2000;

/// Buffers the frontend holds pointers into.
pub struct Memory {
    /// The cart's battery, empty when it has none.
    save_ram: Vec<u8>,
    /// The console's own 2K.
    system_ram: [u8; size::WRAM],
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            save_ram: Vec::new(),
            system_ram: [0; size::WRAM],
        }
    }
}

impl Memory {
    /// Sizes the battery buffer for the cart that just loaded, and fills both from the console.
    pub fn attach(&mut self, deck: &mut ControlDeck) {
        self.save_ram = deck.sram().to_vec();
        self.refresh(deck);
    }

    /// Drops the battery buffer, so a frontend asking after the cart is gone gets nothing rather
    /// than the last cart's saves.
    pub fn detach(&mut self) {
        self.save_ram = Vec::new();
        self.system_ram = [0; size::WRAM];
    }

    /// Copies the console's memory out, after it has run.
    pub fn refresh(&mut self, deck: &mut ControlDeck) {
        self.system_ram.copy_from_slice(deck.wram());
        if !self.save_ram.is_empty() {
            self.save_ram.copy_from_slice(deck.sram());
        }
    }

    /// Copies the frontend's writes back in, before the console runs.
    ///
    /// This is how a restored `.srm` and a cheat engine's pokes reach the emulation - the frontend
    /// simply writes through the pointer and says nothing.
    pub fn commit(&mut self, deck: &mut ControlDeck) {
        deck.wram_mut().copy_from_slice(&self.system_ram);
        if !self.save_ram.is_empty() {
            deck.set_sram(&self.save_ram);
        }
    }

    /// Describes where the console's memory sits in the CPU's address space.
    ///
    /// Separate from `retro_get_memory_data`, which hands over a bare buffer: a cheat search wants
    /// the buffer, while RetroAchievements' NES model addresses memory the way the game does, and
    /// only the map says that `$0000` and `$0800` are the same byte.
    ///
    /// # Safety
    ///
    /// The environment callback must be valid. The descriptors point into this `Memory`, which
    /// must therefore outlive the frontend's use of the map - it lives in the `Core`, so it does.
    pub unsafe fn describe(&mut self, environment: retro_environment_t) {
        // The frontend copies the descriptors during the call, which is why this array may be a
        // local; what it keeps is the pointers inside them.
        let descriptors = [
            retro_memory_descriptor {
                flags: RETRO_MEMDESC_SYSTEM_RAM,
                ptr: self.system_ram.as_mut_ptr().cast::<c_void>(),
                offset: 0,
                start: 0x0000,
                select: SELECT,
                disconnect: 0,
                len: size::WRAM,
                addrspace: std::ptr::null(),
            },
            retro_memory_descriptor {
                flags: RETRO_MEMDESC_SAVE_RAM,
                ptr: self.save_ram.as_mut_ptr().cast::<c_void>(),
                offset: 0,
                start: SAVE_RAM_START,
                select: SELECT,
                disconnect: 0,
                len: self.save_ram.len().min(SAVE_RAM_WINDOW),
                addrspace: std::ptr::null(),
            },
        ];
        // A cart with no battery gets one descriptor, not a second of zero length.
        let count = if self.save_ram.is_empty() { 1 } else { 2 };
        let map = retro_memory_map {
            descriptors: descriptors.as_ptr(),
            num_descriptors: count,
        };
        // SAFETY: the callback reads `num_descriptors` descriptors through the pointer, and both
        // outlive the call.
        let ok = unsafe {
            environment(
                RETRO_ENVIRONMENT_SET_MEMORY_MAPS,
                std::ptr::from_ref(&map).cast_mut().cast::<c_void>(),
            )
        };
        if !ok {
            // Not fatal: it costs achievements and the frontend's memory viewer, not emulation.
            log::info("the frontend does not take memory maps");
        }
    }

    /// The buffer for a `RETRO_MEMORY_*` id, or `None` for one this console has no answer for.
    pub fn region(&mut self, id: u32) -> Option<&mut [u8]> {
        match id {
            crate::sys::RETRO_MEMORY_SAVE_RAM if !self.save_ram.is_empty() => {
                Some(self.save_ram.as_mut_slice())
            }
            crate::sys::RETRO_MEMORY_SYSTEM_RAM => Some(self.system_ram.as_mut_slice()),
            // No RTC on a NES cart, and VRAM is banked through the board rather than being one
            // flat region - handing over CIRAM alone would describe half the picture.
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetanes_core::{control_deck::Config, memory::RamState, video::VideoFilter};

    fn deck_with(rom: &[u8]) -> ControlDeck {
        let mut deck = ControlDeck::with_config(
            Config::default()
                .with_sram_dir(None)
                .with_ram_state(RamState::AllZeros)
                .with_filter(VideoFilter::Pixellate),
        );
        deck.load_rom("test", &mut &rom[..]).expect("loads");
        deck
    }

    /// Zelda has a battery; the test ROMs mostly do not, and a cart without one must not present a
    /// save-RAM region at all.
    #[test]
    fn a_cart_without_a_battery_offers_no_save_ram() {
        const ROM: &[u8] = include_bytes!("../../tetanes-core/test_roms/spritecans.nes");
        let mut deck = deck_with(ROM);
        let mut memory = Memory::default();
        memory.attach(&mut deck);

        assert!(memory.region(crate::sys::RETRO_MEMORY_SAVE_RAM).is_none());
        assert_eq!(
            memory
                .region(crate::sys::RETRO_MEMORY_SYSTEM_RAM)
                .map(|region| region.len()),
            Some(size::WRAM)
        );
        assert!(memory.region(crate::sys::RETRO_MEMORY_RTC).is_none());
        assert!(memory.region(crate::sys::RETRO_MEMORY_VIDEO_RAM).is_none());
    }

    /// What the frontend writes has to reach the console, and what the console does has to come
    /// back - that round trip is the whole point of the shadow.
    #[test]
    fn writes_reach_the_console_and_changes_come_back() {
        const ROM: &[u8] = include_bytes!("../../tetanes-core/test_roms/spritecans.nes");
        let mut deck = deck_with(ROM);
        let mut memory = Memory::default();
        memory.attach(&mut deck);

        let wram = memory
            .region(crate::sys::RETRO_MEMORY_SYSTEM_RAM)
            .expect("work RAM");
        wram[0x10] = 0x5A;
        memory.commit(&mut deck);
        assert_eq!(deck.wram()[0x10], 0x5A, "the frontend's write landed");

        deck.wram_mut()[0x11] = 0xA5;
        memory.refresh(&mut deck);
        let wram = memory
            .region(crate::sys::RETRO_MEMORY_SYSTEM_RAM)
            .expect("work RAM");
        assert_eq!(wram[0x11], 0xA5, "and the console's is visible");
    }

    /// The pointer a frontend caches must survive a save state, which is what rules out pointing
    /// into the console: restoring swaps the whole `Bus`.
    #[test]
    fn the_buffers_do_not_move_across_a_state_restore() {
        const ROM: &[u8] = include_bytes!("../../tetanes-core/test_roms/spritecans.nes");
        let mut deck = deck_with(ROM);
        let mut memory = Memory::default();
        memory.attach(&mut deck);

        let before = memory
            .region(crate::sys::RETRO_MEMORY_SYSTEM_RAM)
            .expect("work RAM")
            .as_ptr();

        let mut state = vec![0; deck.serialized_state_len().expect("sizes")];
        deck.serialize_state_into(&mut state).expect("serializes");
        deck.deserialize_state(&state).expect("restores");
        memory.refresh(&mut deck);

        let after = memory
            .region(crate::sys::RETRO_MEMORY_SYSTEM_RAM)
            .expect("work RAM")
            .as_ptr();
        assert_eq!(before, after, "the frontend's pointer is still good");
    }
}
