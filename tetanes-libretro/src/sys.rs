//! The slice of `libretro.h` this core uses, transcribed by hand.
//!
//! Hand-written rather than generated: the bindgen-based crates run `libclang` from a build script,
//! which every build machine and every cross-compiled target would then need. The C API is versioned
//! ([`RETRO_API_VERSION`]) and grows by adding environment numbers rather than by changing what is
//! here, so the transcription is a one-time cost.
//!
//! What keeps it honest is `tests::field_offsets_match_the_c_header`: every `#[repr(C)]` struct
//! below asserts the offset of each field and its own size, so a mistranscribed field is a failing
//! test rather than a frontend reading garbage.
//!
//! `RETRO_CALLCONV` is empty everywhere this core builds, so every function pointer is plain
//! `extern "C"`.

#![allow(non_camel_case_types)]
// A transcription of someone else's header: the constants are the API's, and which of them this
// core happens to call today is not a reason to leave the others out and re-derive them later.
#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_uint, c_void};

/// ABI revision this core implements, returned from `retro_api_version`.
pub const RETRO_API_VERSION: c_uint = 1;

/// Marks an environment call the frontend may not implement yet.
pub const RETRO_ENVIRONMENT_EXPERIMENTAL: c_uint = 0x10000;

pub const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: c_uint = 9;
pub const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;
pub const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: c_uint = 11;
pub const RETRO_ENVIRONMENT_GET_VARIABLE: c_uint = 15;
pub const RETRO_ENVIRONMENT_SET_VARIABLES: c_uint = 16;
pub const RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: c_uint = 17;
pub const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: c_uint = 27;
pub const RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: c_uint = 31;
pub const RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO: c_uint = 32;
pub const RETRO_ENVIRONMENT_SET_CONTROLLER_INFO: c_uint = 35;
pub const RETRO_ENVIRONMENT_SET_MEMORY_MAPS: c_uint = 36 | RETRO_ENVIRONMENT_EXPERIMENTAL;
pub const RETRO_ENVIRONMENT_SET_GEOMETRY: c_uint = 37;
pub const RETRO_ENVIRONMENT_GET_FASTFORWARDING: c_uint = 49 | RETRO_ENVIRONMENT_EXPERIMENTAL;
pub const RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION: c_uint = 52;
pub const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2: c_uint = 67;

/// Pixel layout the frontend expects in [`retro_video_refresh_t`].
///
/// `XRGB8888` is `0x00RRGGBB` read as a native-endian `u32`.
pub const RETRO_PIXEL_FORMAT_0RGB1555: c_uint = 0;
pub const RETRO_PIXEL_FORMAT_XRGB8888: c_uint = 1;
pub const RETRO_PIXEL_FORMAT_RGB565: c_uint = 2;

/// Which console the frontend is asking about in `retro_get_memory_data`.
pub const RETRO_MEMORY_SAVE_RAM: c_uint = 0;
pub const RETRO_MEMORY_RTC: c_uint = 1;
pub const RETRO_MEMORY_SYSTEM_RAM: c_uint = 2;
pub const RETRO_MEMORY_VIDEO_RAM: c_uint = 3;

/// Timing family, returned from `retro_get_region`.
pub const RETRO_REGION_NTSC: c_uint = 0;
pub const RETRO_REGION_PAL: c_uint = 1;

/// What is plugged into a port.
pub const RETRO_DEVICE_NONE: c_uint = 0;
pub const RETRO_DEVICE_JOYPAD: c_uint = 1;
pub const RETRO_DEVICE_LIGHTGUN: c_uint = 4;

/// A frontend may pass a *subclass* of a device - `RETRO_DEVICE_SUBCLASS` shifts an ordinal into
/// the high bits - so the base device is what survives this mask.
pub const RETRO_DEVICE_MASK: c_uint = 0xFF;

/// Light gun axes and buttons.
///
/// `SCREEN_X` and `SCREEN_Y` are absolute, spanning `-0x8000..=0x7FFF` across the viewport rather
/// than the frame, and are only meaningful when `IS_OFFSCREEN` reads zero.
pub const RETRO_DEVICE_ID_LIGHTGUN_TRIGGER: c_uint = 2;
pub const RETRO_DEVICE_ID_LIGHTGUN_AUX_A: c_uint = 3;
pub const RETRO_DEVICE_ID_LIGHTGUN_AUX_B: c_uint = 4;
pub const RETRO_DEVICE_ID_LIGHTGUN_START: c_uint = 6;
pub const RETRO_DEVICE_ID_LIGHTGUN_SELECT: c_uint = 7;
pub const RETRO_DEVICE_ID_LIGHTGUN_SCREEN_X: c_uint = 13;
pub const RETRO_DEVICE_ID_LIGHTGUN_SCREEN_Y: c_uint = 14;
pub const RETRO_DEVICE_ID_LIGHTGUN_IS_OFFSCREEN: c_uint = 15;
/// A shot deliberately fired off-screen, which is how a game is told to reload.
pub const RETRO_DEVICE_ID_LIGHTGUN_RELOAD: c_uint = 16;

/// RetroPad buttons, in the order the frontend numbers them.
///
/// Note the NES layout is mirrored against this one: RetroPad `B` is the NES `A`.
pub const RETRO_DEVICE_ID_JOYPAD_B: c_uint = 0;
pub const RETRO_DEVICE_ID_JOYPAD_Y: c_uint = 1;
pub const RETRO_DEVICE_ID_JOYPAD_SELECT: c_uint = 2;
pub const RETRO_DEVICE_ID_JOYPAD_START: c_uint = 3;
pub const RETRO_DEVICE_ID_JOYPAD_UP: c_uint = 4;
pub const RETRO_DEVICE_ID_JOYPAD_DOWN: c_uint = 5;
pub const RETRO_DEVICE_ID_JOYPAD_LEFT: c_uint = 6;
pub const RETRO_DEVICE_ID_JOYPAD_RIGHT: c_uint = 7;
pub const RETRO_DEVICE_ID_JOYPAD_A: c_uint = 8;
pub const RETRO_DEVICE_ID_JOYPAD_X: c_uint = 9;

/// What a [`retro_memory_descriptor`] is, and how it may be accessed.
///
/// `ALIGN` and `MINSIZE` say the smallest access the hardware can make; both are absent here
/// because the 6502 addresses memory one byte at a time.
pub const RETRO_MEMDESC_CONST: u64 = 1 << 0;
pub const RETRO_MEMDESC_BIGENDIAN: u64 = 1 << 1;
pub const RETRO_MEMDESC_SYSTEM_RAM: u64 = 1 << 2;
pub const RETRO_MEMDESC_SAVE_RAM: u64 = 1 << 3;
pub const RETRO_MEMDESC_VIDEO_RAM: u64 = 1 << 4;

/// Severity passed to [`retro_log_printf_t`].
pub const RETRO_LOG_DEBUG: c_int = 0;
pub const RETRO_LOG_INFO: c_int = 1;
pub const RETRO_LOG_WARN: c_int = 2;
pub const RETRO_LOG_ERROR: c_int = 3;

/// Access an obscure frontend feature. Returns whether the frontend recognised `cmd`.
pub type retro_environment_t = unsafe extern "C" fn(cmd: c_uint, data: *mut c_void) -> bool;
/// Hand the frontend one frame. `pitch` is bytes per row, so a trimmed view can point into a
/// larger buffer rather than copying out of it.
pub type retro_video_refresh_t =
    unsafe extern "C" fn(data: *const c_void, width: c_uint, height: c_uint, pitch: usize);
/// Hand the frontend one stereo sample.
pub type retro_audio_sample_t = unsafe extern "C" fn(left: i16, right: i16);
/// Hand the frontend interleaved stereo samples. `frames` is pairs, not values.
pub type retro_audio_sample_batch_t =
    unsafe extern "C" fn(data: *const i16, frames: usize) -> usize;
/// Latch input for this frame. Must be called before [`retro_input_state_t`].
pub type retro_input_poll_t = unsafe extern "C" fn();
/// Read one latched button, axis or pointer coordinate.
pub type retro_input_state_t =
    unsafe extern "C" fn(port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16;
/// The frontend's logger, which is where a core's diagnostics have to go: stderr is swallowed on
/// most platforms.
pub type retro_log_printf_t = unsafe extern "C" fn(level: c_int, fmt: *const c_char, ...);

/// Static description of the core, filled in by `retro_get_system_info`.
#[repr(C)]
#[derive(Debug)]
pub struct retro_system_info {
    pub library_name: *const c_char,
    pub library_version: *const c_char,
    /// Pipe-separated, no dots: `"nes|unf|unif"`.
    pub valid_extensions: *const c_char,
    /// Whether the frontend must hand over a path rather than the ROM's bytes.
    pub need_fullpath: bool,
    pub block_extract: bool,
}

/// Frame dimensions and pixel aspect.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct retro_game_geometry {
    pub base_width: c_uint,
    pub base_height: c_uint,
    /// The frontend sizes its texture from this once, so it must cover every geometry the core
    /// will later ask for.
    pub max_width: c_uint,
    pub max_height: c_uint,
    pub aspect_ratio: f32,
}

/// Frame and sample rates the frontend paces itself by.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct retro_system_timing {
    pub fps: f64,
    pub sample_rate: f64,
}

/// Everything the frontend needs to set up video and audio, filled in after content loads.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct retro_system_av_info {
    pub geometry: retro_game_geometry,
    pub timing: retro_system_timing,
}

/// Content handed to `retro_load_game`.
///
/// With `need_fullpath` false the bytes are in `data`, and `path` may be null - an archive member
/// or a netplay client has no file behind it.
#[repr(C)]
#[derive(Debug)]
pub struct retro_game_info {
    pub path: *const c_char,
    pub data: *const c_void,
    pub size: usize,
    pub meta: *const c_char,
}

/// One core option's current value, or the key being asked about.
#[repr(C)]
#[derive(Debug)]
pub struct retro_variable {
    pub key: *const c_char,
    /// Null on the way in; the frontend fills it, or leaves it null if it has no setting.
    pub value: *const c_char,
}

/// What one button on one port is called, for the frontend's remapping UI.
#[repr(C)]
#[derive(Debug)]
pub struct retro_input_descriptor {
    pub port: c_uint,
    pub device: c_uint,
    pub index: c_uint,
    pub id: c_uint,
    pub description: *const c_char,
}

/// A device the core accepts on a port.
#[repr(C)]
#[derive(Debug)]
pub struct retro_controller_description {
    pub desc: *const c_char,
    pub id: c_uint,
}

/// The devices one port accepts, terminated by a zeroed [`retro_controller_description`].
#[repr(C)]
#[derive(Debug)]
pub struct retro_controller_info {
    pub types: *const retro_controller_description,
    pub num_types: c_uint,
}

/// Values one core option may take. The array is fixed by the C API and NUL-terminated in use, so
/// most of it is null for any real option.
pub const RETRO_NUM_CORE_OPTION_VALUES_MAX: usize = 128;

/// One selectable value, and what the menu calls it.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct retro_core_option_value {
    pub value: *const c_char,
    /// Shown instead of `value`, which is the string the core reads back.
    pub label: *const c_char,
}

impl retro_core_option_value {
    /// The terminator, and what the unused tail of a `values` array is filled with.
    pub const NONE: Self = Self {
        value: std::ptr::null(),
        label: std::ptr::null(),
    };
}

/// A submenu options are grouped under.
#[repr(C)]
#[derive(Debug)]
pub struct retro_core_option_v2_category {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub info: *const c_char,
}

/// One core option: its key, how it is described, and what it may be set to.
///
/// The `_categorized` strings are the shorter forms shown once a category already names the
/// context; a frontend without categories uses the plain ones.
#[repr(C)]
#[derive(Debug)]
pub struct retro_core_option_v2_definition {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub desc_categorized: *const c_char,
    pub info: *const c_char,
    pub info_categorized: *const c_char,
    pub category_key: *const c_char,
    pub values: [retro_core_option_value; RETRO_NUM_CORE_OPTION_VALUES_MAX],
    pub default_value: *const c_char,
}

/// The options handed to `SET_CORE_OPTIONS_V2`. Both arrays end with a zeroed entry.
#[repr(C)]
#[derive(Debug)]
pub struct retro_core_options_v2 {
    pub categories: *mut retro_core_option_v2_category,
    pub definitions: *mut retro_core_option_v2_definition,
}

/// One region of the emulated address space, for `SET_MEMORY_MAPS`.
///
/// The mapping is `(addr & select) == start`, and the byte reached is `addr` masked down to
/// `len` - so a region smaller than the window `select` opens is mirrored across it, which is how
/// one descriptor describes the NES' four copies of work RAM.
#[repr(C)]
#[derive(Debug)]
pub struct retro_memory_descriptor {
    pub flags: u64,
    /// The core's own buffer. It must stay where it is for as long as the frontend has the map.
    pub ptr: *mut c_void,
    /// Where in `ptr` this region starts.
    pub offset: usize,
    pub start: usize,
    /// Address bits compared against `start`. Zero means "the smallest window that fits `len`".
    pub select: usize,
    /// Address bits the chip is not wired to.
    pub disconnect: usize,
    pub len: usize,
    /// Names a second address space, for a machine that has one. Null for the CPU's.
    pub addrspace: *const c_char,
}

/// The descriptors handed to `SET_MEMORY_MAPS`.
#[repr(C)]
#[derive(Debug)]
pub struct retro_memory_map {
    pub descriptors: *const retro_memory_descriptor,
    pub num_descriptors: c_uint,
}

/// The frontend's logger, returned by `GET_LOG_INTERFACE`.
#[repr(C)]
#[derive(Debug)]
pub struct retro_log_callback {
    pub log: Option<retro_log_printf_t>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    /// Transcribing a C header by hand risks a field of the wrong width or in the wrong place, and
    /// no compiler here can catch it - the frontend simply reads the wrong bytes. Field offsets are
    /// what that shows up in, so they are what is pinned.
    ///
    /// Written for 64-bit, which is every target this core ships for; a 32-bit build skips it
    /// rather than asserting offsets that would legitimately differ.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn field_offsets_match_the_c_header() {
        assert_eq!(offset_of!(retro_system_info, library_name), 0);
        assert_eq!(offset_of!(retro_system_info, library_version), 8);
        assert_eq!(offset_of!(retro_system_info, valid_extensions), 16);
        assert_eq!(offset_of!(retro_system_info, need_fullpath), 24);
        assert_eq!(offset_of!(retro_system_info, block_extract), 25);
        assert_eq!(size_of::<retro_system_info>(), 32);

        assert_eq!(offset_of!(retro_game_geometry, base_width), 0);
        assert_eq!(offset_of!(retro_game_geometry, base_height), 4);
        assert_eq!(offset_of!(retro_game_geometry, max_width), 8);
        assert_eq!(offset_of!(retro_game_geometry, max_height), 12);
        assert_eq!(offset_of!(retro_game_geometry, aspect_ratio), 16);
        assert_eq!(size_of::<retro_game_geometry>(), 20);

        assert_eq!(offset_of!(retro_system_timing, fps), 0);
        assert_eq!(offset_of!(retro_system_timing, sample_rate), 8);
        assert_eq!(size_of::<retro_system_timing>(), 16);

        // `geometry` ends at 20 and `timing` needs 8-byte alignment, so C pads to 24.
        assert_eq!(offset_of!(retro_system_av_info, geometry), 0);
        assert_eq!(offset_of!(retro_system_av_info, timing), 24);
        assert_eq!(size_of::<retro_system_av_info>(), 40);

        assert_eq!(offset_of!(retro_game_info, path), 0);
        assert_eq!(offset_of!(retro_game_info, data), 8);
        assert_eq!(offset_of!(retro_game_info, size), 16);
        assert_eq!(offset_of!(retro_game_info, meta), 24);
        assert_eq!(size_of::<retro_game_info>(), 32);

        assert_eq!(offset_of!(retro_variable, key), 0);
        assert_eq!(offset_of!(retro_variable, value), 8);
        assert_eq!(size_of::<retro_variable>(), 16);

        assert_eq!(offset_of!(retro_input_descriptor, port), 0);
        assert_eq!(offset_of!(retro_input_descriptor, device), 4);
        assert_eq!(offset_of!(retro_input_descriptor, index), 8);
        assert_eq!(offset_of!(retro_input_descriptor, id), 12);
        assert_eq!(offset_of!(retro_input_descriptor, description), 16);
        assert_eq!(size_of::<retro_input_descriptor>(), 24);

        assert_eq!(offset_of!(retro_controller_description, desc), 0);
        assert_eq!(offset_of!(retro_controller_description, id), 8);
        assert_eq!(size_of::<retro_controller_description>(), 16);

        assert_eq!(offset_of!(retro_controller_info, types), 0);
        assert_eq!(offset_of!(retro_controller_info, num_types), 8);
        assert_eq!(size_of::<retro_controller_info>(), 16);

        assert_eq!(offset_of!(retro_core_option_value, value), 0);
        assert_eq!(offset_of!(retro_core_option_value, label), 8);
        assert_eq!(size_of::<retro_core_option_value>(), 16);

        assert_eq!(offset_of!(retro_core_option_v2_category, key), 0);
        assert_eq!(offset_of!(retro_core_option_v2_category, desc), 8);
        assert_eq!(offset_of!(retro_core_option_v2_category, info), 16);
        assert_eq!(size_of::<retro_core_option_v2_category>(), 24);

        assert_eq!(offset_of!(retro_core_option_v2_definition, key), 0);
        assert_eq!(offset_of!(retro_core_option_v2_definition, desc), 8);
        assert_eq!(
            offset_of!(retro_core_option_v2_definition, desc_categorized),
            16
        );
        assert_eq!(offset_of!(retro_core_option_v2_definition, info), 24);
        assert_eq!(
            offset_of!(retro_core_option_v2_definition, info_categorized),
            32
        );
        assert_eq!(
            offset_of!(retro_core_option_v2_definition, category_key),
            40
        );
        assert_eq!(offset_of!(retro_core_option_v2_definition, values), 48);
        // 48 + 128 values of 16 bytes each.
        assert_eq!(
            offset_of!(retro_core_option_v2_definition, default_value),
            2096
        );
        assert_eq!(size_of::<retro_core_option_v2_definition>(), 2104);

        assert_eq!(offset_of!(retro_core_options_v2, categories), 0);
        assert_eq!(offset_of!(retro_core_options_v2, definitions), 8);
        assert_eq!(size_of::<retro_core_options_v2>(), 16);

        assert_eq!(offset_of!(retro_memory_descriptor, flags), 0);
        assert_eq!(offset_of!(retro_memory_descriptor, ptr), 8);
        assert_eq!(offset_of!(retro_memory_descriptor, offset), 16);
        assert_eq!(offset_of!(retro_memory_descriptor, start), 24);
        assert_eq!(offset_of!(retro_memory_descriptor, select), 32);
        assert_eq!(offset_of!(retro_memory_descriptor, disconnect), 40);
        assert_eq!(offset_of!(retro_memory_descriptor, len), 48);
        assert_eq!(offset_of!(retro_memory_descriptor, addrspace), 56);
        assert_eq!(size_of::<retro_memory_descriptor>(), 64);

        // `num_descriptors` is four bytes, then C pads to the pointer's alignment.
        assert_eq!(offset_of!(retro_memory_map, descriptors), 0);
        assert_eq!(offset_of!(retro_memory_map, num_descriptors), 8);
        assert_eq!(size_of::<retro_memory_map>(), 16);

        assert_eq!(size_of::<retro_log_callback>(), 8);
    }

    /// A nullable `extern "C"` fn pointer has to be exactly a pointer, or a struct holding one does
    /// not match what the frontend writes into it.
    #[test]
    fn a_nullable_callback_is_pointer_sized() {
        assert_eq!(
            size_of::<Option<retro_log_printf_t>>(),
            size_of::<*const c_void>()
        );
        assert_eq!(
            align_of::<Option<retro_log_printf_t>>(),
            align_of::<*const c_void>()
        );
    }
}
