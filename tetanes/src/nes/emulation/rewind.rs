use crate::nes::{emulation::State, renderer::gui::MessageType};
use tetanes_core::{
    bus::Bus,
    fs::{Error, Result},
};
use tracing::error;

/// A ring of serialized console states, one every [`Rewind::interval`] frames.
///
/// Only state is kept. Pixels are not: `Frame::buffer` is `#[serde(skip)]`, so a snapshot used to
/// carry a 120 KiB clone of the frame alongside its ~23 KiB of state - at the default 30 s and an
/// interval of 2 that is 900 snapshots, or ~108 MB of pixels against ~21 MB of everything else.
/// Rewinding now re-renders instead, by clocking one frame off the restored state, which trades
/// ~3 ms on the rewind path - a path with a whole frame's budget and nothing else to do - for a
/// 120 KiB allocation and copy on every push during normal play.
#[derive(Default, Debug)]
#[must_use]
pub struct Rewind {
    pub enabled: bool,
    pub interval_counter: usize,
    pub index: usize,
    pub count: usize,
    pub interval: usize,
    pub seconds: usize,
    pub frames: Vec<Option<Vec<u8>>>,
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
            frames: vec![None; Self::frame_size(seconds, interval)],
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
        self.frames
            .resize(Self::frame_size(self.seconds, self.interval), None);
    }

    pub fn set_interval(&mut self, interval: u32) {
        self.interval = interval as usize;
        self.frames
            .resize(Self::frame_size(self.seconds, self.interval), None);
    }

    pub fn push(&mut self, bus: &Bus) -> Result<()> {
        if !self.enabled || self.frames.is_empty() {
            return Ok(());
        }
        self.interval_counter += 1;
        if self.interval_counter >= self.interval {
            self.interval_counter = 0;

            let config = bincode::config::legacy();
            let state = bincode::serde::encode_to_vec(bus, config)
                .map_err(|err| Error::SerializationFailed(err.to_string()))?;
            self.frames[self.index] = Some(state);

            self.count += 1;
            self.index += 1;
            if self.index >= self.frames.len() {
                self.index = 0;
            }
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<Bus> {
        if !self.enabled || self.frames.is_empty() {
            return None;
        }
        if self.count > 0 {
            self.count -= 1;
            // Wrap before decrementing, not after: `push` leaves `index` at 0 every time it wraps,
            // so decrementing first underflows there - and the slot at 0 was skipped on the way
            // past regardless.
            if self.index == 0 {
                self.index = self.frames.len();
            }
            self.index -= 1;

            let state = self.frames[self.index].take()?;
            let config = bincode::config::legacy();
            bincode::serde::decode_from_slice::<Bus, _>(&state, config)
                .map(|(mut bus, _)| {
                    bus.input.clear(); // Discard inputs while rewinding
                    bus
                })
                .map_err(|err| error!("Failed to deserialize console state: {err:?}"))
                .ok()
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.interval_counter = 0;
        self.index = 0;
        self.count = 0;
        self.frames.fill(None);
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
        let mut rewind = Rewind::new(true, 1, 1);
        assert_eq!(rewind.frames.len(), 60);

        // A full ring whose write cursor has just wrapped. The states are not decodable, which
        // `pop` reports as `None` - the index arithmetic under test runs first either way.
        rewind.frames.fill(Some(Vec::new()));
        rewind.count = rewind.frames.len();
        rewind.index = 0;

        assert!(rewind.pop().is_none());
        assert_eq!(rewind.index, 59, "wrapped to the last slot, not past it");

        assert!(rewind.pop().is_none());
        assert_eq!(rewind.index, 58, "and keeps walking backwards from there");
    }

    /// A zero-length ring is reachable through `set_seconds`, and every slot access assumed
    /// otherwise.
    #[test]
    fn an_empty_ring_is_inert_rather_than_a_panic() {
        let mut rewind = Rewind::new(true, 0, 2);
        assert!(rewind.frames.is_empty());
        assert!(rewind.pop().is_none());
    }

    /// What the rewind path in `State::try_clock_frame` does, without a window: a snapshot carries
    /// no pixels, so restoring one leaves the screen blank until a frame is clocked off it. If
    /// that clock ever stops happening, rewinding goes black.
    #[test]
    fn a_restored_snapshot_is_blank_until_it_is_clocked() {
        use tetanes_core::control_deck::{Config, ControlDeck};
        use tetanes_core::memory::RamState;

        let mut deck = ControlDeck::with_config(Config {
            ram_state: RamState::AllZeros,
            ..Default::default()
        });
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("failed to load rom");

        let mut rewind = Rewind::new(true, 1, 1);
        for _ in 0..120 {
            deck.clock_frame().expect("failed to clock frame");
            rewind.push(deck.bus()).expect("failed to push a snapshot");
        }
        assert!(
            deck.bus().ppu.frame_buffer().iter().any(|&px| px != 0),
            "the rom draws something by frame 120, or this test proves nothing"
        );

        let bus = rewind.pop().expect("a snapshot to rewind to");
        let snapshot_frame = bus.ppu.frame_number();
        deck.load_bus(bus).expect("failed to restore");

        assert!(
            deck.bus().ppu.frame_buffer().iter().all(|&px| px == 0),
            "a snapshot carries no pixels"
        );

        deck.clock_frame().expect("failed to re-render");
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
        while let Some(mut bus) = self.rewind.pop() {
            bus.input.clear(); // Discard inputs while rewinding
            if let Err(err) = self.control_deck.load_bus(bus) {
                error!("failed to rewind: {err:?}");
                return;
            }
            rewind_frames -= 1;
            if rewind_frames == 0 {
                break;
            }
        }
    }
}
