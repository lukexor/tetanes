//! A frontend, small enough to run in a test.
//!
//! The exports are what a frontend actually calls, and calling them from Rust exercises the same
//! code a `dlopen` would - the difference being only the symbol table, which CI checks separately
//! with `nm`. What this catches is the part unit tests cannot: that the lifecycle works in the
//! order a frontend uses it, and that a frame comes out the far end.
//!
//! The core is a process-wide singleton, so these tests take a lock and each one runs a whole
//! init-to-deinit cycle.

use crate::*;
use std::{
    cell::RefCell,
    sync::{Mutex, MutexGuard},
};
use tetanes_core::input::Player;

/// A ROM that draws something busy enough that a blank frame is obviously wrong.
const ROM: &[u8] = include_bytes!("../../tetanes-core/test_roms/spritecans.nes");

/// One frame as the frontend received it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    width: u32,
    height: u32,
    pitch: usize,
    pixels: Vec<u32>,
}

/// What the frontend saw, and what it will answer.
#[derive(Default)]
struct Frontend {
    pixel_format: Option<c_uint>,
    env_calls: Vec<c_uint>,
    frames: Vec<Frame>,
    audio_frames: usize,
    /// Buttons the frontend reports as held, as `(port, id)`.
    held: Vec<(c_uint, c_uint)>,
    polls: usize,
}

thread_local! {
    static FRONTEND: RefCell<Frontend> = RefCell::new(Frontend::default());
}

/// Serialises the tests, since they share one core.
static LOCK: Mutex<()> = Mutex::new(());

unsafe extern "C" fn env(cmd: c_uint, data: *mut c_void) -> bool {
    FRONTEND.with_borrow_mut(|f| f.env_calls.push(cmd));
    match cmd {
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
            // SAFETY: the core passes a pointer to one `unsigned` for this call.
            let format = unsafe { *data.cast::<c_uint>() };
            FRONTEND.with_borrow_mut(|f| f.pixel_format = Some(format));
            true
        }
        // Anything else is a feature this frontend does not have, which a core must cope with.
        _ => false,
    }
}

unsafe extern "C" fn video_refresh(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    let count = (width * height) as usize;
    // SAFETY: the core hands over `width * height` pixels of the format it asked the frontend for.
    let pixels = unsafe { slice::from_raw_parts(data.cast::<u32>(), count) }.to_vec();
    FRONTEND.with_borrow_mut(|f| {
        f.frames.push(Frame {
            width,
            height,
            pitch,
            pixels,
        });
    });
}

unsafe extern "C" fn audio_sample(_left: i16, _right: i16) {}

unsafe extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    // SAFETY: `frames` pairs, so twice that many values.
    let samples = unsafe { slice::from_raw_parts(data, frames * 2) };
    assert!(
        samples
            .as_chunks::<2>()
            .0
            .iter()
            .all(|pair| pair[0] == pair[1]),
        "the NES is mono, so both channels carry the same sample"
    );
    FRONTEND.with_borrow_mut(|f| f.audio_frames += frames);
    frames
}

unsafe extern "C" fn input_poll() {
    FRONTEND.with_borrow_mut(|f| f.polls += 1);
}

unsafe extern "C" fn input_state(port: c_uint, _device: c_uint, _index: c_uint, id: c_uint) -> i16 {
    FRONTEND.with_borrow(|f| i16::from(f.held.contains(&(port, id))))
}

/// Brings a core up the way a frontend does, and tears it down when the guard drops.
struct Session(
    #[expect(dead_code, reason = "held for the session's lifetime")] MutexGuard<'static, ()>,
);

impl Session {
    fn start() -> Self {
        let guard = LOCK.lock().unwrap_or_else(|err| err.into_inner());
        FRONTEND.with_borrow_mut(|f| *f = Frontend::default());
        // SAFETY: this thread is standing in for the frontend's, and holds the lock.
        unsafe {
            retro_set_environment(env);
            retro_set_video_refresh(video_refresh);
            retro_set_audio_sample(audio_sample);
            retro_set_audio_sample_batch(audio_batch);
            retro_set_input_poll(input_poll);
            retro_set_input_state(input_state);
            retro_init();
        }
        Self(guard)
    }

    fn load(&self) -> bool {
        let info = retro_game_info {
            path: c"spritecans.nes".as_ptr(),
            data: ROM.as_ptr().cast::<c_void>(),
            size: ROM.len(),
            meta: std::ptr::null(),
        };
        // SAFETY: `info` outlives the call, and `data` covers `size`.
        unsafe { retro_load_game(&raw const info) }
    }

    fn run(&self, frames: usize) {
        for _ in 0..frames {
            // SAFETY: the callbacks above are still valid.
            unsafe { retro_run() };
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: as above; the core is torn down whether the test passed or panicked.
        unsafe {
            retro_unload_game();
            retro_deinit();
        }
    }
}

#[test]
fn the_core_reports_itself_before_any_content_is_loaded() {
    let _session = Session::start();

    let mut info = std::mem::MaybeUninit::<retro_system_info>::zeroed();
    // SAFETY: the core writes every field.
    let info = unsafe {
        retro_get_system_info(info.as_mut_ptr());
        info.assume_init()
    };
    // SAFETY: the core returns pointers to its own static strings.
    unsafe {
        assert_eq!(CStr::from_ptr(info.library_name).to_str(), Ok("TetaNES"));
        assert_eq!(CStr::from_ptr(info.valid_extensions).to_str(), Ok("nes"));
        assert!(!CStr::from_ptr(info.library_version).is_empty());
    }
    assert!(
        !info.need_fullpath,
        "the frontend hands over bytes, so the core never opens a file"
    );
    assert_eq!(retro_api_version(), 1);
}

#[test]
fn a_loaded_rom_produces_frames_and_audio() {
    let session = Session::start();
    assert!(session.load(), "the test ROM loads");

    assert_eq!(
        FRONTEND.with_borrow(|f| f.pixel_format),
        Some(RETRO_PIXEL_FORMAT_XRGB8888),
        "the core has to ask for the format it then sends"
    );

    let mut av = std::mem::MaybeUninit::<retro_system_av_info>::zeroed();
    // SAFETY: the core writes every field.
    let av = unsafe {
        retro_get_system_av_info(av.as_mut_ptr());
        av.assume_init()
    };
    assert_eq!(av.geometry.base_width, 256);
    assert_eq!(av.geometry.base_height, 240);
    assert_eq!(av.timing.sample_rate, audio::SAMPLE_RATE);
    assert!(
        (av.timing.fps - 60.098_814).abs() < 1e-6,
        "the true NTSC rate, not 60: the frontend paces itself by this"
    );

    session.run(10);

    FRONTEND.with_borrow(|f| {
        assert_eq!(f.frames.len(), 10, "one frame out per `retro_run`");
        assert_eq!(f.polls, 10, "input is latched once per frame");

        let frame = f.frames.last().expect("a frame");
        assert_eq!((frame.width, frame.height), (256, 240));
        assert_eq!(frame.pitch, 256 * 4);
        assert_eq!(frame.pixels.len(), 256 * 240);
        assert!(
            frame.pixels.iter().all(|px| px & 0xFF00_0000 == 0),
            "XRGB8888 leaves the top byte clear"
        );
        assert!(
            frame
                .pixels
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1,
            "spritecans draws something, so a single-colour frame means nothing rendered"
        );

        // ~800 sample frames each at 48 kHz / 60.1 fps, and the count varies by one either way.
        assert!(
            (7_900..8_100).contains(&f.audio_frames),
            "expected about 8000 sample frames across ten video frames, got {}",
            f.audio_frames
        );
    });
}

/// Input has to reach the console and stop reaching it when released.
///
/// The face buttons are the ones that broke: a frontend reports every button every frame, and
/// replaying "turbo is not held" each time cleared A and B, because `set_button` treats a turbo
/// release as a release of the button it fires. The D-pad, START and SELECT have no such rule and
/// kept working, which is what the bug looked like from outside.
#[test]
fn a_held_button_reaches_the_joypad_and_a_released_one_stops() {
    let session = Session::start();
    assert!(session.load());

    let pressed = |button| {
        // SAFETY: the core is initialised and this thread is the frontend's.
        unsafe { core::try_core() }
            .expect("a core")
            .deck
            .joypad(Player::One)
            .button(button)
    };

    for (id, button, name) in [
        (RETRO_DEVICE_ID_JOYPAD_B, JoypadBtnState::A, "B -> A"),
        (RETRO_DEVICE_ID_JOYPAD_A, JoypadBtnState::B, "A -> B"),
        (RETRO_DEVICE_ID_JOYPAD_START, JoypadBtnState::START, "START"),
        (RETRO_DEVICE_ID_JOYPAD_LEFT, JoypadBtnState::LEFT, "LEFT"),
    ] {
        FRONTEND.with_borrow_mut(|f| f.held = vec![(0, id)]);
        // Several frames, because holding across frames is what broke: one frame passed either way.
        for frame in 0..5 {
            session.run(1);
            assert!(pressed(button), "{name} still held on frame {frame}");
        }

        FRONTEND.with_borrow_mut(|f| f.held.clear());
        session.run(1);
        assert!(!pressed(button), "{name} released");
    }
}

/// A frontend will hand over rubbish - a truncated download, the wrong file - and the core has to
/// say no rather than take the process down.
#[test]
fn bad_content_is_refused() {
    let _session = Session::start();

    let empty = retro_game_info {
        path: std::ptr::null(),
        data: std::ptr::null(),
        size: 0,
        meta: std::ptr::null(),
    };
    // SAFETY: a well-formed struct describing no content, which is the case being tested.
    assert!(!unsafe { retro_load_game(&raw const empty) }, "no content");

    let junk = [0u8; 64];
    let not_a_rom = retro_game_info {
        path: std::ptr::null(),
        data: junk.as_ptr().cast::<c_void>(),
        size: junk.len(),
        meta: std::ptr::null(),
    };
    // SAFETY: `data` covers `size`; the bytes are simply not a cartridge.
    assert!(
        !unsafe { retro_load_game(&raw const not_a_rom) },
        "not a cartridge"
    );

    // SAFETY: the core is initialised; running without content must not fault.
    unsafe { retro_run() };
}

/// Runs a ROM off the local filesystem through the whole lifecycle.
///
/// Skipped unless `TETANES_LIBRETRO_ROM` names one, since the committed test ROMs are small and
/// synthetic - a board with expansion audio or an unusual mapper only turns up in a real cart.
#[test]
fn a_local_rom_runs() {
    let Ok(path) = std::env::var("TETANES_LIBRETRO_ROM") else {
        return;
    };
    let rom = std::fs::read(&path).expect("the ROM named by TETANES_LIBRETRO_ROM");
    let session = Session::start();
    let name = std::ffi::CString::new(path.clone()).expect("a path without a NUL");
    let info = retro_game_info {
        path: name.as_ptr(),
        data: rom.as_ptr().cast::<c_void>(),
        size: rom.len(),
        meta: std::ptr::null(),
    };
    // SAFETY: `info` outlives the call and `data` covers `size`.
    assert!(unsafe { retro_load_game(&raw const info) }, "{path} loads");
    // Long enough for a slow boot: Punch-Out!! is still a black screen at 120 frames on hardware
    // too, so a shorter run would be asserting about a logo rather than about the core.
    const FRAMES: usize = 300;
    session.run(FRAMES);

    FRONTEND.with_borrow(|f| {
        assert_eq!(f.frames.len(), FRAMES);
        assert!(
            f.audio_frames > 200_000,
            "audio kept flowing: {}",
            f.audio_frames
        );
        // *Some* frame, not the last one: plenty of games sit on a black transition at any given
        // instant, and asserting about one arbitrary frame tests the game rather than the core.
        let drew = f.frames.iter().any(|frame| {
            frame
                .pixels
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1
        });
        assert!(drew, "{path} drew nothing in {FRAMES} frames");
    });
}
