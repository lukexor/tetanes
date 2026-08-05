//! A [libretro](https://www.libretro.com) core for TetaNES.
//!
//! A frontend `dlopen`s this library and resolves all twenty-five `retro_*` symbols below; a
//! missing one is a core it refuses to load. Most are one-liners, and the emulation itself is
//! [`tetanes_core`].
//!
//! Everything here runs on the frontend's thread, against the single `Core` the `core` module
//! owns. Nothing may unwind across the boundary, so every export body goes through
//! `core::with_core`.
//!
//! Build it with `cargo make build-libretro`, which selects the `libretro` profile - a plain
//! `--release` build sets `panic = "abort"` and would turn a recoverable panic into a dead
//! frontend.

// This is the C side of an FFI boundary; the names are the C API's.
#![allow(non_snake_case)]

mod audio;
mod core;
mod input;
mod log;
/// The C API this core implements.
///
/// Public because the exported functions' signatures are written in these types.
pub mod sys;
#[cfg(test)]
mod tests;
mod video;

use crate::core::with_core;
use std::{
    ffi::{CStr, c_char, c_uint, c_void},
    slice,
};
use sys::*;
use tetanes_core::{
    common::{NesRegion, ResetKind},
    control_deck::Clocked,
    input::JoypadBtnState,
    ppu::size,
};

// A plain `--release` build would set `panic = "abort"`, and a panic in emulation would take the
// frontend down with it rather than becoming a logged message.
#[cfg(panic = "abort")]
compile_error!("build with `--profile libretro`, which sets `panic = \"unwind\"`");

// ---------------------------------------------------------------------------------------------
// Callback registration. These arrive before `retro_init`, so they are kept across it.
// ---------------------------------------------------------------------------------------------

/// Hands over the environment callback, and is the first thing a frontend calls.
///
/// # Safety
///
/// `cb` must be callable for the life of the core.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_set_environment(cb: retro_environment_t) {
    core::guard((), || {
        // Before `retro_init`, so there may be no core yet to hang this on.
        // SAFETY: a libretro entry point, on the frontend's thread.
        unsafe {
            // The logger first, so that anything the console's construction has to say can be said.
            log::init(cb);
            if core::try_core().is_none() {
                core::init();
            }
            if let Some(core) = core::try_core() {
                core.callbacks.environment = Some(cb);
            }
        }
    });
}

macro_rules! setter {
    ($(#[$meta:meta])* $name:ident, $field:ident, $ty:ty) => {
        $(#[$meta])*
        ///
        /// # Safety
        ///
        /// The callback must be callable for the life of the core.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(cb: $ty) {
            with_core((), |core| core.callbacks.$field = Some(cb));
        }
    };
}

setter!(
    /// Hands over the callback one frame of video is passed to.
    retro_set_video_refresh, video_refresh, retro_video_refresh_t
);
setter!(
    /// Hands over the callback a single stereo sample is passed to.
    retro_set_audio_sample, audio_sample, retro_audio_sample_t
);
setter!(
    /// Hands over the callback a frame of interleaved stereo samples is passed to.
    retro_set_audio_sample_batch, audio_batch, retro_audio_sample_batch_t
);
setter!(
    /// Hands over the callback that latches input for a frame.
    retro_set_input_poll, input_poll, retro_input_poll_t
);
setter!(
    /// Hands over the callback that reads one latched button.
    retro_set_input_state, input_state, retro_input_state_t
);

// ---------------------------------------------------------------------------------------------
// Lifecycle.
// ---------------------------------------------------------------------------------------------

/// Creates the console.
///
/// # Safety
///
/// Must be called from a libretro entry point on the frontend's thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_init() {
    core::guard((), || {
        // SAFETY: as above.
        unsafe {
            if core::try_core().is_none() {
                core::init();
            }
        }
    });
}

/// Destroys the console.
///
/// # Safety
///
/// As [`retro_init`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_deinit() {
    core::guard((), || {
        // SAFETY: as above.
        unsafe { core::deinit() };
    });
    log::deinit();
}

/// The ABI revision this core implements.
#[unsafe(no_mangle)]
pub const extern "C" fn retro_api_version() -> c_uint {
    RETRO_API_VERSION
}

/// Describes the core, before any content is loaded.
///
/// # Safety
///
/// `info` must point at a writable [`retro_system_info`].
#[unsafe(no_mangle)]
pub const unsafe extern "C" fn retro_get_system_info(info: *mut retro_system_info) {
    let Some(info) = (unsafe { info.as_mut() }) else {
        return;
    };
    *info = retro_system_info {
        library_name: c"TetaNES".as_ptr(),
        library_version: VERSION.as_ptr().cast::<c_char>(),
        valid_extensions: c"nes".as_ptr(),
        // False, so the frontend hands over the ROM's bytes and this core never opens a file.
        need_fullpath: false,
        block_extract: false,
    };
}

/// Describes the video and audio the loaded content produces.
///
/// # Safety
///
/// `info` must point at a writable [`retro_system_av_info`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut retro_system_av_info) {
    let Some(out) = (unsafe { info.as_mut() }) else {
        return;
    };
    // Never a zeroed struct: a frontend divides by `fps` and `sample_rate` while setting itself
    // up, so answering with zeroes is worse than answering with the wrong region.
    *out = with_core(av_info(NesRegion::Ntsc), |core| av_info(core.deck.region()));
}

/// Selects what is plugged into a port. Unused until four-player and the zapper land.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub extern "C" fn retro_set_controller_port_device(_port: c_uint, _device: c_uint) {}

/// Soft-resets the console, as the front-panel button does.
#[unsafe(no_mangle)]
pub extern "C" fn retro_reset() {
    with_core((), |core| {
        core.wedged = false;
        core.pads.forget();
        core.deck.reset(ResetKind::Soft);
    });
}

/// Runs one frame: polls input, clocks the console, hands back video and audio.
///
/// # Safety
///
/// The frontend's callbacks must still be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_run() {
    with_core((), |core| {
        // SAFETY: callbacks come from the frontend and are valid until it unloads the core.
        unsafe {
            poll_input(core);

            // A display frame is however many NES frames the speed asks for; at 1x that is one.
            loop {
                match core.deck.clock_frame() {
                    Ok(Clocked::Continue) => continue,
                    Ok(_) => break,
                    Err(err) => {
                        log::error(&format!("failed to clock a frame: {err}"));
                        core.wedged = true;
                        return;
                    }
                }
            }

            if let Some(batch) = core.callbacks.audio_batch {
                let samples = core.audio.interleave(core.deck.audio_samples());
                if !samples.is_empty() {
                    batch(samples.as_ptr(), samples.len() / 2);
                }
            }

            if let Some(refresh) = core.callbacks.video_refresh {
                let filter = core.filter();
                let frame = core.video.frame(&mut core.deck, filter);
                refresh(
                    frame.as_ptr().cast::<c_void>(),
                    c_uint::from(size::WIDTH),
                    c_uint::from(size::HEIGHT),
                    video::PITCH,
                );
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Content.
// ---------------------------------------------------------------------------------------------

/// Loads a ROM from the bytes the frontend supplies.
///
/// # Safety
///
/// `game` must point at a [`retro_game_info`] whose `data` covers `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_load_game(game: *const retro_game_info) -> bool {
    let Some(game) = (unsafe { game.as_ref() }) else {
        return false;
    };
    if game.data.is_null() || game.size == 0 {
        log::error("no content was supplied");
        return false;
    }
    // SAFETY: the frontend guarantees `data` covers `size` bytes for the length of this call.
    let rom = unsafe { slice::from_raw_parts(game.data.cast::<u8>(), game.size) };
    // An archive member or a netplay client has no path, so the name is only for diagnostics and
    // has to have a fallback.
    let name = unsafe { game.path.as_ref() }
        .map(|path| {
            unsafe { CStr::from_ptr(path) }
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| "unnamed".to_string());

    with_core(false, |core| {
        core.wedged = false;
        core.pads.forget();
        match core.deck.load_rom(&name, &mut &rom[..]) {
            Ok(loaded) => {
                core.deck.set_sample_rate(audio::SAMPLE_RATE as f32);
                log::info(&format!("loaded {} ({:?})", loaded.name, loaded.region));
                unsafe { set_pixel_format(core) }
            }
            Err(err) => {
                log::error(&format!("failed to load content: {err}"));
                false
            }
        }
    })
}

/// Loading several images at once, which no NES cart needs.
#[unsafe(no_mangle)]
pub const extern "C" fn retro_load_game_special(
    _game_type: c_uint,
    _info: *const retro_game_info,
    _num_info: usize,
) -> bool {
    false
}

/// Ejects the cart.
#[unsafe(no_mangle)]
pub extern "C" fn retro_unload_game() {
    with_core((), |core| {
        core.wedged = false;
        core.pads.forget();
        // Infallible now that the deck performs no battery I/O of its own.
        let _ = core.deck.unload_rom();
    });
}

/// Which timing family the loaded cart runs at.
#[unsafe(no_mangle)]
pub extern "C" fn retro_get_region() -> c_uint {
    with_core(RETRO_REGION_NTSC, |core| {
        match core.deck.region() {
            // `Dendy` is a PAL-timed famiclone, and PAL is the only other answer available.
            NesRegion::Pal | NesRegion::Dendy => RETRO_REGION_PAL,
            NesRegion::Ntsc | NesRegion::Auto => RETRO_REGION_NTSC,
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Save states, memory and cheats. Filled in as the remaining phases land.
// ---------------------------------------------------------------------------------------------

/// Bytes a save state occupies. Zero until save states land.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub extern "C" fn retro_serialize_size() -> usize {
    0
}

/// Writes a save state.
///
/// # Safety
///
/// `data` must cover `size` bytes.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_serialize(_data: *mut c_void, _size: usize) -> bool {
    false
}

/// Restores a save state.
///
/// # Safety
///
/// `data` must cover `size` bytes.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_unserialize(_data: *const c_void, _size: usize) -> bool {
    false
}

/// Removes every cheat.
#[unsafe(no_mangle)]
pub extern "C" fn retro_cheat_reset() {
    with_core((), |core| core.deck.clear_genie_codes());
}

/// Applies or removes one cheat.
///
/// # Safety
///
/// `code` must be a NUL-terminated string.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_cheat_set(_index: c_uint, _enabled: bool, _code: *const c_char) {}

/// A pointer to one of the console's memory regions.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub extern "C" fn retro_get_memory_data(_id: c_uint) -> *mut c_void {
    std::ptr::null_mut()
}

/// How large that region is.
#[expect(
    clippy::missing_const_for_fn,
    reason = "a stub until the phase that fills it in"
)]
#[unsafe(no_mangle)]
pub extern "C" fn retro_get_memory_size(_id: c_uint) -> usize {
    0
}

// ---------------------------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------------------------

/// This crate's version, with the terminator C needs.
///
/// `env!` yields no NUL, and a `CString` built here would be freed while the frontend still held
/// the pointer, so the bytes are laid out at compile time instead.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");

/// Video and audio parameters for a region.
fn av_info(region: NesRegion) -> retro_system_av_info {
    let width = c_uint::from(size::WIDTH);
    let height = c_uint::from(size::HEIGHT);
    retro_system_av_info {
        geometry: retro_game_geometry {
            base_width: width,
            base_height: height,
            // The frontend sizes its texture from these once and never revisits them, so they have
            // to cover any geometry a later `SET_GEOMETRY` asks for.
            max_width: width,
            max_height: height,
            // Pixel aspect times the frame's own, since libretro wants the displayed ratio.
            aspect_ratio: (width as f32 * region.aspect_ratio()) / height as f32,
        },
        timing: retro_system_timing {
            // The true rates, not 60 and 50: a frontend paces itself by this, and rounding it
            // leaves audio drifting against video for as long as the game runs.
            fps: match region {
                NesRegion::Pal => 50.006_98,
                NesRegion::Dendy => 50.006_98,
                NesRegion::Ntsc | NesRegion::Auto => 60.098_814,
            },
            sample_rate: audio::SAMPLE_RATE,
        },
    }
}

/// Tells the frontend this core hands over `XRGB8888`.
///
/// # Safety
///
/// The environment callback must be valid.
unsafe fn set_pixel_format(core: &mut core::Core) -> bool {
    let Some(environment) = core.callbacks.environment else {
        return false;
    };
    let mut format = RETRO_PIXEL_FORMAT_XRGB8888;
    // SAFETY: the callback reads one `unsigned` through the pointer.
    let ok = unsafe {
        environment(
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
            std::ptr::from_mut(&mut format).cast::<c_void>(),
        )
    };
    if !ok {
        log::error("the frontend refused XRGB8888, which this core requires");
    }
    ok
}

/// Latches input and applies it to the joypads.
///
/// # Safety
///
/// The input callbacks must be valid.
unsafe fn poll_input(core: &mut core::Core) {
    let (Some(poll), Some(state)) = (core.callbacks.input_poll, core.callbacks.input_state) else {
        return;
    };
    // SAFETY: both came from the frontend; `poll` must precede any `state` read.
    unsafe {
        poll();
        for (port, player) in input::PORTS.into_iter().enumerate() {
            let mut held = JoypadBtnState::empty();
            for (id, button) in input::BUTTONS {
                if state(port as c_uint, RETRO_DEVICE_JOYPAD, 0, id) != 0 {
                    held.insert(button);
                }
            }
            core.pads.apply(port, core.deck.joypad_mut(player), held);
        }
    }
}
