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
use tetanes_core::{apu::Channel, input::Player, video::VideoFilter};

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

/// One memory descriptor as the frontend received it, with the pointer kept as an address so it
/// can be compared against `retro_get_memory_data` without being dereferenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemoryRegion {
    flags: u64,
    ptr: usize,
    start: usize,
    select: usize,
    len: usize,
}

/// What the frontend saw, and what it will answer.
#[derive(Default)]
struct Frontend {
    pixel_format: Option<c_uint>,
    memory_map: Vec<MemoryRegion>,
    env_calls: Vec<c_uint>,
    frames: Vec<Frame>,
    audio_frames: usize,
    /// Buttons the frontend reports as held, as `(port, id)`.
    held: Vec<(c_uint, c_uint)>,
    polls: usize,
    /// Option keys the core declared, in the order it declared them.
    declared_options: Vec<String>,
    /// What the player has set, as the frontend stores it.
    variables: std::collections::HashMap<String, std::ffi::CString>,
    /// Set when `variables` changed, and cleared by the core's next poll - the frontend's own
    /// contract, since `GET_VARIABLE_UPDATE` says only that something changed.
    variable_update: bool,
    av_infos: Vec<retro_system_av_info>,
    /// Buttons the core described, as `(port, id, description)`.
    input_descriptors: Vec<(c_uint, c_uint, String)>,
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
        RETRO_ENVIRONMENT_SET_MEMORY_MAPS => {
            // SAFETY: the core passes one `retro_memory_map` whose `descriptors` covers
            // `num_descriptors` entries, and it outlives this call - which is why a real frontend
            // copies here too.
            let map = unsafe { &*data.cast::<retro_memory_map>() };
            let descriptors =
                unsafe { slice::from_raw_parts(map.descriptors, map.num_descriptors as usize) };
            FRONTEND.with_borrow_mut(|f| {
                f.memory_map = descriptors
                    .iter()
                    .map(|desc| MemoryRegion {
                        flags: desc.flags,
                        ptr: desc.ptr as usize,
                        start: desc.start,
                        select: desc.select,
                        len: desc.len,
                    })
                    .collect();
            });
            true
        }
        RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION => {
            // SAFETY: the core passes one `unsigned` to be filled in.
            unsafe { *data.cast::<c_uint>() = 2 };
            true
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2 => {
            // SAFETY: the core passes one `retro_core_options_v2` whose `definitions` array runs
            // until an entry with a null key.
            let keys = unsafe {
                let options = &*data.cast::<retro_core_options_v2>();
                let mut keys = Vec::new();
                let mut def = options.definitions;
                while !(*def).key.is_null() {
                    keys.push(CStr::from_ptr((*def).key).to_string_lossy().into_owned());
                    def = def.add(1);
                }
                keys
            };
            FRONTEND.with_borrow_mut(|f| f.declared_options = keys);
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE => {
            // SAFETY: the core passes one `retro_variable` with `key` set, for `value` to be
            // filled in with a pointer the frontend owns.
            let variable = unsafe { &mut *data.cast::<retro_variable>() };
            let key = unsafe { CStr::from_ptr(variable.key) }
                .to_string_lossy()
                .into_owned();
            // The `CString` stays in the map, so the pointer outlives this call as the API wants.
            match FRONTEND.with_borrow(|f| f.variables.get(&key).map(|value| value.as_ptr())) {
                Some(value) => {
                    variable.value = value;
                    true
                }
                // No setting for this key, which a core has to read as "use the default".
                None => false,
            }
        }
        RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE => {
            let updated = FRONTEND.with_borrow_mut(|f| std::mem::take(&mut f.variable_update));
            // SAFETY: the core passes one `bool`.
            unsafe { *data.cast::<bool>() = updated };
            true
        }
        RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS => {
            // SAFETY: the core passes an array running until an entry with a null description.
            let described = unsafe {
                let mut described = Vec::new();
                let mut desc = data.cast::<retro_input_descriptor>();
                while !(*desc).description.is_null() {
                    described.push((
                        (*desc).port,
                        (*desc).id,
                        CStr::from_ptr((*desc).description)
                            .to_string_lossy()
                            .into_owned(),
                    ));
                    desc = desc.add(1);
                }
                described
            };
            FRONTEND.with_borrow_mut(|f| f.input_descriptors = described);
            true
        }
        RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO => {
            // SAFETY: the core passes one `retro_system_av_info`.
            let info = unsafe { *data.cast::<retro_system_av_info>() };
            FRONTEND.with_borrow_mut(|f| f.av_infos.push(info));
            true
        }
        // Anything else is a feature this frontend does not have, which a core must cope with.
        _ => false,
    }
}

/// Sets an option the way a frontend's menu does, and says something changed.
fn set_option(key: &str, value: &str) {
    let value = std::ffi::CString::new(value).expect("a value without a NUL");
    FRONTEND.with_borrow_mut(|f| {
        f.variables.insert(key.to_string(), value);
        f.variable_update = true;
    });
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

/// The property a frontend's save states, rewind, netplay and run-ahead all rest on: restoring a
/// state and running on has to reproduce the run the state was taken from, frame for frame.
///
/// Run through the exports rather than the `state` module, because the container is only half of
/// it - the rest is the frontend's own buffer, which it sizes once from `retro_serialize_size` and
/// hands back untouched.
#[test]
fn a_restored_state_replays_the_run_it_was_taken_from() {
    let session = Session::start();
    assert!(session.load());

    let size = retro_serialize_size();
    assert!(size > 0, "a loaded cart has a state to take");

    session.run(100);
    let mut buffer = vec![0xAA; size];
    // SAFETY: the buffer is the size the core asked for, and outlives the call.
    assert!(unsafe { retro_serialize(buffer.as_mut_ptr().cast::<c_void>(), size) });

    session.run(100);
    let straight = FRONTEND.with_borrow(|f| f.frames[100..200].to_vec());

    // SAFETY: the same buffer, unmodified since the core wrote it.
    assert!(unsafe { retro_unserialize(buffer.as_ptr().cast::<c_void>(), size) });
    session.run(100);
    let replayed = FRONTEND.with_borrow(|f| f.frames[200..300].to_vec());

    assert_eq!(
        straight, replayed,
        "the hundred frames after the restore are the hundred that followed the state"
    );
}

/// A frontend asks for the size once and sizes its rewind ring and netplay buffers from it, so an
/// answer that grew later would be a state with nowhere to go.
#[test]
fn the_state_size_is_promised_once_and_only_while_a_cart_is_in() {
    let session = Session::start();
    assert_eq!(retro_serialize_size(), 0, "no cart, no state");

    assert!(session.load());
    let size = retro_serialize_size();
    assert!(size > 0);

    session.run(120);
    assert_eq!(retro_serialize_size(), size, "and it has not moved");

    retro_unload_game();
    assert_eq!(retro_serialize_size(), 0, "the cart is out again");
    // SAFETY: a state must be refused rather than written with nothing to write about.
    assert!(!unsafe { retro_serialize(vec![0u8; size].as_mut_ptr().cast::<c_void>(), size) });
}

/// What the remapping menu shows. The mirroring is the point: without a descriptor the menu says
/// only "B", and the player has no way to learn that it presses NES A.
#[test]
fn every_button_on_every_port_is_described() {
    let session = Session::start();
    assert!(session.load());

    let described = FRONTEND.with_borrow(|f| f.input_descriptors.clone());
    assert_eq!(
        described.len(),
        input::PORTS.len() * input::BUTTONS.len(),
        "each port describes each button"
    );

    for port in 0..input::PORTS.len() as c_uint {
        let named = |id| {
            described
                .iter()
                .find(|&&(p, i, _)| p == port && i == id)
                .map(|(_, _, name)| name.as_str())
        };
        assert_eq!(named(RETRO_DEVICE_ID_JOYPAD_B), Some("A"), "port {port}");
        assert_eq!(named(RETRO_DEVICE_ID_JOYPAD_A), Some("B"), "port {port}");
        assert_eq!(named(RETRO_DEVICE_ID_JOYPAD_Y), Some("Turbo A"));
        assert_eq!(named(RETRO_DEVICE_ID_JOYPAD_START), Some("Start"));
        assert_eq!(named(RETRO_DEVICE_ID_JOYPAD_LEFT), Some("Left"));
        for (id, _, _) in input::BUTTONS {
            assert!(named(id).is_some(), "port {port} left {id} unnamed");
        }
    }
}

/// The options have to be declared before content is chosen, since that is when a player opens the
/// menu looking for them.
#[test]
fn the_options_are_declared_before_any_content() {
    let _session = Session::start();

    let declared = FRONTEND.with_borrow(|f| f.declared_options.clone());
    assert!(
        declared.contains(&"tetanes_filter".to_string()),
        "declared: {declared:?}"
    );
    assert!(declared.contains(&"tetanes_region".to_string()));
    assert!(declared.contains(&"tetanes_apu_triangle".to_string()));
    assert!(
        declared.iter().all(|key| key.starts_with("tetanes_")),
        "an unprefixed key would collide with another core's"
    );
}

/// What the player picks has to reach the console. This is the failure the whole module is prone
/// to - a key that is declared, shown in the menu, and never read.
#[test]
fn a_changed_option_reaches_the_console() {
    let session = Session::start();
    set_option("tetanes_filter", "ntsc");
    set_option("tetanes_run_ahead", "2");
    set_option("tetanes_apu_triangle", "disabled");
    assert!(session.load());

    // SAFETY: the core is initialised and this thread is the frontend's.
    let core = || unsafe { core::try_core() }.expect("a core");
    assert_eq!(core().filter(), VideoFilter::Ntsc, "applied at load");
    assert_eq!(core().deck.run_ahead(), 2);
    assert!(!core().deck.apu_channel_enabled(Channel::Triangle));

    // And again mid-game, which is the path `GET_VARIABLE_UPDATE` drives.
    set_option("tetanes_filter", "pixellate");
    set_option("tetanes_apu_triangle", "enabled");
    session.run(1);
    assert_eq!(core().filter(), VideoFilter::Pixellate);
    assert!(core().deck.apu_channel_enabled(Channel::Triangle));
}

/// The region decides the frame rate and the pixel aspect, so changing it has to re-declare the AV
/// info - a frontend that was not told would keep pacing the console at the old rate.
#[test]
fn changing_the_region_re_declares_the_av_info() {
    let session = Session::start();
    assert!(session.load());
    FRONTEND.with_borrow_mut(|f| f.av_infos.clear());

    set_option("tetanes_region", "pal");
    session.run(1);

    // SAFETY: as above.
    assert_eq!(
        unsafe { core::try_core() }.expect("a core").deck.region(),
        NesRegion::Pal
    );
    let infos = FRONTEND.with_borrow(|f| f.av_infos.clone());
    assert_eq!(infos.len(), 1, "told exactly once, not once a frame");
    assert!(
        (infos[0].timing.fps - 50.006_98).abs() < 1e-6,
        "the PAL rate, got {}",
        infos[0].timing.fps
    );

    // A poll that changes nothing must not re-declare it: a frontend rebuilds audio and video on
    // this, so doing it per frame would be audible.
    session.run(5);
    assert_eq!(FRONTEND.with_borrow(|f| f.av_infos.len()), 1);
}

/// A minimal battery-backed NROM cart.
///
/// Built rather than committed: no ROM in `test_roms` has a battery, and the save-RAM descriptor
/// and the `RETRO_MEMORY_SAVE_RAM` region only exist for a cart that does.
fn battery_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 16 + 16384 + 8192];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1; // one 16 KiB PRG bank
    rom[5] = 1; // one 8 KiB CHR bank
    rom[6] = 0x02; // battery-backed
    // The reset vector, so the CPU starts somewhere real rather than at whatever $FFFC holds.
    let reset = 16 + 16384 - 4;
    rom[reset] = 0x00;
    rom[reset + 1] = 0x80;
    rom
}

/// What the map says is what RetroAchievements addresses the game through, so the window and the
/// mirroring matter as much as the pointer.
#[test]
fn the_memory_map_describes_the_cpu_address_space() {
    let session = Session::start();
    assert!(session.load(), "the test ROM loads");

    let map = FRONTEND.with_borrow(|f| f.memory_map.clone());
    assert_eq!(
        map.len(),
        1,
        "spritecans has no battery, so there is nothing to describe at $6000"
    );

    let wram = map[0];
    assert_eq!(wram.flags, RETRO_MEMDESC_SYSTEM_RAM);
    assert_eq!(wram.start, 0x0000);
    assert_eq!(wram.len, 0x0800, "2 KiB of work RAM");
    assert_eq!(
        wram.select, 0xE000,
        "decoded across $0000-$1FFF, so the four mirrors are one descriptor"
    );
    assert_eq!(
        wram.ptr,
        retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM) as usize,
        "the map and `retro_get_memory_data` describe the same buffer"
    );
}

/// A cart with a battery gets the second descriptor, and it points at the same buffer the frontend
/// writes a restored `.srm` into.
#[test]
fn a_battery_adds_the_cartridge_ram_to_the_map() {
    let _session = Session::start();
    let rom = battery_rom();
    let info = retro_game_info {
        path: c"battery.nes".as_ptr(),
        data: rom.as_ptr().cast::<c_void>(),
        size: rom.len(),
        meta: std::ptr::null(),
    };
    // SAFETY: `info` outlives the call and `data` covers `size`.
    assert!(
        unsafe { retro_load_game(&raw const info) },
        "the cart loads"
    );

    let map = FRONTEND.with_borrow(|f| f.memory_map.clone());
    assert_eq!(map.len(), 2, "work RAM and the battery");

    let save = map[1];
    assert_eq!(save.flags, RETRO_MEMDESC_SAVE_RAM);
    assert_eq!(save.start, 0x6000);
    assert_eq!(save.select, 0xE000, "decoded across $6000-$7FFF");
    assert_eq!(
        save.len, 0x2000,
        "8 KiB of cartridge RAM, filling the window"
    );
    assert_eq!(
        save.ptr,
        retro_get_memory_data(RETRO_MEMORY_SAVE_RAM) as usize
    );
    assert_eq!(save.len, retro_get_memory_size(RETRO_MEMORY_SAVE_RAM));
}

/// The frontend caches the pointer once and reads through it for the rest of the session, so a
/// restore that moved the buffer would leave it reading freed memory.
#[test]
fn the_memory_pointer_survives_a_state_restore() {
    let session = Session::start();
    assert!(session.load());
    session.run(10);

    let before = retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM);
    let size = retro_serialize_size();
    let mut buffer = vec![0; size];
    // SAFETY: the buffer is the size the core asked for.
    assert!(unsafe { retro_serialize(buffer.as_mut_ptr().cast::<c_void>(), size) });
    session.run(10);
    // SAFETY: the same buffer, as the core wrote it.
    assert!(unsafe { retro_unserialize(buffer.as_ptr().cast::<c_void>(), size) });

    assert_eq!(
        before,
        retro_get_memory_data(RETRO_MEMORY_SYSTEM_RAM),
        "the pointer the frontend cached is still good"
    );
}

/// Cheats through the exports, which is the path a frontend's cheat menu takes.
#[test]
fn a_cheat_reaches_the_game_and_clearing_it_lets_go() {
    let session = Session::start();
    assert!(session.load());
    session.run(10);

    // SAFETY: the core is initialised and this thread is the frontend's.
    let peek = || {
        unsafe { core::try_core() }
            .expect("a core")
            .deck
            .bus()
            .peek(0x00A2)
    };

    // Whatever the game left at $00A2, the cheat has to override it - so the value chosen is one
    // it is not already holding.
    let plain = peek();
    let cheated = plain.wrapping_add(1);
    let code = std::ffi::CString::new(format!("00A2:{cheated:02X}")).expect("a code");
    // SAFETY: a NUL-terminated string that outlives the call.
    unsafe { retro_cheat_set(0, true, code.as_ptr()) };
    assert_eq!(peek(), cheated, "the cheat substitutes on read");

    retro_cheat_reset();
    assert_eq!(peek(), plain, "and resetting puts the game back");

    // Ejecting the cart has to clear them too: the console keeps its patch table across a cart
    // change, so a code entered here would still be substituting in the next game. Asserted
    // against the table rather than a read, since a fresh console's RAM is not the running one's.
    unsafe { retro_cheat_set(0, true, code.as_ptr()) };
    retro_unload_game();
    assert!(session.load(), "the cart goes back in");
    // SAFETY: as above.
    let patches = unsafe { core::try_core() }
        .expect("a core")
        .deck
        .patches()
        .count();
    assert_eq!(patches, 0, "the previous game's cheat is gone");
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

    // And the same round trip, on this cart's board. The committed ROMs are all small mappers;
    // what a sweep of real carts catches is a board whose state is a different size once its
    // registers have been written to, which is the way the promised size gets broken.
    let size = retro_serialize_size();
    assert!(size > 0, "{path} has a state to take");
    let mut buffer = vec![0xAA; size];
    // SAFETY: the buffer is the size the core asked for.
    assert!(
        unsafe { retro_serialize(buffer.as_mut_ptr().cast::<c_void>(), size) },
        "{path} serializes {FRAMES} frames in"
    );

    session.run(100);
    let straight = FRONTEND.with_borrow(|f| f.frames[FRAMES..FRAMES + 100].to_vec());

    // SAFETY: the same buffer, as the core wrote it.
    assert!(
        unsafe { retro_unserialize(buffer.as_ptr().cast::<c_void>(), size) },
        "{path} restores"
    );
    session.run(100);
    let replayed = FRONTEND.with_borrow(|f| f.frames[FRAMES + 100..].to_vec());

    assert_eq!(straight, replayed, "{path} replayed the run differently");
    assert_eq!(retro_serialize_size(), size, "{path} grew its state");
}
