use crate::nes::{
    config::Config,
    event::{EmulationEvent, NesEvent, UiEvent},
};
use egui::{
    global_dark_light_mode_switch, load::SizedTexture, menu, Align, Align2, Area, CentralPanel,
    Color32, Context, CursorIcon, Frame, Image, Layout, Margin, Order, RichText, TopBottomPanel,
    Ui, Vec2,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tetanes_core::{
    common::{NesRegion, ResetKind},
    input::{FourPlayer, Player},
    ppu::Ppu,
    time::{Duration, Instant},
    video::VideoFilter,
};
use tracing::{error, trace};
use winit::{
    event_loop::EventLoopProxy,
    window::{Fullscreen, Window},
};

pub const MSG_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_MESSAGES: usize = 3;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Menu {
    Config(ConfigTab),
    Keybind(Player),
    About,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfigTab {
    General,
    Emulation,
    Audio,
    Video,
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

#[derive(Debug)]
#[must_use]
pub struct Gui {
    pub window: Arc<Window>,
    pub event_proxy: EventLoopProxy<NesEvent>,
    pub texture: SizedTexture,
    pub config: Config,
    pub paused: bool,
    pub menu_height: f32,
    pub preferences_open: bool,
    pub keybinds_open: bool,
    pub about_open: bool,
    pub resize_surface: bool,
    pub resize_texture: bool,
    pub replay_recording: bool,
    pub audio_recording: bool,
    pub version: String,
    pub last_frame_duration: Duration,
    pub messages: Vec<(String, Instant)>,
    pub status: Option<&'static str>,
    pub error: Option<String>,
}

#[derive(Debug)]
#[must_use]
pub enum GuiEvent {
    Nes(UiEvent),
    Emulation(EmulationEvent),
}

impl From<UiEvent> for GuiEvent {
    fn from(event: UiEvent) -> Self {
        Self::Nes(event)
    }
}

impl From<EmulationEvent> for GuiEvent {
    fn from(event: EmulationEvent) -> Self {
        Self::Emulation(event)
    }
}

impl From<GuiEvent> for NesEvent {
    fn from(event: GuiEvent) -> Self {
        match event {
            GuiEvent::Nes(event) => Self::Ui(event),
            GuiEvent::Emulation(event) => Self::Emulation(event),
        }
    }
}

impl Gui {
    /// Create a gui `State`.
    pub fn new(
        window: Arc<Window>,
        event_proxy: EventLoopProxy<NesEvent>,
        texture: SizedTexture,
        config: Config,
    ) -> Self {
        Self {
            window,
            event_proxy,
            texture,
            config,
            paused: false,
            menu_height: 0.0,
            preferences_open: false,
            keybinds_open: false,
            about_open: false,
            resize_surface: false,
            resize_texture: false,
            replay_recording: false,
            audio_recording: false,
            version: format!("Version: {}", env!("CARGO_PKG_VERSION")),
            last_frame_duration: Duration::default(),
            messages: vec![],
            status: None,
            error: None,
        }
    }

    /// Send a custom event to the event loop.
    pub fn send_event(&mut self, event: impl Into<GuiEvent>) {
        let event = event.into();
        trace!("Gui event: {event:?}");
        if let Err(err) = self.event_proxy.send_event(event.into()) {
            error!("failed to send nes event: {err:?}");
            std::process::exit(1);
        }
    }

    /// Create the UI.
    pub fn ui(&mut self, ctx: &Context) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        TopBottomPanel::top("menu_bar").show_animated(
            ctx,
            self.config.read(|cfg| cfg.renderer.show_menubar),
            |ui| self.menu_bar(ui),
        );
        CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| self.nes_frame(ui));

        // TODO: show confirm quit dialog?

        let mut preferences_open = self.preferences_open;
        egui::Window::new("Preferences")
            .open(&mut preferences_open)
            .show(ctx, |ui| self.preferences(ui));
        self.preferences_open = preferences_open;

        let mut keybinds_open = self.keybinds_open;
        egui::Window::new("Keybinds")
            .open(&mut keybinds_open)
            .show(ctx, |ui| self.keybinds(ui));
        self.keybinds_open = keybinds_open;

        let mut about_open = self.about_open;
        egui::Window::new("About TetaNES")
            .open(&mut about_open)
            .show(ctx, |ui| self.about(ui));
        self.about_open = about_open;

        #[cfg(feature = "profiling")]
        puffin_egui::show_viewport_if_enabled(ctx);
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        ui.style_mut().spacing.menu_margin = Margin::ZERO;
        let inner_response = menu::bar(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                global_dark_light_mode_switch(ui);
                ui.separator();

                ui.menu_button("File", |ui| self.file_menu(ui));
                ui.menu_button("Controls", |ui| self.controls_menu(ui));
                ui.menu_button("Settings", |ui| self.settings_menu(ui));
                ui.menu_button("Window", |ui| self.window_menu(ui));
                ui.menu_button("Debug", |ui| self.debug_menu(ui));
                ui.toggle_value(&mut self.about_open, "About");
            });
        });
        let spacing = ui.style().spacing.item_spacing;
        let border = 1.0;
        let height = inner_response.response.rect.height() + spacing.y + border;
        if height != self.menu_height {
            self.menu_height = height;
            self.resize_surface = true;
        }
    }

    fn file_menu(&mut self, ui: &mut Ui) {
        // NOTE: Due to some platforms file dialogs blocking the event loop,
        // loading requires a round-trip in order for the above pause to
        // get processed.
        if ui.button("Load ROM...").clicked() {
            self.send_event(EmulationEvent::Pause(true));
            self.send_event(UiEvent::LoadRomDialog);
            ui.close_menu();
        }
        if ui.button("Load Replay...").clicked() {
            self.send_event(EmulationEvent::Pause(true));
            self.send_event(UiEvent::LoadReplayDialog);
            ui.close_menu();
        }

        // TODO: support saves and recent games on wasm? Requires storing the data
        #[cfg(not(target_arch = "wasm32"))]
        {
            if ui.button("Save State").clicked() {
                self.send_event(EmulationEvent::StateSave);
                ui.close_menu();
            };
            if ui.button("Load State").clicked() {
                self.send_event(EmulationEvent::StateLoad);
                ui.close_menu();
            }

            self.config.write(|cfg| {
                ui.menu_button("Save Slot...", |ui| {
                    for i in 1..=4 {
                        if ui
                            .radio_value(&mut cfg.emulation.save_slot, i, &i.to_string())
                            .clicked()
                        {
                            ui.close_menu();
                        }
                    }
                });
            });

            ui.menu_button("Recently Played...", |ui| {
                use tetanes_core::fs;
                let mut rom = self.config.read(|cfg| {
                    // TODO: add timestamp, save slots, and screenshot
                    for rom in &cfg.renderer.recent_roms {
                        if ui.button(fs::filename(rom)).clicked() {
                            return Some(rom.to_path_buf());
                        }
                    }
                    None
                });
                if let Some(rom) = rom.take() {
                    self.send_event(EmulationEvent::LoadRomPath(rom));
                    ui.close_menu();
                }
            });

            if ui.button("Quit").clicked() {
                self.send_event(UiEvent::Terminate);
                ui.close_menu();
            };
        }
    }

    fn controls_menu(&mut self, ui: &mut Ui) {
        let pause_label = if self.paused { "Resume" } else { "Pause" };
        if ui.button(pause_label).clicked() {
            self.send_event(EmulationEvent::Pause(!self.paused));
            ui.close_menu();
        };
        let audio_enabled = self.config.read(|cfg| cfg.audio.enabled);
        let mute_label = if audio_enabled { "Mute" } else { "Unmute" };
        if ui.button(mute_label).clicked() {
            self.config
                .write(|cfg| cfg.audio.enabled = !cfg.audio.enabled);
            self.send_event(EmulationEvent::SetAudioEnabled(!audio_enabled));
            ui.close_menu();
        };
        if ui.button("Reset").clicked() {
            self.send_event(EmulationEvent::Reset(ResetKind::Soft));
            ui.close_menu();
        };
        if ui.button("Power Cycle").clicked() {
            self.send_event(EmulationEvent::Reset(ResetKind::Hard));
            ui.close_menu();
        };
        if ui.button("Rewind").clicked() {
            self.send_event(EmulationEvent::InstantRewind);
            ui.close_menu();
        };
        if ui.button("Take Screenshot").clicked() {
            self.send_event(EmulationEvent::Screenshot);
            ui.close_menu();
        };
        let replay_label = if self.replay_recording {
            "Stop Replay Recording"
        } else {
            "Record Replay"
        };
        if ui.button(replay_label).clicked() {
            self.send_event(EmulationEvent::ReplayRecord(!self.replay_recording));
            ui.close_menu();
        };
        let audio_label = if self.audio_recording {
            "Stop Audio Recording"
        } else {
            "Record Audio"
        };
        if ui.button(audio_label).clicked() {
            self.send_event(EmulationEvent::AudioRecord(!self.audio_recording));
            ui.close_menu();
        };
    }

    fn settings_menu(&mut self, ui: &mut Ui) {
        self.config.write(|cfg| {
            ui.checkbox(&mut cfg.emulation.cycle_accurate, "Cycle Accurate");
        });
        ui.menu_button("Speed...", |ui| {
            let changed = self.config.write(|cfg| {
                ui.add(
                    egui::Slider::new(&mut cfg.emulation.speed, 0.25..=2.0)
                        .step_by(0.25)
                        .suffix("%"),
                )
                .changed()
            });
            if changed {
                self.send_event(EmulationEvent::SetSpeed(
                    self.config.read(|cfg| cfg.emulation.speed),
                ));
            }
        });
        self.config.write(|cfg| {
            ui.checkbox(&mut cfg.deck.zapper, "Enable Zapper Gun");
        });
        self.resize_texture = self.config.write(|cfg| {
            ui.checkbox(&mut cfg.renderer.hide_overscan, "Hide Overscan")
                .clicked()
        });
        ui.menu_button("Video Filter...", |ui| {
            let filter = self.config.read(|cfg| cfg.deck.filter);
            self.config.write(|cfg| {
                ui.radio_value(&mut cfg.deck.filter, VideoFilter::Pixellate, "Pixellate");
                ui.radio_value(&mut cfg.deck.filter, VideoFilter::Ntsc, "Ntsc");
            });
            if filter != self.config.read(|cfg| cfg.deck.filter) {
                self.send_event(EmulationEvent::SetVideoFilter(
                    self.config.read(|cfg| cfg.deck.filter),
                ));
            }
        });
        ui.menu_button("Four Player...", |ui| {
            let four_player = self.config.read(|cfg| cfg.deck.four_player);
            self.config.write(|cfg| {
                ui.radio_value(&mut cfg.deck.four_player, FourPlayer::Disabled, "Disabled");
                ui.radio_value(
                    &mut cfg.deck.four_player,
                    FourPlayer::FourScore,
                    "Four Score",
                );
                ui.radio_value(
                    &mut cfg.deck.four_player,
                    FourPlayer::Satellite,
                    "Satellite",
                );
            });
            if four_player != self.config.read(|cfg| cfg.deck.four_player) {
                self.send_event(EmulationEvent::SetFourPlayer(
                    self.config.read(|cfg| cfg.deck.four_player),
                ));
            }
        });
        ui.menu_button("Nes Region...", |ui| {
            let region = self.config.read(|cfg| cfg.deck.region);
            self.config.write(|cfg| {
                ui.radio_value(&mut cfg.deck.region, NesRegion::Ntsc, "NTSC");
                ui.radio_value(&mut cfg.deck.region, NesRegion::Pal, "PAL");
                ui.radio_value(&mut cfg.deck.region, NesRegion::Dendy, "Dendy");
            });
            if region != self.config.read(|cfg| cfg.deck.region) {
                self.resize_texture = true;
                self.send_event(EmulationEvent::SetRegion(
                    self.config.read(|cfg| cfg.deck.region),
                ));
            }
        });
        if ui.button("Preferences").clicked() {
            self.preferences_open = true;
            ui.close_menu();
        }
        if ui.button("Keybinds").clicked() {
            self.keybinds_open = true;
            // Keyboard
            // Controllers
            // combobox
            //   Player 1
            //   Player 2
            //   Player 3
            //   Player 4
            ui.close_menu();
        };
    }

    fn window_menu(&mut self, ui: &mut Ui) {
        if ui.button("Maximize").clicked() {
            self.window.set_maximized(true);
            ui.close_menu();
        };
        if ui.button("Minimize").clicked() {
            self.window.set_minimized(true);
            ui.close_menu();
        };
        if ui.button("Toggle Fullscreen").clicked() {
            let fullscreen = self.config.write(|cfg| {
                cfg.renderer.fullscreen = !cfg.renderer.fullscreen;
                cfg.renderer.fullscreen
            });
            self.window
                .set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
            ui.close_menu();
        };
        if ui.button("Hide Menu Bar").clicked() {
            self.config.write(|cfg| cfg.renderer.show_menubar = false);
            ui.close_menu();
        };
    }

    fn debug_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        {
            let mut profile = puffin::are_scopes_on();
            ui.checkbox(&mut profile, "Toggle profiling");
            puffin::set_scopes_on(profile);
        }
        if ui.button("Toggle CPU Debugger").clicked() {
            self.todo();
        };
        if ui.button("Toggle PPU Debugger").clicked() {
            self.todo();
        };
        if ui.button("Toggle APU Debugger").clicked() {
            self.todo();
        };
    }

    fn message_bar(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.messages.retain(|(_, expires)| now < *expires);
        self.messages.dedup_by(|a, b| a.0.eq(&b.0));
        for (message, _) in self.messages.iter().take(MAX_MESSAGES) {
            ui.label(message);
        }
    }

    fn error_bar(&mut self, ui: &mut Ui) {
        let mut clear_error = false;
        if let Some(ref error) = self.error {
            ui.vertical(|ui| {
                ui.label(RichText::new(error).color(Color32::RED));
                clear_error = ui.button("Clear").clicked();
            });
        }
        if clear_error {
            self.error = None;
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        // TODO: Render framerate if enabled
        // TODO: maybe show other statuses like rewinding/playback/recording - bitflags?
        if let Some(status) = self.status {
            ui.label(status);
        }
    }

    fn nes_frame(&mut self, ui: &mut Ui) {
        CentralPanel::default()
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                let image = Image::from_texture(self.texture)
                    .maintain_aspect_ratio(true)
                    .shrink_to_fit();
                let zapper = self.config.read(|cfg| cfg.deck.zapper);
                let frame_resp =
                    ui.add_sized(ui.available_size(), image)
                        .on_hover_cursor(if zapper {
                            CursorIcon::Crosshair
                        } else {
                            CursorIcon::Default
                        });
                if zapper {
                    if let Some(pos) = frame_resp.hover_pos() {
                        let scale_x = frame_resp.rect.width() / Ppu::WIDTH as f32;
                        let scale_y = frame_resp.rect.height() / Ppu::HEIGHT as f32;
                        let aspect_ratio = self.config.read(|cfg| cfg.deck.region.aspect_ratio());
                        let x = pos.x / scale_x / aspect_ratio;
                        let y = (pos.y - self.menu_height - ui.style().spacing.menu_margin.bottom)
                            / scale_y;
                        if x > 0.0 && y > 0.0 {
                            self.send_event(EmulationEvent::ZapperAim((
                                x.round() as u32,
                                y.round() as u32,
                            )));
                        }
                    }
                }
            });
        if !self.messages.is_empty() || self.error.is_some() {
            Area::new("messages")
                .anchor(Align2::LEFT_TOP, Vec2::ZERO)
                .order(Order::Foreground)
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(Layout::top_down(Align::LEFT).with_main_wrap(true), |ui| {
                            ui.set_width(ui.available_width());
                            self.message_bar(ui);
                            self.error_bar(ui);
                        });
                    });
                });
        }
        if self.status.is_some() {
            Area::new("status")
                .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO)
                .order(Order::Foreground)
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(Layout::top_down(Align::LEFT).with_main_wrap(true), |ui| {
                            ui.set_width(ui.available_width());
                            self.status_bar(ui);
                        });
                    });
                });
        }
        if self.config.read(|cfg| cfg.renderer.show_fps) {
            ui.label(format!(
                "Last Frame: {:.4}s",
                self.last_frame_duration.as_secs_f32()
            ));
        }
    }

    fn preferences(&mut self, ui: &mut Ui) {
        ui.label("not yet implemented");

        // fn view_menu(&mut self, ui: &mut Ui, config: &mut Config) {
        //     ui.checkbox(&mut config.show_fps, "Show FPS");
        //     ui.checkbox(&mut config.show_messages, "Show Messages");
        // }
        // button Restore Defaults
        // button Clear Save State
        //
        // General
        //   textedit Config Path
        //   textedit Save Path
        //   textedit Battey-Backed Save Path
        //   checkbox Enable Rewind
        //     textedit Rewind Frames
        //     self.send_event(EmulationEvent::SetRewind(enabled));
        //     textedit Rewind Buffer Size (MB)
        //
        // Emulation
        //   combobox Speed
        //   checkbox Enable Zapper Gun
        //   combobox Four Player Mode
        //   combobox NES Region
        //   combobox RAM State
        //   checkbox Concurrent D-Pad
        //     self.send_event(EmulationEvent::SetConcurrentDpad(enabled));
        //
        // View
        //   combobox Scale
        //   checkbox Show FPS
        //   checkbox Show Messages
        //   combobox Video Filter
        //   checkbox Show Overscan
        //   combobox Fullscreen Mode
        //   checkbox Enable VSync
        //   checkbox Always On Top
        //
        // Audio
        //   checkbox Enabled
        //   combobox Output Device
        //     self.send_event(EmulationEvent::SetAudioDevice(sample_rate));
        //   combobox Sample Rate
        //     self.send_event(EmulationEvent::SetAudioSampleRate(sample_rate));
        //   combobox Latency ms
        //     self.send_event(EmulationEvent::SetAudioLatency(sample_rate));
        //   checkbox APU Channels
        // let mut save_slot = config.save_slot as usize - 1;
        // if ui.combo_box("Save Slot", &mut save_slot, &["1", "2", "3", "4"], 4)? {
        //     self.config.save_slot = save_slot as u8 + 1;
        // }
    }

    fn keybinds(&mut self, ui: &mut Ui) {
        ui.label("not yet implemented");
    }

    fn about(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(&self.version).strong());
        ui.hyperlink("https://github.com/lukexor/tetanes");

        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.separator();

            // TODO: avoid allocations
            if let Some(config_dir) = Config::config_dir() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Configuration: ").strong());
                    ui.label(config_dir.to_string_lossy());
                });
            }

            if let Some(save_dir) = Config::data_dir() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Save States & Battery-Backed Ram: ").strong());
                    ui.label(save_dir.to_string_lossy());
                });
            }
        }
    }

    fn todo(&mut self) {
        self.send_event(UiEvent::Message("not yet implemented".to_string()));
    }
}
