use crate::nes::{
    action::Action,
    config::Config,
    event::{ConfigEvent, NesEventProxy},
    input::{Gamepads, Input},
    renderer::gui::lib::ViewportOptions,
};
use egui::{Align2, Button, CentralPanel, Context, Grid, ScrollArea, Ui, Vec2, ViewportClass};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tetanes_core::input::Player;
use tracing::warn;
use uuid::Uuid;
use winit::event::ElementState;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Shortcuts,
    Joypad(Player),
}

#[derive(Debug)]
#[must_use]
pub struct State {
    tx: NesEventProxy,
    tab: Tab,
    pending_input: Option<PendingInput>,
    gamepad_unassign_confirm: Option<(Player, Player, Uuid)>,
}

#[derive(Debug)]
#[must_use]
pub struct Keybinds {
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    resources: Option<(Config, GamepadState)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingInput {
    action: Action,
    input: Option<Input>,
    binding: usize,
    conflict: Option<Action>,
}

#[derive(Debug)]
#[must_use]
pub struct GamepadState {
    input_events: Vec<(Input, ElementState)>,
    connected: Option<Vec<ConnectedGamepad>>,
}

#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub struct ConnectedGamepad {
    pub uuid: Uuid,
    pub name: String,
    pub assignment: Option<Player>,
}

impl Keybinds {
    const TITLE: &'static str = "🖮 Keybinds";

    pub fn new(tx: NesEventProxy) -> Self {
        Self {
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                tx,
                tab: Tab::default(),
                pending_input: None,
                gamepad_unassign_confirm: None,
            })),
            resources: None,
        }
    }

    pub fn wants_input(&self) -> bool {
        self.state.try_lock().map_or(false, |state| {
            state.pending_input.is_some() || state.gamepad_unassign_confirm.is_some()
        })
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

    pub fn prepare(&mut self, gamepads: &Gamepads, cfg: &Config) {
        self.resources = Some((
            cfg.clone(),
            GamepadState {
                input_events: gamepads
                    .events()
                    .filter_map(|event| gamepads.input_from_event(event, cfg))
                    .collect::<Vec<_>>(),
                connected: gamepads.list().map(|gamepad_list| {
                    gamepad_list
                        .map(|(_, gamepad)| {
                            let uuid = Gamepads::create_uuid(&gamepad);
                            ConnectedGamepad {
                                uuid,
                                name: gamepad.name().to_string(),
                                assignment: cfg.input.gamepad_assignment(&uuid),
                            }
                        })
                        .collect::<Vec<_>>()
                }),
            },
        ));
    }

    pub fn show(&mut self, ctx: &Context, opts: ViewportOptions) {
        if !self.open() {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);
        let Some((cfg, gamepad_state)) = self.resources.take() else {
            warn!("Keybinds::prepare was not called with required resources");
            return;
        };

        let viewport_id = egui::ViewportId::from_hash_of("keybinds");
        let mut viewport_builder = egui::ViewportBuilder::default().with_title(Self::TITLE);
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(Keybinds::TITLE)
                    .open(&mut window_open)
                    .default_rect(ctx.available_rect().shrink(16.0))
                    .show(ctx, |ui| {
                        state.lock().ui(ui, opts.enabled, &cfg, &gamepad_state);
                    });
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ctx, |ui| {
                    state.lock().ui(ui, opts.enabled, &cfg, &gamepad_state);
                });
                if ctx.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
            if !open.load(Ordering::Acquire) {
                let mut state = state.lock();
                state.pending_input = None;
                state.gamepad_unassign_confirm = None;
            }
        });
    }
}

impl State {
    fn ui(&mut self, ui: &mut Ui, enabled: bool, cfg: &Config, gamepad_state: &GamepadState) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        self.show_set_keybind_window(ui.ctx(), cfg, &gamepad_state.input_events);
        self.show_gamepad_unassign_window(ui.ctx());

        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Shortcuts, "Shortcuts");
                ui.selectable_value(&mut self.tab, Tab::Joypad(Player::One), "Player1");
                ui.selectable_value(&mut self.tab, Tab::Joypad(Player::Two), "Player2");
                ui.selectable_value(&mut self.tab, Tab::Joypad(Player::Three), "Player3");
                ui.selectable_value(&mut self.tab, Tab::Joypad(Player::Four), "Player4");
            });

            ui.separator();

            match self.tab {
                Tab::Shortcuts => self.list(ui, None, cfg, gamepad_state.connected.as_deref()),
                Tab::Joypad(player) => {
                    self.list(ui, Some(player), cfg, gamepad_state.connected.as_deref())
                }
            }
        });
    }

    fn list(
        &mut self,
        ui: &mut Ui,
        player: Option<Player>,
        cfg: &Config,
        connected_gamepads: Option<&[ConnectedGamepad]>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.set_min_height(ui.available_height());

        if let Some(player) = player {
            self.player_gamepad_combo(ui, player, connected_gamepads);

            ui.separator();
        }

        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let grid = Grid::new("keybind_list")
                .num_columns(4)
                .spacing([10.0, 6.0]);
            grid.show(ui, |ui| {
                ui.heading("Action");
                ui.heading("Binding #1");
                ui.heading("Binding #2");
                ui.heading("Binding #3");
                ui.end_row();

                let keybinds = match player {
                    None => &cfg.input.shortcuts,
                    Some(player) => &cfg.input.joypads[player as usize],
                };

                let mut clear_bind = None;
                for (action, bind) in keybinds {
                    ui.strong(action.to_string());
                    for (slot, input) in bind.bindings.iter().enumerate() {
                        let button = Button::new(input.map(Input::fmt).unwrap_or_default())
                            // Make enough room for larger inputs like controller joysticks
                            .min_size(Vec2::new(135.0, 0.0));
                        let res = ui
                            .add(button)
                            .on_hover_text("Click to set. Right-click to unset.");
                        if res.clicked() {
                            self.pending_input = Some(PendingInput {
                                action: *action,
                                input: None,
                                binding: slot,
                                conflict: None,
                            });
                        } else if res.secondary_clicked() {
                            if let Some(input) = input {
                                clear_bind = Some(input)
                            }
                        }
                    }
                    ui.end_row();
                }
                if let Some(input) = clear_bind.take() {
                    self.tx.event(ConfigEvent::ActionBindingClear(*input));
                }
            });
        });
    }

    fn player_gamepad_combo(
        &mut self,
        ui: &mut Ui,
        player: Player,
        connected_gamepads: Option<&[ConnectedGamepad]>,
    ) {
        ui.horizontal(|ui| {
            let gamepad_label = "🎮 Assigned Gamepad:";

            let unassigned = "Unassigned".to_string();
            match connected_gamepads {
                Some(gamepads) => {
                    if gamepads.is_empty() {
                        ui.add_enabled_ui(false, |ui| {
                            let combo = egui::ComboBox::from_label(gamepad_label)
                                .selected_text("No Gamepads Connected");
                            combo.show_ui(ui, |_| {});
                        });
                    } else {
                        let mut assigned = gamepads
                            .iter()
                            .find(|gamepad| gamepad.assignment == Some(player));
                        let previous_assigned = assigned;
                        let combo = egui::ComboBox::from_label(gamepad_label).selected_text(
                            assigned
                                .as_ref()
                                .map_or(&unassigned, |assignment| &assignment.name),
                        );
                        combo.show_ui(ui, |ui| {
                            ui.selectable_value(&mut assigned, None, unassigned);
                            for assignment in gamepads {
                                ui.selectable_value(
                                    &mut assigned,
                                    Some(assignment),
                                    &assignment.name,
                                );
                            }
                        });
                        if previous_assigned != assigned {
                            match &assigned {
                                Some(gamepad) => {
                                    match assigned.as_ref().and_then(|gamepad| gamepad.assignment) {
                                        Some(player) => {
                                            self.gamepad_unassign_confirm =
                                                Some((player, player, gamepad.uuid));
                                        }
                                        None => {
                                            self.tx.event(ConfigEvent::GamepadAssign((
                                                player,
                                                gamepad.uuid,
                                            )));
                                        }
                                    }
                                }
                                None => self.tx.event(ConfigEvent::GamepadUnassign(player)),
                            }
                        }
                    }
                }
                None => {
                    ui.add_enabled_ui(false, |ui| {
                        let combo = egui::ComboBox::from_label(gamepad_label)
                            .selected_text("Gamepads not supported");
                        combo.show_ui(ui, |_| {});
                    });
                }
            }
        });
    }

    pub fn show_set_keybind_window(
        &mut self,
        ctx: &Context,
        cfg: &Config,
        gamepad_events: &[(Input, ElementState)],
    ) {
        if self.pending_input.is_none() {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut set_keybind_open = self.pending_input.is_some();
        let res = egui::Window::new("🖮 Set Keybind")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut set_keybind_open)
            .show(ctx, |ui| self.set_keybind(ui, cfg, gamepad_events));
        if let Some(ref res) = res {
            // Force on-top focus when embedded
            if set_keybind_open {
                ctx.move_to_top(res.response.layer_id);
                res.response.request_focus();
            } else {
                ctx.memory_mut(|m| m.surrender_focus(res.response.id));
            }
        }
        if !set_keybind_open {
            self.pending_input = None;
        }
    }

    pub fn set_keybind(
        &mut self,
        ui: &mut Ui,
        cfg: &Config,
        gamepad_events: &[(Input, ElementState)],
    ) {
        let Some(PendingInput {
            action,
            binding,
            mut input,
            mut conflict,
            ..
        }) = self.pending_input
        else {
            return;
        };

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if let Some(action) = conflict {
            ui.label(format!("Conflict with {action}."));
            ui.horizontal(|ui| {
                if ui.button("Overwrite").clicked() {
                    conflict = None;
                }
                if ui.button("Cancel").clicked() {
                    self.pending_input = None;
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
                    self.pending_input = None;
                    self.tx
                        .event(ConfigEvent::ActionBindingSet((action, input, binding)));
                }
            }
            None => {
                if let Some(keybind) = &mut self.pending_input {
                    let input = ui.input(|i| {
                        use egui::Event;

                        // Find first released key/button event
                        for event in &i.events {
                            match *event {
                                Event::Key {
                                    physical_key: Some(key),
                                    pressed: false,
                                    modifiers,
                                    ..
                                } => {
                                    // TODO: Ignore unsupported key mappings for now as egui supports less
                                    // overall than winit
                                    return Input::try_from((key, modifiers)).ok();
                                }
                                Event::PointerButton {
                                    button,
                                    pressed: false,
                                    ..
                                } => {
                                    return Some(Input::from(button));
                                }
                                _ => (),
                            }
                        }
                        for (input, state) in gamepad_events {
                            if *state == ElementState::Released {
                                return Some(*input);
                            }
                        }
                        None
                    });

                    if let Some(input) = input {
                        keybind.input = Some(input);
                        let binds = cfg
                            .input
                            .shortcuts
                            .iter()
                            .chain(cfg.input.joypads.iter().flatten());
                        for (action, bind) in binds {
                            if bind
                                .bindings
                                .iter()
                                .any(|b| b == &Some(input) && *action != keybind.action)
                            {
                                keybind.conflict = Some(*action);
                            }
                        }
                    }
                }
            }
        }
    }

    fn show_gamepad_unassign_window(&mut self, ctx: &Context) {
        if self.gamepad_unassign_confirm.is_none() {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut gamepad_unassign_open = self.gamepad_unassign_confirm.is_some();
        let res = egui::Window::new("🎮 Unassign Gamepad")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut gamepad_unassign_open)
            .show(ctx, |ui| self.gamepad_unassign_confirm(ui));
        if let Some(ref res) = res {
            // Force on-top focus when embedded
            if gamepad_unassign_open {
                ctx.move_to_top(res.response.layer_id);
                res.response.request_focus();
            } else {
                ctx.memory_mut(|m| m.surrender_focus(res.response.id));
            }
        }
        if !gamepad_unassign_open {
            self.gamepad_unassign_confirm = None;
        }
    }

    fn gamepad_unassign_confirm(&mut self, ui: &mut Ui) {
        if let Some((existing_player, new_player, uuid)) = self.gamepad_unassign_confirm {
            ui.label(format!("Unassign gamepad from Player {existing_player}?"));
            ui.horizontal(|ui| {
                if ui.button("Yes").clicked() {
                    self.tx.event(ConfigEvent::GamepadUnassign(existing_player));
                    self.tx
                        .event(ConfigEvent::GamepadAssign((new_player, uuid)));
                    self.gamepad_unassign_confirm = None;
                }
                if ui.button("Cancel").clicked() {
                    self.gamepad_unassign_confirm = None;
                }
            });
        }
    }
}
