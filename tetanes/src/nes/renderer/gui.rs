use crate::{
    nes::{
        action::{Debug, DebugStep, Debugger, Feature, Setting, Ui as UiAction},
        config::{Config, RendererConfig},
        emulation::FrameStats,
        event::{
            ConfigEvent, EmulationEvent, NesEvent, NesEventProxy, RendererEvent, RunState, UiEvent,
        },
        input::Gamepads,
        renderer::{
            gui::{
                keybinds::Keybinds,
                lib::{
                    cursor_to_zapper, input_down, ShortcutText, ShowShortcut, ToggleValue,
                    ViewportOptions,
                },
                ppu_viewer::PpuViewer,
                preferences::Preferences,
            },
            shader::{self, Shader},
        },
        rom::{RomAsset, HOMEBREW_ROMS},
        version::Version,
    },
    platform,
};
use egui::{
    include_image,
    load::SizedTexture,
    menu,
    style::{HandleShape, Selection, WidgetVisuals},
    Align, Area, Button, CentralPanel, Color32, Context, CursorIcon, Direction, FontData,
    FontDefinitions, FontFamily, Frame, Grid, Id, Image, Layout, Order, Pos2, Rect, RichText,
    Rounding, ScrollArea, Sense, Stroke, TopBottomPanel, Ui, Vec2, Visuals,
};
use egui_winit::EventResponse;
use serde::{Deserialize, Serialize};
use tetanes_core::{
    action::Action as DeckAction,
    common::{NesRegion, ResetKind},
    control_deck::LoadedRom,
    ppu::Ppu,
    time::{Duration, Instant},
};
use tracing::{error, info, warn};
use winit::event::WindowEvent;

mod keybinds;
pub mod lib;
mod ppu_viewer;
mod preferences;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Menu {
    About,
    Keybinds,
    PerfStats,
    PpuViewer,
    Preferences,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MessageType {
    Info,
    Warn,
    Error,
}

#[derive(Debug)]
#[must_use]
pub struct Gui {
    pub initialized: bool,
    pub title: String,
    pub tx: NesEventProxy,
    pub cfg: Config,
    pub texture: SizedTexture,
    pub run_state: RunState,
    pub menu_height: f32,
    pub nes_frame: Rect,
    pub about_open: bool,
    pub perf_stats_open: bool,
    pub update_window_open: bool,
    pub version: Version,
    pub keybinds: Keybinds,
    pub preferences: Preferences,
    pub debugger_open: bool,
    pub ppu_viewer: PpuViewer,
    pub apu_mixer_open: bool,
    pub debug_gui_hover: bool,
    pub viewport_info_open: bool,
    pub replay_recording: bool,
    pub audio_recording: bool,
    pub frame_stats: FrameStats,
    pub messages: Vec<(MessageType, String, Instant)>,
    pub loaded_rom: Option<LoadedRom>,
    pub about_homebrew_rom_open: Option<RomAsset>,
    pub start: Instant,
    #[cfg(not(target_arch = "wasm32"))]
    pub sys: Option<sysinfo::System>,
    #[cfg(not(target_arch = "wasm32"))]
    pub sys_updated: Instant,
    pub error: Option<String>,
}

// TODO: Remove once https://github.com/emilk/egui/pull/4372 is released
macro_rules! hex_color {
    ($s:literal) => {{
        let array = color_hex::color_from_hex!($s);
        Color32::from_rgb(array[0], array[1], array[2])
    }};
}

impl Gui {
    const MSG_TIMEOUT: Duration = Duration::from_secs(3);
    const MAX_MESSAGES: usize = 5;
    const MENU_WIDTH: f32 = 250.0;
    const NO_ROM_LOADED: &'static str = "No ROM is loaded.";

    /// Create a `Gui` instance.
    pub fn new(tx: NesEventProxy, texture: SizedTexture, cfg: Config) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let sys = {
            use sysinfo::{ProcessRefreshKind, RefreshKind, System};

            if sysinfo::IS_SUPPORTED_SYSTEM {
                let mut sys = System::new_with_specifics(
                    RefreshKind::new().with_processes(
                        ProcessRefreshKind::new()
                            .with_cpu()
                            .with_memory()
                            .with_disk_usage(),
                    ),
                );
                sys.refresh_specifics(
                    RefreshKind::new().with_processes(
                        ProcessRefreshKind::new()
                            .with_cpu()
                            .with_memory()
                            .with_disk_usage(),
                    ),
                );
                Some(sys)
            } else {
                None
            }
        };

        Self {
            initialized: false,
            title: Config::WINDOW_TITLE.to_string(),
            tx: tx.clone(),
            cfg,
            texture,
            run_state: RunState::Running,
            menu_height: 0.0,
            nes_frame: Rect::ZERO,
            about_open: false,
            perf_stats_open: false,
            update_window_open: false,
            version: Version::new(),
            keybinds: Keybinds::new(tx.clone()),
            preferences: Preferences::new(tx.clone()),
            debugger_open: false,
            ppu_viewer: PpuViewer::new(tx),
            apu_mixer_open: false,
            debug_gui_hover: false,
            viewport_info_open: false,
            replay_recording: false,
            audio_recording: false,
            frame_stats: FrameStats::new(),
            messages: Vec::new(),
            loaded_rom: None,
            about_homebrew_rom_open: None,
            start: Instant::now(),
            #[cfg(not(target_arch = "wasm32"))]
            sys,
            #[cfg(not(target_arch = "wasm32"))]
            sys_updated: Instant::now(),
            error: None,
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) -> EventResponse {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.keybinds.wants_input()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. }
            )
        {
            EventResponse {
                consumed: true,
                ..Default::default()
            }
        } else {
            EventResponse::default()
        }
    }

    pub fn on_event(&mut self, event: &NesEvent) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        match event {
            NesEvent::Ui(UiEvent::UpdateAvailable(version)) => {
                self.version.set_latest(version.clone());
                self.update_window_open = true;
            }
            NesEvent::Emulation(event) => match event {
                EmulationEvent::ReplayRecord(recording) => {
                    self.replay_recording = *recording;
                }
                EmulationEvent::AudioRecord(recording) => {
                    self.audio_recording = *recording;
                }
                EmulationEvent::RunState(mode) => {
                    self.run_state = *mode;
                }
                _ => (),
            },
            NesEvent::Renderer(event) => match event {
                RendererEvent::FrameStats(stats) => {
                    self.frame_stats = *stats;
                }
                RendererEvent::ShowMenubar(show) => {
                    // Toggling true is handled in the menu widget
                    if !show {
                        self.menu_height = 0.0;
                    }
                }
                RendererEvent::ReplayLoaded => self.run_state = RunState::Running,
                RendererEvent::RomUnloaded => {
                    self.run_state = RunState::Running;
                    self.loaded_rom = None;
                    self.title = Config::WINDOW_TITLE.to_string();
                }
                RendererEvent::RomLoaded(rom) => {
                    self.run_state = RunState::Running;
                    self.title = format!("{} :: {}", Config::WINDOW_TITLE, rom.name);
                    self.loaded_rom = Some(rom.clone());
                }
                RendererEvent::Menu(menu) => match menu {
                    Menu::About => self.about_open = !self.about_open,
                    Menu::Keybinds => self.keybinds.toggle_open(),
                    Menu::PerfStats => {
                        self.perf_stats_open = !self.perf_stats_open;
                        self.tx
                            .event(EmulationEvent::ShowFrameStats(self.perf_stats_open));
                    }
                    Menu::PpuViewer => self.ppu_viewer.toggle_open(),
                    Menu::Preferences => self.preferences.toggle_open(),
                },
                _ => (),
            },
            _ => (),
        }
    }

    pub fn add_message<S>(&mut self, ty: MessageType, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        match ty {
            MessageType::Info => info!("{text}"),
            MessageType::Warn => warn!("{text}"),
            MessageType::Error => error!("{text}"),
        }
        self.messages
            .push((ty, text, Instant::now() + Self::MSG_TIMEOUT));
    }

    pub fn loaded_region(&self) -> Option<NesRegion> {
        self.loaded_rom.as_ref().map(|rom| rom.region)
    }

    pub fn aspect_ratio(&self) -> f32 {
        let region = self
            .cfg
            .deck
            .region
            .is_auto()
            .then(|| self.loaded_region())
            .flatten()
            .unwrap_or(self.cfg.deck.region);
        region.aspect_ratio()
    }

    pub fn prepare(&mut self, gamepads: &Gamepads, cfg: &Config) {
        self.cfg = cfg.clone();
        self.preferences.prepare(&self.cfg);
        self.keybinds.prepare(gamepads, &self.cfg);
        self.ppu_viewer.prepare(&self.cfg);
    }

    /// Create the UI.
    pub fn ui(&mut self, ctx: &Context, gamepads: Option<&Gamepads>) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if !self.initialized {
            self.initialize(ctx);
        }

        if self.cfg.renderer.show_menubar {
            TopBottomPanel::top("menu_bar").show(ctx, |ui| self.menu_bar(ui));
        }

        let viewport_opts = ViewportOptions {
            enabled: !self.keybinds.wants_input(),
            always_on_top: self.cfg.renderer.always_on_top,
        };

        CentralPanel::default()
            .frame(Frame::canvas(&ctx.style()))
            .show(ctx, |ui| {
                self.nes_frame(ui, viewport_opts.enabled, gamepads);
            });

        self.preferences.show(ctx, viewport_opts);
        self.keybinds.show(ctx, viewport_opts);
        self.ppu_viewer.show(ctx, viewport_opts);

        self.show_about_window(ctx, viewport_opts.enabled);
        self.show_about_homebrew_window(ctx, viewport_opts.enabled);

        self.show_performance_window(ctx, viewport_opts.enabled);
        self.show_update_window(ctx, viewport_opts.enabled);

        #[cfg(feature = "profiling")]
        if viewport_opts.enabled {
            puffin::profile_scope!("puffin");
            puffin_egui::show_viewport_if_enabled(ctx);
        }
    }

    fn initialize(&mut self, ctx: &Context) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let theme = if self.cfg.renderer.dark_theme {
            Self::dark_theme()
        } else {
            Self::light_theme()
        };
        ctx.set_visuals(theme);

        const FONT: (&str, &[u8]) = (
            "pixeloid-sans",
            include_bytes!("../../../assets/pixeloid-sans.ttf"),
        );
        const BOLD_FONT: (&str, &[u8]) = (
            "pixeloid-sans-bold",
            include_bytes!("../../../assets/pixeloid-sans-bold.ttf"),
        );
        const MONO_FONT: (&str, &[u8]) = (
            "pixeloid-mono",
            include_bytes!("../../../assets/pixeloid-mono.ttf"),
        );

        egui_extras::install_image_loaders(ctx);

        let mut fonts = FontDefinitions::default();
        for (name, data) in [FONT, BOLD_FONT, MONO_FONT] {
            let font_data = FontData::from_static(data);
            fonts.font_data.insert(name.to_string(), font_data);
        }

        match fonts.families.get_mut(&FontFamily::Proportional) {
            Some(font) => font.insert(0, FONT.0.to_string()),
            None => tracing::warn!("failed to set proportional font"),
        }
        match fonts.families.get_mut(&FontFamily::Monospace) {
            Some(font) => font.insert(0, MONO_FONT.0.to_string()),
            None => tracing::warn!("failed to set monospace font"),
        }
        ctx.set_fonts(fonts);

        // Check for update on start
        if self.version.requires_updates() {
            let notify_latest = false;
            self.check_for_updates(notify_latest);
        }

        self.initialized = true;
    }

    fn check_for_updates(&mut self, notify_latest: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let spawn_update = std::thread::Builder::new()
            .name("check_updates".into())
            .spawn({
                let version = self.version.clone();
                let tx = self.tx.clone();
                move || match version.update_available() {
                    Ok(Some(version)) => tx.event(UiEvent::UpdateAvailable(version)),
                    Ok(None) => {
                        if notify_latest {
                            tx.event(UiEvent::Message((
                                MessageType::Info,
                                format!("TetaNES v{} is up to date!", version.current()),
                            )));
                        }
                    }
                    Err(err) => {
                        tx.event(UiEvent::Message((MessageType::Error, err.to_string())));
                    }
                }
            });
        if let Err(err) = spawn_update {
            self.add_message(
                MessageType::Error,
                format!("Failed to check for updates: {err}"),
            );
        }
    }

    fn show_about_window(&mut self, ctx: &Context, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut about_open = self.about_open;
        egui::Window::new("About TetaNES")
            .open(&mut about_open)
            .show(ctx, |ui| self.about(ui, enabled));
        self.about_open = about_open;
    }

    fn show_about_homebrew_window(&mut self, ctx: &Context, enabled: bool) {
        let Some(rom) = self.about_homebrew_rom_open else {
            return;
        };

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut about_homebrew_open = true;
        egui::Window::new(format!("About {}", rom.name))
            .open(&mut about_homebrew_open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        ui.strong("Author(s):");
                        ui.label(rom.authors);
                        ui.add_space(12.0);

                        ui.strong("Description:");
                        ui.label(rom.description);
                        ui.add_space(12.0);

                        ui.strong("Source:");
                        ui.hyperlink(rom.source);
                    });
                });
            });
        if !about_homebrew_open {
            self.about_homebrew_rom_open = None;
        }
    }

    fn show_performance_window(&mut self, ctx: &Context, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut perf_stats_open = self.perf_stats_open;
        egui::Window::new("Performance Stats")
            .open(&mut perf_stats_open)
            .show(ctx, |ui| self.performance_stats(ui, enabled));
        self.perf_stats_open = perf_stats_open;
    }

    fn show_update_window(&mut self, ctx: &Context, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut update_window_open = self.update_window_open;
        let mut close_window = false;
        let enable_auto_update = false;
        egui::Window::new("Update Available")
            .open(&mut update_window_open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.label(format!(
                        "An update is available for TetaNES! (v{})",
                        self.version.latest(),
                    ));
                    ui.hyperlink("https://github.com/lukexor/tetanes/releases");

                    ui.add_space(15.0);
                    ui.separator();
                    ui.add_space(15.0);

                    // TODO: Add auto-update for each platform
                    if enable_auto_update {
                        ui.label("Would you like to install it and restart?");
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            let res = ui.button("Continue").on_hover_text(format!(
                                "Install the latest version (v{}) restart TetaNES.",
                                self.version.current()
                            ));
                            if res.clicked() {
                                if let Err(err) = self.version.install_update_and_restart() {
                                    self.add_message(
                                        MessageType::Error,
                                        format!("Failed to install update: {err}"),
                                    );
                                    close_window = true;
                                }
                            }
                            let res = ui.button("Cancel").on_hover_text(format!(
                                "Keep the current version of TetaNES (v{}).",
                                self.version.current()
                            ));
                            if res.clicked() {
                                close_window = true;
                            }
                        });
                    }
                });
            });
        if close_window {
            update_window_open = false;
        }
        self.update_window_open = update_window_open;
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.set_enabled(!self.keybinds.wants_input());

        let inner_res = menu::bar(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                Self::toggle_dark_mode_button(&self.tx, ui);

                ui.separator();

                ui.menu_button("📁 File", |ui| self.file_menu(ui));
                ui.menu_button("🔧 Controls", |ui| self.controls_menu(ui));
                ui.menu_button("⚙ Config", |ui| self.config_menu(ui));
                // icon: screen
                ui.menu_button("🖵 Window", |ui| self.window_menu(ui));
                ui.menu_button("🕷 Debug", |ui| self.debug_menu(ui));
                ui.menu_button("❓ Help", |ui| self.help_menu(ui));
            });
        });
        let spacing = ui.style().spacing.item_spacing;
        let border = 1.0;
        let height = inner_res.response.rect.height() + spacing.y + border;
        if height != self.menu_height {
            self.menu_height = height;
            self.tx.event(RendererEvent::ResizeTexture);
        }
    }

    pub fn toggle_dark_mode_button(tx: &NesEventProxy, ui: &mut Ui) {
        if ui.ctx().style().visuals.dark_mode {
            let button = Button::new("☀").frame(false);
            let res = ui.add(button).on_hover_text("Switch to light mode");
            if res.clicked() {
                ui.ctx().set_visuals(Self::light_theme());
                tx.event(ConfigEvent::DarkTheme(false));
            }
        } else {
            let button = Button::new("🌙").frame(false);
            let res = ui.add(button).on_hover_text("Switch to dark mode");
            if res.clicked() {
                ui.ctx().set_visuals(Self::dark_theme());
                tx.event(ConfigEvent::DarkTheme(true));
            }
        }
    }

    fn file_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        let button =
            Button::new("📂 Load ROM...").shortcut_text(self.cfg.shortcut(UiAction::LoadRom));
        if ui.add(button).clicked() {
            if self.loaded_rom.is_some() {
                self.run_state = RunState::Paused;
                self.tx.event(EmulationEvent::RunState(RunState::Paused));
            }
            // NOTE: Due to some platforms file dialogs blocking the event loop,
            // loading requires a round-trip in order for the above pause to
            // get processed.
            self.tx.event(UiEvent::LoadRomDialog);
            ui.close_menu();
        }

        ui.menu_button("🍺 Homebrew ROM...", |ui| self.homebrew_rom_menu(ui));

        let tx = &self.tx;
        let cfg = &self.cfg;

        ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
            let button =
                Button::new("⏹ Unload ROM...").shortcut_text(cfg.shortcut(UiAction::UnloadRom));
            let res = ui.add(button).on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::UnloadRom);
                ui.close_menu();
            }

            let button =
                Button::new("🎞 Load Replay").shortcut_text(cfg.shortcut(UiAction::LoadReplay));
            let res = ui
                .add(button)
                .on_hover_text("Load a replay file for the currently loaded ROM.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                self.run_state = RunState::Paused;
                tx.event(EmulationEvent::RunState(RunState::Paused));
                // NOTE: Due to some platforms file dialogs blocking the event loop,
                // loading requires a round-trip in order for the above pause to
                // get processed.
                tx.event(UiEvent::LoadReplayDialog);
                ui.close_menu();
            }
        });

        // TODO: support saves and recent games on wasm? Requires storing the data
        if platform::supports(platform::Feature::Filesystem) {
            ui.menu_button("🗄 Recently Played...", |ui| {
                use tetanes_core::fs;

                if cfg.renderer.recent_roms.is_empty() {
                    ui.label("No recent ROMs");
                } else {
                    ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

                    ScrollArea::vertical().show(ui, |ui| {
                        // TODO: add timestamp, save slots, and screenshot
                        for rom in &cfg.renderer.recent_roms {
                            if ui.button(fs::filename(rom)).clicked() {
                                tx.event(EmulationEvent::LoadRomPath(rom.to_path_buf()));
                                ui.close_menu();
                            }
                        }
                    });
                }
            });

            ui.separator();
        }

        if platform::supports(platform::Feature::Storage) {
            ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
                let button =
                    Button::new("💾 Save State").shortcut_text(cfg.shortcut(DeckAction::SaveState));
                let res = ui
                    .add(button)
                    .on_hover_text("Save the current state to the selected save slot.")
                    .on_disabled_hover_text(Self::NO_ROM_LOADED);
                if res.clicked() {
                    tx.event(EmulationEvent::SaveState(cfg.emulation.save_slot));
                };

                let button =
                    Button::new("⎗ Load State").shortcut_text(cfg.shortcut(DeckAction::LoadState));
                let res = ui
                    .add(button)
                    .on_hover_text("Load a previous state from the selected save slot.")
                    .on_disabled_hover_text(Self::NO_ROM_LOADED);
                if res.clicked() {
                    tx.event(EmulationEvent::LoadState(cfg.emulation.save_slot));
                }
            });

            // icon: # in a square
            ui.menu_button("󾠬 Save Slot...", |ui| {
                Preferences::save_slot_radio(
                    tx,
                    ui,
                    cfg.emulation.save_slot,
                    cfg,
                    ShowShortcut::Yes,
                );
            });
        }

        if platform::supports(platform::Feature::Viewports) {
            ui.separator();

            let button = Button::new("⎆ Quit").shortcut_text(cfg.shortcut(UiAction::Quit));
            if ui.add(button).clicked() {
                tx.event(UiEvent::Terminate);
                ui.close_menu();
            };
        }
    }

    fn homebrew_rom_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ScrollArea::vertical().show(ui, |ui| {
            for rom in HOMEBREW_ROMS {
                ui.horizontal(|ui| {
                    if ui.button(rom.name).clicked() {
                        self.tx
                            .event(EmulationEvent::LoadRom((rom.name.to_string(), rom.data())));
                        ui.close_menu();
                    }
                    let res = ui.button("ℹ").on_hover_ui(|ui| {
                        ui.set_max_width(400.0);
                        Self::about_homebrew(ui, rom);
                    });
                    if res.clicked() {
                        self.about_homebrew_rom_open = Some(rom);
                        ui.close_menu();
                    }
                });
            }
        });
    }

    fn controls_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        let tx = &self.tx;
        let cfg = &self.cfg;

        ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
            let button = Button::new(if self.run_state.paused() {
                "▶ Resume"
            } else {
                "⏸ Pause"
            })
            .shortcut_text(cfg.shortcut(UiAction::TogglePause));
            let res = ui.add(button).on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                self.run_state = match self.run_state {
                    RunState::Running => RunState::ManuallyPaused,
                    RunState::ManuallyPaused | RunState::Paused => RunState::Running,
                };
                tx.event(EmulationEvent::RunState(self.run_state));
                ui.close_menu();
            };
        });

        let button = Button::new(if cfg.audio.enabled {
            "🔇 Mute"
        } else {
            "🔊 Unmute"
        })
        .shortcut_text(cfg.shortcut(Setting::ToggleAudio));
        if ui.add(button).clicked() {
            tx.event(ConfigEvent::AudioEnabled(!cfg.audio.enabled));
        };

        ui.separator();

        ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
            ui.add_enabled_ui(cfg.emulation.rewind, |ui| {
                let button = Button::new("⟲ Instant Rewind")
                    .shortcut_text(cfg.shortcut(Feature::InstantRewind));
                let disabled_hover_text = if self.loaded_rom.is_none() {
                    Self::NO_ROM_LOADED
                } else {
                    "Rewind can be enabled under the `Config` menu."
                };
                let res = ui
                    .add(button)
                    .on_hover_text("Instantly rewind state to a previous point.")
                    .on_disabled_hover_text(disabled_hover_text);
                if res.clicked() {
                    tx.event(EmulationEvent::InstantRewind);
                    ui.close_menu();
                };
            });

            let button = Button::new("🔃 Reset")
                .shortcut_text(cfg.shortcut(DeckAction::Reset(ResetKind::Soft)));
            let res = ui
                .add(button)
                .on_hover_text("Emulate a soft reset of the NES.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::Reset(ResetKind::Soft));
                ui.close_menu();
            };

            let button = Button::new("🔌 Power Cycle")
                .shortcut_text(cfg.shortcut(DeckAction::Reset(ResetKind::Hard)));
            let res = ui
                .add(button)
                .on_hover_text("Emulate a power cycle of the NES.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::Reset(ResetKind::Hard));
                ui.close_menu();
            };
        });

        if platform::supports(platform::Feature::Filesystem) {
            ui.separator();

            ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
                let button = Button::new("🖼 Screenshot")
                    .shortcut_text(cfg.shortcut(Feature::TakeScreenshot));
                let res = ui.add(button).on_disabled_hover_text(Self::NO_ROM_LOADED);
                if res.clicked() {
                    tx.event(EmulationEvent::Screenshot);
                    ui.close_menu();
                };

                let button_txt = if self.replay_recording {
                    "⏹ Stop Replay Recording"
                } else {
                    "🎞 Record Replay"
                };
                let button = Button::new(button_txt)
                    .shortcut_text(cfg.shortcut(Feature::ToggleReplayRecording));
                let res = ui
                    .add(button)
                    .on_hover_text("Record or stop recording a game replay file.")
                    .on_disabled_hover_text(Self::NO_ROM_LOADED);
                if res.clicked() {
                    tx.event(EmulationEvent::ReplayRecord(!self.replay_recording));
                    ui.close_menu();
                };

                let button_txt = if self.audio_recording {
                    "⏹ Stop Audio Recording"
                } else {
                    "🎤 Record Audio"
                };
                let button = Button::new(button_txt)
                    .shortcut_text(cfg.shortcut(Feature::ToggleAudioRecording));
                let res = ui
                    .add(button)
                    .on_hover_text("Record or stop recording a audio file.")
                    .on_disabled_hover_text(Self::NO_ROM_LOADED);
                if res.clicked() {
                    tx.event(EmulationEvent::AudioRecord(!self.audio_recording));
                    ui.close_menu();
                };
            });
        }
    }

    fn config_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        let tx = &self.tx;
        let cfg = &self.cfg;

        Preferences::cycle_accurate_checkbox(
            tx,
            ui,
            cfg.deck.cycle_accurate,
            cfg.shortcut(Setting::ToggleCycleAccurate),
        );
        Preferences::zapper_checkbox(
            tx,
            ui,
            cfg.deck.zapper,
            cfg.shortcut(DeckAction::ToggleZapperConnected),
        );
        Preferences::rewind_checkbox(
            tx,
            ui,
            cfg.emulation.rewind,
            cfg.shortcut(Setting::ToggleRewinding),
        );
        Preferences::overscan_checkbox(
            tx,
            ui,
            cfg.renderer.hide_overscan,
            cfg.shortcut(Setting::ToggleOverscan),
        );

        ui.separator();

        ui.menu_button("🕒 Emulation Speed...", |ui| {
            let speed = cfg.emulation.speed;
            let button =
                Button::new("Increment").shortcut_text(cfg.shortcut(Setting::IncrementSpeed));
            if ui.add(button).clicked() {
                let new_speed = cfg.next_increment_speed();
                if speed != new_speed {
                    tx.event(ConfigEvent::Speed(new_speed));
                }
            }

            let button =
                Button::new("Decrement").shortcut_text(cfg.shortcut(Setting::DecrementSpeed));
            if ui.add(button).clicked() {
                let new_speed = cfg.next_decrement_speed();
                if speed != new_speed {
                    tx.event(ConfigEvent::Speed(new_speed));
                }
            }
            Preferences::speed_slider(tx, ui, cfg.emulation.speed);
        });
        ui.menu_button("🏃 Run Ahead...", |ui| {
            Preferences::run_ahead_slider(tx, ui, cfg.emulation.run_ahead);
        });

        ui.separator();

        ui.menu_button("🌉 Video Filter...", |ui| {
            Preferences::video_filter_radio(tx, ui, cfg.deck.filter);
        });
        ui.menu_button("🕶 Shader...", |ui| {
            Preferences::shader_radio(tx, ui, cfg.renderer.shader);
        });
        ui.menu_button("🌎 Nes Region...", |ui| {
            Preferences::nes_region_radio(tx, ui, cfg.deck.region);
        });
        ui.menu_button("🎮 Four Player...", |ui| {
            Preferences::four_player_radio(tx, ui, cfg.deck.four_player);
        });
        ui.menu_button("📓 Game Genie Codes...", |ui| {
            self.preferences.show_genie_codes_entry(ui, cfg);

            ui.separator();

            Preferences::genie_codes_list(tx, ui, cfg, true);
        });

        ui.separator();

        let mut preferences_open = self.preferences.open();
        // icon: gear
        let toggle = ToggleValue::new(&mut preferences_open, "⛭ Preferences")
            .shortcut_text(cfg.shortcut(Menu::Preferences));
        if ui.add(toggle).clicked() {
            self.preferences.set_open(preferences_open);
            ui.close_menu();
        }

        let mut keybinds_open = self.keybinds.open();
        // icon: keyboard
        let toggle = ToggleValue::new(&mut keybinds_open, "🖮 Keybinds")
            .shortcut_text(cfg.shortcut(Menu::Keybinds));
        if ui.add(toggle).clicked() {
            self.keybinds.set_open(keybinds_open);
            ui.close_menu();
        };
    }

    fn window_menu(&mut self, ui: &mut Ui) {
        use Setting::*;

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        let tx = &self.tx;
        let cfg = &self.cfg;
        let RendererConfig {
            scale,
            fullscreen,
            always_on_top,
            show_menubar,
            show_messages,
            ..
        } = cfg.renderer;

        ui.menu_button("📏 Window Scale...", |ui| {
            let button = Button::new("Increment").shortcut_text(cfg.shortcut(IncrementScale));
            if ui.add(button).clicked() {
                let new_scale = cfg.next_increment_scale();
                if scale != new_scale {
                    tx.event(ConfigEvent::Scale(scale));
                }
            }

            let button = Button::new("Decrement").shortcut_text(cfg.shortcut(DecrementScale));
            if ui.add(button).clicked() {
                let new_scale = cfg.next_decrement_scale();
                if scale != new_scale {
                    tx.event(ConfigEvent::Scale(scale));
                }
            }

            Preferences::window_scale_radio(tx, ui, cfg.renderer.scale);
        });
        egui::gui_zoom::zoom_menu_buttons(ui);

        ui.separator();

        Preferences::fullscreen_checkbox(tx, ui, fullscreen, cfg.shortcut(ToggleFullscreen));
        Preferences::embed_viewports_checkbox(tx, ui, cfg, cfg.shortcut(ToggleEmbedViewports));
        Preferences::always_on_top_checkbox(tx, ui, always_on_top, cfg.shortcut(ToggleAlwaysOnTop));

        ui.separator();

        Preferences::menubar_checkbox(tx, ui, show_menubar, cfg.shortcut(ToggleMenubar));
        Preferences::messages_checkbox(tx, ui, show_messages, cfg.shortcut(ToggleMessages));
    }

    fn debug_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        let tx = &self.tx;
        let cfg = &self.cfg;

        #[cfg(feature = "profiling")]
        {
            let mut profile = puffin::are_scopes_on();
            ui.toggle_value(&mut profile, "Profiler")
                .on_hover_text("Toggle the Puffin profiling window");
            puffin::set_scopes_on(profile);
        }

        let mut perf_stats_open = self.perf_stats_open;
        let toggle = ToggleValue::new(&mut perf_stats_open, "🛠 Performance Stats")
            .shortcut_text(cfg.shortcut(Menu::PerfStats));
        let res = ui
            .add(toggle)
            .on_hover_text("Enable a performance statistics overlay");
        if res.clicked() {
            self.perf_stats_open = perf_stats_open;
            tx.event(EmulationEvent::ShowFrameStats(self.perf_stats_open));
            ui.close_menu();
        }

        #[cfg(debug_assertions)]
        {
            let res = ui.checkbox(&mut self.debug_gui_hover, "Debug GUI Hover");
            if res.clicked() {
                ui.ctx().set_debug_on_hover(self.debug_gui_hover);
            }

            ui.toggle_value(&mut self.viewport_info_open, "Viewport Info");
        }

        ui.separator();

        ui.add_enabled_ui(false, |ui| {
            let debugger_shortcut = cfg.shortcut(Debug::Toggle(Debugger::Cpu));
            let toggle = ToggleValue::new(&mut self.debugger_open, "🚧 Debugger")
                .shortcut_text(debugger_shortcut);
            let res = ui
                .add(toggle)
                .on_hover_text("Toggle the Debugger.")
                .on_disabled_hover_text("Not yet implemented.");
            if res.clicked() {
                ui.close_menu();
            }

            let ppu_viewer_shortcut = cfg.shortcut(Debug::Toggle(Debugger::Ppu));
            let mut open = self.ppu_viewer.open();
            let toggle =
                ToggleValue::new(&mut open, "🌇 PPU Viewer").shortcut_text(ppu_viewer_shortcut);
            let res = ui
                .add(toggle)
                .on_hover_text("Toggle the PPU Viewer.")
                .on_disabled_hover_text("Not yet implemented.");
            if res.clicked() {
                self.ppu_viewer.set_open(open);
                ui.close_menu();
            }

            let apu_mixer_shortcut = cfg.shortcut(Debug::Toggle(Debugger::Apu));
            let toggle = ToggleValue::new(&mut self.apu_mixer_open, "🎼 APU Mixer")
                .shortcut_text(apu_mixer_shortcut);
            let res = ui
                .add(toggle)
                .on_hover_text("Toggle the APU Mixer.")
                .on_disabled_hover_text("Not yet implemented.");
            if res.clicked() {
                ui.close_menu();
            }
        });

        ui.separator();

        ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
            let button =
                Button::new("Step Into").shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Into)));
            let res = ui
                .add(button)
                .on_hover_text("Step a single CPU instruction.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Into));
            }

            let button =
                Button::new("Step Out").shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Out)));
            let res = ui
                .add(button)
                .on_hover_text("Step out of the current CPU function.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Out));
            }

            let button =
                Button::new("Step Over").shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Over)));
            let res = ui
                .add(button)
                .on_hover_text("Step over the next CPU instruction.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Over));
            }

            let button = Button::new("Step Scanline")
                .shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Scanline)));
            let res = ui
                .add(button)
                .on_hover_text("Step an entire PPU scanline.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Scanline));
            }

            let button = Button::new("Step Frame")
                .shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Frame)));
            let res = ui
                .add(button)
                .on_hover_text("Step an entire PPU Frame.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Frame));
            }
        });
    }

    fn nes_frame(&mut self, ui: &mut Ui, enabled: bool, gamepads: Option<&Gamepads>) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            let tx = &self.tx;

            let inner_res = CentralPanel::default()
                .frame(Frame::none())
                .show_inside(ui, |ui| {
                    if self.loaded_rom.is_some() {
                        let layout = Layout {
                            main_dir: Direction::TopDown,
                            main_align: Align::Center,
                            cross_align: Align::Center,
                            ..Default::default()
                        };
                        ui.with_layout(layout, |ui| {
                            let image_sense = Sense::click();
                            let image = Image::from_texture(self.texture)
                                .maintain_aspect_ratio(true)
                                .shrink_to_fit()
                                .sense(image_sense);

                            let hover_cursor = if self.cfg.deck.zapper {
                                CursorIcon::Crosshair
                            } else {
                                CursorIcon::Default
                            };

                            let res = if matches!(self.cfg.renderer.shader, Shader::None) {
                                ui.add(image)
                            } else {
                                let texture_load_res =
                                    image.load_for_size(ui.ctx(), ui.available_size());
                                let image_size =
                                    texture_load_res.as_ref().ok().and_then(|t| t.size());
                                let ui_size = image.calc_size(ui.available_size(), image_size);
                                let (rect, res) = ui.allocate_exact_size(ui_size, image_sense);
                                let res = res.on_hover_cursor(hover_cursor);

                                if ui.is_rect_visible(rect) {
                                    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                                        rect,
                                        shader::Renderer::new(rect),
                                    ));
                                }

                                res
                            };
                            self.nes_frame = res.rect;

                            if self.cfg.deck.zapper {
                                if self
                                    .cfg
                                    .action_input(DeckAction::ZapperAimOffscreen)
                                    .map_or(false, |input| {
                                        input_down(ui, gamepads, &self.cfg, input)
                                    })
                                {
                                    let pos = (Ppu::WIDTH + 10, Ppu::HEIGHT + 10);
                                    tx.event(EmulationEvent::ZapperAim(pos));
                                } else if let Some(Pos2 { x, y }) = res
                                    .hover_pos()
                                    .and_then(|Pos2 { x, y }| cursor_to_zapper(x, y, res.rect))
                                {
                                    let pos = (x.round() as u32, y.round() as u32);
                                    tx.event(EmulationEvent::ZapperAim(pos));
                                }
                                if res.clicked() {
                                    tx.event(EmulationEvent::ZapperTrigger);
                                }
                            }
                        });
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.horizontal_centered(|ui| {
                                let image =
                                    Image::new(include_image!("../../../assets/tetanes.png"))
                                        .shrink_to_fit()
                                        .tint(Color32::GRAY);
                                ui.add(image);
                            });
                        });
                    }
                });

            // Start at the left-top of the NES frame.
            let mut messages_pos = inner_res.response.rect.left_top();

            let mut recording_labels = Vec::new();
            if self.replay_recording {
                recording_labels.push("Replay");
            }
            if self.audio_recording {
                recording_labels.push("Audio");
            }
            if !recording_labels.is_empty() {
                let inner_res = Area::new(Id::new("status"))
                    .order(Order::Foreground)
                    .fixed_pos(messages_pos)
                    .show(ui.ctx(), |ui| {
                        Frame::popup(ui.style()).show(ui, |ui| {
                            ui.with_layout(
                                Layout::top_down_justified(Align::LEFT).with_main_wrap(true),
                                |ui| {
                                    ui.label(format!("Recording {}", recording_labels.join(" & ")))
                                },
                            );
                        });
                    });
                // Update to the left-bottom of this area, if rendered
                messages_pos = inner_res.response.rect.left_bottom();
            }

            if self.cfg.renderer.show_messages
                && (!self.messages.is_empty() || self.error.is_some())
            {
                Area::new(Id::new("messages"))
                    .order(Order::Foreground)
                    .fixed_pos(messages_pos)
                    .show(ui.ctx(), |ui| {
                        Frame::popup(ui.style()).show(ui, |ui| {
                            ui.with_layout(
                                Layout::top_down_justified(Align::LEFT).with_main_wrap(true),
                                |ui| {
                                    self.message_bar(ui);
                                    self.error_bar(ui);
                                },
                            );
                        });
                    });
            }

            let mut frame = Frame::none();
            if self.run_state.paused() {
                frame = Frame::dark_canvas(ui.style()).multiply_with_opacity(0.7);
            }

            frame.show(ui, |ui| {
                ui.with_layout(Layout::centered_and_justified(Direction::TopDown), |ui| {
                    if self.run_state.paused() {
                        ui.heading(RichText::new("⏸").size(40.0));
                    }
                });
            });
        });
    }

    fn performance_stats(&mut self, ui: &mut Ui, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(200.0, 0.0));

        let cfg = &self.cfg;

        ui.add_enabled_ui(enabled, |ui| {
            let grid = Grid::new("perf_stats").num_columns(2).spacing([40.0, 6.0]);
            grid.show(ui, |ui| {
                ui.ctx().request_repaint_after(Duration::from_secs(1));

                #[cfg(not(target_arch = "wasm32"))]
                if let Some(sys) = &mut self.sys {
                    // NOTE: refreshing sysinfo is cpu-intensive if done too frequently and skews the
                    // results
                    let sys_update_interval = Duration::from_secs(1);
                    assert!(sys_update_interval > sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
                    if self.sys_updated.elapsed() >= sys_update_interval {
                        sys.refresh_specifics(
                            sysinfo::RefreshKind::new().with_processes(
                                sysinfo::ProcessRefreshKind::new()
                                    .with_cpu()
                                    .with_memory()
                                    .with_disk_usage(),
                            ),
                        );
                        self.sys_updated = Instant::now();
                    }
                }

                let good_color = if ui.style().visuals.dark_mode {
                    hex_color!("#b8cc52")
                } else {
                    hex_color!("#86b300")
                };
                let warn_color = ui.style().visuals.warn_fg_color;
                let bad_color = ui.style().visuals.error_fg_color;
                let fps_color = |fps| match fps {
                    fps if fps < 30.0 => bad_color,
                    fps if fps < 60.0 => warn_color,
                    _ => good_color,
                };
                let frame_time_color = |time| match time {
                    time if time <= 1000.0 * 1.0 / 60.0 => good_color,
                    time if time <= 1000.0 * 1.0 / 30.0 => warn_color,
                    _ => bad_color,
                };

                let fps = self.frame_stats.fps;
                ui.strong("FPS:");
                if fps.is_finite() {
                    ui.colored_label(fps_color(fps), format!("{fps:.2}"));
                } else {
                    ui.label("N/A");
                }
                ui.end_row();

                let fps_min = self.frame_stats.fps_min;
                ui.strong("FPS (min):");
                if fps_min.is_finite() {
                    ui.colored_label(fps_color(fps_min), format!("{fps_min:.2}"));
                } else {
                    ui.label("N/A");
                }
                ui.end_row();

                let frame_time = self.frame_stats.frame_time;
                ui.strong("Frame Time:");
                if frame_time.is_finite() {
                    ui.colored_label(frame_time_color(frame_time), format!("{frame_time:.2} ms"));
                } else {
                    ui.label("N/A");
                }
                ui.end_row();

                let frame_time_max = self.frame_stats.frame_time_max;
                ui.strong("Frame Time (max):");
                if frame_time_max.is_finite() {
                    ui.colored_label(
                        frame_time_color(frame_time_max),
                        format!("{frame_time_max:.2} ms"),
                    );
                } else {
                    ui.label("N/A");
                }
                ui.end_row();

                ui.strong("Frame Count:");
                ui.label(format!("{}", self.frame_stats.frame_count));
                ui.end_row();

                #[cfg(not(target_arch = "wasm32"))]
                if let Some(ref sys) = self.sys {
                    let cpu_color = |cpu| match cpu {
                        cpu if cpu <= 25.0 => good_color,
                        cpu if cpu <= 50.0 => warn_color,
                        _ => bad_color,
                    };
                    const fn bytes_to_mb(bytes: u64) -> u64 {
                        bytes / 0x100000
                    }

                    ui.label("");
                    ui.end_row();

                    if let Some(proc) = sys.process(sysinfo::Pid::from_u32(std::process::id())) {
                        ui.strong("CPU:");
                        let cpu_usage = proc.cpu_usage();
                        ui.colored_label(cpu_color(cpu_usage), format!("{cpu_usage:.2}%"));
                        ui.end_row();

                        ui.strong("Memory:");
                        ui.label(format!("{} MB", bytes_to_mb(proc.memory()),));
                        ui.end_row();

                        let du = proc.disk_usage();
                        ui.strong("Disk read new/total:");
                        ui.label(format!(
                            "{:.2}/{:.2} MB",
                            bytes_to_mb(du.read_bytes),
                            bytes_to_mb(du.total_read_bytes)
                        ));
                        ui.end_row();

                        ui.strong("Disk written new/total:");
                        ui.label(format!(
                            "{:.2}/{:.2} MB",
                            bytes_to_mb(du.written_bytes),
                            bytes_to_mb(du.total_written_bytes),
                        ));
                        ui.end_row();
                    }
                }

                ui.label("");
                ui.end_row();

                ui.strong("Run Time:");
                ui.label(format!("{} s", self.start.elapsed().as_secs()));
                ui.end_row();

                let (cursor_pos, zapper_pos) = match ui.input(|i| i.pointer.latest_pos()) {
                    Some(Pos2 { x, y }) => {
                        let zapper_pos = match cursor_to_zapper(x, y, self.nes_frame) {
                            Some(Pos2 { x, y }) => format!("({x:.0}, {y:.0})"),
                            None => "(-, -)".to_string(),
                        };
                        (format!("({x:.0}, {y:.0})"), zapper_pos)
                    }
                    None => ("(-, -)".to_string(), "(-, -)".to_string()),
                };

                ui.strong("Cursor Pos:");
                ui.label(cursor_pos);
                ui.end_row();

                if cfg.deck.zapper {
                    ui.strong("Zapper Pos:");
                    ui.label(zapper_pos);
                    ui.end_row();
                }
            });
        });
    }

    fn help_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        if self.version.requires_updates() && ui.button("🌐 Check for Updates...").clicked() {
            let notify_latest = true;
            self.check_for_updates(notify_latest);
            ui.close_menu();
        }
        ui.toggle_value(&mut self.about_open, "ℹ About");
    }

    fn about(&mut self, ui: &mut Ui, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                let image = Image::new(include_image!("../../../assets/tetanes_icon.png"))
                    .max_height(50.0)
                    .shrink_to_fit();
                ui.add(image);

                ui.vertical(|ui| {
                    let grid = Grid::new("version").num_columns(2).spacing([40.0, 6.0]);
                    grid.show(ui, |ui| {
                        ui.strong("Version:");
                        ui.label(self.version.current());
                        ui.end_row();

                        ui.strong("GitHub:");
                        ui.hyperlink("https://github.com/lukexor/tetanes");
                        ui.end_row();
                    });

                    if platform::supports(platform::Feature::Filesystem) {
                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            let grid = Grid::new("directories").num_columns(2).spacing([40.0, 6.0]);
                            grid.show(ui, |ui| {
                                let config_dir = Config::default_config_dir();
                                ui.strong("Preferences:");
                                ui.label(format!("{}", config_dir.display()));
                                ui.end_row();

                                let data_dir = Config::default_data_dir();
                                ui.strong("Save States/RAM, Replays: ");
                                ui.label(format!("{}", data_dir.display()));
                                ui.end_row();

                                let picture_dir = Config::default_picture_dir();
                                ui.strong("Screenshots: ");
                                ui.label(format!("{}", picture_dir.display()));
                                ui.end_row();

                                let audio_dir = Config::default_audio_dir();
                                ui.strong("Audio Recordings: ");
                                ui.label(format!("{}", audio_dir.display()));
                                ui.end_row();
                            });
                        });
                    }
                });
            });
        });
    }

    fn about_homebrew(ui: &mut Ui, rom: RomAsset) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.strong("Author(s):");
            ui.label(rom.authors);
            ui.add_space(12.0);

            ui.strong("Description:");
            ui.label(rom.description);
            ui.add_space(12.0);

            ui.strong("Source:");
            ui.hyperlink(rom.source);
        });
    }

    fn message_bar(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.messages.retain(|(_, _, expires)| now < *expires);
        self.messages.dedup_by(|a, b| a.1.eq(&b.1));
        for (ty, message, _) in self.messages.iter().take(Self::MAX_MESSAGES) {
            let visuals = &ui.style().visuals;
            let (icon, color) = match ty {
                MessageType::Info => ("ℹ", visuals.widgets.noninteractive.fg_stroke.color),
                MessageType::Warn => ("⚠", visuals.warn_fg_color),
                MessageType::Error => ("❗", visuals.error_fg_color),
            };
            ui.colored_label(color, format!("{icon} {message}"));
        }
    }

    fn error_bar(&mut self, ui: &mut Ui) {
        if let Some(error) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(error).color(Color32::RED));
                if ui.button("").clicked() {
                    self.error = None;
                }
            });
        }
    }

    pub fn dark_theme() -> egui::Visuals {
        Visuals {
            dark_mode: true,
            widgets: egui::style::Widgets {
                noninteractive: WidgetVisuals {
                    weak_bg_fill: hex_color!("#14191f"),
                    bg_fill: hex_color!("#14191f"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#253340")), // separators, indentation lines
                    fg_stroke: Stroke::new(1.0, hex_color!("#e6b673")), // normal text color
                    rounding: Rounding::ZERO,
                    expansion: 0.0,
                },
                inactive: WidgetVisuals {
                    weak_bg_fill: hex_color!("#253340"), // button background
                    bg_fill: hex_color!("#253340"),      // checkbox background
                    bg_stroke: Stroke::default(),
                    fg_stroke: Stroke::new(1.0, hex_color!("#a9491f")), // button text
                    rounding: Rounding::ZERO,
                    expansion: 0.0,
                },
                hovered: WidgetVisuals {
                    weak_bg_fill: hex_color!("#212733"),
                    bg_fill: hex_color!("#212733"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#f29718")), // e.g. hover over window edge or button
                    fg_stroke: Stroke::new(1.5, hex_color!("#ffb454")),
                    rounding: Rounding::ZERO,
                    expansion: 1.0,
                },
                active: WidgetVisuals {
                    weak_bg_fill: hex_color!("#253340"),
                    bg_fill: hex_color!("#253340"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#fed7aa")),
                    fg_stroke: Stroke::new(2.0, hex_color!("#fed7aa")),
                    rounding: Rounding::ZERO,
                    expansion: 1.0,
                },
                open: WidgetVisuals {
                    weak_bg_fill: hex_color!("#151a1e"),
                    bg_fill: hex_color!("#14191f"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#253340")),
                    fg_stroke: Stroke::new(1.0, hex_color!("#ffb454")),
                    rounding: Rounding::ZERO,
                    expansion: 0.0,
                },
            },
            selection: Selection {
                bg_fill: hex_color!("#253340"),
                stroke: Stroke::new(1.0, hex_color!("#ffb454")),
            },
            hyperlink_color: hex_color!("#36a3d9"),
            faint_bg_color: Color32::from_additive_luminance(5), // visible, but barely so
            extreme_bg_color: hex_color!("#091015"),             // e.g. TextEdit background
            code_bg_color: hex_color!("#253340"),
            warn_fg_color: hex_color!("#e7c547"),
            error_fg_color: hex_color!("#ff3333"),
            window_rounding: Rounding::ZERO,
            window_fill: hex_color!("#14191f"),
            window_stroke: Stroke::new(1.0, hex_color!("#253340")),
            window_highlight_topmost: true,
            menu_rounding: Rounding::ZERO,
            panel_fill: hex_color!("#14191f"),
            text_cursor: Stroke::new(2.0, hex_color!("#95e6cb")),
            striped: true,
            handle_shape: HandleShape::Rect { aspect_ratio: 1.25 },
            ..Default::default()
        }
    }

    pub fn light_theme() -> egui::Visuals {
        egui::Visuals {
            dark_mode: false,
            widgets: egui::style::Widgets {
                noninteractive: WidgetVisuals {
                    weak_bg_fill: hex_color!("#ffffff"),
                    bg_fill: hex_color!("#ffffff"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#d9d7ce")), // separators, indentation lines
                    fg_stroke: Stroke::new(1.0, hex_color!("#253340")), // normal text color
                    rounding: Rounding::ZERO,
                    expansion: 0.0,
                },
                inactive: WidgetVisuals {
                    weak_bg_fill: hex_color!("#d9d8d7"), // button background
                    bg_fill: hex_color!("#d9d8d7"),      // checkbox background
                    bg_stroke: Stroke::default(),
                    fg_stroke: Stroke::new(1.0, hex_color!("#a2441b")), // button text
                    rounding: Rounding::ZERO,
                    expansion: 0.0,
                },
                hovered: WidgetVisuals {
                    weak_bg_fill: hex_color!("#ffd9b3"),
                    bg_fill: hex_color!("#ffd9b3"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#ff6a00")), // e.g. hover over window edge or button
                    fg_stroke: Stroke::new(1.5, hex_color!("#ff6a00")),
                    rounding: Rounding::ZERO,
                    expansion: 1.0,
                },
                active: WidgetVisuals {
                    weak_bg_fill: hex_color!("#d9d7ce"),
                    bg_fill: hex_color!("#d9d7ce"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#3e4b59")),
                    fg_stroke: Stroke::new(2.0, hex_color!("#3e4b59")),
                    rounding: Rounding::ZERO,
                    expansion: 1.0,
                },
                open: WidgetVisuals {
                    weak_bg_fill: hex_color!("#f3f3f3"),
                    bg_fill: hex_color!("#ffffff"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#d9d7ce")),
                    fg_stroke: Stroke::new(1.0, hex_color!("#ff6a00")),
                    rounding: Rounding::ZERO,
                    expansion: 0.0,
                },
            },
            selection: Selection {
                bg_fill: hex_color!("#efc9a3"),
                stroke: Stroke::new(1.0, hex_color!("#b2340b")),
            },
            hyperlink_color: hex_color!("#36a3d9"),
            faint_bg_color: Color32::from_additive_luminance(5), // visible, but barely so
            extreme_bg_color: hex_color!("#e6e1cf"),             // e.g. TextEdit background
            code_bg_color: hex_color!("#fafafa"),
            warn_fg_color: hex_color!("#e7c547"),
            error_fg_color: hex_color!("#ff3333"),
            window_fill: hex_color!("#f0eee4"),
            window_stroke: Stroke::new(1.0, hex_color!("#d9d8d7")),
            panel_fill: hex_color!("#f0eee4"),
            text_cursor: Stroke::new(2.0, hex_color!("#4cbf99")),
            ..Self::dark_theme()
        }
    }
}
