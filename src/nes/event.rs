use crate::{
    apu::Channel,
    common::{Kind, NesRegion, Reset},
    cpu::{
        instr::{Instr, Operation},
        Cpu,
    },
    input::{JoypadBtn, JoypadBtnState, Slot},
    mapper::MapperRevision,
    mem::{Access, Mem},
    nes::{menu::Menu, state::ReplayMode, Mode, Nes},
    video::VideoFilter,
};
use pixels::wgpu::Color;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt,
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};
use winit::{
    dpi::PhysicalPosition,
    event::{AxisId, ButtonId, DeviceId, ElementState, KeyEvent, MouseButton},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
};

// #[derive(Debug, Copy, Clone, PartialEq)]
// #[must_use]
// pub(crate) struct DeviceId(usize);

// #[derive(Debug, Copy, Clone, PartialEq, Eq)]
// #[must_use]
// pub(crate) enum ControllerButton {
//     Todo,
// }

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub(crate) enum ControllerUpdate {
    Added,
    Removed,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[must_use]
pub(crate) enum CustomEvent {
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

/// Indicates an [Axis] direction.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub(crate) enum AxisDirection {
    /// No direction, axis is in a deadzone/not pressed.
    None,
    /// Positive (Right or Down)
    Positive,
    /// Negative (Left or Up)
    Negative,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub(crate) struct ActionEvent {
    pub(crate) frame: u32,
    pub(crate) slot: Slot,
    pub(crate) action: Action,
    pub(crate) pressed: bool,
    pub(crate) repeat: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub(crate) enum Input {
    Key((Slot, KeyCode, ModifiersState)),
    Button((Slot, ButtonId)),
    Axis((Slot, AxisId, AxisDirection)),
    Mouse((Slot, MouseButton)),
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::Key((_, key, keymod)) => {
                if keymod.is_empty() {
                    write!(f, "{key:?}")
                } else {
                    write!(f, "{keymod:?} {key:?}")
                }
            }
            Input::Button((_, btn)) => write!(f, "{btn:?}"),
            Input::Axis((_, axis, _)) => write!(f, "{axis:?}"),
            Input::Mouse((_, btn)) => write!(f, "{btn:?}"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct KeyBinding {
    pub(crate) controller: Slot,
    pub(crate) key: KeyCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) modifiers: Option<ModifiersState>,
    pub(crate) action: Action,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct MouseBinding {
    pub(crate) controller: Slot,
    pub(crate) button: MouseButton,
    pub(crate) action: Action,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ControllerButtonBinding {
    pub(crate) controller: Slot,
    pub(crate) button_id: ButtonId,
    pub(crate) action: Action,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ControllerAxisBinding {
    pub(crate) controller: Slot,
    pub(crate) axis_id: AxisId,
    pub(crate) direction: AxisDirection,
    pub(crate) action: Action,
}

/// A binding of a inputs to an [Action].
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct InputBindings {
    pub(crate) keys: Vec<KeyBinding>,
    pub(crate) mouse: Vec<MouseBinding>,
    pub(crate) buttons: Vec<ControllerButtonBinding>,
    pub(crate) axes: Vec<ControllerAxisBinding>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputMapping(HashMap<Input, Action>);

impl Deref for InputMapping {
    type Target = HashMap<Input, Action>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for InputMapping {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(variant_size_differences)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Action {
    Nes(NesState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Joypad(JoypadBtn),
    ZapperTrigger,
    Debug(DebugAction),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum NesState {
    Quit,
    TogglePause,
    SoftReset,
    HardReset,
    MapperRevision(MapperRevision),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Feature {
    ToggleGameplayRecording,
    ToggleSoundRecording,
    Rewind,
    TakeScreenshot,
    SaveState,
    LoadState,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Setting {
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

fn render_message(message: &str, color: Color) {
    // TODO: switch to egui
    // s.push();
    // s.stroke(None);
    // s.fill(rgb!(0, 200));
    // let pady = s.theme().spacing.frame_pad.y();
    // let width = s.width()?;
    // s.wrap(width);
    // let (_, height) = s.size_of(message)?;
    // s.rect([
    //     0,
    //     s.cursor_pos().y() - pady,
    //     width as i32,
    //     height as i32 + 2 * pady,
    // ])?;
    // s.fill(color);
    // s.text(message)?;
    // s.pop();
}

impl Nes {
    #[inline]
    pub(crate) fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        self.messages.push((text, Instant::now()));
    }

    pub(crate) fn render_messages(&mut self) {
        self.messages
            .retain(|(_, created)| created.elapsed() < Duration::from_secs(3));
        self.messages.dedup_by(|a, b| a.0.eq(&b.0));
        for (message, _) in &self.messages {
            render_message(message, Color::WHITE);
        }
    }

    pub(crate) fn render_confirm_quit(&mut self) {
        // TODO switch to egui
        // if let Some((ref msg, ref mut confirm)) = self.confirm_quit {
        //     s.push();
        //     s.stroke(None);
        //     s.fill(rgb!(0, 200));
        //     let pady = s.theme().spacing.frame_pad.y();
        //     let width = s.width()?;
        //     s.wrap(width);
        //     let (_, height) = s.size_of(msg)?;
        //     s.rect([
        //         0,
        //         s.cursor_pos().y() - pady,
        //         width as i32,
        //         4 * height as i32 + 2 * pady,
        //     ])?;
        //     s.fill(Color::WHITE);
        //     s.text(msg)?;
        //     if s.button("Confirm")? {
        //         *confirm = true;
        //         s.pop();
        //         return Ok(true);
        //     }
        //     s.same_line(None);
        //     if s.button("Cancel")? {
        //         self.confirm_quit = None;
        //         self.resume_play();
        //     }
        //     s.pop();
        // }
    }

    #[inline]
    pub(crate) fn render_status(&mut self, status: &str) {
        render_message(status, Color::WHITE);
        if let Some(ref err) = self.error {
            render_message(err, Color::RED);
        }
    }

    #[inline]
    pub(crate) fn handle_input(&mut self, slot: Slot, input: Input, pressed: bool, repeat: bool) {
        if let Some(action) = self.config.input_map.get(&input).copied() {
            self.handle_action(slot, action, pressed, repeat);
        }
    }

    pub(crate) fn handle_key_event(&mut self, event: KeyEvent) {
        if let PhysicalKey::Code(key) = event.physical_key {
            for slot in [Slot::One, Slot::Two, Slot::Three, Slot::Four] {
                self.handle_input(
                    slot,
                    Input::Key((slot, key, self.modifiers.state())),
                    event.state.is_pressed(),
                    event.repeat,
                );
            }
        }
    }

    pub fn handle_mouse_event(&mut self, btn: MouseButton, state: ElementState) -> bool {
        // To avoid consuming events while in menus
        if self.mode == Mode::Playing {
            for slot in [Slot::One, Slot::Two] {
                self.handle_input(slot, Input::Mouse((slot, btn)), state.is_pressed(), false);
            }
        }
        false
    }

    #[inline]
    fn handle_zapper_trigger(&mut self) {
        self.control_deck.trigger_zapper();
    }

    pub fn set_zapper_pos(&mut self, pos: PhysicalPosition<f64>) {
        let x = (pos.x / self.config.scale as f64) * 8.0 / 7.0 + 0.5; // Adjust ratio
        let mut y = pos.y / self.config.scale as f64;
        // Account for trimming top 8 scanlines
        if self.config.region == NesRegion::Ntsc {
            y += 8.0;
        };
        self.control_deck
            .aim_zapper(x.round() as i32, y.round() as i32);
    }

    #[inline]
    pub fn handle_mouse_motion(&mut self, pos: PhysicalPosition<f64>) -> bool {
        // To avoid consuming events while in menus
        if self.mode == Mode::Playing {
            self.set_zapper_pos(pos);
            true
        } else {
            false
        }
    }

    #[inline]
    pub(crate) fn handle_controller_update(
        &mut self,
        device_id: DeviceId,
        update: ControllerUpdate,
    ) {
        match update {
            ControllerUpdate::Added => {
                for slot in [Slot::One, Slot::Two, Slot::Three, Slot::Four] {
                    let slot_idx = slot as usize;
                    if self.controllers[slot_idx].is_none() {
                        self.add_message(format!("Controller {} connected.", slot_idx + 1));
                        self.controllers[slot_idx] = Some(device_id);
                    }
                }
            }
            ControllerUpdate::Removed => {
                if let Some(slot) = self.get_controller_slot(device_id) {
                    let slot_idx = slot as usize;
                    self.controllers[slot_idx] = None;
                    self.add_message(format!("Controller {} disconnected.", slot_idx + 1));
                }
            }
        }
    }

    #[inline]
    pub(crate) fn handle_controller_event(
        &mut self,
        device_id: DeviceId,
        button_id: ButtonId,
        pressed: bool,
    ) {
        if let Some(slot) = self.get_controller_slot(device_id) {
            self.handle_input(slot, Input::Button((slot, button_id)), pressed, false);
        }
    }

    #[inline]
    pub(crate) fn handle_controller_axis_motion(
        &mut self,
        device_id: DeviceId,
        axis: AxisId,
        value: f64,
    ) {
        if let Some(slot) = self.get_controller_slot(device_id) {
            let direction = if value < self.config.controller_deadzone {
                AxisDirection::Negative
            } else if value > self.config.controller_deadzone {
                AxisDirection::Positive
            } else {
                // TODO: verify if this is correct
                for button in [
                    JoypadBtn::Left,
                    JoypadBtn::Right,
                    JoypadBtn::Up,
                    JoypadBtn::Down,
                ] {
                    self.handle_joypad_pressed(slot, button, false);
                }
                return;
            };
            let input = Input::Axis((slot, axis, direction));
            self.handle_input(slot, input, true, false);
        }
    }

    pub(crate) fn handle_action(
        &mut self,
        slot: Slot,
        action: Action,
        pressed: bool,
        repeat: bool,
    ) {
        match action {
            Action::Debug(action) if pressed => self.handle_debug(action, repeat),
            Action::Feature(feature) => self.handle_feature(feature, pressed, repeat),
            Action::Nes(state) if pressed => self.handle_nes_state(state),
            Action::Menu(menu) if pressed => self.toggle_menu(menu),
            Action::Setting(setting) => self.handle_setting(setting, pressed, repeat),
            Action::Joypad(button) => self.handle_joypad_pressed(slot, button, pressed),
            Action::ZapperTrigger if pressed => self.handle_zapper_trigger(),
            _ => (),
        }

        if self.replay.mode == ReplayMode::Recording {
            self.replay
                .buffer
                .push(self.action_event(slot, action, pressed, repeat));
        }
    }

    pub(crate) fn replay_action(&mut self) {
        let current_frame = self.control_deck.frame_number();
        while let Some(action_event) = self.replay.buffer.last() {
            match action_event.frame.cmp(&current_frame) {
                Ordering::Equal => {
                    let ActionEvent {
                        slot,
                        action,
                        pressed,
                        repeat,
                        ..
                    } = self.replay.buffer.pop().expect("valid action event");
                    self.handle_action(slot, action, pressed, repeat);
                }
                Ordering::Less => {
                    log::warn!(
                        "Encountered action event out of order: {} < {}",
                        action_event.frame,
                        current_frame
                    );
                    self.replay.buffer.pop();
                }
                Ordering::Greater => break,
            }
        }
        if self.replay.buffer.is_empty() {
            self.stop_replay();
        }
    }
}

impl Nes {
    #[inline]
    const fn action_event(
        &self,
        slot: Slot,
        action: Action,
        pressed: bool,
        repeat: bool,
    ) -> ActionEvent {
        ActionEvent {
            frame: self.control_deck.frame_number(),
            slot,
            action,
            pressed,
            repeat,
        }
    }

    #[inline]
    fn get_controller_slot(&self, device_id: DeviceId) -> Option<Slot> {
        self.controllers.iter().enumerate().find_map(|(slot, id)| {
            (*id == Some(device_id)).then_some(Slot::try_from(slot).expect("valid slot index"))
        })
    }

    fn handle_nes_state(&mut self, state: NesState) {
        if self.replay.mode == ReplayMode::Recording {
            return;
        }
        match state {
            NesState::Quit => {
                self.pause_play();
                self.quitting = true;
            }
            NesState::TogglePause => self.toggle_pause(),
            NesState::SoftReset => {
                self.error = None;
                self.control_deck.reset(Kind::Soft);
                self.add_message("Reset");
                // TODO: add debugger
                // if self.debugger.is_some() && self.mode != Mode::Paused {
                //     self.mode = Mode::Paused;
                // }
            }
            NesState::HardReset => {
                self.error = None;
                self.control_deck.reset(Kind::Hard);
                self.add_message("Power Cycled");
                // TODO: add debugger
                // if self.debugger.is_some() {
                //     self.mode = Mode::Paused;
                // }
            }
            NesState::MapperRevision(_) => todo!("mapper revision"),
        }
    }

    fn handle_feature(&mut self, feature: Feature, pressed: bool, repeat: bool) {
        if feature == Feature::Rewind {
            if repeat {
                if self.config.rewind {
                    self.mode = Mode::Rewinding;
                } else {
                    self.add_message("Rewind disabled. You can enable it in the Config menu.");
                }
            } else if !pressed {
                if self.mode == Mode::Rewinding {
                    self.resume_play();
                } else {
                    self.instant_rewind();
                }
            }
        } else if pressed {
            match feature {
                Feature::ToggleGameplayRecording => match self.replay.mode {
                    ReplayMode::Off => self.start_replay(),
                    ReplayMode::Recording | ReplayMode::Playback => self.stop_replay(),
                },
                Feature::ToggleSoundRecording => self.toggle_sound_recording(),
                Feature::TakeScreenshot => self.save_screenshot(),
                Feature::SaveState => self.save_state(self.config.save_slot),
                Feature::LoadState => self.load_state(self.config.save_slot),
                Feature::Rewind => (), // Handled above
            }
        }
    }

    fn handle_setting(&mut self, setting: Setting, pressed: bool, _repeat: bool) {
        if setting == Setting::FastForward {
            if pressed {
                self.set_speed(2.0);
            } else if !pressed {
                self.set_speed(1.0);
            }
        } else if pressed {
            match setting {
                Setting::SetSaveSlot(slot) => {
                    self.config.save_slot = slot;
                    self.add_message(&format!("Set Save Slot to {slot}"));
                }
                Setting::ToggleFullscreen => {
                    self.config.fullscreen = !self.config.fullscreen;
                    // TODO: toggle winit fullscreen
                }
                Setting::ToggleVsync => {
                    // TODO: send toggle vsync message to render thread
                    // self.config.vsync = !self.config.vsync;
                    // if self.config.vsync {
                    //     self.add_message("Vsync Enabled");
                    // } else {
                    //     self.add_message("Vsync Disabled");
                    // }
                }
                Setting::ToggleNtscFilter => {
                    self.config.filter = match self.config.filter {
                        VideoFilter::Pixellate => VideoFilter::Ntsc,
                        VideoFilter::Ntsc => VideoFilter::Pixellate,
                    };
                    self.control_deck.set_filter(self.config.filter);
                }
                Setting::ToggleSound => {
                    self.config.sound = !self.config.sound;
                    if self.config.sound {
                        self.add_message("Sound Enabled");
                    } else {
                        self.add_message("Sound Disabled");
                    }
                }
                Setting::TogglePulse1 => self.control_deck.toggle_channel(Channel::Pulse1),
                Setting::TogglePulse2 => self.control_deck.toggle_channel(Channel::Pulse2),
                Setting::ToggleTriangle => self.control_deck.toggle_channel(Channel::Triangle),
                Setting::ToggleNoise => self.control_deck.toggle_channel(Channel::Noise),
                Setting::ToggleDmc => self.control_deck.toggle_channel(Channel::Dmc),
                Setting::IncSpeed => self.change_speed(0.25),
                Setting::DecSpeed => self.change_speed(-0.25),
                // Toggling fast forward happens on key release
                _ => (),
            }
        }
    }

    fn handle_joypad_pressed(&mut self, slot: Slot, button: JoypadBtn, pressed: bool) {
        if self.mode != Mode::Playing {
            return;
        }
        let joypad = self.control_deck.joypad_mut(slot);
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

        // Ensure that primary button isn't stuck pressed
        match button {
            JoypadBtn::TurboA => joypad.set_button(JoypadBtnState::A, pressed),
            JoypadBtn::TurboB => joypad.set_button(JoypadBtnState::B, pressed),
            _ => (),
        };
    }

    fn handle_debug(&mut self, action: DebugAction, repeat: bool) {
        // TODO: add debugger
        // let debugging = self.debugger.is_some();
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

    fn debug_step_into(&mut self) {
        self.pause_play();
        if let Err(err) = self.control_deck.clock_instr() {
            self.handle_emulation_error(&err);
        }
    }

    fn next_instr(&mut self) -> Instr {
        let pc = self.control_deck.cpu().pc();
        let opcode = self.control_deck.cpu().peek(pc, Access::Dummy);
        Cpu::INSTRUCTIONS[opcode as usize]
    }

    fn debug_step_over(&mut self) {
        self.pause_play();
        let instr = self.next_instr();
        if let Err(err) = self.control_deck.clock_instr() {
            self.handle_emulation_error(&err);
        }
        if instr.op() == Operation::JSR {
            let rti_addr = self.control_deck.cpu().peek_stack_u16().wrapping_add(1);
            while self.control_deck.cpu().pc() != rti_addr {
                if let Err(err) = self.control_deck.clock_instr() {
                    self.handle_emulation_error(&err);
                    break;
                }
            }
        }
    }

    fn debug_step_out(&mut self) {
        let mut instr = self.next_instr();
        while !matches!(instr.op(), Operation::RTS | Operation::RTI) {
            if let Err(err) = self.control_deck.clock_instr() {
                self.handle_emulation_error(&err);
                break;
            }
            instr = self.next_instr();
        }
        if let Err(err) = self.control_deck.clock_instr() {
            self.handle_emulation_error(&err);
        }
    }

    fn debug_step_frame(&mut self) {
        self.pause_play();
        if let Err(err) = self.control_deck.clock_frame() {
            self.handle_emulation_error(&err);
        }
    }

    fn debug_step_scanline(&mut self) {
        self.pause_play();
        if let Err(err) = self.control_deck.clock_scanline() {
            self.handle_emulation_error(&err);
        }
    }
}
