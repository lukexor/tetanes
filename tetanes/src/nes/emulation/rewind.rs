use crate::nes::{emulation::State, renderer::gui::MessageType};
use tetanes_core::{
    control_deck::ControlDeck,
    fs::{Error, Result},
};
use tracing::error;

/// One slot of the rewind buffer.
///
/// Popping only unmarks a slot, so the allocation is reused for the next push.
#[derive(Default, Debug, Clone)]
#[must_use]
pub struct Slot {
    /// A buffer sized to the cart's state
    pub buf: Vec<u8>,
    /// Whether the buffer is filled.
    pub filled: bool,
}

/// A ring buffer of serialized console states, one every [`Rewind::interval`] *NES* frames.
///
/// Rewinding replays one snapshot per display frame, so the spacing between snapshots is the speed
/// the game rewinds at. Emulation speed changes how many NES frames a single `clock_frame` runs -
/// four at 4x, sometimes none at 0.5x - so counting calls would leave the buffer unevenly spaced in
/// game time, and a stretch that was fast-forwarded would rewind four times as fast as the rest. It
/// also makes the buffer hold [`Rewind::seconds`] of *gameplay*, whatever speed it was played at.
///
/// Only runtime state is kept. ROM data and frame buffers are not to reduce memory. Rewinding
/// re-renders frames, by clocking the restored state.
///
/// `ControlDeck` encodes a state at a fixed size for the life of a cart, so every slot can be sized
/// once to reduce allocations.
#[derive(Default, Debug)]
#[must_use]
pub struct Rewind {
    pub enabled: bool,
    /// Frames since the last snapshot, counted up to `interval`.
    pub interval_counter: usize,
    pub index: usize,
    pub count: usize,
    pub interval: usize,
    pub seconds: usize,
    pub frames: Vec<Slot>,
    /// Bytes one state takes, which is fixed for the life of a cart. `0` until the first push after
    /// a [`Rewind::clear`].
    pub state_len: usize,
}

impl Rewind {
    const TARGET_FPS: usize = 60;

    pub fn new(enabled: bool, seconds: u32, interval: u32) -> Self {
        let interval = interval as usize;
        let seconds = seconds as usize;
        Self {
            enabled,
            interval_counter: 0,
            index: 0,
            count: 0,
            interval,
            seconds,
            frames: vec![Slot::default(); Self::frame_size(seconds, interval)],
            state_len: 0,
        }
    }

    const fn frame_size(seconds: usize, interval: usize) -> usize {
        Self::TARGET_FPS * seconds / interval
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub fn set_seconds(&mut self, seconds: u32) {
        self.seconds = seconds as usize;
        self.resize();
    }

    pub fn set_interval(&mut self, interval: u32) {
        self.interval = interval as usize;
        self.resize();
    }

    /// Resize the ring buffer slots to the current state length.
    fn resize(&mut self) {
        self.frames.resize(
            Self::frame_size(self.seconds, self.interval),
            Slot::default(),
        );
        let state_len = self.state_len;
        for slot in &mut self.frames {
            slot.buf.resize(state_len, 0);
        }
    }

    /// Snapshots `deck` into the next slot, if the rewind interval has elapsed.
    pub fn push(&mut self, deck: &ControlDeck) -> Result<()> {
        if !self.enabled || self.frames.is_empty() {
            return Ok(());
        }
        self.interval_counter += 1;
        if self.interval_counter < self.interval {
            return Ok(());
        }
        self.interval_counter = 0;

        if self.state_len == 0 {
            self.state_len = deck
                .serialized_state_len()
                .map_err(|err| Error::SerializationFailed(err.to_string()))?;
            self.resize();
        }

        deck.serialize_state_into(&mut self.frames[self.index].buf)
            .map_err(|err| Error::SerializationFailed(err.to_string()))?;
        self.frames[self.index].filled = true;

        self.count += 1;
        self.index += 1;
        if self.index >= self.frames.len() {
            self.index = 0;
        }
        Ok(())
    }

    /// Restores `deck` to the most recent snapshot, reporting whether there was one to restore.
    ///
    /// Inputs held when the snapshot was taken are dropped on the way in, since they belong to the
    /// player rather than to the timeline being replayed.
    pub fn pop(&mut self, deck: &mut ControlDeck) -> bool {
        if !self.enabled || self.frames.is_empty() || self.count == 0 {
            return false;
        }
        self.count -= 1;
        // Wrap before decrementing, not after: `push` leaves `index` at 0 every time it wraps, so
        // decrementing first underflows there - and the slot at 0 was skipped on the way past
        // regardless.
        if self.index == 0 {
            self.index = self.frames.len();
        }
        self.index -= 1;

        if !self.frames[self.index].filled {
            return false;
        }
        self.frames[self.index].filled = false;
        if let Err(err) = deck.deserialize_state(&self.frames[self.index].buf) {
            error!("Failed to restore console state: {err:?}");
            return false;
        }
        true
    }

    pub fn clear(&mut self) {
        self.interval_counter = 0;
        self.index = 0;
        self.count = 0;
        // The buffers go too: `clear` runs on unload, and the next cart's state may be a different
        // size.
        self.state_len = 0;
        for slot in &mut self.frames {
            slot.buf = Vec::new();
            slot.filled = false;
        }
    }
}

impl State {
    pub fn rewind_disabled(&mut self) {
        self.add_message(
            MessageType::Warn,
            "Rewind disabled. You can enable it in the Preferences menu.",
        );
    }

    pub fn instant_rewind(&mut self) {
        if !self.rewind.enabled {
            return self.rewind_disabled();
        }
        // ~2 seconds worth of frames @ 60 FPS
        let mut rewind_frames = 120 / self.rewind.interval;
        while self.rewind.pop(&mut self.control_deck) {
            rewind_frames -= 1;
            if rewind_frames == 0 {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `push` leaves `index` at 0 every time it wraps, which is one frame in `frames.len()` - so
    /// starting a rewind right then decremented an unsigned 0. In a release build that wrapped to
    /// `usize::MAX` and indexed out of bounds on the next line; in a debug build it panicked
    /// outright. Either way it took the emulation thread with it.
    #[test]
    fn popping_at_the_wrap_point_does_not_underflow() {
        use tetanes_core::control_deck::ControlDeck;

        let mut rewind = Rewind::new(true, 1, 1);
        assert_eq!(rewind.frames.len(), 60);

        // A full ring whose write cursor has just wrapped. The deck has no rom, so every restore
        // fails and `pop` reports `false` - the index arithmetic under test runs first either way.
        let mut deck = ControlDeck::new();
        for slot in &mut rewind.frames {
            slot.filled = true;
        }
        rewind.count = rewind.frames.len();
        rewind.index = 0;

        assert!(!rewind.pop(&mut deck));
        assert_eq!(rewind.index, 59, "wrapped to the last slot, not past it");

        assert!(!rewind.pop(&mut deck));
        assert_eq!(rewind.index, 58, "and keeps walking backwards from there");
    }

    /// A zero-length ring is reachable through `set_seconds`, and every slot access assumed
    /// otherwise.
    #[test]
    fn an_empty_ring_is_inert_rather_than_a_panic() {
        use tetanes_core::control_deck::ControlDeck;

        let mut rewind = Rewind::new(true, 0, 2);
        let mut deck = ControlDeck::new();
        assert!(rewind.frames.is_empty());
        assert!(!rewind.pop(&mut deck));
    }

    /// Snapshots have to be evenly spaced in *NES* frames, because rewinding replays them one per
    /// display frame - so uneven spacing is uneven rewind speed.
    ///
    /// A display frame is not a frame: at 2x it is two and at 0.5x every other one is none. So the
    /// spacing has to count what `ControlDeck::clock_frame` reports it clocked, not how many times
    /// it was called - counting calls rewinds a fast-forwarded stretch at the speed it was
    /// recorded at, and below 1x records consecutive snapshots with no frame clocked between them.
    #[test]
    fn snapshots_are_evenly_spaced_whatever_the_speed() {
        use tetanes_core::control_deck::{Clocked, Config, ControlDeck};
        use tetanes_core::memory::RamState;

        let mut deck =
            ControlDeck::with_config(Config::default().with_ram_state(RamState::AllZeros));
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("failed to load rom");

        let interval = 2;
        for speed in [1.0, 2.0, 4.0, 0.5] {
            let mut rewind = Rewind::new(true, 5, interval);
            deck.set_frame_speed(speed);

            // What `State::clock_display_frame` does: drain the display frame, snapshotting each
            // NES frame it actually clocked.
            for _ in 0..12 {
                loop {
                    let clocked = deck.clock_frame().expect("failed to clock frame");
                    if clocked != Clocked::Idle {
                        rewind.push(&deck).expect("failed to push a snapshot");
                    }
                    if clocked != Clocked::Continue {
                        break;
                    }
                }
            }

            let mut snapshots = vec![];
            while rewind.pop(&mut deck) {
                snapshots.push(deck.bus().ppu.frame_number());
            }
            // `pop` walks backwards, so each gap is the previous snapshot minus this one.
            let gaps = snapshots
                .windows(2)
                .map(|pair| pair[0] - pair[1])
                .collect::<Vec<_>>();
            assert!(
                gaps.iter().all(|&gap| gap == interval),
                "at {speed}x every gap should be {interval} nes frames, got {gaps:?} from \
                 {snapshots:?}"
            );
        }
    }

    /// What the rewind path in `State::try_clock_frame` does, without a window: a snapshot carries
    /// no pixels, so restoring one leaves the screen blank until a frame is clocked off it. If
    /// that clock ever stops happening, rewinding goes black.
    #[test]
    fn a_restored_snapshot_is_blank_until_it_is_clocked() {
        use tetanes_core::control_deck::{Config, ControlDeck};
        use tetanes_core::memory::RamState;

        let mut deck =
            ControlDeck::with_config(Config::default().with_ram_state(RamState::AllZeros));
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("failed to load rom");

        let mut rewind = Rewind::new(true, 1, 1);
        for _ in 0..120 {
            // At 1x a call is a frame, so this needs no drain loop.
            let _ = deck.clock_frame().expect("failed to clock frame");
            rewind.push(&deck).expect("failed to push a snapshot");
        }
        assert!(
            deck.bus().ppu.frame_buffer().iter().any(|&px| px != 0),
            "the rom draws something by frame 120, or this test proves nothing"
        );

        assert!(rewind.pop(&mut deck), "a snapshot to rewind to");
        let snapshot_frame = deck.bus().ppu.frame_number();

        assert!(
            deck.bus().ppu.frame_buffer().iter().all(|&px| px == 0),
            "a snapshot carries no pixels"
        );

        let _ = deck.clock_frame().expect("failed to re-render");
        assert!(
            deck.bus().ppu.frame_buffer().iter().any(|&px| px != 0),
            "clocking the restored state renders it again"
        );
        assert_eq!(
            deck.bus().ppu.frame_number(),
            snapshot_frame + 1,
            "one frame past the snapshot, which is the offset a rewind shows throughout"
        );
    }

    /// Below 1x a display frame does not always owe an NES frame, so one `clock_frame` is not
    /// enough to re-render a restored snapshot - three calls in four at 0.25x clock nothing and
    /// leave the blank buffer in place. Rewinding steps one snapshot per display frame at any
    /// speed, so it has to keep asking until a frame is actually rendered.
    #[test]
    fn a_restored_snapshot_needs_more_than_one_clock_below_1x() {
        use tetanes_core::control_deck::{Clocked, Config, ControlDeck};
        use tetanes_core::memory::RamState;

        let mut deck =
            ControlDeck::with_config(Config::default().with_ram_state(RamState::AllZeros));
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("failed to load rom");

        let mut rewind = Rewind::new(true, 1, 1);
        for _ in 0..120 {
            let _ = deck.clock_frame().expect("failed to clock frame");
            rewind.push(&deck).expect("failed to push a snapshot");
        }

        deck.set_frame_speed(0.25);
        assert!(rewind.pop(&mut deck), "a snapshot to rewind to");

        assert_eq!(
            deck.clock_frame().expect("clocks"),
            Clocked::Idle,
            "0.25x owes no frame on this call"
        );
        assert!(
            deck.bus().ppu.frame_buffer().iter().all(|&px| px == 0),
            "so the snapshot is still blank - shipping it here is the black-frame bug"
        );

        let mut clocks = 1;
        while deck.clock_frame().expect("clocks") == Clocked::Idle {
            clocks += 1;
        }
        assert!(
            (2..=4).contains(&clocks),
            "0.25x owes a frame on one display frame in four, whatever phase it starts in, \
             but took {clocks}"
        );
        assert!(
            deck.bus().ppu.frame_buffer().iter().any(|&px| px != 0),
            "and now it has pixels"
        );
    }
}
