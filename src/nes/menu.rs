#![allow(dead_code, unused)]
use crate::{
    input::Player,
    nes::{state::Mode, Nes},
    NesResult,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    ffi::OsStr,
    path::{Component, PathBuf},
};

#[cfg(not(target_os = "windows"))]
const PARENT_DIR: &str = "../";

#[cfg(target_os = "windows")]
const PARENT_DIR: &str = "..\\";

#[cfg(target_os = "windows")]
const VERBATIM_PREFIX: &str = r#"\\?\"#;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Menu {
    Main,
    Config(ConfigTab),
    Keybind(Player),
    #[default]
    LoadRom,
    About,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfigTab {
    General,
    Emulation,
    Audio,
    Video,
}

impl ConfigTab {
    #[inline]
    #[must_use]
    pub const fn as_slice() -> &'static [Self] {
        &[Self::General, Self::Emulation, Self::Audio, Self::Video]
    }
}

impl AsRef<str> for ConfigTab {
    fn as_ref(&self) -> &str {
        match self {
            Self::General => "General",
            Self::Emulation => "Emulation",
            Self::Audio => "Audio",
            Self::Video => "Video",
        }
    }
}

impl AsRef<str> for Player {
    fn as_ref(&self) -> &str {
        match self {
            Self::One => "Player One",
            Self::Two => "Player Two",
            Self::Three => "Player Three",
            Self::Four => "Player Four",
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleRate {
    S32,
    S44_1,
    S48,
    S96,
}

impl SampleRate {
    #[inline]
    #[must_use]
    pub const fn as_slice() -> &'static [Self] {
        &[Self::S32, Self::S44_1, Self::S48, Self::S96]
    }

    #[inline]
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        match self {
            Self::S32 => 32000.0,
            Self::S44_1 => 44100.0,
            Self::S48 => 48000.0,
            Self::S96 => 96000.0,
        }
    }
}

impl AsRef<str> for SampleRate {
    fn as_ref(&self) -> &str {
        match self {
            Self::S32 => "32 kHz",
            Self::S44_1 => "44.1 kHz",
            Self::S48 => "48 kHz",
            Self::S96 => "96 kHz",
        }
    }
}

impl TryFrom<usize> for SampleRate {
    type Error = &'static str;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::S32),
            1 => Ok(Self::S44_1),
            2 => Ok(Self::S48),
            3 => Ok(Self::S96),
            _ => Err("Invalid sample rate: {value}"),
        }
    }
}

impl TryFrom<f32> for SampleRate {
    type Error = &'static str;
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        match value as i32 {
            32000 => Ok(Self::S32),
            44100 => Ok(Self::S44_1),
            48000 => Ok(Self::S48),
            96000 => Ok(Self::S96),
            _ => Err("Invalid sample rate: {value}"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Speed {
    S25,
    S50,
    S75,
    S100,
    S125,
    S150,
    S175,
    S200,
}

impl Speed {
    #[inline]
    #[must_use]
    pub const fn as_slice() -> &'static [Self] {
        &[
            Self::S25,
            Self::S50,
            Self::S75,
            Self::S100,
            Self::S125,
            Self::S150,
            Self::S175,
            Self::S200,
        ]
    }

    #[inline]
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        match self {
            Self::S25 => 0.25,
            Self::S50 => 0.50,
            Self::S75 => 0.75,
            Self::S100 => 1.0,
            Self::S125 => 1.25,
            Self::S150 => 1.5,
            Self::S175 => 1.75,
            Self::S200 => 2.0,
        }
    }
}

impl AsRef<str> for Speed {
    fn as_ref(&self) -> &str {
        match self {
            Self::S25 => "25%",
            Self::S50 => "50%",
            Self::S75 => "75%",
            Self::S100 => "100%",
            Self::S125 => "125%",
            Self::S150 => "150%",
            Self::S175 => "175%",
            Self::S200 => "200%",
        }
    }
}

impl From<usize> for Speed {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::S25,
            1 => Self::S50,
            2 => Self::S75,
            4 => Self::S125,
            5 => Self::S150,
            6 => Self::S175,
            7 => Self::S200,
            _ => Self::S100,
        }
    }
}

impl From<f32> for Speed {
    fn from(value: f32) -> Self {
        Self::from(((4.0 * value) as usize).saturating_sub(1))
    }
}

impl Nes {
    pub(crate) fn open_menu(&mut self, menu: Menu) {
        // s.cursor(Cursor::arrow())?;
        self.mode = Mode::Menu(menu);
        self.mixer.pause();
    }

    pub(crate) fn exit_menu(&mut self) {
        if self.config.zapper {
            // s.cursor(None)?;
        }
        self.resume_play();
    }

    pub(crate) fn toggle_menu(&mut self, menu: Menu) {
        if matches!(self.mode, Mode::Menu(current_menu) if current_menu == menu) {
            self.exit_menu();
        } else {
            self.open_menu(menu);
        }
    }

    //     pub(crate) fn render_menu(&mut self, menu: Menu) -> NesResult<()> {
    //         self.messages.clear();
    //         // TODO: switch to egui
    //         // let mut bg = s.theme().colors.background;
    //         // bg.set_alpha(200);
    //         // s.fill(bg);
    //         // s.rect([0, 0, s.width()? as i32, s.height()? as i32])?;
    //         // s.stroke(None);
    //         // s.fill(Color::WHITE);

    //         match menu {
    //             Menu::Main => self.render_main()?,
    //             Menu::Config(section) => self.render_config(section)?,
    //             Menu::Keybind(player) => self.render_keybinds(player)?,
    //             Menu::LoadRom => self.render_load_rom()?,
    //             Menu::About => self.render_about()?,
    //         }

    //         Ok(())
    //     }
}

// impl Nes {
//     fn render_heading(&mut self, heading: &str) -> NesResult<()> {
//         // TODO: switch to egui
//         // s.heading(heading)?;
//         // if self.control_deck.is_running() && s.menu("< Exit")? {
//         //     self.exit_menu()?;
//         // }
//         // s.spacing()?;
//         Ok(())
//     }

//     fn render_config_general(&mut self) -> NesResult<()> {
//         // TODO: switch to egui
//         // s.checkbox("Pause in Background", &mut self.config.pause_in_bg)?;

//         let mut save_slot = self.config.save_slot as usize - 1;
//         // s.next_width(50);
//         // if s.select_box("Save Slot", &mut save_slot, &["1", "2", "3", "4"], 4)? {
//         //     self.config.save_slot = save_slot as u8 + 1;
//         // }

//         // s.checkbox("Enable Rewind", &mut self.config.rewind)?;
//         if self.config.rewind {
//             // s.indent()?;
//             // s.next_width(200);
//             // s.slider("Rewind Frames", &mut self.config.rewind_frames, 1, 10)?;
//             // s.indent()?;
//             // s.next_width(200);
//             // s.slider(
//             //     "Rewind Buffer Size (MB)",
//             //     &mut self.config.rewind_buffer_size,
//             //     8,
//             //     256,
//             // )?;
//         }

//         // s.checkbox("Enable Zapper", &mut self.config.zapper)?;

//         let mut four_player = self.config.four_player as usize;
//         // s.next_width(150);
//         // if s.select_box(
//         //     "Four Player Mode",
//         //     &mut four_player,
//         //     FourPlayer::as_slice(),
//         //     3,
//         // )? {
//         //     self.config.four_player = FourPlayer::from(four_player);
//         //     self.control_deck.set_four_player(self.config.four_player);
//         // }

//         Ok(())
//     }

//     fn render_config_emulation(&mut self) -> NesResult<()> {
//         let mut region = self.config.region as usize;
//         // TODO: switch to egui
//         // s.next_width(150);
//         // if s.select_box("NES Region", &mut region, NesRegion::as_slice(), 3)? {
//         //     self.config.region = NesRegion::from(region);
//         //     self.control_deck.set_region(self.config.region);
//         //     s.set_window_dimensions(self.config.get_dimensions())?;
//         //     self.update_frame_rate(s)?;
//         //     self.audio = Mixer::new(
//         //         self.control_deck.sample_rate(),
//         //         self.config.audio_sample_rate / self.config.speed,
//         //         self.config.audio_buffer_size,
//         //     );
//         //     self.audio.open_playback(s)?;
//         // }

//         // s.next_width(125);
//         let mut selected_state = self.config.ram_state as usize;
//         // if s.select_box(
//         //     "Power-up RAM State",
//         //     &mut selected_state,
//         //     RamState::as_slice(),
//         //     3,
//         // )? {
//         //     self.config.ram_state = selected_state.into();
//         // }

//         let mut selected_speed = EmuSpeed::from(self.config.speed) as usize;
//         // s.next_width(100);
//         // if s.select_box("Speed", &mut selected_speed, EmuSpeed::as_slice(), 4)? {
//         //     self.set_speed(EmuSpeed::from(selected_speed).as_f32());
//         // }

//         // s.checkbox("Concurrent D-Pad", &mut self.config.concurrent_dpad)?;
//         // s.same_line(None);
//         // s.help_marker("Allow pressing U/D and L/R at the same time.")?;

//         Ok(())
//     }

//     fn render_config_audio(&mut self) -> NesResult<()> {
//         // TODO: switch to egui
//         // s.checkbox("Enabled", &mut self.config.sound)?;
//         if self.config.audio_enabled {
//             let audio = &mut self.audio;

//             let mut selected_sample_rate = SampleRate::from(self.config.audio_sample_rate) as usize;
//             // s.next_width(200);
//             // if s.select_box(
//             //     "Sample Rate",
//             //     &mut selected_sample_rate,
//             //     SampleRate::as_slice(),
//             //     4,
//             // )? {
//             //     self.config.audio_sample_rate = SampleRate::from(selected_sample_rate).as_f32();
//             //     audio.set_sample_rate(self.config.audio_sample_rate / self.config.speed);
//             // }

//             // s.next_width(200);
//             // if s.slider("Buffer Size", &mut self.config.audio_buffer_size, 512, 8192)? {
//             //     audio.reset(self.config.audio_buffer_size);
//             //     audio.open_playback(s)?;
//             // }

//             let deck = &mut self.control_deck;
//             // s.collapsing_tree("Channels", |s: &mut PixState| {
//             //     let mut pulse1 = deck.channel_enabled(Channel::Pulse1);
//             //     if s.checkbox("Pulse 1", &mut pulse1)? {
//             //         deck.toggle_channel(Channel::Pulse1);
//             //     }

//             //     let mut pulse2 = deck.channel_enabled(Channel::Pulse2);
//             //     if s.checkbox("Pulse 2", &mut pulse2)? {
//             //         deck.toggle_channel(Channel::Pulse2);
//             //     }

//             //     let mut triangle = deck.channel_enabled(Channel::Triangle);
//             //     if s.checkbox("Triangle", &mut triangle)? {
//             //         deck.toggle_channel(Channel::Triangle);
//             //     }

//             //     let mut noise = deck.channel_enabled(Channel::Noise);
//             //     if s.checkbox("Noise", &mut noise)? {
//             //         deck.toggle_channel(Channel::Noise);
//             //     }

//             //     let mut dmc = deck.channel_enabled(Channel::Dmc);
//             //     if s.checkbox("DMC", &mut dmc)? {
//             //         deck.toggle_channel(Channel::Dmc);
//             //     }
//             //     Ok(())
//             // })?;
//         }

//         Ok(())
//     }

//     fn render_config_video(&mut self) -> NesResult<()> {
//         // TODO: switch to egui
//         let mut scale = self.config.scale as usize - 1;
//         // s.next_width(80);
//         // if s.select_box("Scale", &mut scale, &["1", "2", "3", "4"], 4)? {
//         //     self.set_scale(scale as f32 + 1.0);
//         //     let (width, height) = self.config.get_dimensions();
//         //     s.set_window_dimensions((width, height))?;
//         //     if let Some(debugger) = &self.debugger {
//         //         s.set_window_target(debugger.window_id())?;
//         //         s.set_window_dimensions((width, height))?;
//         //         s.reset_window_target();
//         //     }
//         // }

//         let mut filter = self.config.filter as usize;
//         // s.next_width(150);
//         // if s.select_box(
//         //     "Filter",
//         //     &mut filter,
//         //     &[VideoFilter::Pixellate, VideoFilter::Ntsc],
//         //     2,
//         // )? {
//         //     self.config.filter = VideoFilter::from(filter);
//         //     self.control_deck.set_filter(self.config.filter);
//         // }

//         // if s.checkbox("Fullscreen", &mut self.config.fullscreen)? {
//         //     s.fullscreen(self.config.fullscreen)?;
//         // }

//         // if s.checkbox("VSync Enabled", &mut self.config.vsync)? {
//         //     s.vsync(self.config.vsync)?;
//         // }

//         Ok(())
//     }

//     fn render_main(&mut self) -> NesResult<()> {
//         self.render_heading("Menu")?;

//         // TODO: switch to egui
//         // if s.menu("Config")? {
//         //     self.mode = Mode::InMenu(Menu::Config(ConfigSection::General));
//         // }
//         // if s.menu("Keybinds")? {
//         //     self.mode = Mode::InMenu(Menu::Keybind(Player::One));
//         // }
//         // if s.menu("Load ROM")? {
//         //     self.mode = Mode::InMenu(Menu::LoadRom);
//         // }
//         // if s.menu("About")? {
//         //     self.mode = Mode::InMenu(Menu::About);
//         // }

//         Ok(())
//     }

//     fn render_config(&mut self, mut section: ConfigSection) -> NesResult<()> {
//         self.render_heading("Configuration")?;

//         // TODO: switch to egui
//         // if s.tab_bar(
//         //     "Sections",
//         //     ConfigSection::as_slice(),
//         //     &mut section,
//         //     |section: &ConfigSection| match section {
//         //         ConfigSection::General => self.render_config_general(s),
//         //         ConfigSection::Emulation => self.render_config_emulation(s),
//         //         ConfigSection::Audio => self.render_config_audio(s),
//         //         ConfigSection::Video => self.render_config_video(s),
//         //     },
//         // )? {
//         //     self.mode = Mode::InMenu(Menu::Config(section));
//         // }

//         Ok(())
//     }

//     fn render_keybinds(&mut self, mut player: Player) -> NesResult<()> {
//         self.render_heading("Keybindings")?;

//         // TODO: switch to egui
//         // if s.tab_bar(
//         //     "Sections",
//         //     Player::as_slice(),
//         //     &mut player,
//         //     |player: &Player| self.render_gamepad_binds(*player),
//         // )? {
//         //     self.mode = Mode::InMenu(Menu::Keybind(player));
//         // }

//         Ok(())
//     }

//     fn render_gamepad_binds(&mut self, player: Player) -> NesResult<()> {
//         // TODO: switch to egui
//         // s.text("Coming soon!")?;

//         if player == Player::One {
//             self.render_emulator_binds()?;
//         }
//         Ok(())
//     }

//     fn render_emulator_binds(&mut self) -> NesResult<()> {
//         // Action::Nes
//         // Action::Menu
//         // Action::Feature
//         // Action::Setting
//         // Action::Debug
//         Ok(())
//     }

//     fn render_load_rom(&mut self) -> NesResult<()> {
//         self.render_heading("Load ROM")?;

//         // TODO: switch to egui
//         // let colors = s.theme().colors;
//         // let font_size = s.theme().font_size;
//         // let spacing = s.theme().spacing;

//         if self.paths.is_empty() {
//             self.update_paths();
//         }

//         // TODO: switch to egui
//         if let Some(ref error) = self.error {
//             // s.fill(colors.error);
//             // s.wrap(s.width()? - 2 * spacing.frame_pad.x() as u32);
//             // s.text(error)?;
//             // s.spacing()?;
//         }

//         // let line_height = font_size as i32 + 4 * spacing.item_pad.y();
//         // let displayed_count =
//         //     (s.height()? as usize - s.cursor_pos().y() as usize) / line_height as usize;
//         let rom_dir = if self.config.rom_path.is_file() {
//             self.config
//                 .rom_path
//                 .parent()
//                 .expect("ifiles should always have a parent")
//         } else {
//             self.config.rom_path.as_path()
//         };
//         // TODO: memoize this allocation
//         let path_list: Vec<Cow<'_, str>> = self
//             .paths
//             .iter()
//             .map(|p| p.strip_prefix(rom_dir).unwrap_or(p).to_string_lossy())
//             .collect();

//         // s.fill(colors.secondary);
//         // s.next_width((s.ui_width()? - spacing.scroll_size) as u32);
//         let path = rom_dir.to_string_lossy();
//         #[cfg(target_os = "windows")]
//         let path = path.strip_prefix(VERBATIM_PREFIX).unwrap_or(&path);
//         // s.select_list(
//         //     format!("{path}##{}", self.config.show_hidden_files),
//         //     &mut self.selected_path,
//         //     &path_list,
//         //     displayed_count,
//         // )?;
//         let path = self.paths[self.selected_path].clone();
//         // if s.dbl_clicked() {
//         //     if self.selected_path == 0 {
//         //         if let Some(parent) = self.config.rom_path.parent() {
//         //             self.config.rom_path = parent.to_path_buf();
//         //             self.update_paths();
//         //         }
//         //     } else if path.is_dir() {
//         //         self.config.rom_path = path.clone();
//         //         self.update_paths();
//         //     }
//         // }
//         // if !filesystem::is_nes_rom(path) {
//         // s.disable(true);
//         // }
//         // if s.dbl_clicked() || s.button("Open")? {
//         //     self.config.rom_path = path;
//         //     self.selected_path = 0;
//         //     self.load_rom(s)?;
//         // }
//         // s.disable(false);
//         // if s.checkbox("Show hidden files", &mut self.config.show_hidden_files)? {
//         //     self.update_paths();
//         // }

//         Ok(())
//     }

//     fn update_paths(&mut self) {
//         self.selected_path = 0;
//         self.paths.clear();
//         let mut path = self.config.rom_path.as_path();
//         if path.is_file() {
//             path = path.parent().expect("file should have a parent folder");
//         }
//         let hidden_file = |path: &PathBuf| match path.components().next_back() {
//             Some(Component::Normal(tail)) => tail.to_str().map_or(false, |p| p.starts_with('.')),
//             _ => false,
//         };
//         match path.read_dir() {
//             Ok(read_dir) => {
//                 read_dir
//                     .filter_map(Result::ok)
//                     .map(|f| f.path())
//                     .filter(|p| {
//                         (p.is_dir() || matches!(p.extension().and_then(OsStr::to_str), Some("nes")))
//                             && (self.config.show_hidden_files || !hidden_file(p))
//                     })
//                     .for_each(|p| self.paths.push(p));
//                 self.paths.sort();
//                 if path.parent().is_some() {
//                     self.paths.insert(0, PathBuf::from(PARENT_DIR));
//                 }
//             }
//             Err(err) => {
//                 log::error!("{:?}", err);
//                 self.error = Some(format!("Failed to read {path:?}"));
//             }
//         }
//     }

//     fn render_about(&mut self) -> NesResult<()> {
//         self.render_heading(&format!("TetaNES {}", env!("CARGO_PKG_VERSION")))?;

//         // TODO: switch to egui
//         // if s.link("github.com/lukexor/tetanes")? {
//         //     s.open_url("https://github.com/lukexor/tetanes")?;
//         // }
//         // s.spacing()?;

//         // s.bullet("Configuration: ")?;
//         // s.same_line(None);
//         // s.monospace(config_path(CONFIG).to_string_lossy())?;

//         // s.bullet("Save states: ")?;
//         // s.same_line(None);
//         // s.monospace(config_path(SAVE_DIR).to_string_lossy())?;

//         // s.bullet("Battery-Backed Save RAM: ")?;
//         // s.same_line(None);
//         // s.monospace(config_path(SRAM_DIR).to_string_lossy())?;

//         Ok(())
//     }
// }
