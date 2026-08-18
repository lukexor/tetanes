//! Save states, in the container a frontend keeps them in.
//!
//! A frontend asks for one size, once, and then hands over a buffer of exactly that size for every
//! state it takes - a save state, a rewind frame, and the save-and-restore run-ahead does around
//! each frame. `retro_serialize_size` may never grow, so the size is measured when the cart loads
//! and padded: the buffer is larger than the state that goes in it, and the slack is zeroed,
//! because netplay CRCs the whole buffer and uninitialised padding reads as a desync.

use crate::log;
use tetanes_core::control_deck::ControlDeck;

/// Marks a buffer as this core's, so a state belonging to another core - or a file that is not a
/// state at all - is refused instead of being handed to the decoder.
///
/// The trailing `0x1A` makes it an even 8 bytes, and ends a DOS text stream, so `cat`ing a state
/// stops at the header rather than spraying the terminal.
const MAGIC: &[u8; 8] = b"TETANES\x1a";

/// The version of everything after the magic: this header, and the encoding of the payload under
/// it. Bumping it rejects every older state, which is the honest answer when they can no longer be
/// read.
const FORMAT: u32 = 1;

/// Where [`FORMAT`] sits, and where the payload's length sits after it. Both little-endian `u32`s.
const FORMAT_AT: usize = MAGIC.len();
const LENGTH_AT: usize = FORMAT_AT + size_of::<u32>();

/// Magic, format, payload length.
const HEADER: usize = LENGTH_AT + size_of::<u32>();

/// Slack over the measured state, so a state that grows by a little still fits the size already
/// promised. Rounded up so the buffer the frontend allocates is a whole number of pages.
const PAGE: usize = 4096;

/// How large a buffer a state of `len` bytes is given.
const fn padded(len: usize) -> usize {
    (HEADER + len + len / 16 + PAGE).next_multiple_of(PAGE)
}

/// The size a frontend was told, and the container written into it.
#[derive(Default)]
pub struct State {
    /// What `retro_serialize_size` answers, or zero when there is no cart - which is also how a
    /// frontend is told this core cannot serialize.
    size: usize,
}

impl State {
    /// Measures the cart that just loaded, fixing the size for as long as it stays loaded.
    pub fn attach(&mut self, deck: &ControlDeck) {
        self.size = match deck.serialized_state_len() {
            Ok(len) => padded(len),
            Err(err) => {
                log::error(&format!("failed to size a save state: {err}"));
                0
            }
        };
    }

    /// Forgets the size, so a frontend asking after the cart is gone is told there is no state to
    /// take rather than a size it would then fail to fill.
    pub const fn detach(&mut self) {
        self.size = 0;
    }

    /// Bytes a state occupies.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Writes the console into `dst`, which must be at least [`State::size`] bytes.
    pub fn serialize(&self, deck: &ControlDeck, dst: &mut [u8]) -> bool {
        if self.size == 0 {
            log::error("asked for a save state with no content loaded");
            return false;
        }
        if dst.len() < self.size {
            log::error(&format!(
                "asked to write a save state into {} bytes, having promised {}",
                dst.len(),
                self.size
            ));
            return false;
        }
        let (header, payload) = dst.split_at_mut(HEADER);
        let written = match deck.serialize_state_into(payload) {
            Ok(written) => written,
            Err(err) => {
                log::error(&format!("failed to write a save state: {err}"));
                return false;
            }
        };
        let Ok(len) = u32::try_from(written) else {
            log::error(&format!(
                "a save state of {written} bytes is too large to frame"
            ));
            return false;
        };
        // Everything past the state, not just the padding this core added: two states of the same
        // console must be byte-identical, and netplay CRCs the buffer the frontend allocated.
        payload[written..].fill(0);
        header[..FORMAT_AT].copy_from_slice(MAGIC);
        header[FORMAT_AT..LENGTH_AT].copy_from_slice(&FORMAT.to_le_bytes());
        header[LENGTH_AT..].copy_from_slice(&len.to_le_bytes());
        true
    }

    /// Restores the console from `src`.
    ///
    /// The size is not checked against [`State::size`]: a state written by a build whose padding
    /// differed is still readable, and the payload length in the header is what says where the
    /// state ends.
    pub fn unserialize(deck: &mut ControlDeck, src: &[u8]) -> bool {
        let Some(header) = src.get(..HEADER) else {
            log::error(&format!(
                "a save state of {} bytes is too short to be one",
                src.len()
            ));
            return false;
        };
        if &header[..FORMAT_AT] != MAGIC {
            log::error("that is not a TetaNES save state");
            return false;
        }
        let format =
            u32::from_le_bytes(header[FORMAT_AT..LENGTH_AT].try_into().expect("four bytes"));
        if format != FORMAT {
            log::error(&format!(
                "that save state is version {format}; this core writes {FORMAT}"
            ));
            return false;
        }
        let len = u32::from_le_bytes(header[LENGTH_AT..].try_into().expect("four bytes"));
        let payload = (len as usize)
            .checked_add(HEADER)
            .and_then(|end| src.get(HEADER..end));
        let Some(payload) = payload else {
            log::error(&format!(
                "a save state claiming {len} bytes arrived in {}",
                src.len()
            ));
            return false;
        };
        // The deck is what rejects a state belonging to another cart, which is why this hands over
        // the payload rather than restoring the bus itself.
        if let Err(err) = deck.deserialize_state(payload) {
            log::error(&format!("failed to restore a save state: {err}"));
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetanes_core::{
        control_deck::{Clocked, Config},
        memory::RamState,
        video::VideoFilter,
    };

    const ROM: &[u8] = include_bytes!("../../tetanes-core/test_roms/spritecans.nes");

    fn deck() -> ControlDeck {
        let mut deck = ControlDeck::with_config(
            Config::default()
                .with_sram_dir(None)
                .with_ram_state(RamState::AllZeros)
                .with_filter(VideoFilter::Pixellate),
        );
        deck.load_rom("test", &mut &ROM[..]).expect("loads");
        deck
    }

    fn run(deck: &mut ControlDeck, frames: usize) {
        for _ in 0..frames {
            while deck.clock_frame().expect("clocks") == Clocked::Continue {}
        }
    }

    /// The property run-ahead and netplay depend on: a state taken, restored and run again must
    /// land where the uninterrupted run did.
    #[test]
    fn a_restored_state_reproduces_the_run_it_was_taken_from() {
        let mut straight = deck();
        run(&mut straight, 60);

        let mut state = State::default();
        state.attach(&straight);
        let mut buffer = vec![0xAA; state.size()];
        assert!(state.serialize(&straight, &mut buffer));

        let mut restored = deck();
        assert!(State::unserialize(&mut restored, &buffer));

        run(&mut straight, 100);
        run(&mut restored, 100);
        assert_eq!(
            straight.frame_buffer_raw(),
            restored.frame_buffer_raw(),
            "the restored console drew the same frame as the one it was copied from"
        );
    }

    /// Netplay CRCs the whole buffer, so two states of the same console have to be byte-identical
    /// - the padding included.
    #[test]
    fn two_states_of_one_console_are_identical_to_the_last_byte() {
        let mut deck = deck();
        run(&mut deck, 30);
        let mut state = State::default();
        state.attach(&deck);

        let mut first = vec![0x00; state.size()];
        let mut second = vec![0xFF; state.size()];
        assert!(state.serialize(&deck, &mut first));
        assert!(state.serialize(&deck, &mut second));
        assert_eq!(
            first, second,
            "the tail is zeroed, not left as it was found"
        );
    }

    /// The size is promised once and may never grow, so it has to already cover a state taken at
    /// any point in the game.
    #[test]
    fn the_promised_size_holds_for_the_life_of_the_cart() {
        let mut deck = deck();
        let mut state = State::default();
        state.attach(&deck);
        let promised = state.size();

        for frame in 0..300 {
            run(&mut deck, 1);
            let mut buffer = vec![0; promised];
            assert!(state.serialize(&deck, &mut buffer), "frame {frame}");
        }
    }

    /// A frontend hands over whatever the user picked, and a core that fed it to the decoder would
    /// be restoring from noise.
    #[test]
    fn rubbish_is_refused() {
        let mut deck = deck();
        assert!(!State::unserialize(&mut deck, &[]), "nothing at all");
        assert!(!State::unserialize(&mut deck, &[0; 8]), "too short");
        assert!(
            !State::unserialize(&mut deck, &[0; 1024]),
            "the right length, the wrong magic"
        );

        let mut state = State::default();
        state.attach(&deck);
        let mut buffer = vec![0; state.size()];
        assert!(state.serialize(&deck, &mut buffer));

        let mut wrong_version = buffer.clone();
        wrong_version[FORMAT_AT] = 0xFF;
        assert!(!State::unserialize(&mut deck, &wrong_version));

        let mut overlong = buffer.clone();
        overlong[LENGTH_AT..HEADER].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            !State::unserialize(&mut deck, &overlong),
            "a payload longer than the buffer holding it"
        );
    }

    /// A buffer smaller than the size promised is the frontend and the core disagreeing, which is
    /// worth saying rather than writing a state that only half fits.
    #[test]
    fn too_small_a_buffer_is_refused() {
        let deck = deck();
        let mut state = State::default();
        state.attach(&deck);
        let mut buffer = vec![0; state.size() - 1];
        assert!(!state.serialize(&deck, &mut buffer));
    }

    /// With no cart there is no state, and a size of zero is how libretro says so.
    #[test]
    fn there_is_no_state_without_a_cart() {
        let mut state = State::default();
        state.attach(&ControlDeck::new());
        assert_eq!(state.size(), 0);
        assert!(!state.serialize(&ControlDeck::new(), &mut [0; 1024]));
    }
}
