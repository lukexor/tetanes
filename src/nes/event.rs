use crate::{
    apu::Channel,
    common::{NesRegion, Reset, ResetKind},
    input::{JoypadBtn, JoypadBtnState, Player},
    mapper::MapperRevision,
    nes::{
        menu::{ConfigTab, Menu},
        state::Mode,
        Nes,
    },
    profile, profiling,
    video::VideoFilter,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    ops::{Deref, DerefMut},
};
use winit::{
    event::{ElementState, Event as WinitEvent, Modifiers, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::Fullscreen,
};

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum Event {
    LoadRom((String, Vec<u8>)),
    Pause,
    Terminate,
    // TODO: Verify if DeviceEvent is sufficient or if manual handling is needed
    //     ControllerAxisMotion {
    //         device_id: DeviceId,
    //         axis: AxisId,
    //         value: f64,
    //     },
    //     ControllerInput {
    //         device_id: DeviceId,
    //         button: ControllerButton,
    //         state: ElementState,
    //     },
    //     ControllerUpdate {
    //         device_id: DeviceId,
    //         update: ControllerUpdate,
    //     },
}

#[derive(Default, Debug)]
#[must_use]
pub struct State {
    pub occluded: bool,
    pub modifiers: Modifiers,
    pub quitting: bool,
}

impl Nes {
    pub fn handle_event(
        &mut self,
        event: WinitEvent<Event>,
        window_target: &EventLoopWindowTarget<Event>,
    ) {
        profile!();

        if self.event_state.quitting {
            window_target.exit();
        } else if self.is_paused() || self.event_state.occluded {
            window_target.set_control_flow(ControlFlow::Wait);
        } else {
            window_target.set_control_flow(ControlFlow::Poll);
        }

        let replaying = self.replay_state.is_playing();
        if replaying {
            self.replay_action();
        }

        match event {
            WinitEvent::WindowEvent {
                window_id, event, ..
            } => match event {
                WindowEvent::CloseRequested => {
                    if window_id == self.window.id() {
                        window_target.exit();
                    }
                }
                WindowEvent::Resized(window_size) => {
                    if let Err(err) = self.renderer.resize(window_size.width, window_size.height) {
                        self.handle_error(err);
                    }
                }
                WindowEvent::RedrawRequested => self.draw_frame(),
                WindowEvent::Occluded(occluded) => {
                    if window_id == self.window.id() {
                        self.event_state.occluded = occluded;
                        if let Err(err) = self.renderer.pause(self.event_state.occluded) {
                            self.handle_error(err);
                        }
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if !replaying => {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        self.handle_input(
                            Input::Key(key, self.event_state.modifiers.state()),
                            event.state,
                            event.repeat,
                        );
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => self.event_state.modifiers = modifiers,
                WindowEvent::MouseInput { button, state, .. } if !replaying => {
                    self.handle_input(Input::Mouse(button, state), state, false);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    // Aim zapper
                    if self.config.zapper {
                        let x = (position.x / self.config.scale as f64) * 8.0 / 7.0 + 0.5; // Adjust ratio
                        let mut y = position.y / self.config.scale as f64;
                        // Account for trimming top 8 scanlines
                        if self.config.region.is_ntsc() {
                            y += 8.0;
                        };
                        self.control_deck
                            .aim_zapper(x.round() as i32, y.round() as i32);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                WindowEvent::DroppedFile(rom) => {
                    if self.control_deck.loaded_rom().is_some() {
                        self.mixer.pause();
                        if let Err(err) = self.save_sram() {
                            log::error!("failed to save sram: {err:?}");
                        }
                        if self.replay_state.is_recording() {
                            self.stop_replay();
                        }
                        if self.config.save_on_exit {
                            self.save_state(self.config.save_slot);
                        }
                    }
                    self.load_rom_path(rom);
                }
                _ => {}
            },
            WinitEvent::AboutToWait => self.next_frame(window_target),
            WinitEvent::UserEvent(event) => match event {
                Event::LoadRom((name, rom)) => {
                    self.load_rom(&name, &mut io::Cursor::new(rom));
                    #[cfg(target_arch = "wasm32")]
                    {
                        use winit::platform::web::WindowExtWebSys;
                        let _ = self.window.canvas().map(|canvas| canvas.focus());
                    }
                }
                Event::Pause => self.pause(true),
                Event::Terminate => self.quit(),
            },
            // TODO: Controller support
            // Event::UserEvent(event) => match event {
            //     CustomEvent::ControllerAxisMotion {
            //         device_id,
            //         axis,
            //         value,
            //         ..
            //     } => {
            //         self.handle_controller_axis_motion(device_id, axis, value);
            //     }
            //     CustomEvent::ControllerInput {
            //         device_id,
            //         button,
            //         state,
            //         ..
            //     } => {
            //         self.handle_controller_event(device_id, button, state);
            //     }
            //     CustomEvent::ControllerUpdate {
            //         device_id, update, ..
            //     } => {
            //         self.handle_controller_update(device_id, button, state);
            //     }
            // },
            WinitEvent::LoopExiting => self.handle_exit(),
            _ => {}
        }
    }

    pub fn handle_input(&mut self, input: Input, state: ElementState, repeat: bool) {
        if let Some((player, action)) = self.config.input_map.get(&input).copied() {
            self.handle_action(player, action, state, repeat);
        }
    }

    pub fn handle_action(
        &mut self,
        player: Player,
        action: Action,
        state: ElementState,
        repeat: bool,
    ) {
        log::trace!("player: {player:?}, action: {action:?}, state: {state:?}, repeat: {repeat:?}");
        let released = state == ElementState::Released;
        match action {
            Action::Nes(nes_state) => self.handle_nes_action(nes_state, state),
            Action::Menu(menu) if released => self.toggle_menu(menu),
            Action::Feature(feature) => self.handle_feature_action(feature, state, repeat),
            Action::Setting(setting) => self.handle_setting_action(setting, state, repeat),
            Action::Joypad(button) => self.handle_joypad_action(player, button, state),
            Action::ZapperTrigger => self.handle_zapper_trigger_action(),
            Action::Debug(action) => self.handle_debug_action(action, state, repeat),
            _ => (),
        }

        if self.replay_state.is_recording() {
            self.replay_state.buffer.push(ActionEvent {
                frame: self.control_deck.frame_number(),
                player,
                action,
                state,
                repeat,
            });
        }
    }

    fn handle_nes_action(&mut self, nes_state: NesState, state: ElementState) {
        if state != ElementState::Released || self.replay_state.is_recording() {
            return;
        }
        match nes_state {
            NesState::Quit => {
                self.pause(true);
                self.event_state.quitting = true;
            }
            NesState::TogglePause => self.toggle_pause(),
            NesState::SoftReset => {
                self.error = None;
                self.control_deck.reset(ResetKind::Soft);
                self.add_message("Reset");
            }
            NesState::HardReset => {
                self.error = None;
                self.control_deck.reset(ResetKind::Hard);
                self.add_message("Power Cycled");
            }
            NesState::MapperRevision(_) => todo!("mapper revision"),
        }
    }

    fn handle_feature_action(&mut self, feature: Feature, state: ElementState, repeat: bool) {
        let released = state == ElementState::Released;
        match feature {
            Feature::ToggleGameplayRecording => {
                if self.replay_state.is_recording() || self.replay_state.is_playing() {
                    self.stop_replay();
                } else {
                    self.start_replay();
                }
            }
            Feature::ToggleSoundRecording => self.toggle_sound_recording(),
            Feature::TakeScreenshot => self.save_screenshot(),
            Feature::SaveState => self.save_state(self.config.save_slot),
            Feature::LoadState => self.load_state(self.config.save_slot),
            Feature::Rewind => {
                if repeat {
                    if self.config.rewind {
                        self.mode = Mode::Rewind;
                    } else {
                        self.add_message("Rewind disabled. You can enable it in the Config menu.");
                    }
                } else if released {
                    if self.is_rewinding() {
                        self.pause(false);
                    } else {
                        self.instant_rewind();
                    }
                }
            }
        }
    }

    fn handle_setting_action(&mut self, setting: Setting, state: ElementState, _repeat: bool) {
        let released = state != ElementState::Pressed;
        match setting {
            Setting::SetSaveSlot(slot) if released => {
                self.config.save_slot = slot;
                self.add_message(&format!("Set Save Slot to {slot}"));
            }
            Setting::ToggleFullscreen if released => {
                self.config.fullscreen = !self.config.fullscreen;
                self.window.set_fullscreen(
                    self.config
                        .fullscreen
                        .then_some(Fullscreen::Borderless(None)),
                );
            }
            // Vsync is always on in wasm
            Setting::ToggleVsync if released => {
                #[cfg(not(target_arch = "wasm32"))]
                self.set_vsync(self.config.vsync);
            }
            Setting::ToggleNtscFilter if released => {
                self.config.filter = match self.config.filter {
                    VideoFilter::Pixellate => VideoFilter::Ntsc,
                    VideoFilter::Ntsc => VideoFilter::Pixellate,
                };
                self.control_deck.set_filter(self.config.filter);
            }
            Setting::ToggleSound if released => {
                self.config.audio_enabled = !self.config.audio_enabled;
                self.mixer.set_enabled(self.config.audio_enabled);
                if self.config.audio_enabled {
                    self.add_message("Sound Enabled");
                } else {
                    self.add_message("Sound Disabled");
                }
            }
            Setting::TogglePulse1 if released => self.control_deck.toggle_channel(Channel::Pulse1),
            Setting::TogglePulse2 if released => self.control_deck.toggle_channel(Channel::Pulse2),
            Setting::ToggleTriangle if released => {
                self.control_deck.toggle_channel(Channel::Triangle)
            }
            Setting::ToggleNoise if released => self.control_deck.toggle_channel(Channel::Noise),
            Setting::ToggleDmc if released => self.control_deck.toggle_channel(Channel::Dmc),
            Setting::IncSpeed if released => self.change_speed(0.25),
            Setting::DecSpeed if released => self.change_speed(-0.25),
            Setting::FastForward => {
                if released {
                    self.set_speed(1.0);
                } else {
                    self.set_speed(2.0);
                }
            }
            _ => (),
        }
    }

    fn handle_joypad_action(&mut self, player: Player, button: JoypadBtn, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        let joypad = self.control_deck.joypad_mut(player);
        if !self.config.concurrent_dpad && pressed {
            match button {
                JoypadBtn::Left => joypad.set_button(JoypadBtnState::RIGHT, false),
                JoypadBtn::Right => joypad.set_button(JoypadBtnState::LEFT, false),
                JoypadBtn::Up => joypad.set_button(JoypadBtnState::DOWN, false),
                JoypadBtn::Down => joypad.set_button(JoypadBtnState::UP, false),
                _ => (),
            }
        }
        joypad.set_button(button.into(), pressed);
    }

    #[inline]
    fn handle_zapper_trigger_action(&mut self) {
        self.control_deck.trigger_zapper();
    }

    fn handle_debug_action(&mut self, action: DebugAction, state: ElementState, _repeat: bool) {
        if state != ElementState::Released {
            return;
        }
        match action {
            // DebugAction::ToggleCpuDebugger if !repeat => self.toggle_debugger()?,
            // DebugAction::TogglePpuDebugger if !repeat => self.toggle_ppu_viewer()?,
            // DebugAction::ToggleApuDebugger if !repeat => self.toggle_apu_viewer()?,
            // DebugAction::StepInto if debugging => self.debug_step_into()?,
            // DebugAction::StepOver if debugging => self.debug_step_over()?,
            // DebugAction::StepOut if debugging => self.debug_step_out()?,
            // DebugAction::StepFrame if debugging => self.debug_step_frame()?,
            // DebugAction::StepScanline if debugging => self.debug_step_scanline()?,
            DebugAction::IncScanline => {
                // TODO: add ppu viewer
                // if let Some(ref mut viewer) = self.ppu_viewer {
                // TODO: check keydown
                // let increment = if s.keymod_down(ModifiersState::SHIFT) { 10 } else { 1 };
                // viewer.inc_scanline(increment);
            }
            DebugAction::DecScanline => {
                // TODO: add ppu viewer
                // if let Some(ref mut viewer) = self.ppu_viewer {
                // TODO: check keydown
                // let decrement = if s.keymod_down(ModifiersState::SHIFT) { 10 } else { 1 };
                // viewer.dec_scanline(decrement);
            }
            _ => (),
        }
    }

    /// Handle saving and exiting.
    fn handle_exit(&mut self) {
        log::info!("exiting...");
        profiling::disable();
        if self.is_playing() {
            self.pause(true);
            if let Err(err) = self.save_sram() {
                log::error!("failed to save sram: {err:?}");
            }
            if self.config.save_on_exit {
                self.save_state(self.config.save_slot);
            }
        }
        self.config.save();
    }

    /// Quit the application.
    pub fn quit(&mut self) {
        self.event_state.quitting = true;
    }
}

// #[derive(Debug, Copy, Clone, PartialEq)]
// #[must_use]
// pub struct DeviceId(usize);

// #[derive(Debug, Copy, Clone, PartialEq, Eq)]
// #[must_use]
// pub enum ControllerButton {
//     Todo,
// }

// #[derive(Debug, Copy, Clone, PartialEq, Eq)]
// #[must_use]
// pub enum ControllerUpdate {
//     Added,
//     Removed,
// }

// /// Indicates an [Axis] direction.
// #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// #[must_use]
// pub enum AxisDirection {
//     /// No direction, axis is in a deadzone/not pressed.
//     None,
//     /// Positive (Right or Down)
//     Positive,
//     /// Negative (Left or Up)
//     Negative,
// }

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct ActionEvent {
    pub frame: u32,
    pub player: Player,
    pub action: Action,
    pub state: ElementState,
    pub repeat: bool,
}

macro_rules! key_map {
    ($map:expr, $player:expr, $key:expr, $action:expr) => {
        $map.insert(
            Input::Key($key, ModifiersState::empty()),
            ($player, $action.into()),
        );
    };
    ($map:expr, $player:expr, $key:expr, $modifiers:expr, $action:expr) => {
        $map.insert(Input::Key($key, $modifiers), ($player, $action.into()));
    };
}

macro_rules! mouse_map {
    ($map:expr, $player:expr, $button:expr, $action:expr) => {
        $map.insert(
            Input::Mouse($button, ElementState::Released),
            ($player, $action.into()),
        );
    };
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Input {
    Key(KeyCode, ModifiersState),
    Mouse(MouseButton, ElementState),
    // ControllerBtn(InputControllerBtn),
    // ControllerAxis(InputControllerAxis),
}

pub type InputBinding = (Input, Player, Action);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputMap(HashMap<Input, (Player, Action)>);

impl InputMap {
    pub fn from_bindings(bindings: &[InputBinding]) -> Self {
        let mut map = HashMap::with_capacity(bindings.len());
        for (input, player, action) in bindings {
            map.insert(*input, (*player, *action));
        }
        map.shrink_to_fit();
        Self(map)
    }
}

impl Deref for InputMap {
    type Target = HashMap<Input, (Player, Action)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for InputMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for InputMap {
    fn default() -> Self {
        use KeyCode::*;
        use Player::*;
        const SHIFT: ModifiersState = ModifiersState::SHIFT;
        const CONTROL: ModifiersState = ModifiersState::CONTROL;

        let mut map = HashMap::new();

        key_map!(map, One, ArrowLeft, JoypadBtn::Left);
        key_map!(map, One, ArrowRight, JoypadBtn::Right);
        key_map!(map, One, ArrowUp, JoypadBtn::Up);
        key_map!(map, One, ArrowDown, JoypadBtn::Down);
        key_map!(map, One, KeyZ, JoypadBtn::A);
        key_map!(map, One, KeyX, JoypadBtn::B);
        key_map!(map, One, KeyA, JoypadBtn::TurboA);
        key_map!(map, One, KeyS, JoypadBtn::TurboB);
        key_map!(map, One, Enter, JoypadBtn::Start);
        key_map!(map, One, ShiftRight, JoypadBtn::Select);
        key_map!(map, One, ShiftLeft, JoypadBtn::Select);
        key_map!(map, Two, KeyJ, JoypadBtn::Left);
        key_map!(map, Two, KeyL, JoypadBtn::Right);
        key_map!(map, Two, KeyI, JoypadBtn::Up);
        key_map!(map, Two, KeyK, JoypadBtn::Down);
        key_map!(map, Two, KeyN, JoypadBtn::A);
        key_map!(map, Two, KeyM, JoypadBtn::B);
        key_map!(map, Two, Numpad8, JoypadBtn::Start);
        key_map!(map, Two, Numpad9, SHIFT, JoypadBtn::Select);
        key_map!(map, Three, KeyF, JoypadBtn::Left);
        key_map!(map, Three, KeyH, JoypadBtn::Right);
        key_map!(map, Three, KeyT, JoypadBtn::Up);
        key_map!(map, Three, KeyG, JoypadBtn::Down);
        key_map!(map, Three, KeyV, JoypadBtn::A);
        key_map!(map, Three, KeyB, JoypadBtn::B);
        key_map!(map, Three, Numpad5, JoypadBtn::Start);
        key_map!(map, Three, Numpad6, SHIFT, JoypadBtn::Select);
        key_map!(map, One, Escape, NesState::TogglePause);
        key_map!(map, One, KeyH, CONTROL, Menu::About);
        key_map!(map, One, F1, Menu::About);
        key_map!(map, One, KeyC, CONTROL, Menu::Config(ConfigTab::General));
        key_map!(map, One, F2, Menu::Config(ConfigTab::General));
        key_map!(map, One, KeyO, CONTROL, Menu::LoadRom);
        key_map!(map, One, F3, Menu::LoadRom);
        key_map!(map, One, KeyK, CONTROL, Menu::Keybind(Player::One));
        key_map!(map, One, KeyQ, CONTROL, NesState::Quit);
        key_map!(map, One, KeyR, CONTROL, NesState::SoftReset);
        key_map!(map, One, KeyP, CONTROL, NesState::HardReset);
        key_map!(map, One, Equal, CONTROL, Setting::IncSpeed);
        key_map!(map, One, Minus, CONTROL, Setting::DecSpeed);
        key_map!(map, One, Space, Setting::FastForward);
        key_map!(map, One, Digit1, CONTROL, Setting::SetSaveSlot(1));
        key_map!(map, One, Digit2, CONTROL, Setting::SetSaveSlot(2));
        key_map!(map, One, Digit3, CONTROL, Setting::SetSaveSlot(3));
        key_map!(map, One, Digit4, CONTROL, Setting::SetSaveSlot(4));
        key_map!(map, One, Numpad1, CONTROL, Setting::SetSaveSlot(1));
        key_map!(map, One, Numpad2, CONTROL, Setting::SetSaveSlot(2));
        key_map!(map, One, Numpad3, CONTROL, Setting::SetSaveSlot(3));
        key_map!(map, One, Numpad4, CONTROL, Setting::SetSaveSlot(4));
        key_map!(map, One, KeyS, CONTROL, Feature::SaveState);
        key_map!(map, One, KeyL, CONTROL, Feature::LoadState);
        key_map!(map, One, KeyR, Feature::Rewind);
        key_map!(map, One, F10, Feature::TakeScreenshot);
        key_map!(map, One, KeyV, SHIFT, Feature::ToggleGameplayRecording);
        key_map!(map, One, KeyR, SHIFT, Feature::ToggleSoundRecording);
        key_map!(map, One, KeyM, CONTROL, Setting::ToggleSound);
        key_map!(map, One, Numpad1, SHIFT, Setting::TogglePulse1);
        key_map!(map, One, Numpad2, SHIFT, Setting::TogglePulse2);
        key_map!(map, One, Numpad3, SHIFT, Setting::ToggleTriangle);
        key_map!(map, One, Numpad4, SHIFT, Setting::ToggleNoise);
        key_map!(map, One, Numpad5, SHIFT, Setting::ToggleDmc);
        key_map!(map, One, Enter, CONTROL, Setting::ToggleFullscreen);
        key_map!(map, One, KeyV, CONTROL, Setting::ToggleVsync);
        key_map!(map, One, KeyN, CONTROL, Setting::ToggleNtscFilter);
        key_map!(map, One, KeyD, SHIFT, DebugAction::ToggleCpuDebugger);
        key_map!(map, One, KeyP, SHIFT, DebugAction::TogglePpuDebugger);
        key_map!(map, One, KeyA, SHIFT, DebugAction::ToggleApuDebugger);
        key_map!(map, One, KeyC, DebugAction::StepInto);
        key_map!(map, One, KeyO, DebugAction::StepOver);
        key_map!(map, One, KeyO, SHIFT, DebugAction::StepOut);
        key_map!(map, One, KeyL, SHIFT, DebugAction::StepScanline);
        key_map!(map, One, KeyF, SHIFT, DebugAction::StepFrame);
        key_map!(map, One, ArrowDown, CONTROL, DebugAction::IncScanline);
        key_map!(map, One, ArrowUp, CONTROL, DebugAction::DecScanline);
        key_map!(
            map,
            One,
            ArrowDown,
            SHIFT | CONTROL,
            DebugAction::IncScanline
        );
        key_map!(map, One, ArrowUp, SHIFT | CONTROL, DebugAction::DecScanline);

        mouse_map!(map, Two, MouseButton::Left, Action::ZapperTrigger);

        // TODO: controller bindings
        // controller_bind!(One, ControllerButton::DPadLeft, JoypadBtn::Left),
        // controller_bind!(One, ControllerButton::DPadRight, JoypadBtn::Right),
        // controller_bind!(One, ControllerButton::DPadUp, JoypadBtn::Up),
        // controller_bind!(One, ControllerButton::DPadDown, JoypadBtn::Down),
        // controller_bind!(One, ControllerButton::A, JoypadBtn::A),
        // controller_bind!(One, ControllerButton::B, JoypadBtn::B),
        // controller_bind!(One, ControllerButton::X, JoypadBtn::TurboA),
        // controller_bind!(One, ControllerButton::Y, JoypadBtn::TurboB),
        // controller_bind!(One, ControllerButton::Guide, Menu::Main),
        // controller_bind!(One, ControllerButton::Start, JoypadBtn::Start),
        // controller_bind!(One, ControllerButton::Back, JoypadBtn::Select),
        // controller_bind!(One, ControllerButton::RightShoulder, Setting::IncSpeed),
        // controller_bind!(One, ControllerButton::LeftShoulder, Setting::DecSpeed),
        // controller_axis_bind!(One, Axis::LeftX, Direction::Negative, JoypadBtn::Left),
        // controller_axis_bind!(One, Axis::LeftX, Direction::Positive, JoypadBtn::Right),
        // controller_axis_bind!(One, Axis::LeftY, Direction::Negative, JoypadBtn::Up),
        // controller_axis_bind!(One, Axis::LeftY, Direction::Positive, JoypadBtn::Down),
        // controller_axis_bind!(
        //     One,
        //     Axis::TriggerLeft,
        //     Direction::Positive,
        //     Feature::SaveState
        // ),
        // controller_axis_bind!(
        //     One,
        //     Axis::TriggerRight,
        //     Direction::Positive,
        //     Feature::LoadState
        // ),

        Self(map)
    }
}

#[allow(variant_size_differences)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Nes(NesState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Joypad(JoypadBtn),
    ZapperTrigger,
    Debug(DebugAction),
}

impl From<NesState> for Action {
    fn from(state: NesState) -> Self {
        Self::Nes(state)
    }
}

impl From<Menu> for Action {
    fn from(menu: Menu) -> Self {
        Self::Menu(menu)
    }
}

impl From<Feature> for Action {
    fn from(feature: Feature) -> Self {
        Self::Feature(feature)
    }
}

impl From<Setting> for Action {
    fn from(setting: Setting) -> Self {
        Self::Setting(setting)
    }
}

impl From<JoypadBtn> for Action {
    fn from(btn: JoypadBtn) -> Self {
        Self::Joypad(btn)
    }
}

impl From<DebugAction> for Action {
    fn from(action: DebugAction) -> Self {
        Self::Debug(action)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NesState {
    Quit,
    TogglePause,
    SoftReset,
    HardReset,
    MapperRevision(MapperRevision),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Feature {
    ToggleGameplayRecording,
    ToggleSoundRecording,
    Rewind,
    TakeScreenshot,
    SaveState,
    LoadState,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Setting {
    SetSaveSlot(u8),
    ToggleFullscreen,
    ToggleVsync,
    ToggleNtscFilter,
    SetVideoFilter(VideoFilter),
    SetNesFormat(NesRegion),
    ToggleSound,
    TogglePulse1,
    TogglePulse2,
    ToggleTriangle,
    ToggleNoise,
    ToggleDmc,
    FastForward,
    IncSpeed,
    DecSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DebugAction {
    ToggleCpuDebugger,
    TogglePpuDebugger,
    ToggleApuDebugger,
    StepInto,
    StepOver,
    StepOut,
    StepFrame,
    StepScanline,
    IncScanline,
    DecScanline,
}

// const fn render_message(_message: &str, _color: Color) {
//     // TODO: switch to egui
//     // s.push();
//     // s.stroke(None);
//     // s.fill(rgb!(0, 200));
//     // let pady = s.theme().spacing.frame_pad.y();
//     // let width = s.width()?;
//     // s.wrap(width);
//     // let (_, height) = s.size_of(message)?;
//     // s.rect([
//     //     0,
//     //     s.cursor_pos().y() - pady,
//     //     width as i32,
//     //     height as i32 + 2 * pady,
//     // ])?;
//     // s.fill(color);
//     // s.text(message)?;
//     // s.pop();
// }

// impl Nes {
//     #[inline]
//     pub fn handle_controller_update(&mut self, device_id: DeviceId, update: ControllerUpdate) {
//         match update {
//             ControllerUpdate::Added => {
//                 for player in [Slot::One, Slot::Two, Slot::Three, Slot::Four] {
//                     let player_idx = player as usize;
//                     if self.controllers[player_idx].is_none() {
//                         self.add_message(format!("Controller {} connected.", player_idx + 1));
//                         self.controllers[player_idx] = Some(device_id);
//                     }
//                 }
//             }
//             ControllerUpdate::Removed => {
//                 if let Some(player) = self.get_controller_player(device_id) {
//                     let player_idx = player as usize;
//                     self.controllers[player_idx] = None;
//                     self.add_message(format!("Controller {} disconnected.", player_idx + 1));
//                 }
//             }
//         }
//     }

//     #[inline]
//     pub fn handle_controller_event(
//         &mut self,
//         device_id: DeviceId,
//         button_id: ButtonId,
//         pressed: bool,
//     ) {
//         if let Some(player) = self.get_controller_player(device_id) {
//             self.handle_input(
//                 player,
//                 Input::ControllerBtn(InputControllerBtn::new(player, button_id)),
//                 pressed,
//                 false,
//             );
//         }
//     }

//     #[inline]
//     pub fn handle_controller_axis_motion(&mut self, device_id: DeviceId, axis: AxisId, value: f64) {
//         if let Some(player) = self.get_controller_player(device_id) {
//             let direction = if value < self.config.controller_deadzone {
//                 AxisDirection::Negative
//             } else if value > self.config.controller_deadzone {
//                 AxisDirection::Positive
//             } else {
//                 // TODO: verify if this is correct
//                 for button in [
//                     JoypadBtn::Left,
//                     JoypadBtn::Right,
//                     JoypadBtn::Up,
//                     JoypadBtn::Down,
//                 ] {
//                     self.handle_joypad_pressed(player, button, false);
//                 }
//                 return;
//             };
//             self.handle_input(
//                 player,
//                 Input::ControllerAxis(InputControllerAxis::new(player, axis, direction)),
//                 true,
//                 false,
//             );
//         }
//     }

// }

// impl Nes {
//     #[inline]
//     fn get_controller_player(&self, device_id: DeviceId) -> Option<Slot> {
//         self.controllers.iter().enumerate().find_map(|(player, id)| {
//             (*id == Some(device_id)).then_some(Slot::try_from(player).expect("valid player index"))
//         })
//     }

//     // fn debug_step_into(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     if let Err(err) = self.control_deck.clock_instr() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }

//     // fn next_instr(&mut self) -> Instr {
//     //     let pc = self.control_deck.cpu().pc();
//     //     let opcode = self.control_deck.cpu().peek(pc, Access::Dummy);
//     //     Cpu::INSTRUCTIONS[opcode as usize]
//     // }

//     // fn debug_step_over(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     let instr = self.next_instr();
//     //     if let Err(err) = self.control_deck.clock_instr() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     //     if instr.op() == Operation::JSR {
//     //         let rti_addr = self.control_deck.cpu().peek_stack_u16().wrapping_add(1);
//     //         while self.control_deck.cpu().pc() != rti_addr {
//     //             if let Err(err) = self.control_deck.clock_instr() {
//     //                 self.handle_emulation_error(&err);
//     //                 break;
//     //             }
//     //         }
//     //     }
//     // }

//     // fn debug_step_out(&mut self) {
//     //     let mut instr = self.next_instr();
//     //     while !matches!(instr.op(), Operation::RTS | Operation::RTI) {
//     //         if let Err(err) = self.control_deck.clock_instr() {
//     //             self.handle_emulation_error(&err);
//     //             break;
//     //         }
//     //         instr = self.next_instr();
//     //     }
//     //     if let Err(err) = self.control_deck.clock_instr() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }

//     // fn debug_step_frame(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     if let Err(err) = self.control_deck.clock_frame() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }

//     // fn debug_step_scanline(&mut self) {
//     //     self.pause_play(PauseMode::Manual);
//     //     if let Err(err) = self.control_deck.clock_scanline() {
//     //         self.handle_emulation_error(&err);
//     //     }
//     // }
// }
