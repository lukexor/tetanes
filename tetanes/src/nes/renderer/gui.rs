use crate::{
    nes::{
        action::{Action, Debug, DebugStep, Debugger, Feature, Setting, Ui as UiAction},
        config::Config,
        emulation::FrameStats,
        event::{ConfigEvent, EmulationEvent, NesEvent, SendNesEvent, UiEvent},
        input::{ActionBindings, GamepadUuid, Gamepads, Input},
    },
    platform,
};
use egui::{
    ahash::{HashMap, HashMapExt},
    global_dark_light_mode_switch, include_image,
    load::SizedTexture,
    menu, Align, Align2, Button, CentralPanel, Checkbox, Color32, Context, CursorIcon, Direction,
    DragValue, FontData, FontDefinitions, FontFamily, Frame, Grid, Image, Key, KeyboardShortcut,
    Layout, Modifiers, PointerButton, Pos2, Rect, Response, RichText, ScrollArea, Sense, Slider,
    TextStyle, TopBottomPanel, Ui, Vec2, ViewportClass, ViewportCommand, ViewportId, Widget,
    WidgetText,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    mem,
    ops::{Deref, DerefMut},
    sync::Arc,
};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
use tetanes_core::{
    action::Action as DeckAction,
    apu::Channel,
    common::{NesRegion, ResetKind},
    fs,
    genie::GenieCode,
    input::{FourPlayer, Player},
    mem::RamState,
    ppu::Ppu,
    time::{Duration, Instant},
    video::VideoFilter,
};
use tracing::info;
use winit::{
    event::{ElementState, MouseButton},
    event_loop::EventLoopProxy,
    keyboard::{KeyCode, ModifiersState},
    window::Window,
};

pub trait ShortcutText<'a>
where
    Self: Sized + 'a,
{
    fn shortcut_text(self, shortcut_text: impl Into<RichText>) -> ShortcutWidget<'a, Self> {
        ShortcutWidget {
            inner: self,
            shortcut_text: shortcut_text.into(),
            phantom: std::marker::PhantomData,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Menu {
    Preferences,
    Keybinds,
    About,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PreferencesTab {
    Emulation,
    Audio,
    Video,
    Input,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum KeybindsTab {
    Shortcuts,
    Joypad(Player),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingKeybind {
    action: Action,
    player: Option<Player>,
    binding: usize,
    input: Option<Input>,
    conflict: Option<Action>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PendingGenieEntry {
    code: String,
    error: Option<String>,
}

impl PendingGenieEntry {
    pub fn empty() -> Self {
        Self::default()
    }
}

type Keybind = (Action, [Option<Input>; 2]);

#[derive(Debug)]
#[must_use]
pub struct Gui {
    pub initialized: bool,
    pub window: Arc<Window>,
    pub title: String,
    pub tx: EventLoopProxy<NesEvent>,
    pub debounced_events: HashMap<&'static str, (NesEvent, Instant)>,
    pub texture: SizedTexture,
    pub paused: bool,
    pub menu_height: f32,
    pub nes_frame: Rect,
    pub pending_genie_entry: PendingGenieEntry,
    pub preferences_open: bool,
    pub keybinds_open: bool,
    pub about_open: bool,
    pub preferences_tab: PreferencesTab,
    pub keybinds_tab: KeybindsTab,
    pub pending_keybind: Option<PendingKeybind>,
    pub gamepad_conflict: Option<(Player, Player, GamepadUuid)>,
    pub cpu_debugger_open: bool,
    pub ppu_debugger_open: bool,
    pub apu_debugger_open: bool,
    pub debug_on_hover: bool,
    pub loaded_region: NesRegion,
    pub resize_window: bool,
    pub resize_texture: bool,
    pub replay_recording: bool,
    pub audio_recording: bool,
    pub shortcut_keybinds: BTreeMap<String, Keybind>,
    pub joypad_keybinds: [BTreeMap<String, Keybind>; 4],
    pub frame_stats: FrameStats,
    pub messages: Vec<(String, Instant)>,
    pub loaded_rom: Option<String>,
    pub start: Instant,
    pub sys: Option<System>,
    pub sys_updated: Instant,
    pub status: Option<&'static str>,
    pub error: Option<String>,
}

impl Gui {
    const MSG_TIMEOUT: Duration = Duration::from_secs(3);
    const MAX_MESSAGES: usize = 5;
    const MENU_WIDTH: f32 = 200.0;
    const NO_ROM_LOADED: &'static str = "No ROM is loaded.";

    /// Create a gui `State`.
    pub fn new(
        window: Arc<Window>,
        tx: EventLoopProxy<NesEvent>,
        texture: SizedTexture,
        cfg: Config,
    ) -> Self {
        let sys = if sysinfo::IS_SUPPORTED_SYSTEM {
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
        };
        Self {
            initialized: false,
            window,
            title: Config::WINDOW_TITLE.to_string(),
            tx,
            debounced_events: HashMap::new(),
            texture,
            paused: false,
            menu_height: 0.0,
            nes_frame: Rect::ZERO,
            pending_genie_entry: PendingGenieEntry::empty(),
            preferences_open: false,
            preferences_tab: PreferencesTab::Emulation,
            keybinds_tab: KeybindsTab::Shortcuts,
            keybinds_open: false,
            pending_keybind: None,
            gamepad_conflict: None,
            about_open: false,
            cpu_debugger_open: false,
            ppu_debugger_open: false,
            apu_debugger_open: false,
            debug_on_hover: false,
            loaded_region: cfg.deck.region,
            resize_window: false,
            resize_texture: false,
            replay_recording: false,
            audio_recording: false,
            shortcut_keybinds: Action::BINDABLE
                .into_iter()
                .filter(|action| !action.is_joypad())
                .map(ActionBindings::empty)
                .chain(cfg.input.shortcuts)
                .map(|b| (b.action.to_string(), (b.action, b.bindings)))
                .collect::<BTreeMap<_, _>>(),
            joypad_keybinds: [Player::One, Player::Two, Player::Three, Player::Four].map(
                |player| {
                    Action::BINDABLE
                        .into_iter()
                        .filter(|action| action.is_joypad())
                        .map(ActionBindings::empty)
                        .chain(cfg.input.joypad_bindings[player as usize].iter().copied())
                        .map(|b| (b.action.to_string(), (b.action, b.bindings)))
                        .collect::<BTreeMap<_, _>>()
                },
            ),
            frame_stats: FrameStats::new(),
            messages: Vec::new(),
            loaded_rom: None,
            start: Instant::now(),
            sys,
            sys_updated: Instant::now(),
            status: None,
            error: None,
        }
    }

    pub fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        info!("{text}");
        self.messages
            .push((text, Instant::now() + Self::MSG_TIMEOUT));
    }

    pub fn add_debounced_event(&mut self, id: &'static str, event: impl Into<NesEvent>) {
        self.debounced_events
            .entry(id)
            .and_modify(|(_, instant)| *instant = Instant::now())
            .or_insert((event.into(), Instant::now()));
    }

    pub fn aspect_ratio(&self, cfg: &Config) -> f32 {
        if cfg.deck.region.is_auto() {
            self.loaded_region.aspect_ratio()
        } else {
            cfg.deck.region.aspect_ratio()
        }
    }

    /// Create the UI.
    pub fn ui(&mut self, ctx: &Context, gamepads: &mut Gamepads, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if !self.initialized {
            self.initialize(ctx);
        }

        self.handle_debounced_events();

        TopBottomPanel::top("menu_bar")
            .show_animated(ctx, cfg.renderer.show_menubar, |ui| self.menu_bar(ui, cfg));
        CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| self.nes_frame(ui, gamepads, cfg));

        self.show_keybinds_viewport(ctx, gamepads, cfg);

        self.show_performance_window(ctx, cfg);
        self.show_preferences_viewport(ctx, cfg);
        self.show_about_viewport(ctx);

        #[cfg(feature = "profiling")]
        if self.pending_keybind.is_none() {
            puffin::profile_scope!("puffin");
            puffin_egui::show_viewport_if_enabled(ctx);
        }
    }

    fn initialize(&mut self, ctx: &Context) {
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

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .expect("proportional font family defined")
            .insert(0, FONT.0.to_string());
        fonts
            .families
            .get_mut(&egui::FontFamily::Monospace)
            .expect("monospace font family defined")
            .insert(0, MONO_FONT.0.to_string());
        ctx.set_fonts(fonts);

        self.initialized = true;
    }

    fn handle_debounced_events(&mut self) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        self.debounced_events.retain(|_, (event, instant)| {
            let should_send = instant.elapsed() < Duration::from_millis(300);
            if should_send {
                self.tx.nes_event(event.clone());
            }
            !should_send
        });
    }

    fn show_set_keybind_window(&mut self, ui: &mut Ui, gamepads: &mut Gamepads, cfg: &mut Config) {
        let mut open = self.pending_keybind.is_some();
        let res = egui::Window::new("Set Keybind")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| self.set_keybind(ui, gamepads, cfg));
        if let Some(ref res) = res {
            // Force on-top focus when embedded
            if open {
                ui.ctx().move_to_top(res.response.layer_id);
                res.response.request_focus();
            } else {
                ui.ctx().memory_mut(|m| m.surrender_focus(res.response.id));
            }
        }
        if !open {
            self.pending_keybind = None;
        }
    }

    fn set_keybind(&mut self, ui: &mut Ui, gamepads: &mut Gamepads, cfg: &mut Config) {
        let Some(PendingKeybind {
            action,
            player,
            mut input,
            binding,
            mut conflict,
            ..
        }) = self.pending_keybind
        else {
            return;
        };

        if let Some(action) = conflict {
            ui.label(format!("Conflict with {action}."));
            ui.horizontal(|ui| {
                if ui.button("Overwrite").clicked() {
                    conflict = None;
                }
                if ui.button("Cancel").clicked() {
                    self.pending_keybind = None;
                    input = None;
                }
            });
        } else {
            ui.label(format!(
                "Press any key on your keyboard or controller to set a new binding for {action}.",
            ));
        }

        match input {
            Some(input) => {
                if conflict.is_none() {
                    cfg.input.clear_binding(input);
                    match player {
                        Some(player) => {
                            Self::update_keybind(
                                &mut self.joypad_keybinds[player as usize],
                                &mut cfg.input.joypad_bindings[player as usize],
                                action,
                                input,
                                binding,
                            );
                        }
                        None => {
                            Self::update_keybind(
                                &mut self.shortcut_keybinds,
                                &mut cfg.input.shortcuts,
                                action,
                                input,
                                binding,
                            );
                        }
                    }
                    self.pending_keybind = None;
                    self.tx.nes_event(ConfigEvent::InputBindings);
                }
            }
            None => {
                if let Some(pending_keybind) = &mut self.pending_keybind {
                    let event = ui.input(|i| {
                        use egui::Event;

                        for event in &i.events {
                            match *event {
                                Event::Key {
                                    physical_key: Some(key),
                                    pressed,
                                    modifiers,
                                    ..
                                } => {
                                    // TODO: Ignore unsupported key mappings for now as egui supports less
                                    // overall than winit
                                    return Input::try_from((key, modifiers))
                                        .ok()
                                        .map(|input| (input, pressed));
                                }
                                Event::PointerButton {
                                    button, pressed, ..
                                } => {
                                    return Some((Input::from(button), pressed));
                                }
                                _ => (),
                            }
                        }
                        while let Some(event) = gamepads.next_event() {
                            if let Some(input) = gamepads
                                .input_from_event(&event, cfg)
                                .map(|(input, state)| (input, state == ElementState::Pressed))
                            {
                                return Some(input);
                            }
                        }
                        None
                    });

                    if let Some((input, pressed)) = event {
                        // Only set on key release
                        if !pressed {
                            pending_keybind.input = Some(input);
                            for bind in cfg
                                .input
                                .shortcuts
                                .iter()
                                .chain(cfg.input.joypad_bindings.iter().flatten())
                            {
                                if bind.bindings.iter().any(|b| {
                                    b == &Some(input) && bind.action != pending_keybind.action
                                }) {
                                    pending_keybind.conflict = Some(bind.action);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn show_gamepad_conflict_window(&mut self, ui: &mut Ui, gamepads: &Gamepads, cfg: &mut Config) {
        let mut open = self.gamepad_conflict.is_some();
        let res = egui::Window::new("Unassign Gamepad")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ui.ctx(), |ui| {
                if let Some((existing_player, new_player, uuid)) = self.gamepad_conflict {
                    ui.label(format!("Unassign gamepad from Player {existing_player}?"));
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            self.unassign_gamepad(existing_player, gamepads, cfg);
                            self.assign_gamepad(new_player, uuid, gamepads, cfg);
                            self.gamepad_conflict = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.gamepad_conflict = None;
                        }
                    });
                }
            });
        if let Some(ref res) = res {
            // Force on-top focus when embedded
            if open {
                ui.ctx().move_to_top(res.response.layer_id);
                res.response.request_focus();
            } else {
                ui.ctx().memory_mut(|m| m.surrender_focus(res.response.id));
            }
        }
        if !open {
            self.gamepad_conflict = None;
        }
    }

    fn assign_gamepad(
        &mut self,
        player: Player,
        uuid: GamepadUuid,
        gamepads: &Gamepads,
        cfg: &mut Config,
    ) {
        cfg.input.assign_gamepad(player, uuid);
        if let Some(name) = gamepads.gamepad_name_by_uuid(&uuid) {
            self.add_message(format!("Assigned gamepad `{name}` to player {player:?}.",));
        }
    }

    fn unassign_gamepad(&mut self, player: Player, gamepads: &Gamepads, cfg: &mut Config) {
        if let Some(uuid) = cfg.input.unassign_gamepad(player) {
            if let Some(name) = gamepads.gamepad_name_by_uuid(&uuid) {
                self.add_message(format!(
                    "Unassigned gamepad `{name}` from player {player:?}.",
                ));
            }
        }
    }

    fn show_performance_window(&mut self, ctx: &Context, cfg: &mut Config) {
        let mut show_perf_stats = cfg.renderer.show_perf_stats;
        egui::Window::new("Performance Stats")
            .open(&mut show_perf_stats)
            .show(ctx, |ui| self.performance_stats(ui, cfg));
        cfg.renderer.show_perf_stats = show_perf_stats;
    }

    fn show_preferences_viewport(&mut self, ctx: &Context, cfg: &mut Config) {
        if !self.preferences_open {
            return;
        }

        let title = "Preferences";
        // TODO: Make this deferred? Requires `tx` and `cfg` to be Send + Sync
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("preferences"),
            egui::ViewportBuilder::default().with_title(title),
            |ctx, class| {
                if class == ViewportClass::Embedded {
                    let mut preferences_open = self.preferences_open;
                    egui::Window::new(title)
                        .open(&mut preferences_open)
                        .show(ctx, |ui| self.preferences(ui, cfg));
                    self.preferences_open = preferences_open;
                } else {
                    CentralPanel::default().show(ctx, |ui| self.preferences(ui, cfg));
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.preferences_open = false;
                    }
                }
            },
        );
    }

    fn show_keybinds_viewport(&mut self, ctx: &Context, gamepads: &mut Gamepads, cfg: &mut Config) {
        if !self.keybinds_open {
            self.pending_keybind = None;
            self.gamepad_conflict = None;
            return;
        }

        let title = "Keybinds";
        // TODO: Make this deferred? Requires `tx` and `cfg` to be Send + Sync
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("keybinds"),
            egui::ViewportBuilder::default().with_title(title),
            |ctx, class| {
                if class == ViewportClass::Embedded {
                    let mut keybinds_open = self.keybinds_open;
                    egui::Window::new("Keybinds")
                        .open(&mut keybinds_open)
                        .show(ctx, |ui| self.keybinds(ui, gamepads, cfg));
                    self.keybinds_open = keybinds_open;
                } else {
                    CentralPanel::default().show(ctx, |ui| self.keybinds(ui, gamepads, cfg));
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.keybinds_open = false;
                    }
                }
            },
        );
    }

    fn show_about_viewport(&mut self, ctx: &Context) {
        if !self.about_open {
            return;
        }

        let title = "About TetaNES";
        // TODO: Make this deferred? Requires `tx` and `cfg` to be Send + Sync
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("about"),
            egui::ViewportBuilder::default().with_title(title),
            |ctx, class| {
                if class == ViewportClass::Embedded {
                    let mut about_open = self.about_open;
                    egui::Window::new("About TetaNES")
                        .open(&mut about_open)
                        .show(ctx, |ui| self.about(ui));
                    self.about_open = about_open;
                } else {
                    CentralPanel::default().show(ctx, |ui| self.about(ui));
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.about_open = false;
                    }
                }
            },
        );
    }

    fn menu_bar(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.set_enabled(self.pending_keybind.is_none());

        let inner_response = menu::bar(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                global_dark_light_mode_switch(ui);
                ui.separator();

                ui.menu_button("📁 File", |ui| self.file_menu(ui, cfg));
                ui.menu_button("🔧 Controls", |ui| self.controls_menu(ui, cfg));
                ui.menu_button("⚙ Config", |ui| self.config_menu(ui, cfg));
                // icon: screen
                ui.menu_button("🖵 Window", |ui| self.window_menu(ui, cfg));
                ui.menu_button("🕷 Debug", |ui| self.debug_menu(ui, cfg));
                ui.toggle_value(&mut self.about_open, "🔎 About");
            });
        });
        let spacing = ui.style().spacing.item_spacing;
        let border = 1.0;
        let height = inner_response.response.rect.height() + spacing.y + border;
        if height != self.menu_height {
            self.menu_height = height;
            self.resize_window = true;
        }
    }

    fn file_menu(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        // NOTE: Due to some platforms file dialogs blocking the event loop,
        // loading requires a round-trip in order for the above pause to
        // get processed.
        if ui
            .add(Button::new("Load ROM...").shortcut_text(self.fmt_shortcut(UiAction::LoadRom)))
            .clicked()
        {
            self.tx.nes_event(EmulationEvent::Pause(true));
            self.tx.nes_event(UiEvent::LoadRomDialog);
            ui.close_menu();
        }

        if ui
            .add(Button::new("Unload ROM...").shortcut_text(self.fmt_shortcut(UiAction::UnloadRom)))
            .on_disabled_hover_text(Self::NO_ROM_LOADED)
            .clicked()
        {
            self.tx.nes_event(EmulationEvent::UnloadRom);
            ui.close_menu();
        }
        if ui
            .add(Button::new("Load Replay").shortcut_text(self.fmt_shortcut(UiAction::LoadReplay)))
            .on_hover_text("Load a replay file for the currently loaded ROM.")
            .on_disabled_hover_text(Self::NO_ROM_LOADED)
            .clicked()
        {
            self.tx.nes_event(EmulationEvent::Pause(true));
            self.tx.nes_event(UiEvent::LoadReplayDialog);
            ui.close_menu();
        }

        // TODO: support saves and recent games on wasm? Requires storing the data
        if platform::supports(platform::Feature::Filesystem) {
            ui.menu_button("Recently Played...", |ui| {
                use tetanes_core::fs;

                if cfg.renderer.recent_roms.is_empty() {
                    ui.label("No recent ROMs");
                } else {
                    ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));
                    // TODO: add timestamp, save slots, and screenshot
                    for rom in &cfg.renderer.recent_roms {
                        if ui.button(fs::filename(rom)).clicked() {
                            self.tx
                                .nes_event(EmulationEvent::LoadRomPath(rom.to_path_buf()));
                            ui.close_menu();
                        }
                    }
                }
            });

            ui.separator();

            if ui
                .add(
                    Button::new("Save State")
                        .shortcut_text(self.fmt_shortcut(DeckAction::SaveState)),
                )
                .on_hover_text("Save the current state to the selected save slot.")
                .clicked()
            {
                self.tx
                    .nes_event(EmulationEvent::SaveState(cfg.emulation.save_slot));
            };
            if ui
                .add(
                    Button::new("Load State")
                        .shortcut_text(self.fmt_shortcut(DeckAction::LoadState)),
                )
                .on_hover_text("Load a previous state from the selected save slot.")
                .clicked()
            {
                self.tx
                    .nes_event(EmulationEvent::LoadState(cfg.emulation.save_slot));
            }

            ui.menu_button("Save Slot...", |ui| self.save_slot_radio(ui, cfg, true));

            ui.separator();

            if ui
                .add(Button::new("Quit").shortcut_text(self.fmt_shortcut(UiAction::Quit)))
                .clicked()
            {
                self.tx.nes_event(UiEvent::Terminate);
                ui.close_menu();
            };
        }
    }

    fn controls_menu(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
            if ui
                .add(
                    Button::new(if self.paused { "Resume" } else { "Pause" })
                        .shortcut_text(self.fmt_shortcut(UiAction::TogglePause)),
                )
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx.nes_event(EmulationEvent::Pause(!self.paused));
                ui.close_menu();
            };
        });

        if ui
            .add(
                Button::new(if cfg.audio.enabled { "Mute" } else { "Unmute" })
                    .shortcut_text(self.fmt_shortcut(Setting::ToggleAudio)),
            )
            .clicked()
        {
            cfg.audio.enabled = !cfg.audio.enabled;
            self.tx
                .nes_event(ConfigEvent::AudioEnabled(cfg.audio.enabled));
            ui.close_menu();
        };

        ui.separator();

        ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
            if platform::supports(platform::Feature::Filesystem) {
                ui.add_enabled_ui(cfg.emulation.rewind, |ui| {
                    if ui
                        .add(
                            Button::new("Instant Rewind")
                                .shortcut_text(self.fmt_shortcut(Feature::InstantRewind)),
                        )
                        .on_hover_text("Instantly rewind state to a previous point.")
                        .on_disabled_hover_text(if self.loaded_rom.is_none() {
                            Self::NO_ROM_LOADED
                        } else {
                            "Rewind can be enabled under the `Config` menu."
                        })
                        .clicked()
                    {
                        self.tx.nes_event(EmulationEvent::InstantRewind);
                        ui.close_menu();
                    };
                });
            }

            if ui
                .add(
                    Button::new("Reset")
                        .shortcut_text(self.fmt_shortcut(DeckAction::Reset(ResetKind::Soft))),
                )
                .on_hover_text("Emulate a soft reset of the NES.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx.nes_event(EmulationEvent::Reset(ResetKind::Soft));
                ui.close_menu();
            };
            if ui
                .add(
                    Button::new("Power Cycle")
                        .shortcut_text(self.fmt_shortcut(DeckAction::Reset(ResetKind::Hard))),
                )
                .on_hover_text("Emulate a power cycle of the NES.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx.nes_event(EmulationEvent::Reset(ResetKind::Hard));
                ui.close_menu();
            };
        });

        if platform::supports(platform::Feature::Filesystem) {
            ui.separator();

            ui.add_enabled_ui(self.loaded_rom.is_some(), |ui| {
                if ui
                    .add(
                        Button::new("Screenshot")
                            .shortcut_text(self.fmt_shortcut(Feature::TakeScreenshot)),
                    )
                    .on_disabled_hover_text(Self::NO_ROM_LOADED)
                    .clicked()
                {
                    self.tx.nes_event(EmulationEvent::Screenshot);
                    ui.close_menu();
                };
                if ui
                    .add(
                        Button::new(if self.replay_recording {
                            "Stop Replay Recording"
                        } else {
                            "Record Replay"
                        })
                        .shortcut_text(self.fmt_shortcut(Feature::ToggleReplayRecording)),
                    )
                    .on_hover_text("Record or stop recording a game replay file.")
                    .on_disabled_hover_text(Self::NO_ROM_LOADED)
                    .clicked()
                {
                    self.tx
                        .nes_event(EmulationEvent::ReplayRecord(!self.replay_recording));
                    ui.close_menu();
                };
                if ui
                    .add(
                        Button::new(if self.audio_recording {
                            "Stop Audio Recording"
                        } else {
                            "Record Audio"
                        })
                        .shortcut_text(self.fmt_shortcut(Feature::ToggleAudioRecording)),
                    )
                    .on_hover_text("Record or stop recording a audio file.")
                    .on_disabled_hover_text(Self::NO_ROM_LOADED)
                    .clicked()
                {
                    self.tx
                        .nes_event(EmulationEvent::AudioRecord(!self.audio_recording));
                    ui.close_menu();
                };
            });
        }
    }

    fn config_menu(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        self.cycle_acurate_checkbox(ui, cfg, true);
        self.zapper_checkbox(ui, cfg, true);
        self.rewind_checkbox(ui, cfg, true);
        self.overscan_checkbox(ui, cfg, true);

        ui.separator();

        ui.menu_button("Emulation Speed...", |ui| {
            let speed = cfg.emulation.speed;
            if ui
                .add(
                    Button::new("Increment")
                        .shortcut_text(self.fmt_shortcut(Setting::IncrementSpeed)),
                )
                .clicked()
            {
                let new_speed = cfg.increment_speed();
                if speed != new_speed {
                    self.tx.nes_event(ConfigEvent::Speed(new_speed));
                }
            }
            if ui
                .add(
                    Button::new("Decrement")
                        .shortcut_text(self.fmt_shortcut(Setting::DecrementSpeed)),
                )
                .clicked()
            {
                let new_speed = cfg.decrement_speed();
                if speed != new_speed {
                    self.tx.nes_event(ConfigEvent::Speed(new_speed));
                }
            }
            self.speed_slider(ui, cfg);
        });
        ui.menu_button("Run Ahead...", |ui| self.run_ahead_slider(ui, cfg));

        ui.separator();

        ui.menu_button("Video Filter...", |ui| self.video_filter_radio(ui, cfg));
        ui.menu_button("Nes Region...", |ui| self.nes_region_radio(ui, cfg));
        ui.menu_button("Four Player...", |ui| self.four_player_radio(ui, cfg));
        ui.menu_button("Game Genie Codes...", |ui| self.genie_codes_entry(ui, cfg));

        ui.separator();

        if ui
            .add(Button::new("Preferences").shortcut_text(self.fmt_shortcut(Menu::Preferences)))
            .clicked()
        {
            self.preferences_open = !self.preferences_open;
            ui.close_menu();
        }
        if ui
            .add(Button::new("Keybinds").shortcut_text(self.fmt_shortcut(Menu::Keybinds)))
            .clicked()
        {
            self.keybinds_open = !self.keybinds_open;
            ui.close_menu();
        };
    }

    fn window_menu(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        ui.menu_button("Window Scale...", |ui| {
            let scale = cfg.renderer.scale;
            if ui
                .add(
                    Button::new("Increment")
                        .shortcut_text(self.fmt_shortcut(Setting::IncrementScale)),
                )
                .clicked()
            {
                let new_scale = cfg.increment_scale();
                if scale != new_scale {
                    self.resize_window = true;
                    self.resize_texture = true;
                    self.tx.nes_event(ConfigEvent::Scale(cfg.renderer.scale));
                }
            }
            if ui
                .add(
                    Button::new("Decrement")
                        .shortcut_text(self.fmt_shortcut(Setting::DecrementScale)),
                )
                .clicked()
            {
                let new_scale = cfg.decrement_scale();
                if scale != new_scale {
                    self.resize_window = true;
                    self.resize_texture = true;
                    self.tx.nes_event(ConfigEvent::Scale(cfg.renderer.scale));
                }
            }
            self.window_scale_radio(ui, cfg);
        });

        ui.separator();

        self.fullscreen_checkbox(ui, cfg, true);

        if platform::supports(platform::Feature::Viewports) {
            let mut embed_viewports = ui.ctx().embed_viewports();
            ui.checkbox(&mut embed_viewports, "Embed viewports");
            ui.ctx().set_embed_viewports(embed_viewports);
        }

        ui.separator();

        self.menubar_checkbox(ui, cfg, true);
        self.messages_checkbox(ui, cfg, true);
    }

    fn debug_menu(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));

        #[cfg(feature = "profiling")]
        {
            let mut profile = puffin::are_scopes_on();
            ui.checkbox(&mut profile, "Enable Profiling")
                .on_hover_text("Toggle the Puffin profiling window");
            puffin::set_scopes_on(profile);
        }
        if ui
            .add(
                ToggleValue::new(&mut cfg.renderer.show_perf_stats, "Performance Stats")
                    .shortcut_text(self.fmt_shortcut(Setting::TogglePerfStats)),
            )
            .on_hover_text("Enable a performance statistics overlay")
            .clicked()
        {
            ui.close_menu();
        }
        #[cfg(debug_assertions)]
        if ui
            .checkbox(&mut self.debug_on_hover, "Debug on Hover")
            .clicked()
        {
            ui.ctx().set_debug_on_hover(self.debug_on_hover);
        }

        ui.separator();

        let cpu_debugger_shortcut = self.fmt_shortcut(Debug::Toggle(Debugger::Cpu));
        if ui
            .add(
                ToggleValue::new(&mut self.cpu_debugger_open, "CPU Debugger")
                    .shortcut_text(cpu_debugger_shortcut),
            )
            .on_hover_text("Toggle the CPU Debugger.")
            .clicked()
        {
            ui.close_menu();
        }

        let ppu_debugger_shortcut = self.fmt_shortcut(Debug::Toggle(Debugger::Ppu));
        if ui
            .add(
                ToggleValue::new(&mut self.ppu_debugger_open, "PPU Debugger")
                    .shortcut_text(ppu_debugger_shortcut),
            )
            .on_hover_text("Toggle the PPU Debugger.")
            .clicked()
        {
            ui.close_menu();
        }

        let apu_debugger_shortcut = self.fmt_shortcut(Debug::Toggle(Debugger::Apu));
        if ui
            .add(
                ToggleValue::new(&mut self.apu_debugger_open, "APU Debugger")
                    .shortcut_text(apu_debugger_shortcut),
            )
            .on_hover_text("Toggle the APU Debugger.")
            .clicked()
        {
            ui.close_menu();
        }

        ui.separator();

        ui.add_enabled_ui(self.paused && self.loaded_rom.is_some(), |ui| {
            if ui
                .add(
                    Button::new("Step Into")
                        .shortcut_text(self.fmt_shortcut(Debug::Step(DebugStep::Into))),
                )
                .on_hover_text("Step a single CPU instruction.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx
                    .nes_event(EmulationEvent::DebugStep(DebugStep::Into));
            }
            if ui
                .add(
                    Button::new("Step Out")
                        .shortcut_text(self.fmt_shortcut(Debug::Step(DebugStep::Out))),
                )
                .on_hover_text("Step out of the current CPU function.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx.nes_event(EmulationEvent::DebugStep(DebugStep::Out));
            }
            if ui
                .add(
                    Button::new("Step Over")
                        .shortcut_text(self.fmt_shortcut(Debug::Step(DebugStep::Over))),
                )
                .on_hover_text("Step over the next CPU instruction.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx
                    .nes_event(EmulationEvent::DebugStep(DebugStep::Over));
            }
            if ui
                .add(
                    Button::new("Step Scanline")
                        .shortcut_text(self.fmt_shortcut(Debug::Step(DebugStep::Scanline))),
                )
                .on_hover_text("Step an entire PPU scanline.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx
                    .nes_event(EmulationEvent::DebugStep(DebugStep::Scanline));
            }
            if ui
                .add(
                    Button::new("Step Frame")
                        .shortcut_text(self.fmt_shortcut(Debug::Step(DebugStep::Frame))),
                )
                .on_hover_text("Step an entire PPU Frame.")
                .on_disabled_hover_text(Self::NO_ROM_LOADED)
                .clicked()
            {
                self.tx
                    .nes_event(EmulationEvent::DebugStep(DebugStep::Frame));
            }
        });
    }

    fn nes_frame(&mut self, ui: &mut Ui, gamepads: &Gamepads, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.set_enabled(self.pending_keybind.is_none());

        CentralPanel::default()
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                if self.loaded_rom.is_some() {
                    ui.with_layout(
                        Layout {
                            main_dir: Direction::TopDown,
                            main_align: Align::Center,
                            cross_align: Align::Center,
                            ..Default::default()
                        },
                        |ui| {
                            let frame_resp = ui
                                .add(
                                    Image::from_texture(self.texture)
                                        .maintain_aspect_ratio(true)
                                        .shrink_to_fit()
                                        .sense(Sense::click()),
                                )
                                .on_hover_cursor(if cfg.deck.zapper {
                                    CursorIcon::Crosshair
                                } else {
                                    CursorIcon::Default
                                });
                            self.nes_frame = frame_resp.rect;

                            if cfg.deck.zapper {
                                if self
                                    .action_input(DeckAction::ZapperAimOffscreen)
                                    .map_or(false, |input| input_down(ui, gamepads, cfg, input))
                                {
                                    self.tx.nes_event(EmulationEvent::ZapperAim((
                                        Ppu::WIDTH + 10,
                                        Ppu::HEIGHT + 10,
                                    )));
                                } else if let Some(Pos2 { x, y }) =
                                    frame_resp.hover_pos().and_then(|Pos2 { x, y }| {
                                        cursor_to_zapper(x, y, frame_resp.rect)
                                    })
                                {
                                    self.tx.nes_event(EmulationEvent::ZapperAim((
                                        x.round() as u32,
                                        y.round() as u32,
                                    )));
                                }
                                if frame_resp.clicked() {
                                    self.tx.nes_event(EmulationEvent::ZapperTrigger);
                                }
                            }
                        },
                    );
                } else {
                    ui.vertical_centered(|ui| {
                        ui.horizontal_centered(|ui| {
                            ui.add(
                                Image::new(include_image!("../../../assets/tetanes.png"))
                                    .shrink_to_fit()
                                    .tint(Color32::GRAY),
                            );
                        });
                    });
                }
            });

        if cfg.renderer.show_messages && (!self.messages.is_empty() || self.error.is_some()) {
            Frame::canvas(ui.style()).show(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::LEFT).with_main_wrap(true), |ui| {
                    self.message_bar(ui);
                    self.error_bar(ui);
                });
            });
        }

        if self.status.is_some() {
            Frame::canvas(ui.style()).show(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::LEFT).with_main_wrap(true), |ui| {
                    self.status_bar(ui);
                });
            });
        }
    }

    fn message_bar(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.messages.retain(|(_, expires)| now < *expires);
        self.messages.dedup_by(|a, b| a.0.eq(&b.0));
        for (message, _) in self.messages.iter().take(Self::MAX_MESSAGES) {
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
        // TODO: maybe show other statuses like rewinding/playback/recording - bitflags?
        if let Some(status) = self.status {
            ui.label(status);
        }
    }

    fn performance_stats(&mut self, ui: &mut Ui, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.allocate_space(Vec2::new(200.0, 0.0));
        ui.set_enabled(self.pending_keybind.is_none());

        Grid::new("perf_stats")
            .num_columns(2)
            .spacing([40.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                ui.ctx().request_repaint_after(Duration::from_secs(1));

                if let Some(sys) = &mut self.sys {
                    // NOTE: refreshing sysinfo is cpu-intensive if done too frequently and skews the
                    // results
                    let sys_update_interval = Duration::from_secs(1);
                    assert!(sys_update_interval > sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
                    if self.sys_updated.elapsed() >= sys_update_interval {
                        sys.refresh_specifics(
                            RefreshKind::new().with_processes(
                                ProcessRefreshKind::new()
                                    .with_cpu()
                                    .with_memory()
                                    .with_disk_usage(),
                            ),
                        );
                        self.sys_updated = Instant::now();
                    }
                }

                let fps_color = |fps| match fps {
                    fps if fps < 30.0 => Color32::RED,
                    fps if fps < 60.0 => Color32::YELLOW,
                    _ => Color32::GREEN,
                };
                let frame_time_color = |time| match time {
                    time if time <= 1000.0 * 1.0 / 60.0 => Color32::GREEN,
                    time if time <= 1000.0 * 1.0 / 30.0 => Color32::YELLOW,
                    _ => Color32::RED,
                };
                let cpu_color = |cpu| match cpu {
                    cpu if cpu <= 25.0 => Color32::GREEN,
                    cpu if cpu <= 50.0 => Color32::YELLOW,
                    _ => Color32::RED,
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

                if let Some(ref sys) = self.sys {
                    ui.label("");
                    ui.end_row();

                    match sys.process(Pid::from_u32(std::process::id())) {
                        Some(proc) => {
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
                        None => todo!(),
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
    }

    fn preferences(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.set_enabled(self.pending_keybind.is_none());

        ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.preferences_tab,
                    PreferencesTab::Emulation,
                    "Emulation",
                );
                ui.selectable_value(&mut self.preferences_tab, PreferencesTab::Audio, "Audio");
                ui.selectable_value(&mut self.preferences_tab, PreferencesTab::Video, "Video");
                ui.selectable_value(&mut self.preferences_tab, PreferencesTab::Input, "Input");
            });

            ui.separator();

            match self.preferences_tab {
                PreferencesTab::Emulation => self.emulation_preferences(ui, cfg),
                PreferencesTab::Audio => self.audio_preferences(ui, cfg),
                PreferencesTab::Video => self.video_preferences(ui, cfg),
                PreferencesTab::Input => self.input_preferences(ui, cfg),
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Restore Defaults").clicked() {
                    cfg.reset();
                    self.tx.nes_event(ConfigEvent::InputBindings);
                }
                if platform::supports(platform::Feature::Filesystem) {
                    if let Some(data_dir) = Config::default_data_dir() {
                        if ui.button("Clear Save States").clicked() {
                            match fs::clear_dir(data_dir) {
                                Ok(_) => self.add_message("Save States cleared."),
                                Err(_) => self.add_message("Failed to clear Save States."),
                            }
                        }
                        if ui.button("Clear Recent ROMs").clicked() {
                            cfg.renderer.recent_roms.clear();
                        }
                    }
                }
            });
        });
    }

    fn emulation_preferences(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        Grid::new("emulation_checkboxes")
            .num_columns(2)
            .spacing([80.0, 6.0])
            .show(ui, |ui| {
                self.cycle_acurate_checkbox(ui, cfg, false);
                ui.checkbox(&mut cfg.emulation.auto_load, "Auto-Load")
                    .on_hover_text(
                        "Automatically load game state from the current save slot on load.",
                    );
                ui.end_row();

                self.rewind_checkbox(ui, cfg, false);
                ui.vertical(|ui| {
                    ui.checkbox(&mut cfg.emulation.auto_save, "Auto-Save")
                        .on_hover_text(concat!(
                            "Automatically save game state to the current save slot ",
                            "on exit or unloading and an optional interval. ",
                            "Setting to 0 will disable saving on an interval.",
                        ));

                    ui.add_enabled_ui(cfg.emulation.auto_save, |ui| {
                        let mut auto_save_interval = cfg.emulation.auto_save_interval.as_secs();
                        ui.indent("auto_save_interval", |ui| {
                            ui.horizontal(|ui| {
                                ui.strong("Interval:");
                                let res = ui.add(
                                    DragValue::new(&mut auto_save_interval)
                                        .clamp_range(0..=60)
                                        .suffix(" seconds"),
                                );
                                if res.lost_focus() && res.changed() {
                                    cfg.emulation.auto_save_interval =
                                        Duration::from_secs(auto_save_interval);
                                    self.tx.nes_event(ConfigEvent::AutoSaveInterval(
                                        cfg.emulation.auto_save_interval,
                                    ));
                                }
                            });
                        });
                    });
                });
                ui.end_row();
            });

        ui.separator();

        Grid::new("emulation_preferences")
            .num_columns(2)
            .spacing([40.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Emulation Speed:");
                self.speed_slider(ui, cfg);
                ui.end_row();

                ui.strong("Run Ahead:")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text(
                        "Simulate a number of frames in the future to reduce input lag.",
                    );
                self.run_ahead_slider(ui, cfg);
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Save Slot:")
                        .on_hover_cursor(CursorIcon::Help)
                        .on_hover_text(
                            "Select which slot to use when saving or loading game state.",
                        );
                });
                Grid::new("save_slots")
                    .num_columns(2)
                    .spacing([20.0, 6.0])
                    .show(ui, |ui| {
                        self.save_slot_radio(ui, cfg, false);
                    });
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Four Player:")
                    .on_hover_cursor(CursorIcon::Help)
                    .on_hover_text(
                    "Some game titles support up to 4 players (requires connected controllers).",
                );
                });
                ui.vertical(|ui| self.four_player_radio(ui, cfg));
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("NES Region:")
                        .on_hover_cursor(CursorIcon::Help)
                        .on_hover_text("Which regional NES hardware to emulate.");
                });
                ui.vertical(|ui| self.nes_region_radio(ui, cfg));
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("RAM State:")
                        .on_hover_cursor(CursorIcon::Help)
                        .on_hover_text("What values are read from NES RAM on load.");
                });
                ui.vertical(|ui| self.ram_state_radio(ui, cfg));
                ui.end_row();
            });
    }

    fn audio_preferences(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if ui
            .checkbox(&mut cfg.audio.enabled, "Enable Audio")
            .clicked()
        {
            self.tx
                .nes_event(ConfigEvent::AudioEnabled(cfg.audio.enabled));
        }

        ui.add_enabled_ui(cfg.audio.enabled, |ui| {
            ui.indent("apu_channels", |ui| {
                let channels = &mut cfg.deck.channels_enabled;
                Grid::new("apu_channels")
                    .spacing([60.0, 6.0])
                    .num_columns(2)
                    .show(ui, |ui| {
                        if ui.checkbox(&mut channels[0], "Enable Pulse1").clicked() {
                            self.tx.nes_event(ConfigEvent::ApuChannelEnabled((
                                Channel::Pulse1,
                                channels[0],
                            )));
                        }
                        if ui.checkbox(&mut channels[3], "Enable Noise").clicked() {
                            self.tx.nes_event(ConfigEvent::ApuChannelEnabled((
                                Channel::Noise,
                                channels[3],
                            )));
                        }
                        ui.end_row();

                        if ui.checkbox(&mut channels[1], "Enable Pulse2").clicked() {
                            self.tx.nes_event(ConfigEvent::ApuChannelEnabled((
                                Channel::Pulse2,
                                channels[1],
                            )));
                        }
                        if ui.checkbox(&mut channels[4], "Enable DMC").clicked() {
                            self.tx.nes_event(ConfigEvent::ApuChannelEnabled((
                                Channel::Dmc,
                                channels[4],
                            )));
                        }
                        ui.end_row();

                        if ui.checkbox(&mut channels[2], "Enable Triangle").clicked() {
                            self.tx.nes_event(ConfigEvent::ApuChannelEnabled((
                                Channel::Triangle,
                                channels[2],
                            )));
                        }
                        if ui.checkbox(&mut channels[5], "Enable Mapper").clicked() {
                            self.tx.nes_event(ConfigEvent::ApuChannelEnabled((
                                Channel::Mapper,
                                channels[5],
                            )));
                        }
                        ui.end_row();
                    });

                ui.separator();

                Grid::new("audio_settings")
                    .spacing([40.0, 6.0])
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.strong("Buffer Size:")
                            .on_hover_cursor(CursorIcon::Help)
                            .on_hover_text(
                                "The audio sample buffer size allocated to the sound driver. Increased audio buffer size can help reduce audio underruns.",
                            );
                        let res = ui
                            .add(
                                DragValue::new(&mut cfg.audio.buffer_size)
                                    .speed(10)
                                    .clamp_range(0..=8192)
                                    .suffix(" samples"),
                            );
                        if res.lost_focus() && res.changed() {
                            self.tx.nes_event(
                                ConfigEvent::AudioBuffer(cfg.audio.buffer_size),
                            );
                        }
                        ui.end_row();

                        ui.strong("Latency:")
                            .on_hover_cursor(CursorIcon::Help)
                            .on_hover_text(
                                "The amount of queued audio before sending to the sound driver. Increased audio latency can help reduce audio underruns.",
                            );
                        let mut latency = cfg.audio.latency.as_millis() as u64;
                        let res = ui
                            .add(
                                DragValue::new(&mut latency)
                                    .clamp_range(0..=1000)
                                    .suffix(" ms"),
                            );
                        if res.lost_focus() && res.changed() {
                            cfg.audio.latency = Duration::from_millis(latency);
                            self.tx.nes_event(
                                ConfigEvent::AudioLatency(cfg.audio.latency),
                            );
                        }
                        ui.end_row();
                    });
            });
        });
    }

    fn video_preferences(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        Grid::new("video_checkboxes")
            .spacing([80.0, 6.0])
            .num_columns(2)
            .show(ui, |ui| {
                self.menubar_checkbox(ui, cfg, false);
                self.fullscreen_checkbox(ui, cfg, false);
                ui.end_row();

                self.messages_checkbox(ui, cfg, false);
                ui.end_row();

                self.overscan_checkbox(ui, cfg, false);
                ui.end_row();
            });

        ui.separator();

        Grid::new("video_preferences")
            .num_columns(2)
            .spacing([40.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Window Scale:");
                });
                Grid::new("save_slots")
                    .num_columns(2)
                    .spacing([20.0, 6.0])
                    .show(ui, |ui| {
                        self.window_scale_radio(ui, cfg);
                    });
                ui.end_row();

                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.strong("Video Filter:");
                });
                ui.vertical(|ui| {
                    self.video_filter_radio(ui, cfg);
                });
            });
    }

    fn input_preferences(&mut self, ui: &mut Ui, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        Grid::new("input_checkboxes")
            .num_columns(2)
            .spacing([80.0, 6.0])
            .show(ui, |ui| {
                self.zapper_checkbox(ui, cfg, false);
                ui.end_row();

                if ui
                    .checkbox(&mut cfg.deck.concurrent_dpad, "Enable Concurrent D-Pad")
                    .clicked()
                {
                    self.tx
                        .nes_event(ConfigEvent::ConcurrentDpad(cfg.deck.concurrent_dpad));
                }
            });
    }

    fn keybinds(&mut self, ui: &mut Ui, gamepads: &mut Gamepads, cfg: &mut Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        self.show_set_keybind_window(ui, gamepads, cfg);
        self.show_gamepad_conflict_window(ui, gamepads, cfg);
        ui.set_enabled(self.pending_keybind.is_none() && self.gamepad_conflict.is_none());

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.keybinds_tab, KeybindsTab::Shortcuts, "Shortcuts");
            ui.selectable_value(
                &mut self.keybinds_tab,
                KeybindsTab::Joypad(Player::One),
                "Player1",
            );
            ui.selectable_value(
                &mut self.keybinds_tab,
                KeybindsTab::Joypad(Player::Two),
                "Player2",
            );
            ui.selectable_value(
                &mut self.keybinds_tab,
                KeybindsTab::Joypad(Player::Three),
                "Player3",
            );
            ui.selectable_value(
                &mut self.keybinds_tab,
                KeybindsTab::Joypad(Player::Four),
                "Player4",
            );
        });

        ui.separator();

        match self.keybinds_tab {
            KeybindsTab::Shortcuts => self.keybind_list(ui, gamepads, cfg, None),
            KeybindsTab::Joypad(player) => self.keybind_list(ui, gamepads, cfg, Some(player)),
        }
    }

    fn keybind_list(
        &mut self,
        ui: &mut Ui,
        gamepads: &mut Gamepads,
        cfg: &mut Config,
        player: Option<Player>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if let Some(player) = player {
            ui.horizontal(|ui| {
                ui.strong("Assigned Gamepad:");

                let unassigned = "Unassigned".to_string();
                match gamepads.list() {
                    Some(mut list) => {
                        if list.peek().is_some() {
                            let mut assigned_gamepad = cfg.input.gamepad_assigned_to(player);
                            let previous_gamepad = assigned_gamepad;
                            let gamepad_name = assigned_gamepad
                                .and_then(|uuid| gamepads.gamepad_name_by_uuid(&uuid))
                                .unwrap_or_else(|| unassigned.clone());
                            egui::ComboBox::from_id_source("assigned_gamepad")
                                .selected_text(gamepad_name.clone())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut assigned_gamepad, None, unassigned);
                                    for (_, gamepad) in list {
                                        ui.selectable_value(
                                            &mut assigned_gamepad,
                                            Some(GamepadUuid(gamepad.uuid())),
                                            gamepad.name(),
                                        );
                                    }
                                });
                            if previous_gamepad != assigned_gamepad {
                                match &assigned_gamepad {
                                    Some(uuid) => {
                                        match assigned_gamepad
                                            .as_ref()
                                            .and_then(|name| cfg.input.gamepad_assignment(name))
                                        {
                                            Some(existing_player) => {
                                                self.gamepad_conflict =
                                                    Some((existing_player, player, *uuid));
                                            }
                                            None => {
                                                self.assign_gamepad(player, *uuid, gamepads, cfg);
                                            }
                                        }
                                    }
                                    None => {
                                        self.unassign_gamepad(player, gamepads, cfg);
                                    }
                                }
                            }
                        } else {
                            ui.set_enabled(false);
                            egui::ComboBox::from_id_source("assigned_gamepad")
                                .selected_text("No Gamepads Connected")
                                .show_ui(ui, |_| {});
                        }
                    }
                    None => {
                        ui.set_enabled(false);
                        egui::ComboBox::from_id_source("assigned_gamepad")
                            .selected_text("Gamepads not supported")
                            .show_ui(ui, |_| {});
                    }
                }
            });

            ui.separator();
        }

        let keybinds = match player {
            None => &mut self.shortcut_keybinds,
            Some(player) => &mut self.joypad_keybinds[player as usize],
        };

        let row_height = ui.text_style_height(&TextStyle::Body);
        let total_rows = keybinds.len();
        ScrollArea::vertical().show_rows(ui, row_height, total_rows, |ui, row_range| {
            Grid::new("keybind_list")
                .num_columns(3)
                .spacing([80.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.heading("Action");
                    ui.heading("Binding #1");
                    ui.heading("Binding #2");
                    ui.end_row();

                    for (action, input) in keybinds
                        .values_mut()
                        .skip(row_range.start)
                        .take(row_range.end)
                    {
                        ui.strong(action.to_string());
                        for (slot, input) in input.iter_mut().enumerate() {
                            let res = ui
                                .add(
                                    Button::new(input.map(format_input).unwrap_or_default())
                                        .min_size(Vec2::new(100.0, 0.0)),
                                )
                                .on_hover_text("Click to set. Right-click to unset.");
                            if res.clicked() {
                                self.pending_keybind = Some(PendingKeybind {
                                    action: *action,
                                    player,
                                    binding: slot,
                                    input: None,
                                    conflict: None,
                                });
                            } else if res.secondary_clicked() {
                                if let Some(input) = input.take() {
                                    cfg.input.clear_binding(input);
                                    self.tx.nes_event(ConfigEvent::InputBindings);
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        });
    }

    fn about(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.set_enabled(self.pending_keybind.is_none());

        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            ui.add(
                Image::new(include_image!("../../../assets/tetanes_icon.png"))
                    .max_height(50.0)
                    .shrink_to_fit(),
            );
            ui.vertical(|ui| {
                Grid::new("version")
                    .num_columns(2)
                    .spacing([40.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Version:");
                        ui.label(env!("CARGO_PKG_VERSION").to_string());
                        ui.end_row();

                        ui.strong("GitHub:");
                        ui.hyperlink("https://github.com/lukexor/tetanes");
                        ui.end_row();
                    });

                if platform::supports(platform::Feature::Filesystem) {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        Grid::new("directories")
                            .num_columns(2)
                            .spacing([40.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                if let Some(config_dir) = Config::default_config_dir() {
                                    ui.strong("Preferences:");
                                    ui.label(format!("{}", config_dir.display()));
                                    ui.end_row();
                                }
                                if let Some(data_dir) = Config::default_data_dir() {
                                    ui.strong("Save States/RAM, Replays: ");
                                    ui.label(format!("{}", data_dir.display()));
                                    ui.end_row();
                                }
                                if let Some(picture_dir) = Config::default_picture_dir() {
                                    ui.strong("Screenshots: ");
                                    ui.label(format!("{}", picture_dir.display()));
                                    ui.end_row();
                                }
                                if let Some(audio_dir) = Config::default_audio_dir() {
                                    ui.strong("Audio Recordings: ");
                                    ui.label(format!("{}", audio_dir.display()));
                                    ui.end_row();
                                }
                            });
                    });
                }
            });
        });
    }

    fn save_slot_radio(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        ui.vertical(|ui| {
            for slot in 1..=4 {
                ui.add(
                    RadioValue::new(&mut cfg.emulation.save_slot, slot, slot.to_string())
                        .shortcut_text(
                            shortcut
                                .then(|| self.fmt_shortcut(DeckAction::SetSaveSlot(slot)))
                                .unwrap_or_default(),
                        ),
                );
            }
        });
        ui.vertical(|ui| {
            for slot in 5..=8 {
                ui.add(
                    RadioValue::new(&mut cfg.emulation.save_slot, slot, slot.to_string())
                        .shortcut_text(
                            shortcut
                                .then(|| self.fmt_shortcut(DeckAction::SetSaveSlot(slot)))
                                .unwrap_or_default(),
                        ),
                );
            }
        });
    }

    fn speed_slider(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let res = ui
            .add(
                Slider::new(&mut cfg.emulation.speed, 0.25..=2.0)
                    .step_by(0.25)
                    .suffix("x"),
            )
            .on_hover_text("Adjust the speed of the NES emulation.");
        if res.lost_focus() && res.changed() {
            self.tx.nes_event(ConfigEvent::Speed(cfg.emulation.speed));
        }
    }

    fn run_ahead_slider(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let res = ui
            .add(Slider::new(&mut cfg.emulation.run_ahead, 0..=4))
            .on_hover_text("Simulate a number of frames in the future to reduce input lag.");
        if res.lost_focus() && res.changed() {
            self.tx
                .nes_event(ConfigEvent::RunAhead(cfg.emulation.run_ahead));
        }
    }

    fn cycle_acurate_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        if ui
            .add(
                Checkbox::new(&mut cfg.deck.cycle_accurate, "Cycle Accurate").shortcut_text(
                    shortcut
                        .then(|| self.fmt_shortcut(Setting::ToggleCycleAccurate))
                        .unwrap_or_default(),
                ),
            )
            .on_hover_text("Enables more accurate NES emulation at a slight cost in performance.")
            .clicked()
        {
            self.tx
                .nes_event(ConfigEvent::CycleAccurate(cfg.deck.cycle_accurate));
        }
    }

    fn rewind_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        if ui
            .add(
                Checkbox::new(&mut cfg.emulation.rewind, "Enable Rewinding").shortcut_text(
                    shortcut
                        .then(|| self.fmt_shortcut(Setting::ToggleRewinding))
                        .unwrap_or_default(),
                ),
            )
            .on_hover_text("Enable instant and visual rewinding. Increases memory usage.")
            .clicked()
        {
            self.tx
                .nes_event(ConfigEvent::RewindEnabled(cfg.emulation.rewind));
        }
    }

    fn zapper_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        if ui
            .add(
                Checkbox::new(&mut cfg.deck.zapper, "Enable Zapper Gun").shortcut_text(
                    shortcut
                        .then(|| self.fmt_shortcut(DeckAction::ToggleZapperConnected))
                        .unwrap_or_default(),
                ),
            )
            .on_hover_text("Enable the Zapper Light Gun for games that support it.")
            .clicked()
        {
            self.tx
                .nes_event(ConfigEvent::ZapperConnected(cfg.deck.zapper));
        }
    }

    fn overscan_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        if ui.add(
            Checkbox::new(&mut cfg.renderer.hide_overscan, "Hide Overscan").shortcut_text(
                shortcut
                    .then(|| self.fmt_shortcut(Setting::ToggleOverscan))
                    .unwrap_or_default(),
                ),
            )
            .on_hover_text("Traditional CRT displays would crop the top and bottom edges of the image. Disable this to show the overscan.")
            .clicked()
        {
            self.resize_texture = self.loaded_region.is_ntsc();
            self.tx
                .nes_event(ConfigEvent::HideOverscan(cfg.renderer.hide_overscan));
        }
    }

    fn video_filter_radio(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let filter = cfg.deck.filter;
        ui.radio_value(&mut cfg.deck.filter, VideoFilter::Pixellate, "Pixellate")
            .on_hover_text("Basic pixel-perfect rendering");
        ui.radio_value(&mut cfg.deck.filter, VideoFilter::Ntsc, "Ntsc")
            .on_hover_text(
                "Emulate traditional NTSC rendering where chroma spills over into luma.",
            );
        if filter != cfg.deck.filter {
            self.tx.nes_event(ConfigEvent::VideoFilter(cfg.deck.filter));
        }
    }

    fn four_player_radio(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let four_player = cfg.deck.four_player;
        ui.radio_value(&mut cfg.deck.four_player, FourPlayer::Disabled, "Disabled");
        ui.radio_value(
            &mut cfg.deck.four_player,
            FourPlayer::FourScore,
            "Four Score",
        )
        .on_hover_text("Enable NES Four Score for games that support 4 players.");
        ui.radio_value(
            &mut cfg.deck.four_player,
            FourPlayer::Satellite,
            "Satellite",
        )
        .on_hover_text("Enable NES Satellite for games that support 4 players.");
        if four_player != cfg.deck.four_player {
            self.tx
                .nes_event(ConfigEvent::FourPlayer(cfg.deck.four_player));
        }
    }

    fn nes_region_radio(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let region = cfg.deck.region;
        ui.radio_value(&mut cfg.deck.region, NesRegion::Auto, "Auto")
            .on_hover_text("Auto-detect region based on loaded ROM.");
        ui.radio_value(&mut cfg.deck.region, NesRegion::Ntsc, "NTSC")
            .on_hover_text("Emulate NTSC timing and aspect-ratio.");
        ui.radio_value(&mut cfg.deck.region, NesRegion::Pal, "PAL")
            .on_hover_text("Emulate PAL timing and aspect-ratio.");
        ui.radio_value(&mut cfg.deck.region, NesRegion::Dendy, "Dendy")
            .on_hover_text("Emulate Dendy timing and aspect-ratio.");
        if region != cfg.deck.region {
            self.resize_window = true;
            self.resize_texture = true;
            self.tx.nes_event(ConfigEvent::Region(cfg.deck.region));
        }
    }

    fn ram_state_radio(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let ram_state = cfg.deck.ram_state;
        ui.radio_value(&mut cfg.deck.ram_state, RamState::AllZeros, "All 0x00")
            .on_hover_text("Clear startup RAM to all zeroes for predictable emulation.");
        ui.radio_value(&mut cfg.deck.ram_state, RamState::AllOnes, "All 0xFF")
            .on_hover_text("Clear startup RAM to all ones for predictable emulation.");
        ui.radio_value(&mut cfg.deck.ram_state, RamState::Random, "Random")
            .on_hover_text("Randomize startup RAM, which some games use as a basic RNG seed.");
        if ram_state != cfg.deck.ram_state {
            self.tx.nes_event(ConfigEvent::RamState(cfg.deck.ram_state));
        }
    }

    fn genie_codes_entry(&mut self, ui: &mut Ui, cfg: &mut Config) {
        ui.strong("Add Genie Code:")
            .on_hover_cursor(CursorIcon::Help)
            .on_hover_text(
                "A Game Genie Code is a 6 or 8 letter string that temporarily modifies game memory during operation. e.g. `AATOZE` will start Super Mario Bros. with 9 lives."
            );
        ui.horizontal(|ui| {
            let entry_res = ui.text_edit_singleline(&mut self.pending_genie_entry.code);
            let has_entry = !self.pending_genie_entry.code.is_empty();
            let submit_res = ui.add_enabled(has_entry, Button::new("➕"));
            if entry_res.changed() {
                self.pending_genie_entry.error = None;
            }
            if (has_entry && entry_res.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                || submit_res.clicked()
            {
                match GenieCode::parse(&self.pending_genie_entry.code) {
                    Ok(hex) => {
                        let code =
                            GenieCode::from_raw(mem::take(&mut self.pending_genie_entry.code), hex);
                        if !cfg.deck.genie_codes.contains(&code) {
                            cfg.deck.genie_codes.push(code.clone());
                            self.tx.nes_event(ConfigEvent::GenieCodeAdded(code));
                        }
                    }
                    Err(err) => self.pending_genie_entry.error = Some(err.to_string()),
                }
            }
        });
        if let Some(error) = &self.pending_genie_entry.error {
            ui.allocate_space(Vec2::new(Self::MENU_WIDTH, 0.0));
            ui.colored_label(Color32::RED, error);
        }

        if !cfg.deck.genie_codes.is_empty() {
            ui.separator();
            ui.strong("Current Genie Codes:");
            cfg.deck.genie_codes.retain(|genie| {
                ui.horizontal(|ui| {
                    ui.label(genie.code());
                    // icon: waste basket
                    if ui.button("🗑").clicked() {
                        self.tx
                            .nes_event(ConfigEvent::GenieCodeRemoved(genie.code().to_string()));
                        false
                    } else {
                        true
                    }
                })
                .inner
            });
        }
    }

    fn menubar_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        ui.add(
            Checkbox::new(&mut cfg.renderer.show_menubar, "Show Menu Bar").shortcut_text(
                shortcut
                    .then(|| self.fmt_shortcut(Setting::ToggleMenubar))
                    .unwrap_or_default(),
            ),
        )
        .on_hover_text("Show the menu bar.");
    }

    fn messages_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        ui.add(
            Checkbox::new(&mut cfg.renderer.show_messages, "Show Messages").shortcut_text(
                shortcut
                    .then(|| self.fmt_shortcut(Setting::ToggleMessages))
                    .unwrap_or_default(),
            ),
        )
        .on_hover_text("Show shortcut and emulator messages.");
    }

    fn window_scale_radio(&mut self, ui: &mut Ui, cfg: &mut Config) {
        let scale = cfg.renderer.scale;
        ui.vertical(|ui| {
            ui.radio_value(&mut cfg.renderer.scale, 1.0, "1x");
            ui.radio_value(&mut cfg.renderer.scale, 2.0, "2x");
            ui.radio_value(&mut cfg.renderer.scale, 3.0, "3x");
        });
        ui.vertical(|ui| {
            ui.radio_value(&mut cfg.renderer.scale, 4.0, "4x");
            ui.radio_value(&mut cfg.renderer.scale, 5.0, "5x");
        });
        if scale != cfg.renderer.scale {
            self.resize_window = true;
            self.resize_texture = true;
            self.tx.nes_event(ConfigEvent::Scale(cfg.renderer.scale));
        }
    }

    fn fullscreen_checkbox(&mut self, ui: &mut Ui, cfg: &mut Config, shortcut: bool) {
        if ui
            .add(
                Checkbox::new(&mut cfg.renderer.fullscreen, "Fullscreen").shortcut_text(
                    shortcut
                        .then(|| self.fmt_shortcut(Setting::ToggleFullscreen))
                        .unwrap_or_default(),
                ),
            )
            .clicked()
        {
            if platform::supports(platform::Feature::Viewports) {
                ui.ctx().set_embed_viewports(cfg.renderer.fullscreen);
            }
            ui.ctx()
                .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
            ui.ctx().send_viewport_cmd_to(
                ViewportId::ROOT,
                ViewportCommand::Fullscreen(cfg.renderer.fullscreen),
            );
        }
    }

    fn update_keybind(
        keybinds: &mut BTreeMap<String, Keybind>,
        cfg_bindings: &mut Vec<ActionBindings>,
        action: Action,
        input: Input,
        binding: usize,
    ) {
        // Clear any conflicts
        for binding in keybinds
            .values_mut()
            .map(|(_, bindings)| bindings)
            .chain(cfg_bindings.iter_mut().map(|bind| &mut bind.bindings))
            .flatten()
        {
            if *binding == Some(input) {
                *binding = None;
            }
        }
        keybinds
            .entry(action.to_string())
            .and_modify(|(_, bindings)| bindings[binding] = Some(input))
            .or_insert_with(|| {
                let mut bindings = [None, None];
                bindings[binding] = Some(input);
                (action, bindings)
            });
        let current_binding = cfg_bindings.iter_mut().find(|b| b.action == action);
        match current_binding {
            Some(bind) => bind.bindings[binding] = Some(input),
            None => cfg_bindings.push(ActionBindings {
                action,
                bindings: [Some(input), None],
            }),
        }
    }

    fn action_input(&self, action: impl Into<Action>) -> Option<Input> {
        let action = action.into();
        self.shortcut_keybinds
            .get(action.as_ref())
            .or_else(|| {
                self.joypad_keybinds
                    .iter()
                    .map(|bind| bind.get(action.as_ref()))
                    .next()
                    .flatten()
            })
            .and_then(|(_, binding)| binding[0])
    }

    fn fmt_shortcut(&self, action: impl Into<Action>) -> String {
        let action = action.into();
        self.shortcut_keybinds
            .get(action.as_ref())
            .or_else(|| {
                self.joypad_keybinds
                    .iter()
                    .map(|bind| bind.get(action.as_ref()))
                    .next()
                    .flatten()
            })
            .and_then(|(_, binding)| binding[0])
            .map(format_input)
            .unwrap_or_default()
    }
}

const fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / 0x100000
}

fn cursor_to_zapper(x: f32, y: f32, rect: Rect) -> Option<Pos2> {
    let width = Ppu::WIDTH as f32;
    let height = Ppu::HEIGHT as f32;
    // Normalize x/y to 0..=1 and scale to PPU dimensions
    let x = ((x - rect.min.x) / rect.width()) * width;
    let y = ((y - rect.min.y) / rect.height()) * height;
    ((0.0..width).contains(&x) && (0.0..height).contains(&y)).then_some(Pos2::new(x, y))
}

fn input_down(ui: &mut Ui, gamepads: &Gamepads, cfg: &Config, input: Input) -> bool {
    ui.input_mut(|i| match input {
        Input::Key(keycode, modifier_state) => key_from_keycode(keycode).map_or(false, |key| {
            let modifiers = modifiers_from_modifiers_state(modifier_state);
            i.key_down(key) && i.modifiers == modifiers
        }),
        Input::Button(player, button) => cfg
            .input
            .gamepad_assigned_to(player)
            .and_then(|uuid| gamepads.gamepad_by_uuid(&uuid))
            .map_or(false, |g| g.is_pressed(button)),
        Input::Mouse(mouse_button) => pointer_button_from_mouse(mouse_button)
            .map_or(false, |pointer| i.pointer.button_down(pointer)),
        // Doesn't make sense for an `Axis` to be `down`
        Input::Axis(..) => false,
    })
}

#[must_use]
pub struct ShortcutWidget<'a, T> {
    inner: T,
    shortcut_text: RichText,
    phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a, T> Deref for ShortcutWidget<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for ShortcutWidget<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T> Widget for ShortcutWidget<'a, T>
where
    T: Widget,
{
    fn ui(self, ui: &mut Ui) -> Response {
        if self.shortcut_text.is_empty() {
            self.inner.ui(ui)
        } else {
            ui.horizontal(|ui| {
                let res = self.inner.ui(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.weak(self.shortcut_text);
                });
                res
            })
            .inner
        }
    }
}

#[must_use]
pub struct ToggleValue<'a> {
    selected: &'a mut bool,
    text: WidgetText,
}

impl<'a> ToggleValue<'a> {
    pub fn new(selected: &'a mut bool, text: impl Into<WidgetText>) -> Self {
        Self {
            selected,
            text: text.into(),
        }
    }
}

impl<'a> Widget for ToggleValue<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut response = ui.selectable_label(*self.selected, self.text);
        if response.clicked() {
            *self.selected = !*self.selected;
            response.mark_changed();
        }
        response
    }
}

#[must_use]
pub struct RadioValue<'a, T> {
    current_value: &'a mut T,
    alternative: T,
    text: WidgetText,
}

impl<'a, T: PartialEq> RadioValue<'a, T> {
    pub fn new(current_value: &'a mut T, alternative: T, text: impl Into<WidgetText>) -> Self {
        Self {
            current_value,
            alternative,
            text: text.into(),
        }
    }
}

impl<'a, T: PartialEq> Widget for RadioValue<'a, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut response = ui.radio(*self.current_value == self.alternative, self.text);
        if response.clicked() && *self.current_value != self.alternative {
            *self.current_value = self.alternative;
            response.mark_changed();
        }
        response
    }
}

impl<'a> ShortcutText<'a> for Checkbox<'a> {}
impl<'a> ShortcutText<'a> for ToggleValue<'a> {}
impl<'a, T> ShortcutText<'a> for RadioValue<'a, T> {}

fn format_input(input: Input) -> String {
    match input {
        Input::Key(keycode, modifiers) => {
            let mut s = String::with_capacity(32);
            if modifiers.contains(ModifiersState::CONTROL) {
                s += "Ctrl";
            }
            if modifiers.contains(ModifiersState::SHIFT) {
                if !s.is_empty() {
                    s += "+";
                }
                s += "Shift";
            }
            if modifiers.contains(ModifiersState::ALT) {
                if !s.is_empty() {
                    s += "+";
                }
                s += "Alt";
            }
            if modifiers.contains(ModifiersState::SUPER) {
                if !s.is_empty() {
                    s += "+";
                }
                s += "Super";
            }
            let ch = match keycode {
                KeyCode::Backquote => "`",
                KeyCode::Backslash | KeyCode::IntlBackslash => "\\",
                KeyCode::BracketLeft => "[",
                KeyCode::BracketRight => "]",
                KeyCode::Comma | KeyCode::NumpadComma => ",",
                KeyCode::Digit0 => "0",
                KeyCode::Digit1 => "1",
                KeyCode::Digit2 => "2",
                KeyCode::Digit3 => "3",
                KeyCode::Digit4 => "4",
                KeyCode::Digit5 => "5",
                KeyCode::Digit6 => "6",
                KeyCode::Digit7 => "7",
                KeyCode::Digit8 => "8",
                KeyCode::Digit9 => "9",
                KeyCode::Equal => "=",
                KeyCode::KeyA => "A",
                KeyCode::KeyB => "B",
                KeyCode::KeyC => "C",
                KeyCode::KeyD => "D",
                KeyCode::KeyE => "E",
                KeyCode::KeyF => "F",
                KeyCode::KeyG => "G",
                KeyCode::KeyH => "H",
                KeyCode::KeyI => "I",
                KeyCode::KeyJ => "J",
                KeyCode::KeyK => "K",
                KeyCode::KeyL => "L",
                KeyCode::KeyM => "M",
                KeyCode::KeyN => "N",
                KeyCode::KeyO => "O",
                KeyCode::KeyP => "P",
                KeyCode::KeyQ => "Q",
                KeyCode::KeyR => "R",
                KeyCode::KeyS => "S",
                KeyCode::KeyT => "T",
                KeyCode::KeyU => "U",
                KeyCode::KeyV => "V",
                KeyCode::KeyW => "W",
                KeyCode::KeyX => "X",
                KeyCode::KeyY => "Y",
                KeyCode::KeyZ => "Z",
                KeyCode::Minus | KeyCode::NumpadSubtract => "-",
                KeyCode::Period | KeyCode::NumpadDecimal => ".",
                KeyCode::Quote => "'",
                KeyCode::Semicolon => ";",
                KeyCode::Slash | KeyCode::NumpadDivide => "/",
                KeyCode::Backspace | KeyCode::NumpadBackspace => "Backspace",
                KeyCode::Enter | KeyCode::NumpadEnter => "Enter",
                KeyCode::Space => "Space",
                KeyCode::Tab => "Tab",
                KeyCode::Delete => "Delete",
                KeyCode::End => "End",
                KeyCode::Help => "Help",
                KeyCode::Home => "Home",
                KeyCode::Insert => "Ins",
                KeyCode::PageDown => "PageDown",
                KeyCode::PageUp => "PageUp",
                KeyCode::ArrowDown => "Down",
                KeyCode::ArrowLeft => "Left",
                KeyCode::ArrowRight => "Right",
                KeyCode::ArrowUp => "Up",
                KeyCode::Numpad0 => "Num0",
                KeyCode::Numpad1 => "Num1",
                KeyCode::Numpad2 => "Num2",
                KeyCode::Numpad3 => "Num3",
                KeyCode::Numpad4 => "Num4",
                KeyCode::Numpad5 => "Num5",
                KeyCode::Numpad6 => "Num6",
                KeyCode::Numpad7 => "Num7",
                KeyCode::Numpad8 => "Num8",
                KeyCode::Numpad9 => "Num9",
                KeyCode::NumpadAdd => "+",
                KeyCode::NumpadEqual => "=",
                KeyCode::NumpadHash => "#",
                KeyCode::NumpadMultiply => "*",
                KeyCode::NumpadParenLeft => "(",
                KeyCode::NumpadParenRight => ")",
                KeyCode::NumpadStar => "*",
                KeyCode::Escape => "Escape",
                KeyCode::Fn => "Fn",
                KeyCode::F1 => "F1",
                KeyCode::F2 => "F2",
                KeyCode::F3 => "F3",
                KeyCode::F4 => "F4",
                KeyCode::F5 => "F5",
                KeyCode::F6 => "F6",
                KeyCode::F7 => "F7",
                KeyCode::F8 => "F8",
                KeyCode::F9 => "F9",
                KeyCode::F10 => "F10",
                KeyCode::F11 => "F11",
                KeyCode::F12 => "F12",
                KeyCode::F13 => "F13",
                KeyCode::F14 => "F14",
                KeyCode::F15 => "F15",
                KeyCode::F16 => "F16",
                KeyCode::F17 => "F17",
                KeyCode::F18 => "F18",
                KeyCode::F19 => "F19",
                KeyCode::F20 => "F20",
                KeyCode::F21 => "F21",
                KeyCode::F22 => "F22",
                KeyCode::F23 => "F23",
                KeyCode::F24 => "F24",
                KeyCode::F25 => "F25",
                KeyCode::F26 => "F26",
                KeyCode::F27 => "F27",
                KeyCode::F28 => "F28",
                KeyCode::F29 => "F29",
                KeyCode::F30 => "F30",
                KeyCode::F31 => "F31",
                KeyCode::F32 => "F32",
                KeyCode::F33 => "F33",
                KeyCode::F34 => "F34",
                KeyCode::F35 => "F35",
                _ => "",
            };
            if !ch.is_empty() {
                if !s.is_empty() {
                    s += "+";
                }
                s += ch;
            }
            s.shrink_to_fit();
            s
        }
        Input::Button(_, button) => format!("{button:#?}"),
        Input::Axis(_, axis) => format!("{axis:#?}"),
        Input::Mouse(button) => match button {
            MouseButton::Left => String::from("Left Click"),
            MouseButton::Right => String::from("Right Click"),
            MouseButton::Middle => String::from("Middle Click"),
            MouseButton::Back => String::from("Back Click"),
            MouseButton::Forward => String::from("Forward Click"),
            MouseButton::Other(id) => format!("Button {id} Click"),
        },
    }
}

impl TryFrom<Input> for KeyboardShortcut {
    type Error = ();

    fn try_from(val: Input) -> Result<Self, Self::Error> {
        if let Input::Key(keycode, modifier_state) = val {
            Ok(KeyboardShortcut {
                logical_key: key_from_keycode(keycode).ok_or(())?,
                modifiers: modifiers_from_modifiers_state(modifier_state),
            })
        } else {
            Err(())
        }
    }
}

impl TryFrom<(Key, Modifiers)> for Input {
    type Error = ();
    fn try_from((key, modifiers): (Key, Modifiers)) -> Result<Self, Self::Error> {
        let keycode = keycode_from_key(key).ok_or(())?;
        let modifiers = modifiers_state_from_modifiers(modifiers);
        Ok(Input::Key(keycode, modifiers))
    }
}

impl From<PointerButton> for Input {
    fn from(button: PointerButton) -> Self {
        Input::Mouse(mouse_button_from_pointer(button))
    }
}

const fn key_from_keycode(keycode: KeyCode) -> Option<Key> {
    Some(match keycode {
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,

        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,

        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,

        // Punctuation
        KeyCode::Space => Key::Space,
        KeyCode::Comma => Key::Comma,
        KeyCode::Period => Key::Period,
        KeyCode::Semicolon => Key::Semicolon,
        KeyCode::Backslash => Key::Backslash,
        KeyCode::Slash | KeyCode::NumpadDivide => Key::Slash,
        KeyCode::BracketLeft => Key::OpenBracket,
        KeyCode::BracketRight => Key::CloseBracket,
        KeyCode::Backquote => Key::Backtick,

        KeyCode::Cut => Key::Cut,
        KeyCode::Copy => Key::Copy,
        KeyCode::Paste => Key::Paste,
        KeyCode::Minus | KeyCode::NumpadSubtract => Key::Minus,
        KeyCode::NumpadAdd => Key::Plus,
        KeyCode::Equal => Key::Equals,

        KeyCode::Digit0 | KeyCode::Numpad0 => Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Key::Num9,

        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,

        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::F13 => Key::F13,
        KeyCode::F14 => Key::F14,
        KeyCode::F15 => Key::F15,
        KeyCode::F16 => Key::F16,
        KeyCode::F17 => Key::F17,
        KeyCode::F18 => Key::F18,
        KeyCode::F19 => Key::F19,
        KeyCode::F20 => Key::F20,
        KeyCode::F21 => Key::F21,
        KeyCode::F22 => Key::F22,
        KeyCode::F23 => Key::F23,
        KeyCode::F24 => Key::F24,
        KeyCode::F25 => Key::F25,
        KeyCode::F26 => Key::F26,
        KeyCode::F27 => Key::F27,
        KeyCode::F28 => Key::F28,
        KeyCode::F29 => Key::F29,
        KeyCode::F30 => Key::F30,
        KeyCode::F31 => Key::F31,
        KeyCode::F32 => Key::F32,
        KeyCode::F33 => Key::F33,
        KeyCode::F34 => Key::F34,
        KeyCode::F35 => Key::F35,

        _ => {
            return None;
        }
    })
}

const fn keycode_from_key(key: Key) -> Option<KeyCode> {
    Some(match key {
        Key::ArrowDown => KeyCode::ArrowDown,
        Key::ArrowLeft => KeyCode::ArrowLeft,
        Key::ArrowRight => KeyCode::ArrowRight,
        Key::ArrowUp => KeyCode::ArrowUp,

        Key::Escape => KeyCode::Escape,
        Key::Tab => KeyCode::Tab,
        Key::Backspace => KeyCode::Backspace,
        Key::Enter => KeyCode::Enter,

        Key::Insert => KeyCode::Insert,
        Key::Delete => KeyCode::Delete,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,

        // Punctuation
        Key::Space => KeyCode::Space,
        Key::Comma => KeyCode::Comma,
        Key::Period => KeyCode::Period,
        Key::Semicolon => KeyCode::Semicolon,
        Key::Backslash => KeyCode::Backslash,
        Key::Slash => KeyCode::Slash,
        Key::OpenBracket => KeyCode::BracketLeft,
        Key::CloseBracket => KeyCode::BracketRight,

        Key::Cut => KeyCode::Cut,
        Key::Copy => KeyCode::Copy,
        Key::Paste => KeyCode::Paste,
        Key::Minus => KeyCode::Minus,
        Key::Plus => KeyCode::NumpadAdd,
        Key::Equals => KeyCode::Equal,

        Key::Num0 => KeyCode::Digit0,
        Key::Num1 => KeyCode::Digit1,
        Key::Num2 => KeyCode::Digit2,
        Key::Num3 => KeyCode::Digit3,
        Key::Num4 => KeyCode::Digit4,
        Key::Num5 => KeyCode::Digit5,
        Key::Num6 => KeyCode::Digit6,
        Key::Num7 => KeyCode::Digit7,
        Key::Num8 => KeyCode::Digit8,
        Key::Num9 => KeyCode::Digit9,

        Key::A => KeyCode::KeyA,
        Key::B => KeyCode::KeyB,
        Key::C => KeyCode::KeyC,
        Key::D => KeyCode::KeyD,
        Key::E => KeyCode::KeyE,
        Key::F => KeyCode::KeyF,
        Key::G => KeyCode::KeyG,
        Key::H => KeyCode::KeyH,
        Key::I => KeyCode::KeyI,
        Key::J => KeyCode::KeyJ,
        Key::K => KeyCode::KeyK,
        Key::L => KeyCode::KeyL,
        Key::M => KeyCode::KeyM,
        Key::N => KeyCode::KeyN,
        Key::O => KeyCode::KeyO,
        Key::P => KeyCode::KeyP,
        Key::Q => KeyCode::KeyQ,
        Key::R => KeyCode::KeyR,
        Key::S => KeyCode::KeyS,
        Key::T => KeyCode::KeyT,
        Key::U => KeyCode::KeyU,
        Key::V => KeyCode::KeyV,
        Key::W => KeyCode::KeyW,
        Key::X => KeyCode::KeyX,
        Key::Y => KeyCode::KeyY,
        Key::Z => KeyCode::KeyZ,

        Key::F1 => KeyCode::F1,
        Key::F2 => KeyCode::F2,
        Key::F3 => KeyCode::F3,
        Key::F4 => KeyCode::F4,
        Key::F5 => KeyCode::F5,
        Key::F6 => KeyCode::F6,
        Key::F7 => KeyCode::F7,
        Key::F8 => KeyCode::F8,
        Key::F9 => KeyCode::F9,
        Key::F10 => KeyCode::F10,
        Key::F11 => KeyCode::F11,
        Key::F12 => KeyCode::F12,
        Key::F13 => KeyCode::F13,
        Key::F14 => KeyCode::F14,
        Key::F15 => KeyCode::F15,
        Key::F16 => KeyCode::F16,
        Key::F17 => KeyCode::F17,
        Key::F18 => KeyCode::F18,
        Key::F19 => KeyCode::F19,
        Key::F20 => KeyCode::F20,
        Key::F21 => KeyCode::F21,
        Key::F22 => KeyCode::F22,
        Key::F23 => KeyCode::F23,
        Key::F24 => KeyCode::F24,
        Key::F25 => KeyCode::F25,
        Key::F26 => KeyCode::F26,
        Key::F27 => KeyCode::F27,
        Key::F28 => KeyCode::F28,
        Key::F29 => KeyCode::F29,
        Key::F30 => KeyCode::F30,
        Key::F31 => KeyCode::F31,
        Key::F32 => KeyCode::F32,
        Key::F33 => KeyCode::F33,
        Key::F34 => KeyCode::F34,
        Key::F35 => KeyCode::F35,

        _ => return None,
    })
}

fn modifiers_from_modifiers_state(modifier_state: ModifiersState) -> Modifiers {
    Modifiers {
        alt: modifier_state.alt_key(),
        ctrl: modifier_state.control_key(),
        shift: modifier_state.shift_key(),
        #[cfg(target_os = "macos")]
        mac_cmd: modifier_state.super_key(),
        #[cfg(not(target_os = "macos"))]
        mac_cmd: false,
        #[cfg(target_os = "macos")]
        command: modifier_state.super_key(),
        #[cfg(not(target_os = "macos"))]
        command: modifier_state.control_key(),
    }
}

fn modifiers_state_from_modifiers(modifiers: Modifiers) -> ModifiersState {
    let mut modifiers_state = ModifiersState::empty();
    if modifiers.shift {
        modifiers_state |= ModifiersState::SHIFT;
    }
    if modifiers.ctrl {
        modifiers_state |= ModifiersState::CONTROL;
    }
    if modifiers.alt {
        modifiers_state |= ModifiersState::ALT;
    }
    #[cfg(target_os = "macos")]
    if modifiers.mac_cmd {
        modifiers_state |= ModifiersState::SUPER;
    }
    // TODO: egui doesn't seem to support SUPER on Windows/Linux
    modifiers_state
}

const fn pointer_button_from_mouse(button: MouseButton) -> Option<PointerButton> {
    Some(match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Extra1,
        MouseButton::Forward => PointerButton::Extra2,
        MouseButton::Other(_) => return None,
    })
}

const fn mouse_button_from_pointer(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Primary => MouseButton::Left,
        PointerButton::Secondary => MouseButton::Right,
        PointerButton::Middle => MouseButton::Middle,
        PointerButton::Extra1 => MouseButton::Back,
        PointerButton::Extra2 => MouseButton::Forward,
    }
}
