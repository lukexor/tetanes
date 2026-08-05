//! The settings a frontend exposes, and what they do to the console.
//!
//! libretro's model is one-way and stringly typed: the core declares its options once, the frontend
//! stores whatever the player picked, and the core reads each key back by name. There is no event
//! when one changes - `GET_VARIABLE_UPDATE` says only that *something* did - so every option is
//! re-read and re-applied together, which is why [`Options`] holds the last applied set and
//! compares.
//!
//! Only settings the frontend cannot do itself belong here. Overscan cropping and aspect
//! correction are deliberately absent: RetroArch has both, and a second copy in the core would be
//! one more place for them to disagree.

use crate::{
    core::Core,
    log,
    sys::{
        RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION, RETRO_ENVIRONMENT_GET_FASTFORWARDING,
        RETRO_ENVIRONMENT_GET_VARIABLE, RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE,
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2, RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO,
        RETRO_ENVIRONMENT_SET_VARIABLES, RETRO_NUM_CORE_OPTION_VALUES_MAX,
        retro_core_option_v2_category, retro_core_option_v2_definition, retro_core_option_value,
        retro_core_options_v2, retro_environment_t, retro_variable,
    },
};
use std::ffi::{CStr, CString, c_uint, c_void};
use tetanes_core::{
    apu::Channel,
    common::NesRegion,
    input::FourPlayer,
    mapper::{Bf909Revision, MapperRevision, Mmc3Revision},
    memory::RamState,
    video::VideoFilter,
};

/// A submenu, for a frontend that groups options.
struct Category {
    key: &'static CStr,
    desc: &'static CStr,
    info: &'static CStr,
}

/// One option, in the compact form this core writes it in. The C shape is built from these.
struct Def {
    key: &'static CStr,
    desc: &'static CStr,
    info: &'static CStr,
    category: &'static CStr,
    /// `(value, label)`, the first of which is the default.
    values: &'static [(&'static CStr, &'static CStr)],
}

const VIDEO: &CStr = c"video";
const SYSTEM: &CStr = c"system";
const INPUT: &CStr = c"input";
const AUDIO: &CStr = c"audio";
const BOARDS: &CStr = c"boards";

const CATEGORIES: &[Category] = &[
    Category {
        key: VIDEO,
        desc: c"Video",
        info: c"How the picture is produced.",
    },
    Category {
        key: SYSTEM,
        desc: c"System",
        info: c"What machine is being emulated, and how it starts up.",
    },
    Category {
        key: INPUT,
        desc: c"Input",
        info: c"Controllers and adapters.",
    },
    Category {
        key: AUDIO,
        desc: c"Audio",
        info: c"Individual sound channels.",
    },
    Category {
        key: BOARDS,
        desc: c"Cartridge Boards",
        info: c"Revisions of boards that cannot be told apart from a ROM header.",
    },
];

const OFF_ON: &[(&CStr, &CStr)] = &[(c"disabled", c"Disabled"), (c"enabled", c"Enabled")];
const ON_OFF: &[(&CStr, &CStr)] = &[(c"enabled", c"Enabled"), (c"disabled", c"Disabled")];

/// Keys, kept as constants because each is named twice - once declared, once read back.
const REGION: &CStr = c"tetanes_region";
const FILTER: &CStr = c"tetanes_filter";
const RAM_STATE: &CStr = c"tetanes_ram_state";
const PPU_WARMUP: &CStr = c"tetanes_ppu_warmup";
const FOUR_PLAYER: &CStr = c"tetanes_four_player";
const CONCURRENT_DPAD: &CStr = c"tetanes_concurrent_dpad";
const RUN_AHEAD: &CStr = c"tetanes_run_ahead";
const MMC3_REVISION: &CStr = c"tetanes_mmc3_revision";
const BF909_REVISION: &CStr = c"tetanes_bf909_revision";

/// The six APU channels, each a `disabled`/`enabled` option of its own.
const CHANNELS: [(&CStr, Channel); 6] = [
    (c"tetanes_apu_pulse1", Channel::Pulse1),
    (c"tetanes_apu_pulse2", Channel::Pulse2),
    (c"tetanes_apu_triangle", Channel::Triangle),
    (c"tetanes_apu_noise", Channel::Noise),
    (c"tetanes_apu_dmc", Channel::Dmc),
    (c"tetanes_apu_mapper", Channel::Mapper),
];

const DEFS: &[Def] = &[
    Def {
        key: REGION,
        desc: c"Region",
        info: c"Which timing family to run at. Auto takes it from the ROM and the game database, \
                and is right for all but a handful of carts.",
        category: SYSTEM,
        values: &[
            (c"auto", c"Auto"),
            (c"ntsc", c"NTSC"),
            (c"pal", c"PAL"),
            (c"dendy", c"Dendy"),
        ],
    },
    Def {
        key: FILTER,
        desc: c"Video Filter",
        info: c"Pixellate hands over the palette as-is. NTSC reproduces composite artefacts - the \
                colour blending games were drawn for - and costs more per frame.",
        category: VIDEO,
        values: &[(c"pixellate", c"Pixellate"), (c"ntsc", c"NTSC")],
    },
    Def {
        key: RAM_STATE,
        desc: c"Power-on RAM State",
        info: c"What memory holds before the game writes it. Random is closest to a real console; \
                a fixed fill makes a run reproducible. Takes effect on the next power cycle.",
        category: SYSTEM,
        values: &[
            (c"random", c"Random"),
            (c"all_zeros", c"All Zeros"),
            (c"all_ones", c"All Ones"),
        ],
    },
    Def {
        key: PPU_WARMUP,
        desc: c"Emulate PPU Warmup",
        info: c"The PPU ignores writes for the first frames after power-on, as hardware does. A \
                few homebrew ROMs rely on it; some older ones break under it.",
        category: SYSTEM,
        values: OFF_ON,
    },
    Def {
        key: FOUR_PLAYER,
        desc: c"Four Player Adapter",
        info: c"Four Score is the NES adapter, Satellite the Famicom one. A game supports one or \
                the other, not both.",
        category: INPUT,
        values: &[
            (c"disabled", c"Disabled"),
            (c"four_score", c"NES Four Score"),
            (c"satellite", c"Famicom Four Players Adapter"),
        ],
    },
    Def {
        key: CONCURRENT_DPAD,
        desc: c"Concurrent D-Pad",
        info: c"Allows left+right and up+down at once, which the original hardware could not do. \
                Some games glitch when given both.",
        category: INPUT,
        values: OFF_ON,
    },
    Def {
        key: RUN_AHEAD,
        desc: c"Run-Ahead Frames",
        info: c"Hides input lag by clocking the console ahead and rewinding. Cheaper here than \
                the frontend's own, which serializes a state every frame - use one or the other, \
                not both. Only applies at 1x speed.",
        category: SYSTEM,
        values: &[
            (c"0", c"0 (Disabled)"),
            (c"1", c"1"),
            (c"2", c"2"),
            (c"3", c"3"),
            (c"4", c"4"),
        ],
    },
    Def {
        key: MMC3_REVISION,
        desc: c"MMC3 Revision",
        info: c"Only tells itself apart by which games misbehave: the IRQ fires a scanline later \
                on an A. The game database overrides this for carts it knows.",
        category: BOARDS,
        values: &[(c"bc", c"MMC3B/C"), (c"a", c"MMC3A"), (c"acc", c"MMC3Acc")],
    },
    Def {
        key: BF909_REVISION,
        desc: c"BF909 Revision",
        info: c"Camerica's board, whose two revisions differ in how the PRG bank register is \
                decoded.",
        category: BOARDS,
        values: &[(c"bf909x", c"BF909x"), (c"bf9097", c"BF9097")],
    },
];

/// What the frontend was last found to have set.
///
/// Kept so that applying can tell a change from a re-read: most settings are idempotent, but
/// region re-declares the AV info, which a frontend may act on by tearing down its audio.
#[derive(Clone, Copy)]
pub struct Options {
    region: NesRegion,
    filter: VideoFilter,
    ram_state: RamState,
    ppu_warmup: bool,
    four_player: FourPlayer,
    concurrent_dpad: bool,
    run_ahead: usize,
    mmc3: Mmc3Revision,
    bf909: Bf909Revision,
    channels: [bool; CHANNELS.len()],
    /// False until the first apply, so that one runs even where nothing differs from these.
    applied: bool,
    /// Set when the region changed and the frontend has yet to be told, which only `poll` may do.
    announce_av: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            region: NesRegion::Auto,
            filter: VideoFilter::Pixellate,
            ram_state: RamState::Random,
            ppu_warmup: false,
            four_player: FourPlayer::Disabled,
            concurrent_dpad: false,
            run_ahead: 0,
            mmc3: Mmc3Revision::BC,
            bf909: Bf909Revision::Bf909x,
            channels: [true; CHANNELS.len()],
            applied: false,
            announce_av: false,
        }
    }
}

/// Tells the frontend what this core can be set to.
///
/// Called from `retro_set_environment`, before there is a console, so that the options appear in
/// the frontend's menu before any content is loaded.
///
/// # Safety
///
/// The environment callback must be valid.
pub unsafe fn declare(environment: retro_environment_t) {
    let mut version: c_uint = 0;
    // SAFETY: the callback writes one `unsigned`. A frontend that does not know the call leaves it
    // at zero, which is the answer meaning "only the original `SET_VARIABLES`".
    let known = unsafe {
        environment(
            RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION,
            std::ptr::from_mut(&mut version).cast::<c_void>(),
        )
    };
    if known && version >= 2 {
        // SAFETY: as below; the arrays outlive the call.
        if unsafe { declare_v2(environment) } {
            return;
        }
    }
    // SAFETY: as above.
    unsafe { declare_variables(environment) };
}

/// The modern form: categories, per-value labels, and help text.
///
/// # Safety
///
/// As [`declare`].
unsafe fn declare_v2(environment: retro_environment_t) -> bool {
    let mut categories: Vec<retro_core_option_v2_category> = CATEGORIES
        .iter()
        .map(|category| retro_core_option_v2_category {
            key: category.key.as_ptr(),
            desc: category.desc.as_ptr(),
            info: category.info.as_ptr(),
        })
        .collect();
    categories.push(retro_core_option_v2_category {
        key: std::ptr::null(),
        desc: std::ptr::null(),
        info: std::ptr::null(),
    });

    let mut definitions: Vec<retro_core_option_v2_definition> =
        defs().map(|def| def.to_c()).collect();
    definitions.push(retro_core_option_v2_definition {
        key: std::ptr::null(),
        desc: std::ptr::null(),
        desc_categorized: std::ptr::null(),
        info: std::ptr::null(),
        info_categorized: std::ptr::null(),
        category_key: std::ptr::null(),
        values: [retro_core_option_value::NONE; RETRO_NUM_CORE_OPTION_VALUES_MAX],
        default_value: std::ptr::null(),
    });

    let mut options = retro_core_options_v2 {
        categories: categories.as_mut_ptr(),
        definitions: definitions.as_mut_ptr(),
    };
    // SAFETY: the frontend copies what it needs during the call - every string in it is `'static`
    // and the two arrays outlive this statement.
    unsafe {
        environment(
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2,
            std::ptr::from_mut(&mut options).cast::<c_void>(),
        )
    }
}

/// The original form, for a frontend that has no core options: one string per option, the
/// description and the values run together.
///
/// # Safety
///
/// As [`declare`].
unsafe fn declare_variables(environment: retro_environment_t) {
    // The `CString`s have to outlive the call, so they are held until it returns.
    let joined: Vec<(&CStr, CString)> = defs()
        .map(|def| {
            let values = def
                .values
                .iter()
                .map(|(value, _)| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("|");
            let text = format!("{}; {values}", def.desc.to_string_lossy());
            (def.key, CString::new(text).unwrap_or_default())
        })
        .collect();
    let mut variables: Vec<retro_variable> = joined
        .iter()
        .map(|(key, text)| retro_variable {
            key: key.as_ptr(),
            value: text.as_ptr(),
        })
        .collect();
    variables.push(retro_variable {
        key: std::ptr::null(),
        value: std::ptr::null(),
    });
    // SAFETY: the frontend reads until the terminator, and everything pointed at outlives the call.
    unsafe {
        environment(
            RETRO_ENVIRONMENT_SET_VARIABLES,
            variables.as_mut_ptr().cast::<c_void>(),
        );
    }
}

/// Re-reads every option if the frontend says one changed, and applies what it finds.
///
/// # Safety
///
/// The environment callback must be valid.
pub unsafe fn poll(core: &mut Core) {
    let Some(environment) = core.callbacks.environment else {
        return;
    };
    let mut updated = false;
    // SAFETY: the callback writes one `bool`.
    let known = unsafe {
        environment(
            RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE,
            std::ptr::from_mut(&mut updated).cast::<c_void>(),
        )
    };
    if known && updated {
        // SAFETY: as above.
        unsafe { apply(core) };
    }
    // Deferred out of `apply`, which also runs from `retro_load_game`: the API permits this call
    // only from inside `retro_run`, which is the one place `poll` is called from.
    if core.options.announce_av {
        core.options.announce_av = false;
        let mut info = crate::av_info(core.deck.region());
        // SAFETY: the callback reads one `retro_system_av_info`, which outlives the call.
        let ok = unsafe {
            environment(
                RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO,
                std::ptr::from_mut(&mut info).cast::<c_void>(),
            )
        };
        if !ok {
            log::info("the frontend kept the old timings; the region applies on the next reload");
        }
    }
}

/// Reads every option and pushes it down to the console.
///
/// # Safety
///
/// The environment callback must be valid.
pub unsafe fn apply(core: &mut Core) {
    let Some(environment) = core.callbacks.environment else {
        return;
    };
    // SAFETY: `read` hands the callback one `retro_variable` per call.
    let read = |key: &CStr| unsafe { read(environment, key) };
    let mut next = Options {
        applied: true,
        ..Options::default()
    };

    next.region = match read(REGION).as_deref() {
        Some("ntsc") => NesRegion::Ntsc,
        Some("pal") => NesRegion::Pal,
        Some("dendy") => NesRegion::Dendy,
        _ => NesRegion::Auto,
    };
    next.filter = match read(FILTER).as_deref() {
        Some("ntsc") => VideoFilter::Ntsc,
        _ => VideoFilter::Pixellate,
    };
    next.ram_state = match read(RAM_STATE).as_deref() {
        Some("all_zeros") => RamState::AllZeros,
        Some("all_ones") => RamState::AllOnes,
        _ => RamState::Random,
    };
    next.ppu_warmup = read(PPU_WARMUP).as_deref() == Some("enabled");
    next.four_player = match read(FOUR_PLAYER).as_deref() {
        Some("four_score") => FourPlayer::FourScore,
        Some("satellite") => FourPlayer::Satellite,
        _ => FourPlayer::Disabled,
    };
    next.concurrent_dpad = read(CONCURRENT_DPAD).as_deref() == Some("enabled");
    next.run_ahead = read(RUN_AHEAD)
        .and_then(|frames| frames.parse().ok())
        .unwrap_or(0);
    next.mmc3 = match read(MMC3_REVISION).as_deref() {
        Some("a") => Mmc3Revision::A,
        Some("acc") => Mmc3Revision::Acc,
        _ => Mmc3Revision::BC,
    };
    next.bf909 = match read(BF909_REVISION).as_deref() {
        Some("bf9097") => Bf909Revision::Bf9097,
        _ => Bf909Revision::Bf909x,
    };
    for (slot, (key, _)) in next.channels.iter_mut().zip(CHANNELS) {
        *slot = read(key).as_deref() != Some("disabled");
    }

    let previous = std::mem::replace(&mut core.options, next);
    let next = core.options;
    let first = !previous.applied;

    if first || next.filter != previous.filter {
        core.set_filter(next.filter);
    }
    if first || next.ram_state != previous.ram_state {
        core.deck.set_ram_state(next.ram_state);
    }
    if first || next.ppu_warmup != previous.ppu_warmup {
        core.deck.set_emulate_ppu_warmup(next.ppu_warmup);
    }
    if first || next.four_player != previous.four_player {
        core.deck.set_four_player(next.four_player);
        core.pads.forget();
    }
    if first || next.concurrent_dpad != previous.concurrent_dpad {
        core.deck.set_concurrent_dpad(next.concurrent_dpad);
    }
    // Run-ahead is deliberately absent here: `sync_run_ahead` is its single writer, because what
    // the deck should be set to depends on the frontend's speed as well as this option.
    if first || next.mmc3 != previous.mmc3 {
        core.deck
            .set_mapper_revision(MapperRevision::Mmc3(next.mmc3));
    }
    if first || next.bf909 != previous.bf909 {
        core.deck
            .set_mapper_revision(MapperRevision::Bf909(next.bf909));
    }
    for (&enabled, (_, channel)) in next.channels.iter().zip(CHANNELS) {
        core.deck.set_apu_channel_enabled(channel, enabled);
    }

    if first || next.region != previous.region {
        core.deck.set_region(next.region);
        // The region decides the frame rate and the pixel aspect, so the frontend has to be told
        // again - but not from here. `SET_SYSTEM_AV_INFO` may only be sent from inside `retro_run`
        // (it reinitialises the frontend's audio and video drivers, and doing that mid-load has
        // been seen to call straight back into the core), and this also runs from
        // `retro_load_game`. So the announcement is left for the next poll.
        //
        // Not on the first apply: `retro_get_system_av_info`, which the frontend calls right after
        // loading, already reports the region set here.
        core.options.announce_av = !first;
    }
}

/// Pushes run-ahead down to the console, suppressed while the frontend is fast-forwarding.
///
/// Run-ahead only does anything at 1x - above it the speculated frames cost more than the latency
/// they hide, and the deck ignores the setting anyway - but the *cost* is paid regardless: each
/// display frame clocks the console `frames + 1` times. Left on, it divides the fast-forward
/// ceiling by that much, which reads as fast-forward being broken rather than as run-ahead being
/// expensive.
///
/// The single writer of the deck's run-ahead, since the value depends on the frontend's speed as
/// well as the option.
///
/// # Safety
///
/// The environment callback must be valid.
pub unsafe fn sync_run_ahead(core: &mut Core) {
    let Some(environment) = core.callbacks.environment else {
        return;
    };
    let mut fast_forwarding = false;
    // SAFETY: the callback writes one `bool`. A frontend that does not know the call leaves it
    // false, which is the answer that changes nothing.
    let known = unsafe {
        environment(
            RETRO_ENVIRONMENT_GET_FASTFORWARDING,
            std::ptr::from_mut(&mut fast_forwarding).cast::<c_void>(),
        )
    };
    let frames = if known && fast_forwarding {
        0
    } else {
        core.options.run_ahead
    };
    core.deck.set_run_ahead(frames);
}

/// Reads one option's current value, or `None` where the frontend has no setting for it.
///
/// # Safety
///
/// The environment callback must be valid.
unsafe fn read(environment: retro_environment_t, key: &CStr) -> Option<String> {
    let mut variable = retro_variable {
        key: key.as_ptr(),
        value: std::ptr::null(),
    };
    // SAFETY: the callback fills `value` with a pointer to a string it owns.
    let known = unsafe {
        environment(
            RETRO_ENVIRONMENT_GET_VARIABLE,
            std::ptr::from_mut(&mut variable).cast::<c_void>(),
        )
    };
    if !known || variable.value.is_null() {
        return None;
    }
    // SAFETY: a NUL-terminated string owned by the frontend, valid until the next call.
    Some(
        unsafe { CStr::from_ptr(variable.value) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// Every option, the six APU channels expanded into one each.
fn defs() -> impl Iterator<Item = Def> {
    DEFS.iter()
        .map(|def| Def {
            key: def.key,
            desc: def.desc,
            info: def.info,
            category: def.category,
            values: def.values,
        })
        .chain(CHANNELS.into_iter().map(|(key, channel)| Def {
            key,
            desc: channel_desc(channel),
            info: c"Silences this channel without the game knowing, which is what a mute does.",
            category: AUDIO,
            values: ON_OFF,
        }))
}

/// What the menu calls one APU channel.
const fn channel_desc(channel: Channel) -> &'static CStr {
    match channel {
        Channel::Pulse1 => c"Pulse 1",
        Channel::Pulse2 => c"Pulse 2",
        Channel::Triangle => c"Triangle",
        Channel::Noise => c"Noise",
        Channel::Dmc => c"DMC",
        Channel::Mapper => c"Cartridge Expansion Audio",
    }
}

impl Def {
    /// Builds the C shape, whose `values` array is a fixed 128 however few the option has.
    fn to_c(&self) -> retro_core_option_v2_definition {
        let mut values = [retro_core_option_value::NONE; RETRO_NUM_CORE_OPTION_VALUES_MAX];
        // One slot is left for the terminator, which the array is already filled with.
        for (slot, (value, label)) in values.iter_mut().zip(
            self.values
                .iter()
                .take(RETRO_NUM_CORE_OPTION_VALUES_MAX - 1),
        ) {
            *slot = retro_core_option_value {
                value: value.as_ptr(),
                label: label.as_ptr(),
            };
        }
        retro_core_option_v2_definition {
            key: self.key.as_ptr(),
            desc: self.desc.as_ptr(),
            // The category already says which part of the machine this is, so there is nothing
            // shorter to say; a frontend showing categories falls back to `desc`.
            desc_categorized: std::ptr::null(),
            info: self.info.as_ptr(),
            info_categorized: std::ptr::null(),
            category_key: self.category.as_ptr(),
            values,
            // The first value listed, so the two cannot drift apart.
            default_value: self
                .values
                .first()
                .map_or(std::ptr::null(), |(value, _)| value.as_ptr()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A duplicate key means one option silently shadows another in the frontend's store.
    #[test]
    fn every_key_is_unique_and_prefixed() {
        let mut keys = HashSet::new();
        for def in defs() {
            let key = def.key.to_string_lossy().into_owned();
            assert!(
                key.starts_with("tetanes_"),
                "{key} would collide with another core's option"
            );
            assert!(keys.insert(key.clone()), "{key} is declared twice");
        }
    }

    /// Each option's category has to exist, or the frontend files it under nothing.
    #[test]
    fn every_option_lands_in_a_declared_category() {
        let categories: HashSet<_> = CATEGORIES.iter().map(|c| c.key).collect();
        for def in defs() {
            assert!(
                categories.contains(&def.category),
                "{:?} is in an undeclared category",
                def.key
            );
        }
    }

    /// The values array is fixed at 128 and has to end in a null entry, so an option may declare
    /// at most 127 - and every option here declares far fewer.
    #[test]
    fn the_values_array_is_terminated() {
        for def in defs() {
            assert!(!def.values.is_empty(), "{:?} offers nothing", def.key);
            assert!(def.values.len() < RETRO_NUM_CORE_OPTION_VALUES_MAX);
            let c = def.to_c();
            assert!(
                c.values[def.values.len()].value.is_null(),
                "{:?} is not terminated",
                def.key
            );
            assert_eq!(
                c.default_value,
                def.values[0].0.as_ptr(),
                "the default is the first value listed"
            );
        }
    }

    /// Pins the value strings, which are what the frontend stores and hands back.
    ///
    /// Renaming one is how a saved setting silently stops being recognised - the frontend keeps
    /// answering with the old string and `apply` falls through to the default. This is a second
    /// copy on purpose, so the rename has to be made twice deliberately.
    ///
    /// It does *not* prove `apply` reads them; `tests::every_declared_value_reaches_the_console`
    /// drives the real thing through the exports for that.
    #[test]
    fn the_declared_value_strings_do_not_change_by_accident() {
        let known: &[(&CStr, &[&str])] = &[
            (REGION, &["auto", "ntsc", "pal", "dendy"]),
            (FILTER, &["pixellate", "ntsc"]),
            (RAM_STATE, &["random", "all_zeros", "all_ones"]),
            (PPU_WARMUP, &["disabled", "enabled"]),
            (FOUR_PLAYER, &["disabled", "four_score", "satellite"]),
            (CONCURRENT_DPAD, &["disabled", "enabled"]),
            (RUN_AHEAD, &["0", "1", "2", "3", "4"]),
            (MMC3_REVISION, &["bc", "a", "acc"]),
            (BF909_REVISION, &["bf909x", "bf9097"]),
        ];
        for (key, values) in known {
            let def = defs().find(|def| def.key == *key).expect("a declared key");
            let declared: Vec<_> = def
                .values
                .iter()
                .map(|(value, _)| value.to_string_lossy().into_owned())
                .collect();
            assert_eq!(declared, *values, "{key:?}");
        }
        for (key, _) in CHANNELS {
            let def = defs().find(|def| def.key == key).expect("a channel");
            assert_eq!(def.values, ON_OFF);
        }
    }

    /// The defaults this core starts with have to be the ones it declares, or the first frame runs
    /// under settings the menu is not showing.
    #[test]
    fn the_declared_defaults_match_the_starting_options() {
        let options = Options::default();
        let default_of = |key: &CStr| {
            defs()
                .find(|def| def.key == key)
                .expect("a declared key")
                .values[0]
                .0
                .to_string_lossy()
                .into_owned()
        };
        assert_eq!(default_of(REGION), "auto");
        assert!(options.region.is_auto());
        assert_eq!(default_of(FILTER), "pixellate");
        assert_eq!(options.filter, VideoFilter::Pixellate);
        assert_eq!(default_of(RAM_STATE), "random");
        assert_eq!(options.ram_state, RamState::Random);
        assert_eq!(default_of(PPU_WARMUP), "disabled");
        assert!(!options.ppu_warmup);
        assert_eq!(default_of(FOUR_PLAYER), "disabled");
        assert_eq!(options.four_player, FourPlayer::Disabled);
        assert_eq!(default_of(RUN_AHEAD), "0");
        assert_eq!(options.run_ahead, 0);
        assert!(options.channels.iter().all(|&on| on));
    }
}
