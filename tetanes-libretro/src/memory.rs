//! The console's memory, as a frontend expects to see it.
//!
//! `retro_get_memory_data` hands back a pointer the frontend keeps: it reads through it every frame
//! for cheat search and achievements, and writes through it to restore a battery file. That pointer
//! therefore has to outlive anything the console does to itself - and it cannot point into the
//! console, because restoring a save state swaps the whole `Bus`, leaving `Memory`'s arena at a
//! different address.
//!
//! So the core owns the buffers and copies. Two of at most 10 KiB, twice a frame, against a frame
//! that is milliseconds long.

use tetanes_core::{bus::size, control_deck::ControlDeck};

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
        let mut deck = ControlDeck::with_config(Config {
            sram_dir: None,
            ram_state: RamState::AllZeros,
            filter: VideoFilter::Pixellate,
            ..Default::default()
        });
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
