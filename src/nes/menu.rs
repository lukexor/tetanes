use crate::{
    apu::AudioChannel,
    audio::Audio,
    common::{config_path, NesRegion, SAVE_DIR, SRAM_DIR},
    input::GamepadSlot,
    memory::RamState,
    nes::{
        config::CONFIG,
        event::{Action, Input},
        filesystem::is_nes_rom,
        menu::types::{EmuSpeed, SampleRate},
        Mode, Nes,
    },
    ppu::VideoFilter,
};
use pix_engine::prelude::*;
use std::{borrow::Cow, ffi::OsStr, path::PathBuf};

pub(crate) mod types;
pub(crate) use types::{Menu, Player};

impl Nes {
    pub(crate) fn open_menu(&mut self, s: &mut PixState, menu: Menu) -> PixResult<()> {
        s.cursor(Cursor::arrow())?;
        self.mode = Mode::InMenu(menu, Player::One);
        self.audio.pause();
        Ok(())
    }

    pub(crate) fn exit_menu(&mut self, s: &mut PixState) -> PixResult<()> {
        if self.set_zapper_pos(s.mouse_pos()) {
            s.cursor(None)?;
        }
        self.resume_play();
        Ok(())
    }

    pub(crate) fn toggle_menu(&mut self, menu: Menu, s: &mut PixState) -> PixResult<()> {
        if let Mode::InMenu(..) = self.mode {
            self.exit_menu(s)
        } else {
            self.open_menu(s, menu)
        }
    }

    pub(crate) fn render_menu(
        &mut self,
        s: &mut PixState,
        menu: Menu,
        player: Player,
    ) -> PixResult<()> {
        let mut bg = s.theme().colors.background;
        bg.set_alpha(200);
        s.fill(bg);
        s.rect([0, 0, s.width()? as i32, s.height()? as i32])?;
        s.stroke(None);
        s.fill(Color::WHITE);

        s.heading("Menu")?;
        if self.control_deck.is_running() && s.menu("< Exit")? {
            self.exit_menu(s)?;
        }
        s.spacing()?;

        let render_menu = |tab: &Menu, s: &mut PixState| match tab {
            Menu::Config => self.render_config(s),
            Menu::Keybind => self.render_keybinds(s, menu, player),
            Menu::LoadRom => self.render_load_rom(s),
            Menu::About => self.render_about(s),
        };
        let mut menu_selection = menu;
        if s.tab_bar(
            "Menu",
            &[Menu::Config, Menu::Keybind, Menu::LoadRom, Menu::About],
            &mut menu_selection,
            render_menu,
        )? {
            self.mode = Mode::InMenu(menu_selection, player);
        }

        Ok(())
    }
}

impl Nes {
    fn render_config(&mut self, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("General", |s: &mut PixState| {
            let c = &mut self.config;

            s.spacing()?;

            s.checkbox("Pause in Background", &mut c.pause_in_bg)?;

            let mut save_slot = c.save_slot as usize - 1;
            s.next_width(50);
            if s.select_box("Save Slot:", &mut save_slot, &["1", "2", "3", "4"], 4)? {
                c.save_slot = save_slot as u8 + 1;
            }

            s.checkbox("Enable Rewind", &mut c.rewind)?;
            if c.rewind {
                s.indent()?;
                s.next_width(200);
                s.slider("Rewind Frames", &mut c.rewind_frames, 1, 10)?;
                s.indent()?;
                s.next_width(200);
                s.slider("Rewind Buffer Size (MB)", &mut c.rewind_buffer_size, 8, 256)?;
            }

            s.spacing()?;
            Ok(())
        })?;

        s.collapsing_header("Emulation", |s: &mut PixState| {
            s.spacing()?;

            let mut nes_region = self.config.nes_region as usize;
            s.next_width(150);
            if s.select_box("NES Format", &mut nes_region, NesRegion::as_slice(), 3)? {
                self.config.nes_region = NesRegion::from(nes_region);
                self.control_deck.set_nes_region(self.config.nes_region);
                s.set_window_dimensions(self.config.get_dimensions())?;
                self.update_frame_rate(s)?;
                self.audio = Audio::new(
                    self.control_deck.apu().sample_rate(),
                    self.config.audio_sample_rate / self.config.speed,
                    self.config.audio_buffer_size,
                );
                self.audio.open_playback(s)?;
            }

            s.next_width(125);
            let mut selected_state = self.config.ram_state as usize;
            if s.select_box(
                "Power-up RAM State:",
                &mut selected_state,
                RamState::as_slice(),
                3,
            )? {
                self.config.ram_state = selected_state.into();
            }

            let mut selected_speed = EmuSpeed::from(self.config.speed) as usize;
            s.next_width(100);
            if s.select_box("Speed:", &mut selected_speed, EmuSpeed::as_slice(), 4)? {
                self.set_speed(EmuSpeed::from(selected_speed).as_f32());
            }

            s.checkbox("Concurrent D-Pad", &mut self.config.concurrent_dpad)?;
            s.same_line(None);
            s.help_marker("Allow pressing U/D and L/R at the same time.")?;

            s.spacing()?;
            Ok(())
        })?;

        s.collapsing_header("Sound", |s: &mut PixState| {
            let c = &mut self.config;

            s.spacing()?;

            s.checkbox("Enabled", &mut c.sound)?;
            if c.sound {
                let audio = &mut self.audio;

                let mut selected_sample_rate = SampleRate::from(c.audio_sample_rate) as usize;
                s.next_width(200);
                if s.select_box(
                    "Sample Rate",
                    &mut selected_sample_rate,
                    SampleRate::as_slice(),
                    4,
                )? {
                    c.audio_sample_rate = SampleRate::from(selected_sample_rate).as_f32();
                    audio.set_output_frequency(c.audio_sample_rate / c.speed);
                }
                s.next_width(200);
                if s.slider("Buffer Size", &mut c.audio_buffer_size, 512, 8192)? {
                    audio.reset(c.audio_buffer_size);
                    audio.open_playback(s)?;
                }
                s.checkbox("Dynamic Rate Control", &mut c.dynamic_rate_control)?;
                if c.dynamic_rate_control {
                    s.next_width(200);
                    s.slider("Dynamic Rate Delta", &mut c.dynamic_rate_delta, 0.001, 0.1)?;
                }

                let deck = &mut self.control_deck;
                s.text("Channels:")?;
                let mut pulse1 = deck.channel_enabled(AudioChannel::Pulse1);
                if s.checkbox("Pulse 1", &mut pulse1)? {
                    deck.toggle_channel(AudioChannel::Pulse1);
                }
                let mut pulse2 = deck.channel_enabled(AudioChannel::Pulse2);
                if s.checkbox("Pulse 2", &mut pulse2)? {
                    deck.toggle_channel(AudioChannel::Pulse2);
                }
                let mut triangle = deck.channel_enabled(AudioChannel::Triangle);
                if s.checkbox("Triangle", &mut triangle)? {
                    deck.toggle_channel(AudioChannel::Triangle);
                }
                let mut noise = deck.channel_enabled(AudioChannel::Noise);
                if s.checkbox("Noise", &mut noise)? {
                    deck.toggle_channel(AudioChannel::Noise);
                }
                let mut dmc = deck.channel_enabled(AudioChannel::Dmc);
                if s.checkbox("DMC", &mut dmc)? {
                    deck.toggle_channel(AudioChannel::Dmc);
                }
            }

            s.spacing()?;
            Ok(())
        })?;

        s.collapsing_header("Video", |s: &mut PixState| {
            s.spacing()?;

            let mut scale = self.config.scale as usize - 1;
            s.next_width(50);
            if s.select_box("Scale:", &mut scale, &["1", "2", "3", "4"], 4)? {
                self.config.scale = scale as f32 + 1.0;
                let (width, height) = self.config.get_dimensions();
                s.set_window_dimensions((width, height))?;
                if let Some(debugger) = &self.debugger {
                    s.with_window(debugger.view.window_id, |s: &mut PixState| {
                        s.set_window_dimensions((width, height))
                    })?;
                }
                let (font_size, pad, ipady) = match scale {
                    0 => (6, 4, 3),
                    1 => (8, 6, 4),
                    2 => (12, 8, 6),
                    3 => (16, 10, 8),
                    _ => unreachable!("invalid scale"),
                };
                s.font_size(font_size)?;
                s.theme_mut().spacing.frame_pad = point!(pad, pad);
                s.theme_mut().spacing.item_pad = point!(pad, ipady);
            }

            let mut filter = self.config.filter as usize;
            s.next_width(150);
            if s.select_box(
                "Filter",
                &mut filter,
                &[VideoFilter::None, VideoFilter::Ntsc],
                2,
            )? {
                self.config.filter = VideoFilter::from(filter);
                self.control_deck.set_filter(self.config.filter);
            }

            if s.checkbox("Fullscreen", &mut self.config.fullscreen)? {
                s.fullscreen(self.config.fullscreen)?;
            }

            if s.checkbox("VSync Enabled", &mut self.config.vsync)? {
                s.vsync(self.config.vsync)?;
            }

            s.spacing()?;
            Ok(())
        })?;

        Ok(())
    }

    fn render_keybinds(&mut self, s: &mut PixState, menu: Menu, player: Player) -> PixResult<()> {
        let mut zapper = self.control_deck.zapper_connected(GamepadSlot::Two);
        if s.checkbox("Enable Zapper on Port #2", &mut zapper)? {
            self.control_deck.connect_zapper(GamepadSlot::Two, zapper);
            let input = Input::Mouse((GamepadSlot::Two, Mouse::Left));
            if zapper {
                let action = Action::ZapperTrigger;
                self.config.add_binding(input, action);
            } else {
                self.config.remove_binding(input);
            }
        }
        let mut fourscore = self.control_deck.fourscore();
        if s.checkbox("Enable Four Score (4-Player)", &mut fourscore)? {
            self.control_deck.set_fourscore(fourscore);
            self.config.fourscore = fourscore;
        }

        s.spacing()?;

        let mut selected_player = player as usize;
        s.next_width(200);
        if s.select_box("", &mut selected_player, Player::as_slice(), 4)? {
            self.mode = Mode::InMenu(menu, selected_player.try_into()?);
        }
        s.spacing()?;

        self.render_gamepad_binds(player, s)?;
        if player == Player::One {
            self.render_emulator_binds(player, s)?;
            self.render_debugger_binds(player, s)?;
        }
        Ok(())
    }

    fn render_gamepad_binds(&mut self, _player: Player, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Gamepad", |s: &mut PixState| {
            s.text("Coming soon!")?;
            s.spacing()?;
            Ok(())
        })?;
        Ok(())
    }

    fn render_emulator_binds(&mut self, _player: Player, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Emulator", |s: &mut PixState| {
            s.text("Coming soon!")?;
            // Action::Nes
            // Action::Menu
            // Action::Feature
            // Action::Setting
            s.spacing()?;
            Ok(())
        })?;
        Ok(())
    }

    fn render_debugger_binds(&mut self, _player: Player, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Debugger", |s: &mut PixState| {
            s.text("Coming soon!")?;
            // Action::Debug
            s.spacing()?;
            Ok(())
        })?;
        Ok(())
    }

    fn render_load_rom(&mut self, s: &mut PixState) -> PixResult<()> {
        let colors = s.theme().colors;
        let font_size = s.theme().font_size;
        let spacing = s.theme().spacing;

        if self.paths.is_empty() {
            self.update_paths();
        }

        if let Some(ref error) = self.error {
            s.fill(colors.error);
            s.wrap(s.width()? - 2 * spacing.frame_pad.x() as u32);
            s.text(&error)?;
            s.spacing()?;
        }

        let line_height = font_size as i32 + 4 * spacing.item_pad.y();
        let displayed_count =
            (s.height()? as usize - s.cursor_pos().y() as usize) / line_height as usize;
        let rom_dir = if self.config.rom_path.is_file() {
            self.config.rom_path.parent().unwrap()
        } else {
            self.config.rom_path.as_path()
        };
        let path_list: Vec<Cow<'_, str>> = self
            .paths
            .iter()
            .map(|p| p.strip_prefix(&rom_dir).unwrap_or(p).to_string_lossy())
            .collect();

        s.fill(colors.secondary);
        s.next_width((s.ui_width()? - spacing.scroll_size) as u32);
        s.select_list(
            format!("{}:", rom_dir.to_string_lossy()),
            &mut self.selected_path,
            &path_list,
            displayed_count,
        )?;
        let path = self.paths[self.selected_path].clone();
        if s.dbl_clicked() {
            if self.selected_path == 0 {
                if let Some(parent) = self.config.rom_path.parent() {
                    self.config.rom_path = parent.to_path_buf();
                    self.update_paths();
                }
            } else if path.is_dir() {
                self.config.rom_path = path.clone();
                self.update_paths();
            }
        }
        if !is_nes_rom(&path) {
            s.disable(true);
        }
        if s.dbl_clicked() || s.button("Open")? {
            self.config.rom_path = path;
            self.selected_path = 0;
            self.load_rom(s)?;
        }
        s.disable(false);

        Ok(())
    }

    fn update_paths(&mut self) {
        self.selected_path = 0;
        self.paths.clear();
        let mut path = self.config.rom_path.as_path();
        if path.is_file() {
            path = path.parent().expect("file should have a parent folder");
        }
        match path.read_dir() {
            Ok(read_dir) => {
                read_dir
                    .filter_map(Result::ok)
                    .map(|f| f.path())
                    .filter(|p| {
                        p.is_dir() || matches!(p.extension().and_then(OsStr::to_str), Some("nes"))
                    })
                    .for_each(|p| self.paths.push(p));
                self.paths.sort();
                if path.parent().is_some() {
                    self.paths.insert(0, PathBuf::from("../"));
                }
            }
            Err(err) => {
                log::error!("{:?}", err);
                self.error = Some(format!("Failed to read {:?}", path));
            }
        }
    }

    fn render_about(&self, s: &mut PixState) -> PixResult<()> {
        s.heading("TetaNES v0.8.0")?;
        s.spacing()?;

        if s.link("github.com/lukexor/tetanes")? {
            s.open_url("https://github.com/lukexor/tetanes")?;
        }
        s.spacing()?;

        s.bullet("Configuration: ")?;
        s.same_line(None);
        s.monospace(config_path(CONFIG).to_string_lossy())?;

        s.bullet("Save states: ")?;
        s.same_line(None);
        s.monospace(config_path(SAVE_DIR).to_string_lossy())?;

        s.bullet("Battery-Backed Save RAM: ")?;
        s.same_line(None);
        s.monospace(config_path(SRAM_DIR).to_string_lossy())?;

        Ok(())
    }
}
