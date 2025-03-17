use crate::{
    feature,
    nes::{
        action::Setting,
        config::{AudioConfig, Config, EmulationConfig, RendererConfig},
        event::{ConfigEvent, NesEventProxy, UiEvent},
        renderer::{
            gui::{
                MessageType,
                lib::{RadioValue, ShortcutText, ShowShortcut, ViewportOptions},
            },
            shader::Shader,
        },
    },
};
use egui::{
    Align, CentralPanel, Checkbox, Context, CursorIcon, DragValue, Grid, Key, Layout, ScrollArea,
    Slider, TextEdit, Ui, Vec2, ViewportClass, ViewportId,
};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tetanes_core::{
    action::Action as DeckAction, apu::Channel, common::NesRegion,
    control_deck::Config as DeckConfig, fs, genie::GenieCode, input::FourPlayer, mem::RamState,
    time::Duration, video::VideoFilter,
};
use tracing::warn;

#[derive(Debug)]
#[must_use]
pub struct State {
    tx: NesEventProxy,
    tab: Tab,
    genie_entry: GenieEntry,
}

#[derive(Debug)]
#[must_use]
pub struct Preferences {
    id: ViewportId,
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    resources: Option<Config>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Emulation,
    Audio,
    Video,
    Input,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct GenieEntry {
    code: String,
    error: Option<String>,
}

impl Preferences {
    const TITLE: &'static str = "Preferences";

    pub fn new(tx: NesEventProxy) -> Self {
        Self {
            id: egui::ViewportId::from_hash_of(Self::TITLE),
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                tx,
                tab: Tab::default(),
                genie_entry: GenieEntry::default(),
            })),
            resources: None,
        }
    }

    pub const fn id(&self) -> ViewportId {
        self.id
    }

    pub fn open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    pub fn set_open(&self, open: bool) {
        self.open.store(open, Ordering::Release);
    }

    pub fn toggle_open(&self) {
        let _ = self
            .open
            .fetch_update(Ordering::Release, Ordering::Acquire, |open| Some(!open));
    }

    pub fn prepare(&mut self, cfg: &Config) {
        self.resources = Some(cfg.clone());
    }

    pub fn show(&mut self, ctx: &Context, opts: ViewportOptions) {
        if !self.open() {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);
        let Some(cfg) = self.resources.take() else {
            warn!("Preferences::prepare was not called with required resources");
            return;
        };

        let mut viewport_builder = egui::ViewportBuilder::default().with_title(Self::TITLE);
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        ctx.show_viewport_deferred(self.id, viewport_builder, move |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(Preferences::TITLE)
                    .open(&mut window_open)
                    .default_rect(ctx.available_rect().shrink(16.0))
                    .show(ctx, |ui| state.lock().ui(ui, opts.enabled, &cfg));
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ctx, |ui| state.lock().ui(ui, opts.enabled, &cfg));
                if ctx.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
        });
    }

    pub fn show_genie_codes_entry(&mut self, ui: &mut Ui, cfg: &Config) {
        self.state.lock().genie_codes_entry(ui, cfg);
    }

    pub fn genie_codes_list(tx: &NesEventProxy, ui: &mut Ui, cfg: &Config, scroll: bool) {
        if !cfg.deck.genie_codes.is_empty() {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.strong("Current Genie Codes:");
                    if ui.button("Clear All").clicked() {
                        tx.event(ConfigEvent::GenieCodeClear);
                    }
                });

                let render_codes = |ui: &mut Ui, cfg: &Config| {
                    ui.indent("current_genie_codes", |ui| {
                        let grid = Grid::new("genie_codes").num_columns(2).spacing([40.0, 6.0]);
                        grid.show(ui, |ui| {
                            for genie in &cfg.deck.genie_codes {
                                ui.label(genie.code());
                                // icon: waste basket
                                if ui.button("🗑").clicked() {
                                    tx.event(ConfigEvent::GenieCodeRemoved(
                                        genie.code().to_string(),
                                    ));
                                }
                                ui.end_row();
                            }
                        });
                    })
                };
                if scroll {
                    ScrollArea::vertical().show(ui, |ui| {
                        render_codes(ui, cfg);
                    });
                } else {
                    render_codes(ui, cfg);
                }
            });
        }
    }

    pub fn save_slot_radio(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut save_slot: u8,
        cfg: &Config,
        show_shortcut: ShowShortcut,
    ) {
        ui.vertical(|ui| {
            for slot in 1..=4 {
                let radio = RadioValue::new(&mut save_slot, slot, slot.to_string()).shortcut_text(
                    show_shortcut
                        .then(|| cfg.shortcut(DeckAction::SetSaveSlot(slot)))
                        .unwrap_or_default(),
                );
                if ui.add(radio).changed() {
                    tx.event(ConfigEvent::SaveSlot(save_slot));
                }
            }
        });
        ui.vertical(|ui| {
            for slot in 5..=8 {
                let radio = RadioValue::new(&mut save_slot, slot, slot.to_string()).shortcut_text(
                    show_shortcut
                        .then(|| cfg.shortcut(DeckAction::SetSaveSlot(slot)))
                        .unwrap_or_default(),
                );
                if ui.add(radio).changed() {
                    tx.event(ConfigEvent::SaveSlot(save_slot));
                }
            }
        });
    }

    pub fn speed_slider(tx: &NesEventProxy, ui: &mut Ui, mut speed: f32) {
        let slider = Slider::new(&mut speed, 0.25..=2.0)
            .step_by(0.25)
            .suffix("x");
        let res = ui
            .add(slider)
            .on_hover_text("Adjust the speed of the NES emulation.");
        if res.changed() {
            tx.event(ConfigEvent::Speed(speed));
        }
    }

    pub fn run_ahead_slider(tx: &NesEventProxy, ui: &mut Ui, mut run_ahead: usize) {
        let slider = Slider::new(&mut run_ahead, 0..=4);
        let res = ui
            .add(slider)
            .on_hover_text("Simulate a number of frames in the future to reduce input lag.");
        if res.changed() {
            tx.event(ConfigEvent::RunAhead(run_ahead));
        }
    }

    pub fn cycle_accurate_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut cycle_accurate: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        let icon = shortcut.is_some().then_some("📐 ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut cycle_accurate, format!("{icon}Cycle Accurate"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui
            .add(checkbox)
            .on_hover_text("Enables more accurate NES emulation at a slight cost in performance.");
        if res.clicked() {
            tx.event(ConfigEvent::CycleAccurate(cycle_accurate));
        }
    }

    pub fn rewind_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut rewind: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        let icon = shortcut.is_some().then_some("🔄 ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut rewind, format!("{icon}Enable Rewinding"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui
            .add(checkbox)
            .on_hover_text("Enable instant and visual rewinding. Increases memory usage.");
        if res.clicked() {
            tx.event(ConfigEvent::RewindEnabled(rewind));
        }
    }

    pub fn zapper_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut zapper: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        let icon = shortcut.is_some().then_some("🔫 ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut zapper, format!("{icon}Enable Zapper Gun"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui
            .add(checkbox)
            .on_hover_text("Enable the Zapper Light Gun for games that support it.");
        if res.clicked() {
            tx.event(ConfigEvent::ZapperConnected(zapper));
        }
    }

    pub fn overscan_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut hide_overscan: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        let icon = shortcut.is_some().then_some("📺 ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut hide_overscan, format!("{icon}Hide Overscan"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui.add(checkbox)
            .on_hover_text("Traditional CRT displays would crop the top and bottom edges of the image. Disable this to show the overscan.");
        if res.clicked() {
            tx.event(ConfigEvent::HideOverscan(hide_overscan));
        }
    }

    pub fn video_filter_radio(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut filter: VideoFilter,
        cfg: &Config,
        show_shortcut: ShowShortcut,
    ) {
        let previous_filter = filter;

        let shortcut =
            show_shortcut.then(|| cfg.shortcut(DeckAction::SetVideoFilter(VideoFilter::Pixellate)));
        let icon = shortcut.is_some().then_some("🌁 ").unwrap_or_default();
        let radio = RadioValue::new(
            &mut filter,
            VideoFilter::Pixellate,
            format!("{icon}Pixellate"),
        )
        .shortcut_text(shortcut.unwrap_or_default());
        ui.add(radio).on_hover_text("Basic pixel-perfect rendering");

        let shortcut =
            show_shortcut.then(|| cfg.shortcut(DeckAction::SetVideoFilter(VideoFilter::Ntsc)));
        let icon = shortcut.is_some().then_some("📼 ").unwrap_or_default();
        let radio = RadioValue::new(&mut filter, VideoFilter::Ntsc, format!("{icon}Ntsc"))
            .shortcut_text(shortcut.unwrap_or_default());
        ui.add(radio).on_hover_text(
            "Emulate traditional NTSC rendering where chroma spills over into luma.",
        );

        if filter != previous_filter {
            tx.event(ConfigEvent::VideoFilter(filter));
        }
    }

    pub fn shader_radio(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut shader: Shader,
        cfg: &Config,
        show_shortcut: ShowShortcut,
    ) {
        let previous_shader = shader;

        let shortcut = show_shortcut.then(|| cfg.shortcut(Setting::SetShader(Shader::Default)));
        let icon = shortcut.is_some().then_some("🗋 ").unwrap_or_default();
        let radio = RadioValue::new(&mut shader, Shader::Default, format!("{icon}Default"))
            .shortcut_text(shortcut.unwrap_or_default());
        ui.add(radio).on_hover_text("Default shader.");

        let shortcut = show_shortcut.then(|| cfg.shortcut(Setting::SetShader(Shader::CrtEasymode)));
        let icon = shortcut.is_some().then_some("📺 ").unwrap_or_default();
        let radio = RadioValue::new(
            &mut shader,
            Shader::CrtEasymode,
            format!("{icon}CRT Easymode"),
        )
        .shortcut_text(shortcut.unwrap_or_default());
        ui.add(radio)
            .on_hover_text("Emulate traditional CRT aperture grill masking.");

        if shader != previous_shader {
            tx.event(ConfigEvent::Shader(shader));
        }
    }

    pub fn four_player_radio(tx: &NesEventProxy, ui: &mut Ui, mut four_player: FourPlayer) {
        let previous_four_player = four_player;
        ui.radio_value(&mut four_player, FourPlayer::Disabled, "Disabled");
        ui.radio_value(&mut four_player, FourPlayer::FourScore, "Four Score")
            .on_hover_text("Enable NES Four Score for games that support 4 players.");
        ui.radio_value(&mut four_player, FourPlayer::Satellite, "Satellite")
            .on_hover_text("Enable NES Satellite for games that support 4 players.");
        if four_player != previous_four_player {
            tx.event(ConfigEvent::FourPlayer(four_player));
        }
    }

    pub fn nes_region_radio(tx: &NesEventProxy, ui: &mut Ui, mut region: NesRegion) {
        let previous_region = region;
        ui.radio_value(&mut region, NesRegion::Auto, "Auto")
            .on_hover_text("Auto-detect region based on loaded ROM.");
        ui.radio_value(&mut region, NesRegion::Ntsc, "NTSC")
            .on_hover_text("Emulate NTSC timing and aspect-ratio.");
        ui.radio_value(&mut region, NesRegion::Pal, "PAL")
            .on_hover_text("Emulate PAL timing and aspect-ratio.");
        ui.radio_value(&mut region, NesRegion::Dendy, "Dendy")
            .on_hover_text("Emulate Dendy timing and aspect-ratio.");
        if region != previous_region {
            tx.event(ConfigEvent::Region(region));
        }
    }

    pub fn ram_state_radio(tx: &NesEventProxy, ui: &mut Ui, mut ram_state: RamState) {
        let previous_ram_state = ram_state;
        ui.radio_value(&mut ram_state, RamState::AllZeros, "All 0x00")
            .on_hover_text("Clear startup RAM to all zeroes for predictable emulation.");
        ui.radio_value(&mut ram_state, RamState::AllOnes, "All 0xFF")
            .on_hover_text("Clear startup RAM to all ones for predictable emulation.");
        ui.radio_value(&mut ram_state, RamState::Random, "Random")
            .on_hover_text("Randomize startup RAM, which some games use as a basic RNG seed.");
        if ram_state != previous_ram_state {
            tx.event(ConfigEvent::RamState(ram_state));
        }
    }

    pub fn menubar_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut show_menubar: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        let icon = shortcut.is_some().then_some("☰ ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut show_menubar, format!("{icon}Show Menu Bar"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui.add(checkbox).on_hover_text("Show the menu bar.");
        if res.clicked() {
            tx.event(ConfigEvent::ShowMenubar(show_menubar));
        }
    }

    pub fn messages_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut show_messages: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        // icon: document with text
        let icon = shortcut.is_some().then_some("🖹 ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut show_messages, format!("{icon}Show Messages"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui
            .add(checkbox)
            .on_hover_text("Show shortcut and emulator messages.");
        if res.clicked() {
            tx.event(ConfigEvent::ShowMessages(show_messages));
        }
    }

    pub fn screen_reader_checkbox(ui: &mut Ui, shortcut: impl Into<Option<String>>) {
        let shortcut = shortcut.into();
        // icon: document with text
        let icon = shortcut.is_some().then_some("🔈 ").unwrap_or_default();
        let mut screen_reader = ui.ctx().options(|o| o.screen_reader);
        let checkbox = Checkbox::new(&mut screen_reader, format!("{icon}Enable Screen Reader"))
            .shortcut_text(shortcut.unwrap_or_default());
        let res = ui
            .add(checkbox)
            .on_hover_text("Enable screen reader to read buttons and labels out loud.");
        if res.clicked() {
            ui.ctx().options_mut(|o| o.screen_reader = screen_reader);
        }
    }

    pub fn window_scale_radio(tx: &NesEventProxy, ui: &mut Ui, mut scale: f32) {
        let previous_scale = scale;
        ui.vertical(|ui| {
            ui.radio_value(&mut scale, 1.0, "1x");
            ui.radio_value(&mut scale, 2.0, "2x");
            ui.radio_value(&mut scale, 3.0, "3x");
        });
        ui.vertical(|ui| {
            ui.radio_value(&mut scale, 4.0, "4x");
            ui.radio_value(&mut scale, 5.0, "5x");
        });
        if scale != previous_scale {
            tx.event(ConfigEvent::Scale(scale));
        }
    }

    pub fn fullscreen_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut fullscreen: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        let shortcut = shortcut.into();
        // icon: screen
        let icon = shortcut.is_some().then_some("🖵 ").unwrap_or_default();
        let checkbox = Checkbox::new(&mut fullscreen, format!("{icon}Fullscreen"))
            .shortcut_text(shortcut.unwrap_or_default());
        if ui.add(checkbox).clicked() {
            tx.event(ConfigEvent::Fullscreen(fullscreen));
        }
    }

    pub fn embed_viewports_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        cfg: &Config,
        shortcut: impl Into<Option<String>>,
    ) {
        if feature!(OsViewports) {
            ui.add_enabled_ui(!cfg.renderer.fullscreen, |ui| {
                let shortcut = shortcut.into();
                // icon: maximize
                let icon = shortcut.is_some().then_some("🗖 ").unwrap_or_default();
                let mut embed_viewports = ui.ctx().embed_viewports();
                let checkbox =
                    Checkbox::new(&mut embed_viewports, format!("{icon}Embed Viewports"))
                        .shortcut_text(shortcut.unwrap_or_default());
                let res = ui.add(checkbox).on_disabled_hover_text(
                    "Non-embedded viewports are not supported while in fullscreen.",
                );
                if res.clicked() {
                    ui.ctx().set_embed_viewports(embed_viewports);
                    tx.event(ConfigEvent::EmbedViewports(embed_viewports));
                }
            });
        }
    }

    pub fn always_on_top_checkbox(
        tx: &NesEventProxy,
        ui: &mut Ui,
        mut always_on_top: bool,
        shortcut: impl Into<Option<String>>,
    ) {
        if feature!(OsViewports) {
            let shortcut = shortcut.into();
            let icon = shortcut.is_some().then_some("🔝 ").unwrap_or_default();
            let checkbox = Checkbox::new(&mut always_on_top, format!("{icon}Always on Top"))
                .shortcut_text(shortcut.unwrap_or_default());
            // FIXME: Currently when not using embeded viewports, toggling always on top from
            // the preferences window will focus the primary window, potentially obscuring the
            // preferences window
            if ui.add(checkbox).clicked() {
                tx.event(ConfigEvent::AlwaysOnTop(always_on_top));
            }
        }
    }
}

impl State {
    fn ui(&mut self, ui: &mut Ui, enabled: bool, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            ui.set_min_height(ui.available_height());

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Emulation, "Emulation");
                ui.selectable_value(&mut self.tab, Tab::Audio, "Audio");
                ui.selectable_value(&mut self.tab, Tab::Video, "Video");
                ui.selectable_value(&mut self.tab, Tab::Input, "Input");
            });

            ui.separator();

            ScrollArea::both().show(ui, |ui| {
                match self.tab {
                    Tab::Emulation => self.emulation_tab(ui, cfg),
                    Tab::Audio => Self::audio_tab(&self.tx, ui, cfg),
                    Tab::Video => Self::video_tab(&self.tx, ui, cfg),
                    Tab::Input => Self::input_tab(&self.tx, ui, cfg),
                }

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Restore Defaults").clicked() {
                        Self::restore_defaults(&self.tx, ui.ctx());
                    }

                    if feature!(Storage) && ui.button("Clear Save States").clicked() {
                        Self::clear_save_states(&self.tx);
                    }

                    if feature!(Filesystem) && ui.button("Clear Recent ROMs").clicked() {
                        self.tx.event(ConfigEvent::RecentRomsClear);
                    }

                    #[cfg(target_arch = "wasm32")]
                    if ui.button("Download Save States").clicked() {
                        if let Err(err) = crate::platform::download_save_states() {
                            self.tx
                                .event(UiEvent::Message((MessageType::Error, err.to_string())));
                        }
                    }
                });
            });
        });
    }

    fn emulation_tab(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let EmulationConfig {
            mut auto_save,
            auto_save_interval,
            mut auto_load,
            rewind,
            mut rewind_interval,
            mut rewind_seconds,
            run_ahead,
            save_slot,
            speed,
            ..
        } = cfg.emulation;
        let DeckConfig {
            cycle_accurate,
            mut emulate_ppu_warmup,
            four_player,
            ram_state,
            region,
            ..
        } = cfg.deck;

        let grid = Grid::new("emulation_checkboxes")
            .num_columns(2)
            .spacing([80.0, 6.0]);
        grid.show(ui, |ui| {
            let tx = &self.tx;

            Preferences::cycle_accurate_checkbox(tx, ui, cycle_accurate, None);
            let res = ui.checkbox(&mut auto_load, "Auto-Load")
                .on_hover_text("Automatically load game state from the current save slot on load.");
            if res.changed() {
                tx.event(ConfigEvent::AutoLoad(
                    auto_load,
                ));
            }
            ui.end_row();

            ui.vertical(|ui| {
                Preferences::rewind_checkbox(tx, ui, rewind, None);

                ui.add_enabled_ui(rewind, |ui| {
                    ui.indent("rewind_settings", |ui| {
                        ui.horizontal(|ui| {
                            let suffix = if rewind_seconds == 1 { " second" } else { " seconds" };
                            let drag = DragValue::new(&mut rewind_seconds)
                                .range(1..=360)
                                .suffix(suffix);
                            let res = ui.add(drag)
                                .on_hover_text("The maximum number of seconds to rewind.");
                            if res.changed() {
                                tx.event(ConfigEvent::RewindSeconds(rewind_seconds));
                            }
                        });

                        ui.horizontal(|ui| {
                    let suffix = if rewind_interval == 1 { " frame" } else { " frames" };
                            let drag = DragValue::new(&mut rewind_interval)
                                .range(1..=60)
                                .prefix("every ")
                                .suffix(suffix);
                            let res = ui.add(drag)
                                .on_hover_text("The frame interval to save rewind states.");
                            if res.changed() {
                                tx.event(ConfigEvent::RewindInterval(rewind_interval));
                            }
                        });
                    });
                });
            });

            ui.vertical(|ui| {
                let res = ui.checkbox(&mut auto_save, "Auto-Save")
                    .on_hover_text(concat!(
                        "Automatically save game state to the current save slot ",
                        "on exit or unloading and an optional interval. ",
                        "Setting to 0 will disable saving on an interval.",
                    ));
                if res.changed() {
                    tx.event(ConfigEvent::AutoSave(
                        auto_save,
                    ));
                }

                ui.add_enabled_ui(auto_save, |ui| {
                    ui.indent("auto_save_settings", |ui| {
                        ui.horizontal(|ui| {
                            let mut auto_save_interval = auto_save_interval.as_secs();
                            let suffix = if auto_save_interval == 1 { " second" } else { " seconds" };
                            let drag = DragValue::new(&mut auto_save_interval)
                                .range(0..=60)
                                .prefix("every ")
                                .suffix(suffix);
                            let res = ui.add(drag)
                                .on_hover_text(concat!(
                                    "Set the interval to auto-save game state. ",
                                    "A value of `0` will still save on exit or unload while Auto-Save is enabled."
                                ));
                            if res.changed() {
                                tx.event(ConfigEvent::AutoSaveInterval(Duration::from_secs(auto_save_interval)));
                            }
                        });
                    });
                });
            });
            ui.end_row();

            let res = ui.checkbox(&mut emulate_ppu_warmup, "Emulate PPU Warmup")
                .on_hover_text(concat!(
                    "Set whether to emulate PPU warmup where writes to certain registers are ignored. ",
                    "Can result in some games not working correctly"
                ));
            if res.clicked() {
                tx.event(ConfigEvent::EmulatePpuWarmup(emulate_ppu_warmup));
            }
            ui.end_row();
        });

        ui.separator();

        let grid = Grid::new("emulation_sliders")
            .num_columns(2)
            .spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            let tx = &self.tx;

            ui.horizontal(|ui| {
                Preferences::speed_slider(tx, ui, speed);
                ui.label("Emulation Speed")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text("Change the speed of the emulation.");
            });
            ui.end_row();

            ui.horizontal(|ui| {
                Preferences::run_ahead_slider(tx, ui, run_ahead);
                ui.label("Run Ahead")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text(
                        "Simulate a number of frames in the future to reduce input lag.",
                    );
            });
            ui.end_row();
        });

        ui.separator();

        let grid = Grid::new("emulation_radios")
            .num_columns(4)
            .spacing([20.0, 6.0]);
        grid.show(ui, |ui| {
            let tx = &self.tx;

            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.strong("Save Slot:")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text("Select which slot to use when saving or loading game state.");
            });
            Grid::new("save_slots")
                .num_columns(2)
                .spacing([20.0, 6.0])
                .show(ui, |ui| {
                    Preferences::save_slot_radio(tx, ui, save_slot, cfg, ShowShortcut::No)
                });

            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.strong("Four Player:")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text(
                    "Some game titles support up to 4 players (requires connected controllers).",
                );
            });
            ui.vertical(|ui| Preferences::four_player_radio(tx, ui, four_player));
            ui.end_row();

            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.strong("NES Region:")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text("Which regional NES hardware to emulate.");
            });
            ui.vertical(|ui| Preferences::nes_region_radio(tx, ui, region));

            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.strong("RAM State:")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text("What values are read from NES RAM on load.");
            });
            ui.vertical(|ui| Preferences::ram_state_radio(tx, ui, ram_state));
            ui.end_row();
        });

        let grid = Grid::new("genie_codes").num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            self.genie_codes_entry(ui, cfg);
            Preferences::genie_codes_list(&self.tx, ui, cfg, false);
        });
    }

    fn audio_tab(tx: &NesEventProxy, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let AudioConfig {
            latency,
            mut buffer_size,
            mut enabled,
        } = cfg.audio;
        let DeckConfig {
            channels_enabled, ..
        } = cfg.deck;

        let res = ui.checkbox(&mut enabled, "Enable Audio");
        if res.clicked() {
            tx.event(ConfigEvent::AudioEnabled(enabled));
        }

        ui.add_enabled_ui(cfg.audio.enabled, |ui| {
            ui.indent("apu_channels", |ui| {
                Grid::new("apu_channels")
                    .spacing([60.0, 6.0])
                    .num_columns(2)
                    .show(ui, |ui| {

                        let mut pulse1_enabled = channels_enabled[0];
                        if ui.checkbox(&mut pulse1_enabled, "Enable Pulse1").clicked() {
                            tx.event(ConfigEvent::ApuChannelEnabled((Channel::Pulse1, pulse1_enabled)));
                        }
                        let mut noise_enabled = channels_enabled[3];
                        if ui.checkbox(&mut noise_enabled, "Enable Noise").clicked() {
                            tx.event(ConfigEvent::ApuChannelEnabled((Channel::Noise, noise_enabled)));
                        }
                        ui.end_row();

                        let mut pulse1_enabled = channels_enabled[1];
                        if ui.checkbox(&mut pulse1_enabled, "Enable Pulse2").clicked() {
                            tx.event(ConfigEvent::ApuChannelEnabled((Channel::Pulse2, pulse1_enabled)));
                        }
                        let mut dmc_enabled = channels_enabled[4];
                        if ui.checkbox(&mut dmc_enabled, "Enable DMC").clicked() {
                            tx.event(ConfigEvent::ApuChannelEnabled((Channel::Dmc, dmc_enabled)));
                        }
                        ui.end_row();

                        let mut triangle_enabled = channels_enabled[2];
                        if ui.checkbox(&mut triangle_enabled, "Enable Triangle").clicked() {
                            tx.event(ConfigEvent::ApuChannelEnabled((Channel::Triangle, triangle_enabled)));
                        }
                        let mut mapper_enabled = channels_enabled[5];
                        if ui.checkbox(&mut mapper_enabled, "Enable Mapper").clicked() {
                            tx.event(ConfigEvent::ApuChannelEnabled((Channel::Mapper, mapper_enabled)));
                        }
                        ui.end_row();
                    });

                ui.separator();

                Grid::new("audio_settings")
                    .spacing([40.0, 6.0])
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let drag = DragValue::new(&mut buffer_size)
                                .speed(10)
                                .range(128..=8192)
                                .prefix("buffer ")
                                .suffix(" samples");
                            let res = ui.add(drag)
                                .on_hover_text(
                                    "The audio sample buffer size allocated to the sound driver. Increased audio buffer size can help reduce audio underruns.",
                                );
                            if res.changed() {
                                tx.event(ConfigEvent::AudioBuffer(buffer_size));
                            }
                        });
                        ui.end_row();

                        ui.horizontal(|ui| {
                            let mut latency = latency.as_millis() as u64;
                            let drag = DragValue::new(&mut latency)
                                .range(1..=1000)
                                .suffix(" ms latency");
                            let res = ui.add(drag)
                                .on_hover_text(
                                    "The amount of queued audio before sending to the sound driver. Increased audio latency can help reduce audio underruns.",
                                );
                            if res.changed() {
                                tx.event(ConfigEvent::AudioLatency(Duration::from_millis(latency)));
                            }
                    });
                        ui.end_row();
                    });
            });
        });
    }

    fn video_tab(tx: &NesEventProxy, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let RendererConfig {
            always_on_top,
            fullscreen,
            hide_overscan,
            scale,
            shader,
            show_menubar,
            show_messages,
            ..
        } = cfg.renderer;
        let DeckConfig { filter, .. } = cfg.deck;

        Grid::new("video_checkboxes")
            .spacing([80.0, 6.0])
            .num_columns(2)
            .show(ui, |ui| {
                Preferences::menubar_checkbox(tx, ui, show_menubar, None);
                Preferences::fullscreen_checkbox(tx, ui, fullscreen, None);
                ui.end_row();

                Preferences::messages_checkbox(tx, ui, show_messages, None);
                Preferences::embed_viewports_checkbox(tx, ui, cfg, None);
                ui.end_row();

                Preferences::overscan_checkbox(tx, ui, hide_overscan, None);
                Preferences::always_on_top_checkbox(tx, ui, always_on_top, None);
                ui.end_row();
            });

        ui.separator();

        Grid::new("video_preferences")
            .num_columns(2)
            .spacing([40.0, 6.0])
            .show(ui, |ui| {
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Window Scale:");
                });
                Grid::new("save_slots")
                    .num_columns(2)
                    .spacing([20.0, 6.0])
                    .show(ui, |ui| {
                        Preferences::window_scale_radio(tx, ui, scale);
                    });
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Video Filter:");
                });
                ui.vertical(|ui| {
                    Preferences::video_filter_radio(tx, ui, filter, cfg, ShowShortcut::No);
                });
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Shader:");
                });
                ui.vertical(|ui| Preferences::shader_radio(tx, ui, shader, cfg, ShowShortcut::No));
            });
    }

    fn input_tab(tx: &NesEventProxy, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let DeckConfig {
            mut concurrent_dpad,
            zapper,
            ..
        } = cfg.deck;

        Grid::new("input_checkboxes")
            .num_columns(2)
            .spacing([80.0, 6.0])
            .show(ui, |ui| {
                Preferences::zapper_checkbox(tx, ui, zapper, None);
                ui.end_row();

                let res = ui.checkbox(&mut concurrent_dpad, "Enable Concurrent D-Pad");
                if res.clicked() {
                    tx.event(ConfigEvent::ConcurrentDpad(concurrent_dpad));
                }
            });
    }

    pub fn genie_codes_entry(&mut self, ui: &mut Ui, cfg: &Config) {
        let tx = &self.tx;
        ui.vertical(|ui| {
            // desired_width below doesn't have the desired effect
            ui.allocate_space(Vec2::new(200.0, 0.0));

            let genie_label = ui.strong("Add Genie Code(s):")
                .on_hover_cursor(CursorIcon::Help)
                .on_hover_text(
                    "A Game Genie Code is a 6 or 8 letter string that temporarily modifies game memory during operation. e.g. `AATOZE` will start Super Mario Bros. with 9 lives.\n\nYou can enter one code per line."
                );

            let text_edit = TextEdit::multiline(&mut self.genie_entry.code)
                .hint_text("e.g. AATOZE")
                .desired_width(200.0);
            let entry_res = ui.add(text_edit)
                .labelled_by(genie_label.id);
            if entry_res.changed() {
                self.genie_entry.error = None;
            }

            let has_entry = !self.genie_entry.code.is_empty();
            let add_clicked = ui.horizontal(|ui| {
                ui.add_enabled_ui(has_entry, |ui| {
                    let add_clicked = ui.button("Add").clicked();
                    if ui.button("Clear").clicked() {
                        self.genie_entry.code.clear();
                        self.genie_entry.error = None;
                    }
                    add_clicked
                }).inner
            }).inner;

            if (has_entry && entry_res.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                || add_clicked
            {
                for code in self.genie_entry.code.lines() {
                    let code = code.trim();
                    if code.is_empty() {
                        continue;
                    }
                    match GenieCode::parse(code) {
                        Ok(hex) => {
                            let code = GenieCode::from_raw(code.to_string(), hex);
                            if !cfg.deck.genie_codes.contains(&code) {
                                tx.event(ConfigEvent::GenieCodeAdded(code));
                            }
                        }
                        Err(err) => self.genie_entry.error = Some(err.to_string()),
                    }
                }
                if self.genie_entry.error.is_none() {
                    self.genie_entry.code.clear();
                }
            }

            if let Some(error) = &self.genie_entry.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        });
    }

    fn restore_defaults(tx: &NesEventProxy, ctx: &Context) {
        ctx.memory_mut(|mem| *mem = Default::default());

        // Inform all cfg updates
        let Config {
            deck,
            emulation,
            audio,
            renderer,
            input,
        } = Config::default();

        let events = [
            ConfigEvent::ActionBindings(input.action_bindings),
            ConfigEvent::AlwaysOnTop(renderer.always_on_top),
            ConfigEvent::ApuChannelsEnabled(deck.channels_enabled),
            ConfigEvent::AudioBuffer(audio.buffer_size),
            ConfigEvent::AudioEnabled(audio.enabled),
            ConfigEvent::AudioLatency(audio.latency),
            ConfigEvent::AutoLoad(emulation.auto_load),
            ConfigEvent::AutoSave(emulation.auto_save),
            ConfigEvent::AutoSaveInterval(emulation.auto_save_interval),
            ConfigEvent::ConcurrentDpad(deck.concurrent_dpad),
            ConfigEvent::CycleAccurate(deck.cycle_accurate),
            ConfigEvent::DarkTheme(renderer.dark_theme),
            ConfigEvent::EmbedViewports(renderer.embed_viewports),
            ConfigEvent::FourPlayer(deck.four_player),
            ConfigEvent::Fullscreen(renderer.fullscreen),
            ConfigEvent::GamepadAssignments(input.gamepad_assignments),
            ConfigEvent::GenieCodeClear,
            ConfigEvent::HideOverscan(renderer.hide_overscan),
            ConfigEvent::MapperRevisions(deck.mapper_revisions),
            ConfigEvent::RamState(deck.ram_state),
            // Clearing recent roms is handled in a separate button
            ConfigEvent::Region(deck.region),
            ConfigEvent::RewindEnabled(emulation.rewind),
            ConfigEvent::RewindInterval(emulation.rewind_interval),
            ConfigEvent::RewindSeconds(emulation.rewind_seconds),
            ConfigEvent::RunAhead(emulation.run_ahead),
            ConfigEvent::SaveSlot(emulation.save_slot),
            ConfigEvent::Shader(renderer.shader),
            ConfigEvent::ShowMenubar(renderer.show_menubar),
            ConfigEvent::ShowMessages(renderer.show_messages),
            ConfigEvent::Speed(emulation.speed),
            ConfigEvent::VideoFilter(deck.filter),
            ConfigEvent::ZapperConnected(deck.zapper),
        ];

        for event in events {
            tx.event(event);
        }
    }

    fn clear_save_states(tx: &NesEventProxy) {
        let data_dir = Config::default_data_dir();
        match fs::clear_dir(data_dir) {
            Ok(_) => tx.event(UiEvent::Message((
                MessageType::Info,
                "Save States cleared.".to_string(),
            ))),
            Err(_) => tx.event(UiEvent::Message((
                MessageType::Error,
                "Failed to clear Save States.".to_string(),
            ))),
        }
    }
}
