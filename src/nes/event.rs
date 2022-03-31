use crate::{
    apu::AudioChannel,
    common::{Clocked, Powered},
    cpu::instr::Operation,
    input::{GamepadBtn, GamepadSlot},
    nes::{config::DEFAULT_KEYBINDS, menu::Menu, Mode, Nes, NesResult},
    ppu::{VideoFormat, RENDER_HEIGHT},
};
use anyhow::Context;
use chrono::Local;
use log::{debug, info};
use pix_engine::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt,
    fs::File,
    io::BufReader,
    ops::{Deref, DerefMut},
    path::Path,
    time::{Duration, Instant},
};

/// Indicates an [Axis] direction.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum AxisDirection {
    /// No direction, axis is in a deadzone/not pressed.
    None,
    /// Positive (Right or Down)
    Positive,
    /// Negative (Left or Up)
    Negative,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Input {
    Key((GamepadSlot, Key, KeyMod)),
    Button((GamepadSlot, ControllerButton)),
    Axis((GamepadSlot, Axis, AxisDirection)),
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::Key((_, key, keymod)) => {
                if keymod.is_empty() {
                    write!(f, "{:?}", key)
                } else {
                    write!(f, "{:?} {:?}", keymod, key)
                }
            }
            Input::Button((_, btn)) => {
                write!(f, "{:?}", btn)
            }
            Input::Axis((_, axis, _)) => {
                write!(f, "{:?}", axis)
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct KeyBinding {
    player: GamepadSlot,
    key: Key,
    keymod: KeyMod,
    action: Action,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ControllerButtonBinding {
    player: GamepadSlot,
    button: ControllerButton,
    action: Action,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ControllerAxisBinding {
    player: GamepadSlot,
    axis: Axis,
    direction: AxisDirection,
    action: Action,
}

/// A binding of a [`KeyInput`] or [`ControllerInput`] to an [Action].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct InputBinds {
    pub(crate) keys: Vec<KeyBinding>,
    pub(crate) buttons: Vec<ControllerButtonBinding>,
    pub(crate) axes: Vec<ControllerAxisBinding>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputBindings(HashMap<Input, Action>);

impl InputBindings {
    pub(crate) fn from_file<P: AsRef<Path>>(path: P) -> NesResult<Self> {
        let path = path.as_ref();
        let file =
            BufReader::new(File::open(path).with_context(|| format!("`{}`", path.display()))?);

        let input_binds: InputBinds = serde_json::from_reader(file)
            .or_else(|err| {
                log::error!(
                    "Invalid `{}`, reverting to defaults. Error: {}",
                    path.display(),
                    err
                );
                serde_json::from_reader(DEFAULT_KEYBINDS)
            })
            .with_context(|| format!("failed to parse `{}`", path.display()))?;

        let mut bindings = HashMap::new();
        for bind in input_binds.keys {
            bindings.insert(
                Input::Key((bind.player, bind.key, bind.keymod)),
                bind.action,
            );
        }
        for bind in input_binds.buttons {
            bindings.insert(Input::Button((bind.player, bind.button)), bind.action);
        }
        for bind in input_binds.axes {
            bindings.insert(
                Input::Axis((bind.player, bind.axis, bind.direction)),
                bind.action,
            );
        }

        Ok(Self(bindings))
    }
}

impl Deref for InputBindings {
    type Target = HashMap<Input, Action>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for InputBindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Action {
    Nes(NesState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Gamepad(GamepadBtn),
    ZeroAxis([GamepadBtn; 2]),
    Debug(DebugAction),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum NesState {
    ToggleMenu,
    Quit,
    TogglePause,
    Reset,
    PowerCycle,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Feature {
    ToggleGameplayRecording,
    ToggleSoundRecording,
    Rewind,
    TakeScreenshot,
    SaveState,
    LoadState,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Setting {
    SetSaveSlot(u8),
    ToggleFullscreen,
    ToggleVsync,
    ToggleNtscFilter,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum DebugAction {
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

impl Nes {
    pub(crate) fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        info!("{}", text);
        self.messages.push((text, Instant::now()));
    }

    pub(crate) fn render_messages(&mut self, s: &mut PixState) -> NesResult<()> {
        self.messages
            .retain(|(_, created)| created.elapsed() < Duration::from_secs(3));
        self.messages.dedup();
        s.push();
        s.stroke(None);
        for (message, _) in &self.messages {
            s.fill(rgb!(0, 200));
            s.rect([
                0,
                s.cursor_pos().y() - s.theme().spacing.frame_pad.y(),
                s.width()? as i32,
                34,
            ])?;
            s.fill(Color::WHITE);
            s.text(message)?;
        }
        s.pop();
        Ok(())
    }

    pub(crate) fn render_status(&mut self, s: &mut PixState, status: &str) -> PixResult<()> {
        s.push();
        s.stroke(None);
        s.fill(rgb!(0, 200));
        s.rect([
            0,
            s.cursor_pos().y() - s.theme().spacing.frame_pad.y(),
            s.width()? as i32,
            34,
        ])?;
        s.fill(Color::WHITE);
        s.text(status)?;
        if let Some(ref err) = self.error {
            s.fill(Color::RED);
            s.text(err)?;
        }
        s.pop();
        Ok(())
    }

    #[inline]
    pub(crate) fn handle_key_event(
        &mut self,
        s: &mut PixState,
        event: KeyEvent,
        pressed: bool,
    ) -> bool {
        for slot in [
            GamepadSlot::One,
            GamepadSlot::Two,
            GamepadSlot::Three,
            GamepadSlot::Four,
        ] {
            let input = Input::Key((slot, event.key, event.keymod));
            if let Ok(true) = self
                .config
                .input_bindings
                .get(&input)
                .copied()
                .map_or(Ok(false), |action| {
                    self.handle_input_action(s, slot, action, pressed, event.repeat)
                })
            {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn handle_mouse_event(
        &mut self,
        s: &mut PixState,
        btn: Mouse,
        _pos: Point<i32>,
        clicked: bool,
    ) {
        if self.mode == Mode::Playing && clicked && btn == Mouse::Left {
            if let Some(view) = self.emulation {
                if s.focused_window(view.window_id) {
                    self.control_deck.zapper_mut().trigger();
                }
            }
        }
    }

    #[inline]
    pub fn handle_mouse_motion(&mut self, s: &mut PixState, pos: Point<i32>) {
        if self.mode == Mode::Playing {
            if let Some(view) = self.emulation {
                if s.focused_window(view.window_id) {
                    let mut zapper = self.control_deck.zapper_mut();
                    let mut pos = pos / self.config.scale as i32;
                    pos.set_x((pos.x() as f32 * 7.0 / 8.0) as i32); // Adjust ratio
                    zapper.pos = pos;
                }
            }
        }
    }

    #[inline]
    pub(crate) fn handle_controller_event(
        &mut self,
        s: &mut PixState,
        event: ControllerEvent,
        pressed: bool,
    ) -> PixResult<bool> {
        if let Some(slot) = self.get_controller_slot(event.controller_id) {
            let input = Input::Button((slot, event.button));
            self.config
                .input_bindings
                .get(&input)
                .copied()
                .map_or(Ok(false), |action| {
                    self.handle_input_action(s, slot, action, pressed, false)
                })
        } else {
            Ok(false)
        }
    }

    #[inline]
    pub(crate) fn handle_controller_axis(
        &mut self,
        s: &mut PixState,
        controller_id: ControllerId,
        axis: Axis,
        value: i32,
    ) -> PixResult<bool> {
        if let Some(slot) = self.get_controller_slot(controller_id) {
            let direction = match value.cmp(&0) {
                Ordering::Greater => AxisDirection::Positive,
                Ordering::Less => AxisDirection::Negative,
                Ordering::Equal => AxisDirection::None,
            };
            let input = Input::Axis((slot, axis, direction));
            self.config
                .input_bindings
                .get(&input)
                .copied()
                .map_or(Ok(false), |action| {
                    self.handle_input_action(s, slot, action, true, false)
                })
        } else {
            Ok(false)
        }
    }
}

impl Nes {
    #[inline]
    fn get_controller_slot(&self, controller_id: ControllerId) -> Option<GamepadSlot> {
        self.players.iter().find_map(|(&slot, &id)| {
            if id == controller_id {
                Some(slot)
            } else {
                None
            }
        })
    }

    #[inline]
    fn handle_input_action(
        &mut self,
        s: &mut PixState,
        slot: GamepadSlot,
        action: Action,
        pressed: bool,
        repeat: bool,
    ) -> PixResult<bool> {
        if !repeat {
            debug!(
                "Input: {{ action: {:?}, slot: {:?}, pressed: {} }}",
                action, slot, pressed
            );
        }
        if repeat {
            if let Action::Debug(debug_action) = action {
                self.handle_debug(s, debug_action, pressed, repeat)?;
            }
        } else if pressed {
            match action {
                Action::Nes(state) => self.handle_nes_state(s, state)?,
                Action::Menu(menu) => self.open_menu(s, menu)?,
                Action::Feature(feature) => self.handle_feature(s, feature, false),
                Action::Setting(setting) => self.handle_setting(s, setting)?,
                Action::Gamepad(button) => self.handle_gamepad_pressed(slot, button, pressed),
                Action::ZeroAxis(buttons) => {
                    for button in buttons {
                        self.handle_gamepad_pressed(slot, button, false);
                    }
                }
                Action::Debug(action) => self.handle_debug(s, action, pressed, false)?,
            }
        } else {
            match action {
                Action::Feature(Feature::Rewind) if !self.rewinding => todo!("Rewind 5 seconds"),
                Action::Setting(Setting::FastForward) => self.set_speed(1.0),
                Action::Gamepad(button) => self.handle_gamepad_pressed(slot, button, pressed),
                _ => (),
            }
        }
        Ok(false)
    }

    #[inline]
    fn handle_nes_state(&mut self, s: &mut PixState, state: NesState) -> NesResult<()> {
        match state {
            NesState::ToggleMenu => self.toggle_menu(Menu::Config, s)?,
            NesState::Quit => s.quit(),
            NesState::TogglePause => match self.mode {
                Mode::Playing | Mode::Recording | Mode::Replaying => self.mode = Mode::Paused,
                Mode::Paused | Mode::PausedBg => {
                    if let Some(ref debugger) = self.debugger {
                        if debugger.on_breakpoint {
                            self.control_deck.clock();
                        }
                    }
                    self.mode = Mode::Playing;
                }
                Mode::InMenu(..) => self.exit_menu(s)?,
            },
            NesState::Reset => {
                self.error = None;
                self.control_deck.reset();
                s.run(true);
                self.add_message("Reset");
            }
            NesState::PowerCycle => {
                self.error = None;
                self.control_deck.power_cycle();
                s.run(true);
                self.add_message("Power Cycled");
            }
        }
        Ok(())
    }

    #[inline]
    fn handle_feature(&mut self, s: &mut PixState, feature: Feature, repeat: bool) {
        match feature {
            Feature::ToggleGameplayRecording => {
                if self.mode == Mode::Recording {
                    self.mode = Mode::Playing;
                    self.add_message("Recording Stopped");
                    todo!("Save recording");
                } else {
                    self.mode = Mode::Recording;
                    self.add_message("Recording Started");
                    todo!("Recording")
                }
            }
            Feature::ToggleSoundRecording => {
                todo!("Toggle sound recording")
            }
            Feature::Rewind => {
                if repeat {
                    self.rewinding = true;
                    todo!("Rewinding")
                } else {
                    todo!("Rewind 5 seconds");
                }
            }
            Feature::TakeScreenshot => {
                let filename = Local::now()
                    .format("Screen_Shot_%Y-%m-%d_at_%H_%M_%S.png")
                    .to_string();
                match s.save_canvas(None, &filename) {
                    Ok(()) => self.add_message(filename),
                    Err(e) => self.add_message(e.to_string()),
                }
            }
            Feature::SaveState => self.save_state(),
            Feature::LoadState => self.load_state(),
        }
    }

    #[inline]
    fn handle_setting(&mut self, s: &mut PixState, setting: Setting) -> NesResult<()> {
        match setting {
            Setting::SetSaveSlot(slot) => {
                self.config.save_slot = slot;
                self.add_message(&format!("Set Save Slot to {}", slot));
            }
            Setting::ToggleFullscreen => {
                self.config.fullscreen = !self.config.fullscreen;
                s.fullscreen(self.config.fullscreen)?;
            }
            Setting::ToggleVsync => {
                self.config.vsync = !self.config.vsync;
                s.vsync(self.config.vsync)?;
                if self.config.vsync {
                    self.add_message("Vsync Enabled");
                } else {
                    self.add_message("Vsync Disabled");
                }
            }
            Setting::ToggleNtscFilter => {
                let enabled = self.control_deck.filter() == VideoFormat::Ntsc;
                self.control_deck.set_filter(if enabled {
                    VideoFormat::None
                } else {
                    VideoFormat::Ntsc
                });
            }
            Setting::ToggleSound => {
                self.config.sound = !self.config.sound;
                if self.config.sound {
                    self.add_message("Sound Enabled");
                } else {
                    self.add_message("Sound Disabled");
                }
            }
            Setting::TogglePulse1 => self.control_deck.toggle_channel(AudioChannel::Pulse1),
            Setting::TogglePulse2 => self.control_deck.toggle_channel(AudioChannel::Pulse2),
            Setting::ToggleTriangle => self.control_deck.toggle_channel(AudioChannel::Triangle),
            Setting::ToggleNoise => self.control_deck.toggle_channel(AudioChannel::Noise),
            Setting::ToggleDmc => self.control_deck.toggle_channel(AudioChannel::Dmc),
            Setting::FastForward => self.set_speed(2.0),
            Setting::IncSpeed => self.change_speed(0.25),
            Setting::DecSpeed => self.change_speed(-0.25),
        }
        Ok(())
    }

    #[inline]
    fn handle_gamepad_pressed(&mut self, slot: GamepadSlot, button: GamepadBtn, pressed: bool) {
        let mut gamepad = self.control_deck.gamepad_mut(slot);
        if !self.config.concurrent_dpad && pressed {
            match button {
                GamepadBtn::Left => gamepad.right = !pressed,
                GamepadBtn::Right => gamepad.left = !pressed,
                GamepadBtn::Up => gamepad.down = !pressed,
                GamepadBtn::Down => gamepad.up = !pressed,
                _ => (),
            }
        }
        match button {
            GamepadBtn::Left => gamepad.left = pressed,
            GamepadBtn::Right => gamepad.right = pressed,
            GamepadBtn::Up => gamepad.up = pressed,
            GamepadBtn::Down => gamepad.down = pressed,
            GamepadBtn::A => gamepad.a = pressed,
            GamepadBtn::B => gamepad.b = pressed,
            GamepadBtn::TurboA => {
                gamepad.turbo_a = pressed;
                gamepad.a = pressed; // Ensures that primary button isn't stuck pressed
            }
            GamepadBtn::TurboB => {
                gamepad.turbo_b = pressed;
                gamepad.b = pressed; // Ensures that primary button isn't stuck pressed
            }
            GamepadBtn::Select => gamepad.select = pressed,
            GamepadBtn::Start => gamepad.start = pressed,
            GamepadBtn::Zapper => (), // Zapper handled only with mouse currently
        };
    }

    #[inline]
    fn handle_debug(
        &mut self,
        s: &mut PixState,
        action: DebugAction,
        _pressed: bool,
        _repeat: bool,
    ) -> PixResult<()> {
        let debugging = self.debugger.is_some();
        match action {
            DebugAction::ToggleCpuDebugger => self.toggle_debugger(s)?,
            DebugAction::TogglePpuDebugger => self.toggle_ppu_viewer(s)?,
            DebugAction::ToggleApuDebugger => self.toggle_apu_viewer(s)?,
            DebugAction::StepInto if debugging => {
                if self.mode == Mode::Playing {
                    self.mode = Mode::Paused;
                }
                self.control_deck.clock();
            }
            DebugAction::StepOver if debugging => {
                if self.mode == Mode::Playing {
                    self.mode = Mode::Paused;
                }
                let instr = self.control_deck.next_instr();
                self.control_deck.clock();
                if instr.op() == Operation::JSR {
                    let rti_addr = self.control_deck.stack_addr().wrapping_add(1);
                    while self.control_deck.pc() != rti_addr {
                        self.control_deck.clock();
                    }
                }
            }
            DebugAction::StepOut if debugging => {
                let mut instr = self.control_deck.next_instr();
                while !matches!(instr.op(), Operation::RTS | Operation::RTI) {
                    self.control_deck.clock();
                    instr = self.control_deck.next_instr();
                }
                self.control_deck.clock();
            }
            DebugAction::StepFrame if debugging => {
                if self.mode == Mode::Playing {
                    self.mode = Mode::Paused;
                }
                self.control_deck.clock_frame();
            }
            DebugAction::StepScanline if debugging => {
                if self.mode == Mode::Playing {
                    self.mode = Mode::Paused;
                }
                self.control_deck.clock_scanline();
            }
            DebugAction::IncScanline if self.ppu_viewer.is_some() => {
                let increment = if s.keymod_down(KeyMod::SHIFT) { 10 } else { 1 };
                self.scanline = (self.scanline + increment).clamp(0, RENDER_HEIGHT as u16 - 1);
                self.control_deck.ppu_mut().debug_scanline = self.scanline;
            }
            DebugAction::DecScanline if self.ppu_viewer.is_some() => {
                let decrement = if s.keymod_down(KeyMod::SHIFT) { 10 } else { 1 };
                self.scanline = self.scanline.saturating_sub(decrement);
                self.control_deck.ppu_mut().debug_scanline = self.scanline;
            }
            _ => (),
        }
        Ok(())
    }
}
