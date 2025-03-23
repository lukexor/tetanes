use crate::{
    feature,
    nes::{
        RunState,
        action::{Debug, DebugKind, DebugStep, Feature, Setting, Ui as UiAction},
        config::{Config, RecentRom, RendererConfig},
        emulation::FrameStats,
        event::{
            ConfigEvent, DebugEvent, EmulationEvent, NesEvent, NesEventProxy, RendererEvent,
            Response, UiEvent,
        },
        input::Gamepads,
        renderer::{
            gui::{
                keybinds::Keybinds,
                lib::{
                    ShortcutText, ShowShortcut, ToggleValue, ViewportOptions, cursor_to_zapper,
                    input_down,
                },
                ppu_viewer::PpuViewer,
                preferences::Preferences,
            },
            painter::RenderState,
            texture::Texture,
        },
        rom::{HOMEBREW_ROMS, RomAsset},
        version::Version,
    },
    sys::{SystemInfo, info::System},
};
use egui::{
    Align, Button, CentralPanel, Color32, Context, CornerRadius, CursorIcon, Direction, FontData,
    FontDefinitions, FontFamily, Frame, Grid, Image, Layout, Pos2, Rect, RichText, ScrollArea,
    Sense, Stroke, TopBottomPanel, Ui, UiBuilder, ViewportClass, Visuals, hex_color, include_image,
    menu,
    style::{HandleShape, Selection, TextCursorStyle, WidgetVisuals},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tetanes_core::{
    action::Action as DeckAction,
    common::{NesRegion, ResetKind},
    control_deck::LoadedRom,
    cpu::instr::Instr,
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
    ctx: Context,
    initialized: bool,
    title: String,
    tx: NesEventProxy,
    pub nes_texture: Texture,
    corrupted_cpu_instr: Option<Instr>,
    pub run_state: RunState,
    pub menu_height: f32,
    nes_frame: Rect,
    about_open: bool,
    gui_settings_open: Arc<AtomicBool>,
    #[cfg(debug_assertions)]
    gui_inspection_open: Arc<AtomicBool>,
    #[cfg(debug_assertions)]
    gui_memory_open: Arc<AtomicBool>,
    perf_stats_open: bool,
    update_window_open: bool,
    version: Version,
    pub keybinds: Keybinds,
    preferences: Preferences,
    debugger_open: bool,
    ppu_viewer: PpuViewer,
    apu_mixer_open: bool,
    viewport_info_open: bool,
    replay_recording: bool,
    audio_recording: bool,
    frame_stats: FrameStats,
    messages: Vec<(MessageType, String, Instant)>,
    pub loaded_rom: Option<LoadedRom>,
    about_homebrew_rom_open: Option<RomAsset>,
    start: Instant,
    sys: System,
    pub error: Option<String>,
    enable_auto_update: bool,
    dont_show_updates: bool,
}

impl Gui {
    const MSG_TIMEOUT: Duration = Duration::from_secs(3);
    const MAX_MESSAGES: usize = 5;
    const NO_ROM_LOADED: &'static str = "No ROM is loaded.";

    /// Create a `Gui` instance.
    pub fn new(
        ctx: Context,
        tx: NesEventProxy,
        render_state: &mut RenderState,
        cfg: &Config,
    ) -> Self {
        let nes_texture = Texture::new(
            render_state,
            cfg.texture_size(),
            cfg.deck.region.aspect_ratio(),
            Some("nes frame"),
        );

        Self {
            ctx,
            initialized: false,
            title: Config::WINDOW_TITLE.to_string(),
            tx: tx.clone(),
            nes_texture,
            corrupted_cpu_instr: None,
            run_state: RunState::Running,
            menu_height: 0.0,
            nes_frame: Rect::ZERO,
            about_open: false,
            gui_settings_open: Arc::new(AtomicBool::new(false)),
            #[cfg(debug_assertions)]
            gui_inspection_open: Arc::new(AtomicBool::new(false)),
            #[cfg(debug_assertions)]
            gui_memory_open: Arc::new(AtomicBool::new(false)),
            perf_stats_open: false,
            update_window_open: false,
            version: Version::new(),
            keybinds: Keybinds::new(tx.clone()),
            preferences: Preferences::new(tx.clone()),
            debugger_open: false,
            ppu_viewer: PpuViewer::new(tx, render_state),
            apu_mixer_open: false,
            viewport_info_open: false,
            replay_recording: false,
            audio_recording: false,
            frame_stats: FrameStats::new(),
            messages: Vec::new(),
            loaded_rom: None,
            about_homebrew_rom_open: None,
            start: Instant::now(),
            sys: System::default(),
            error: None,
            enable_auto_update: false,
            dont_show_updates: false,
        }
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) -> Response {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.keybinds.wants_input()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. } | WindowEvent::MouseInput { .. }
            )
        {
            Response {
                consumed: true,
                ..Default::default()
            }
        } else {
            Response::default()
        }
    }

    pub fn on_event(&mut self, queue: &wgpu::Queue, event: &mut NesEvent) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        match event {
            NesEvent::Ui(UiEvent::UpdateAvailable(version)) => {
                self.version.set_latest(version.clone());
                self.update_window_open = true;
                self.ctx.request_repaint();
            }
            NesEvent::Emulation(event) => match event {
                EmulationEvent::ReplayRecord(recording) => {
                    self.replay_recording = *recording;
                }
                EmulationEvent::AudioRecord(recording) => {
                    self.audio_recording = *recording;
                }
                EmulationEvent::CpuCorrupted { instr } => {
                    self.corrupted_cpu_instr = Some(*instr);
                    self.ctx.request_repaint();
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
                    if !*show {
                        self.menu_height = 0.0;
                    }
                }
                RendererEvent::ReplayLoaded => {
                    self.run_state = RunState::Running;
                    self.tx.event(EmulationEvent::RunState(self.run_state));
                }
                RendererEvent::RomUnloaded => {
                    self.run_state = RunState::Running;
                    self.tx.event(EmulationEvent::RunState(self.run_state));
                    self.loaded_rom = None;
                    self.title = Config::WINDOW_TITLE.to_string();
                }
                RendererEvent::RomLoaded(rom) => {
                    self.run_state = RunState::Running;
                    self.tx.event(EmulationEvent::RunState(self.run_state));
                    self.title = format!("{} :: {}", Config::WINDOW_TITLE, rom.name);
                    self.loaded_rom = Some(rom.clone());
                }
                RendererEvent::Menu(menu) => match menu {
                    Menu::About => self.about_open = !self.about_open,
                    Menu::Keybinds => self.keybinds.toggle_open(&self.ctx),
                    Menu::PerfStats => {
                        self.perf_stats_open = !self.perf_stats_open;
                        self.tx
                            .event(EmulationEvent::ShowFrameStats(self.perf_stats_open));
                    }
                    Menu::PpuViewer => self.ppu_viewer.toggle_open(&self.ctx),
                    Menu::Preferences => self.preferences.toggle_open(&self.ctx),
                },
                _ => (),
            },
            NesEvent::Debug(DebugEvent::Ppu(ppu)) => {
                self.ppu_viewer.update_ppu(queue, std::mem::take(ppu));
                self.ctx.request_repaint_of(self.ppu_viewer.id());
            }
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

    pub fn aspect_ratio(&self, cfg: &Config) -> f32 {
        let region = cfg
            .deck
            .region
            .is_auto()
            .then(|| self.loaded_region())
            .flatten()
            .unwrap_or(cfg.deck.region);
        region.aspect_ratio()
    }

    /// Create the UI.
    pub fn ui(&mut self, ctx: &Context, cfg: &Config, gamepads: &Gamepads) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if !self.initialized {
            self.initialize(ctx, cfg);
        }

        if cfg.renderer.show_menubar {
            TopBottomPanel::top("menubar").show(ctx, |ui| self.menubar(ui, cfg));
        }

        let viewport_opts = ViewportOptions {
            enabled: !self.keybinds.wants_input(),
            always_on_top: cfg.renderer.always_on_top,
        };

        CentralPanel::default()
            .frame(Frame::canvas(&ctx.style()))
            .show(ctx, |ui| {
                self.nes_frame(ui, viewport_opts.enabled, cfg, gamepads);
            });

        self.preferences.show(ctx, viewport_opts, cfg.clone());
        self.keybinds
            .show(ctx, viewport_opts, cfg.clone(), gamepads);
        self.ppu_viewer.show(ctx, viewport_opts);

        self.show_about_window(ctx, viewport_opts.enabled);
        self.show_about_homebrew_window(ctx, viewport_opts.enabled);

        self.show_performance_window(ctx, viewport_opts.enabled, cfg);
        self.show_update_window(ctx, viewport_opts.enabled, cfg);

        Self::show_viewport(
            "🔧 UI Settings",
            ctx,
            viewport_opts,
            &self.gui_settings_open,
            |ctx, ui| {
                ScrollArea::both().show(ui, |ui| ctx.settings_ui(ui));
            },
        );

        #[cfg(debug_assertions)]
        {
            Self::show_viewport(
                "🔍 UI Inspection",
                ctx,
                viewport_opts,
                &self.gui_inspection_open,
                |ctx, ui| {
                    ScrollArea::both().show(ui, |ui| ctx.inspection_ui(ui));
                },
            );
            Self::show_viewport(
                "📝 UI Memory",
                ctx,
                viewport_opts,
                &self.gui_memory_open,
                |ctx, ui| {
                    ScrollArea::both().show(ui, |ui| ctx.memory_ui(ui));
                },
            );
        }

        #[cfg(feature = "profiling")]
        if viewport_opts.enabled {
            puffin::profile_scope!("puffin");
            puffin_egui::show_viewport_if_enabled(ctx);
        }
    }

    fn initialize(&mut self, ctx: &Context, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let theme = if cfg.renderer.dark_theme {
            Self::dark_theme()
        } else {
            Self::light_theme()
        };
        ctx.set_visuals(theme);
        ctx.style_mut(|ctx| {
            let scroll = &mut ctx.spacing.scroll;
            scroll.floating = false;
            scroll.foreground_color = false;
            scroll.bar_width = 8.0;
        });

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
            fonts.font_data.insert(name.to_string(), font_data.into());
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
            self.version.check_for_updates(&self.tx, notify_latest);
        }

        self.initialized = true;
    }

    fn show_about_window(&mut self, ctx: &Context, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut about_open = self.about_open;
        egui::Window::new("ℹ About TetaNES")
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
        egui::Window::new(format!("ℹ About {}", rom.name))
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

    pub(super) fn show_viewport_info_window(
        &mut self,
        ctx: &Context,
        id: egui::ViewportId,
        info: &egui::ViewportInfo,
    ) {
        egui::Window::new(format!("ℹ Viewport Info ({id:?})"))
            .open(&mut self.viewport_info_open)
            .show(ctx, |ui| info.ui(ui));
    }

    fn show_performance_window(&mut self, ctx: &Context, enabled: bool, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut perf_stats_open = self.perf_stats_open;
        egui::Window::new("🛠 Performance Stats")
            .open(&mut perf_stats_open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(enabled, |ui| self.performance_stats(ui, cfg));
            });
        self.perf_stats_open = perf_stats_open;
    }

    fn show_viewport(
        title: impl Into<String>,
        ctx: &Context,
        opts: ViewportOptions,
        open: &Arc<AtomicBool>,
        add_contents: impl Fn(&Context, &mut Ui) + Send + Sync + 'static,
    ) {
        if !open.load(Ordering::Acquire) {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let title = title.into();
        let viewport_id = egui::ViewportId::from_hash_of(&title);
        let mut viewport_builder = egui::ViewportBuilder::default().with_title(&title);
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        let open = Arc::clone(open);
        ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(&title)
                    .open(&mut window_open)
                    .vscroll(true)
                    .show(ctx, |ui| {
                        ui.add_enabled_ui(opts.enabled, |ui| add_contents(ctx, ui));
                    });
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(opts.enabled, |ui| add_contents(ctx, ui));
                });
                if ctx.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
        });
    }

    fn show_update_window(&mut self, ctx: &Context, enabled: bool, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut update_window_open = self.update_window_open && cfg.renderer.show_updates;
        let mut close_window = false;
        egui::Window::new("🌐 Update Available")
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

                    // TODO: Add auto-update for each platform
                    if self.enable_auto_update {
                        ui.label("Would you like to install it and restart?");
                        ui.add_space(15.0);

                        ui.checkbox(&mut self.dont_show_updates, "Don't show this again");
                        ui.add_space(15.0);

                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            let res = ui.button("Skip").on_hover_text(format!(
                                "Keep the current version of TetaNES (v{}).",
                                self.version.current()
                            ));
                            if res.clicked() {
                                close_window = true;
                            }

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
                        });
                    } else {
                        ui.label("Click the above link to download the update for your system.");
                        ui.add_space(15.0);

                        ui.checkbox(&mut self.dont_show_updates, "Don't show this again");
                        ui.add_space(15.0);

                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if ui.button("  OK  ").clicked() {
                                close_window = true;
                            }
                        });
                    }
                });
            });
        if close_window
            || update_window_open != self.update_window_open && cfg.renderer.show_updates
        {
            self.update_window_open = false;
            if self.dont_show_updates == cfg.renderer.show_updates {
                self.tx
                    .event(ConfigEvent::ShowUpdates(!self.dont_show_updates));
                self.dont_show_updates = false;
            }
        }
    }

    fn menubar(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(!self.keybinds.wants_input(), |ui| {
            let inner_res = menu::bar(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    Self::toggle_dark_mode_button(&self.tx, ui);

                    ui.separator();

                    ui.menu_button("📁 File", |ui| self.file_menu(ui, cfg));
                    ui.menu_button("🔨 Controls", |ui| self.controls_menu(ui, cfg));
                    ui.menu_button("🔧 Config", |ui| self.config_menu(ui, cfg));
                    // icon: screen
                    ui.menu_button("🖵 Window", |ui| self.window_menu(ui, cfg));
                    ui.menu_button("🕷 Debug", |ui| self.debug_menu(ui, cfg));
                    ui.menu_button("❓ Help", |ui| self.help_menu(ui));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::warn_if_debug_build(ui);
                    });
                });
            });
            let spacing = ui.style().spacing.item_spacing;
            let border = 1.0;
            let height = inner_res.response.rect.height() + spacing.y + border;
            if height != self.menu_height {
                self.menu_height = height;
                self.tx.event(RendererEvent::ResizeTexture);
            }
        });
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

    fn file_menu(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let button = Button::new("📂 Load ROM...").shortcut_text(cfg.shortcut(UiAction::LoadRom));
        if ui.add(button).clicked() {
            if self.loaded_rom.is_some() {
                self.run_state = RunState::AutoPaused;
                self.tx.event(EmulationEvent::RunState(self.run_state));
            }
            // NOTE: Due to some platforms file dialogs blocking the event loop,
            // loading requires a round-trip in order for the above pause to
            // get processed.
            self.tx.event(UiEvent::LoadRomDialog);
            ui.close_menu();
        }

        ui.menu_button("🍺 Homebrew ROM...", |ui| self.homebrew_rom_menu(ui));

        let tx = &self.tx;

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
                self.run_state = RunState::AutoPaused;
                tx.event(EmulationEvent::RunState(self.run_state));
                // NOTE: Due to some platforms file dialogs blocking the event loop,
                // loading requires a round-trip in order for the above pause to
                // get processed.
                tx.event(UiEvent::LoadReplayDialog);
                ui.close_menu();
            }
        });

        if feature!(Filesystem) {
            ui.menu_button("🗄 Recently Played...", |ui| {
                // Sizing pass here since the width of the submenu can change as recent ROMS are
                // added or cleared.
                ui.scope_builder(UiBuilder::new().sizing_pass(), |ui| {
                    if cfg.renderer.recent_roms.is_empty() {
                        ui.label("No recent ROMs");
                    } else {
                        for rom in &cfg.renderer.recent_roms {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            if ui.button(rom.name()).clicked() {
                                match rom {
                                    RecentRom::Homebrew { name } => {
                                        match HOMEBREW_ROMS.iter().find(|rom| rom.name == name) {
                                            Some(rom) => {
                                                tx.event(EmulationEvent::LoadRom((
                                                    rom.name.to_string(),
                                                    rom.data(),
                                                )));
                                            }
                                            None => {
                                                tx.event(UiEvent::Message((
                                                    MessageType::Error,
                                                    "Failed to load rom".into(),
                                                )));
                                            }
                                        }
                                    }
                                    RecentRom::Path(path) => {
                                        tx.event(EmulationEvent::LoadRomPath(path.to_path_buf()))
                                    }
                                }
                                ui.close_menu();
                            }
                        }
                    }
                });
            });

            ui.separator();
        }

        if feature!(Storage) {
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

        if feature!(OsViewports) {
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

    fn controls_menu(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let tx = &self.tx;

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
                    RunState::ManuallyPaused | RunState::AutoPaused => RunState::Running,
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

        if feature!(Filesystem) {
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

    fn config_menu(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let tx = &self.tx;

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
            Preferences::video_filter_radio(tx, ui, cfg.deck.filter, cfg, ShowShortcut::Yes);
        });
        ui.menu_button("🕶 Shader...", |ui| {
            Preferences::shader_radio(tx, ui, cfg.renderer.shader, cfg, ShowShortcut::Yes);
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
        let toggle = ToggleValue::new(&mut preferences_open, "🔧 Preferences")
            .shortcut_text(cfg.shortcut(Menu::Preferences));
        if ui.add(toggle).clicked() {
            self.preferences.set_open(preferences_open, &self.ctx);
            ui.close_menu();
        }

        let mut keybinds_open = self.keybinds.open();
        // icon: keyboard
        let toggle = ToggleValue::new(&mut keybinds_open, "🖮 Keybinds")
            .shortcut_text(cfg.shortcut(Menu::Keybinds));
        if ui.add(toggle).clicked() {
            self.keybinds.set_open(keybinds_open, &self.ctx);
            ui.close_menu();
        };
    }

    fn window_menu(&mut self, ui: &mut Ui, cfg: &Config) {
        use Setting::*;

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let tx = &self.tx;
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
        if feature!(ScreenReader) {
            Preferences::screen_reader_checkbox(ui, cfg.shortcut(ToggleScreenReader));
        }
    }

    fn debug_menu(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let tx = &self.tx;

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

        let mut gui_settings_open = self.gui_settings_open.load(Ordering::Acquire);
        let toggle = ToggleValue::new(&mut gui_settings_open, "🔧 UI Settings");
        let res = ui.add(toggle).on_hover_text("Toggle the UI style window");
        if res.clicked() {
            self.gui_settings_open
                .store(gui_settings_open, Ordering::Release);
            ui.close_menu();
        }

        #[cfg(debug_assertions)]
        {
            let mut gui_inspection_open = self.gui_inspection_open.load(Ordering::Acquire);
            let toggle = ToggleValue::new(&mut gui_inspection_open, "🔍 UI Inspection");
            let res = ui
                .add(toggle)
                .on_hover_text("Toggle the UI inspection window");
            if res.clicked() {
                self.gui_inspection_open
                    .store(gui_inspection_open, Ordering::Release);
                ui.close_menu();
            }

            let mut gui_memory_open = self.gui_memory_open.load(Ordering::Acquire);
            let toggle = ToggleValue::new(&mut gui_memory_open, "📝 UI Memory");
            let res = ui.add(toggle).on_hover_text("Toggle the UI memory window");
            if res.clicked() {
                self.gui_memory_open
                    .store(gui_memory_open, Ordering::Release);
                ui.close_menu();
            }

            let res = ui.toggle_value(&mut self.viewport_info_open, "ℹ Viewport Info");
            if res.clicked() {
                ui.close_menu();
            }

            #[cfg(target_arch = "wasm32")]
            if ui.button("❗Test panic!").clicked() {
                panic!("panic test");
            }
        }

        ui.separator();

        ui.add_enabled_ui(false, |ui| {
            let debugger_shortcut = cfg.shortcut(Debug::Toggle(DebugKind::Cpu));
            let toggle = ToggleValue::new(&mut self.debugger_open, "🚧 Debugger")
                .shortcut_text(debugger_shortcut);
            let res = ui
                .add(toggle)
                .on_hover_text("Toggle the Debugger.")
                .on_disabled_hover_text("Not yet implemented.");
            if res.clicked() {
                ui.close_menu();
            }
        });

        let ppu_viewer_shortcut = cfg.shortcut(Debug::Toggle(DebugKind::Ppu));
        let mut open = self.ppu_viewer.open();
        let toggle =
            ToggleValue::new(&mut open, "🌇 PPU Viewer").shortcut_text(ppu_viewer_shortcut);
        let res = ui.add(toggle).on_hover_text("Toggle the PPU Viewer.");
        if res.clicked() {
            self.ppu_viewer.set_open(open, &self.ctx);
            ui.close_menu();
        }

        ui.add_enabled_ui(false, |ui| {
            let apu_mixer_shortcut = cfg.shortcut(Debug::Toggle(DebugKind::Apu));
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
                Button::new("➡ Step").shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Into)));
            let res = ui
                .add(button)
                .on_hover_text("Step a single CPU instruction.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Into));
            }

            let button =
                Button::new("⬆ Step Out").shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Out)));
            let res = ui
                .add(button)
                .on_hover_text("Step out of the current CPU function.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Out));
            }

            let button = Button::new("⮫ Step Over")
                .shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Over)));
            let res = ui
                .add(button)
                .on_hover_text("Step over the next CPU instruction.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Over));
            }

            let button = Button::new("➖ Step Scanline")
                .shortcut_text(cfg.shortcut(Debug::Step(DebugStep::Scanline)));
            let res = ui
                .add(button)
                .on_hover_text("Step an entire PPU scanline.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED);
            if res.clicked() {
                tx.event(EmulationEvent::DebugStep(DebugStep::Scanline));
            }

            let button = Button::new("🖼 Step Frame")
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

    fn nes_frame(&mut self, ui: &mut Ui, enabled: bool, cfg: &Config, gamepads: &Gamepads) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            let tx = &self.tx;

            CentralPanel::default().show_inside(ui, |ui| {
                if self.loaded_rom.is_some() {
                    let layout = Layout {
                        main_dir: Direction::TopDown,
                        main_align: Align::Center,
                        cross_align: Align::Center,
                        ..Default::default()
                    };
                    ui.with_layout(layout, |ui| {
                        let image = Image::from_texture(self.nes_texture.sized())
                            .shrink_to_fit()
                            .sense(Sense::click());

                        let hover_cursor = if cfg.deck.zapper {
                            CursorIcon::Crosshair
                        } else {
                            CursorIcon::Default
                        };

                        let res = ui.add(image).on_hover_cursor(hover_cursor);
                        self.nes_frame = res.rect;

                        if cfg.deck.zapper {
                            if res.clicked() {
                                tx.event(EmulationEvent::ZapperTrigger);
                            }
                            if
                                cfg
                                .action_input(DeckAction::ZapperAimOffscreen)
                                .is_some_and(|input| input_down(ui, gamepads, cfg, input))
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
                        }
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.horizontal_centered(|ui| {
                            let image = Image::new(include_image!("../../../assets/tetanes.png"))
                                .shrink_to_fit()
                                .tint(Color32::GRAY);
                            ui.add(image);
                        });
                    });
                }
            });

            let mut recording_labels = Vec::new();
            if self.replay_recording {
                recording_labels.push("Replay");
            }
            if self.audio_recording {
                recording_labels.push("Audio");
            }
            if !recording_labels.is_empty() {
                Frame::side_top_panel(ui.style()).show(ui, |ui| {
                    ui.with_layout(
                        Layout::top_down_justified(Align::LEFT).with_main_wrap(true),
                        |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Recording {}...",
                                    recording_labels.join(" & ")
                                ))
                                .italics(),
                            )
                        },
                    );
                });
            }

            if cfg.renderer.show_messages {
                if let Some(instr) = self.corrupted_cpu_instr {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(
                            Layout::top_down_justified(Align::LEFT).with_main_wrap(true),
                            |ui| {
                                ui.colored_label(
                                    Color32::RED,
                                    format!(
                                        "Invalid CPU opcode: ${:02X} {:?} #{:?} encountered. Title: {}",
                                        instr.opcode(),
                                        instr.op(),
                                        instr.addr_mode(),
                                        self.loaded_rom.as_ref().map(|rom| rom.name.as_str()).unwrap_or_default()
                                    ),
                                );

                                ui.vertical(|ui| {
                                    ui.label("Recovery options:");
                                    ui.horizontal(|ui| {
                                        if ui.button("Reset").clicked() {
                                            self.tx.event(EmulationEvent::Reset(ResetKind::Soft));
                                            self.corrupted_cpu_instr = None;
                                        }
                                        if ui.button("Power Cycle").clicked() {
                                            self.tx.event(EmulationEvent::Reset(ResetKind::Hard));
                                            self.corrupted_cpu_instr = None;
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Clear Save States").clicked() {
                                            preferences::State::clear_save_states(&self.tx);
                                        }
                                        if ui.button("Load ROM").clicked() {
                                            self.tx.event(UiEvent::LoadRomDialog);
                                        }
                                    });
                                });
                            },
                        );
                    });
                }

                if self.error.is_some() {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(
                            Layout::top_down_justified(Align::LEFT).with_main_wrap(true),
                            |ui| {
                                self.error_bar(ui);
                            },
                        );
                    });
                }

                if !self.messages.is_empty() {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(
                            Layout::top_down_justified(Align::LEFT).with_main_wrap(true),
                            |ui| {
                                self.message_bar(ui);
                            },
                        );
                    });
                }

                if self.run_state.paused() {
                    Frame::new().inner_margin(5.0).show(ui, |ui| {
                        ui.heading(RichText::new("⏸").color(Color32::LIGHT_GRAY).size(40.0));
                    });
                }
            }
        });
    }

    fn performance_stats(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let grid = Grid::new("perf_stats").num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.ctx().request_repaint_after(Duration::from_secs(1));

            self.sys.update();

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

            if let Some(stats) = self.sys.stats() {
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

                ui.strong("CPU:");
                ui.colored_label(
                    cpu_color(stats.cpu_usage),
                    format!("{:.2}%", stats.cpu_usage),
                );
                ui.end_row();

                ui.strong("Memory:");
                ui.label(format!("{} MB", bytes_to_mb(stats.memory)));
                ui.end_row();

                let du = stats.disk_usage;
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
    }

    fn help_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.version.requires_updates() && ui.button("🌐 Check for Updates...").clicked() {
            let notify_latest = true;
            self.version.check_for_updates(&self.tx, notify_latest);
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

                    if feature!(Filesystem) {
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
            let available_width = ui.available_width();
            ui.set_min_width(available_width);
            ui.horizontal(|ui| {
                let res = ui.colored_label(Color32::RED, error);
                ui.add_space(available_width - res.rect.width() - 30.0);
                if ui.button("❌").clicked() {
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
                    corner_radius: CornerRadius::ZERO,
                    expansion: 0.0,
                },
                inactive: WidgetVisuals {
                    weak_bg_fill: hex_color!("#253340"), // button background
                    bg_fill: hex_color!("#253340"),      // checkbox background
                    bg_stroke: Stroke::default(),
                    fg_stroke: Stroke::new(1.0, hex_color!("#a9491f")), // button text
                    corner_radius: CornerRadius::ZERO,
                    expansion: 0.0,
                },
                hovered: WidgetVisuals {
                    weak_bg_fill: hex_color!("#212733"),
                    bg_fill: hex_color!("#212733"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#f29718")), // e.g. hover over window edge or button
                    fg_stroke: Stroke::new(1.5, hex_color!("#ffb454")),
                    corner_radius: CornerRadius::ZERO,
                    expansion: 1.0,
                },
                active: WidgetVisuals {
                    weak_bg_fill: hex_color!("#253340"),
                    bg_fill: hex_color!("#253340"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#fed7aa")),
                    fg_stroke: Stroke::new(2.0, hex_color!("#fed7aa")),
                    corner_radius: CornerRadius::ZERO,
                    expansion: 1.0,
                },
                open: WidgetVisuals {
                    weak_bg_fill: hex_color!("#151a1e"),
                    bg_fill: hex_color!("#14191f"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#253340")),
                    fg_stroke: Stroke::new(1.0, hex_color!("#ffb454")),
                    corner_radius: CornerRadius::ZERO,
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
            window_corner_radius: CornerRadius::ZERO,
            window_fill: hex_color!("#14191f"),
            window_stroke: Stroke::new(1.0, hex_color!("#253340")),
            window_highlight_topmost: true,
            menu_corner_radius: CornerRadius::ZERO,
            panel_fill: hex_color!("#14191f"),
            text_cursor: TextCursorStyle {
                stroke: Stroke::new(2.0, hex_color!("#95e6cb")),
                ..Default::default()
            },
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
                    corner_radius: CornerRadius::ZERO,
                    expansion: 0.0,
                },
                inactive: WidgetVisuals {
                    weak_bg_fill: hex_color!("#d9d8d7"), // button background
                    bg_fill: hex_color!("#d9d8d7"),      // checkbox background
                    bg_stroke: Stroke::default(),
                    fg_stroke: Stroke::new(1.0, hex_color!("#a2441b")), // button text
                    corner_radius: CornerRadius::ZERO,
                    expansion: 0.0,
                },
                hovered: WidgetVisuals {
                    weak_bg_fill: hex_color!("#ffd9b3"),
                    bg_fill: hex_color!("#ffd9b3"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#ff6a00")), // e.g. hover over window edge or button
                    fg_stroke: Stroke::new(1.5, hex_color!("#ff6a00")),
                    corner_radius: CornerRadius::ZERO,
                    expansion: 1.0,
                },
                active: WidgetVisuals {
                    weak_bg_fill: hex_color!("#d9d7ce"),
                    bg_fill: hex_color!("#d9d7ce"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#3e4b59")),
                    fg_stroke: Stroke::new(2.0, hex_color!("#3e4b59")),
                    corner_radius: CornerRadius::ZERO,
                    expansion: 1.0,
                },
                open: WidgetVisuals {
                    weak_bg_fill: hex_color!("#f3f3f3"),
                    bg_fill: hex_color!("#ffffff"),
                    bg_stroke: Stroke::new(1.0, hex_color!("#d9d7ce")),
                    fg_stroke: Stroke::new(1.0, hex_color!("#ff6a00")),
                    corner_radius: CornerRadius::ZERO,
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
            text_cursor: TextCursorStyle {
                stroke: Stroke::new(2.0, hex_color!("#4cbf99")),
                ..Default::default()
            },
            ..Self::dark_theme()
        }
    }
}
