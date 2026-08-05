//! The single console instance, and the frontend callbacks it talks back through.

use crate::{
    audio::Audio,
    cheat::Cheats,
    input::Pads,
    log,
    memory::Memory,
    options::Options,
    state::State,
    sys::{
        retro_audio_sample_batch_t, retro_audio_sample_t, retro_environment_t, retro_input_poll_t,
        retro_input_state_t, retro_video_refresh_t,
    },
    video::Video,
};
use std::cell::UnsafeCell;
use tetanes_core::{
    control_deck::{Config, ControlDeck},
    video::VideoFilter,
};

/// Function pointers the frontend hands over before `retro_init`.
///
/// All optional: a frontend sets them one at a time, and `retro_set_environment` in particular
/// arrives before the rest.
#[derive(Default)]
pub struct Callbacks {
    pub environment: Option<retro_environment_t>,
    pub video_refresh: Option<retro_video_refresh_t>,
    pub audio_sample: Option<retro_audio_sample_t>,
    pub audio_batch: Option<retro_audio_sample_batch_t>,
    pub input_poll: Option<retro_input_poll_t>,
    pub input_state: Option<retro_input_state_t>,
}

/// Everything this core owns between `retro_init` and `retro_deinit`.
pub struct Core {
    pub deck: ControlDeck,
    pub callbacks: Callbacks,
    pub video: Video,
    pub audio: Audio,
    pub pads: Pads,
    pub memory: Memory,
    pub state: State,
    pub cheats: Cheats,
    pub options: Options,
    /// Which filter the deck renders with.
    ///
    /// Tracked here because `ControlDeck` takes one but does not hand it back, and the two video
    /// routes need to know which is in force. Anything that changes it has to set it on the deck
    /// too, or the copies drift.
    filter: VideoFilter,
    /// Set when a panic was caught, after which `retro_run` stops clocking.
    ///
    /// A console whose state is unknown must not keep running - it would turn one bug into a
    /// stream of them - but the frontend has no way to be told "stop", so the core keeps handing
    /// back black frames and silence until the game is unloaded.
    pub wedged: bool,
}

impl Core {
    fn new() -> Self {
        let deck = ControlDeck::with_config(Config {
            // The frontend owns save files: it has its own directory layout, and its own idea of
            // when to write. `retro_get_memory_data` is how battery RAM reaches it.
            sram_dir: None,
            filter: VideoFilter::Pixellate,
            ..Default::default()
        });
        Self {
            deck,
            callbacks: Callbacks::default(),
            video: Video::default(),
            audio: Audio::default(),
            pads: Pads::default(),
            memory: Memory::default(),
            state: State::default(),
            cheats: Cheats::default(),
            options: Options::default(),
            filter: VideoFilter::Pixellate,
            wedged: false,
        }
    }

    /// Which filter the deck is rendering with.
    pub const fn filter(&self) -> VideoFilter {
        self.filter
    }

    /// Changes the filter, on both the deck and the copy the video path reads.
    ///
    /// The single way in, so the two cannot drift.
    pub const fn set_filter(&mut self, filter: VideoFilter) {
        self.filter = filter;
        self.deck.set_filter(filter);
    }
}

/// Holds the one [`Core`] a loaded library has.
///
/// libretro loads a fresh copy of this library per instance and calls every entry point from one
/// thread, so there is nothing to synchronise; a `Mutex` would buy nothing but contention. A
/// `thread_local!` would be worse than nothing, silently handing a second, empty core to a
/// frontend that broke the contract instead of failing where it could be seen.
struct CoreCell(UnsafeCell<Option<Core>>);

// SAFETY: see the type's own docs - one instance per loaded library, one thread. `core_mut` is the
// only way in, and is `unsafe` for exactly this reason.
unsafe impl Sync for CoreCell {}

static CORE: CoreCell = CoreCell(UnsafeCell::new(None));

/// Creates the core. Called from `retro_init`.
///
/// # Safety
///
/// The caller must be inside a libretro entry point, on the frontend's thread.
pub unsafe fn init() {
    // SAFETY: as above.
    let slot = unsafe { &mut *CORE.0.get() };
    let callbacks = slot.take().map(|core| core.callbacks).unwrap_or_default();
    // `retro_set_*` runs before `retro_init`, so whatever the frontend has already handed over has
    // to survive being handed a fresh console.
    *slot = Some(Core {
        callbacks,
        ..Core::new()
    });
}

/// Destroys the core. Called from `retro_deinit`.
///
/// # Safety
///
/// As [`init`].
pub unsafe fn deinit() {
    // SAFETY: as above.
    unsafe { *CORE.0.get() = None };
}

/// The core, if it has been initialised.
///
/// # Safety
///
/// As [`init`]. The returned borrow must not outlive the entry point that took it, and no second
/// borrow may be taken while it lives.
#[allow(clippy::mut_from_ref)]
pub unsafe fn try_core<'a>() -> Option<&'a mut Core> {
    // SAFETY: as above.
    unsafe { (*CORE.0.get()).as_mut() }
}

/// Runs `f`, containing any panic.
///
/// Nothing may unwind out of an `extern "C"` function: the process aborts, taking the frontend
/// with it. Every export therefore goes through this or [`with_core`], including the ones that run
/// before there is a core to talk about.
pub fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(panic) => {
            // The hook has already reported where; this says what it cost.
            log::error(&format!(
                "the core has stopped: {}. Unload the game to reset it.",
                log::payload(&*panic)
            ));
            // SAFETY: the panic unwound out of `f`, so any borrow it held is gone.
            if let Some(core) = unsafe { try_core() } {
                core.wedged = true;
            }
            default
        }
    }
}

/// Runs `f` against the core, containing any panic.
///
/// `default` is returned when there is no core, when a previous panic wedged it, or when this call
/// panics - whichever "this did not work" answer the export in question has.
pub fn with_core<T>(default: T, f: impl FnOnce(&mut Core) -> T) -> T {
    guard(None, || {
        // SAFETY: a libretro entry point, on the frontend's thread. Nothing below re-enters.
        match unsafe { try_core() } {
            Some(core) if !core.wedged => Some(f(core)),
            _ => None,
        }
    })
    .unwrap_or(default)
}
