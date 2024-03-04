use crate::{
    apu::Channel,
    common::{NesRegion, ResetKind},
    input::{JoypadBtn, Player},
    mapper::MapperRevision,
    nes::{
        config::{FrameSpeed, Scale},
        renderer::gui::{ConfigTab, Menu},
        Nes,
    },
    platform::time::Duration,
    profile,
    video::VideoFilter,
};
use anyhow::anyhow;
use crossbeam::channel::{self, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};
use tracing::{debug, error, trace};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event as WinitEvent, Modifiers, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoopWindowTarget},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::Fullscreen,
};

#[derive(Debug, Clone)]
#[must_use]
pub enum NesEvent {
    Error(String),
    Message(String),
    SetTitle(String),
    RequestRedraw,
    ResizeWindow((LogicalSize<f32>, LogicalSize<f32>)),
    Terminate,
    Pause(bool),
    TogglePause,
    LoadRomDialog,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RomData(Vec<u8>);

impl std::fmt::Debug for RomData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RomData({} bytes)", self.0.len())
    }
}

impl AsRef<[u8]> for RomData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl RomData {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub enum EmulationEvent {
    Joypad((Player, JoypadBtn, ElementState)),
    #[cfg(not(target_arch = "wasm32"))]
    LoadRomPath(std::path::PathBuf),
    LoadRom((String, RomData)),
    Pause(bool),
    Reset(ResetKind),
    Rewind((ElementState, bool)),
    Screenshot,
    SetAudioEnabled(bool),
    SetFrameSpeed(FrameSpeed),
    SetHideOverscan(bool),
    SetRegion(NesRegion),
    SetSaveSlot(u8),
    StateLoad,
    StateSave,
    ToggleApuChannel(Channel),
    ToggleAudioRecord,
    ToggleReplayRecord,
    SetVideoFilter(VideoFilter),
    ZapperAim((u32, u32)),
    ZapperConnect(bool),
    ZapperTrigger,
}

#[derive(Debug, Clone)]
#[must_use]
pub enum RendererEvent {
    Frame(Duration),
    Pause(bool),
    Menu(Menu),
    SetScale(Scale),
    SetVSync(bool),
}

#[derive(Debug, Clone)]
#[must_use]
pub enum Event {
    Nes(NesEvent),
    Emulation(EmulationEvent),
    Renderer(RendererEvent),
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

impl From<NesEvent> for Event {
    fn from(event: NesEvent) -> Self {
        Self::Nes(event)
    }
}

impl From<EmulationEvent> for Event {
    fn from(event: EmulationEvent) -> Self {
        Self::Emulation(event)
    }
}

impl From<RendererEvent> for Event {
    fn from(event: RendererEvent) -> Self {
        Self::Renderer(event)
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    #[allow(unused)]
    pub tx: Sender<Event>,
    pub rx: Receiver<Event>,
    pub modifiers: Modifiers,
    pub occluded: bool,
    pub paused: bool,
    pub quitting: bool,
}

impl State {
    pub fn new() -> Self {
        let (tx, rx) = channel::bounded(1024);
        Self {
            tx,
            rx,
            modifiers: Modifiers::default(),
            occluded: false,
            paused: false,
            quitting: false,
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl Nes {
    pub fn on_event(&mut self, event: WinitEvent<()>, window_target: &EventLoopWindowTarget<()>) {
        profile!();

        if self.state.quitting {
            window_target.exit();
        }

        match event {
            WinitEvent::WindowEvent {
                window_id, event, ..
            } => {
                self.renderer.on_window_event(&self.window, &event);
                match event {
                    WindowEvent::CloseRequested => {
                        if window_id == self.window.id() {
                            window_target.exit();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let Err(err) =
                            self.renderer.request_redraw(&self.window, &mut self.config)
                        {
                            self.on_error(err);
                        }
                        self.window.request_redraw();
                    }
                    WindowEvent::Occluded(occluded) => {
                        if window_id == self.window.id() {
                            self.state.occluded = occluded;
                            self.trigger_event(NesEvent::Pause(self.state.occluded));
                            if self.state.occluded {
                                window_target.set_control_flow(ControlFlow::Wait);
                            } else {
                                window_target.set_control_flow(ControlFlow::Poll);
                            }
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(key) = event.physical_key {
                            self.on_input(
                                Input::Key(key, self.state.modifiers.state()),
                                event.state,
                                event.repeat,
                            );
                        }
                    }
                    WindowEvent::ModifiersChanged(modifiers) => self.state.modifiers = modifiers,
                    WindowEvent::MouseInput { button, state, .. } => {
                        self.on_input(Input::Mouse(button, state), state, false);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    WindowEvent::DroppedFile(path) => {
                        self.trigger_event(EmulationEvent::LoadRomPath(path));
                    }
                    WindowEvent::HoveredFile(_) => (), // TODO: Show file drop cursor
                    WindowEvent::HoveredFileCancelled => (), // TODO: Restore cursor
                    _ => (),
                }
            }
            WinitEvent::AboutToWait => {
                while let Ok(event) = self.state.rx.try_recv() {
                    match event {
                        Event::Nes(event) => self.on_nes_event(event),
                        Event::Emulation(event) => self.emulation.on_event(event),
                        Event::Renderer(event) => self.renderer.on_event(event, &self.config),
                    }
                }
                self.next_frame();
            }
            WinitEvent::LoopExiting => {
                #[cfg(feature = "profiling")]
                crate::profiling::enable(false);
                if let Err(err) = self.config.save() {
                    error!("failed to save config: {err:?}");
                }
            }
            // WinitEvent::DeviceEvent { device_id, event } => todo!(),
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
            _ => (),
        }
    }

    pub fn on_nes_event(&mut self, event: NesEvent) {
        match event {
            NesEvent::Message(msg) => self.add_message(msg),
            NesEvent::Error(err) => self.on_error(anyhow!(err)),
            NesEvent::Terminate => self.state.quitting = true,
            NesEvent::SetTitle(title) => self.window.set_title(&title),
            NesEvent::ResizeWindow((inner_size, min_inner_size)) => {
                let _ = self.window.request_inner_size(inner_size);
                self.window.set_min_inner_size(Some(min_inner_size));
            }
            NesEvent::RequestRedraw => self.window.request_redraw(),
            NesEvent::Pause(paused) => {
                self.state.paused = paused;
                self.emulation
                    .on_event(EmulationEvent::Pause(self.state.paused));
                self.renderer
                    .on_event(RendererEvent::Pause(self.state.paused), &self.config);
            }
            NesEvent::TogglePause => {
                self.state.paused = !self.state.paused;
                self.emulation
                    .on_event(EmulationEvent::Pause(self.state.paused));
                self.renderer
                    .on_event(RendererEvent::Pause(self.state.paused), &self.config);
            }
            NesEvent::LoadRomDialog => {
                #[cfg(target_arch = "wasm32")]
                {
                    use crate::nes::platform::html_ids;
                    use wasm_bindgen::JsCast;
                    use web_sys::HtmlInputElement;

                    let input = web_sys::window()
                        .and_then(|window| window.document())
                        .and_then(|document| document.get_element_by_id(html_ids::ROM_INPUT))
                        .and_then(|input| input.dyn_into::<HtmlInputElement>().ok());
                    match input {
                        Some(input) => input.click(),
                        None => {
                            self.trigger_event(NesEvent::Error("failed to open rom".to_string()))
                        }
                    }
                    if let Some(canvas) = crate::nes::platform::get_canvas() {
                        let _ = canvas.focus();
                    }
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("NES ROMs", &["nes"])
                        .pick_file()
                    {
                        self.trigger_event(EmulationEvent::LoadRomPath(path));
                    }
                }
            }
        }
    }

    /// Trigger a custom event.
    pub fn trigger_event(&mut self, event: impl Into<Event>) {
        let event = event.into();
        debug!("Nes event: {event:?}");

        match event {
            Event::Nes(event) => self.on_nes_event(event),
            Event::Emulation(event) => self.emulation.on_event(event),
            Event::Renderer(event) => self.renderer.on_event(event, &self.config),
        }
    }

    /// Handle user input mapped to key bindings.
    pub fn on_input(&mut self, input: Input, state: ElementState, repeat: bool) {
        if let Some((player, action)) = self.config.input_map.get(&input).copied() {
            trace!("player: {player:?}, action: {action:?}, state: {state:?}, repeat: {repeat:?}");
            let released = state == ElementState::Released;
            match action {
                Action::Nes(nes_state) if released => match nes_state {
                    NesState::Quit => self.trigger_event(NesEvent::Terminate),
                    NesState::TogglePause => self.trigger_event(NesEvent::TogglePause),
                    NesState::SoftReset => {
                        self.trigger_event(EmulationEvent::Reset(ResetKind::Soft))
                    }
                    NesState::HardReset => {
                        self.trigger_event(EmulationEvent::Reset(ResetKind::Hard))
                    }
                    NesState::MapperRevision(_) => todo!("mapper revision"),
                },
                Action::Menu(menu) if released => self.trigger_event(RendererEvent::Menu(menu)),
                Action::Feature(feature) => match feature {
                    Feature::ToggleReplayRecord if released => {
                        self.trigger_event(EmulationEvent::ToggleReplayRecord);
                    }
                    Feature::ToggleAudioRecord if released => {
                        self.trigger_event(EmulationEvent::ToggleAudioRecord);
                    }
                    Feature::TakeScreenshot if released => {
                        self.trigger_event(EmulationEvent::Screenshot)
                    }
                    Feature::SaveState if released => self.trigger_event(EmulationEvent::StateSave),
                    Feature::LoadState if released => self.trigger_event(EmulationEvent::StateLoad),
                    Feature::Rewind => self.trigger_event(EmulationEvent::Rewind((state, repeat))),
                    _ => (),
                },
                Action::Setting(setting) => match setting {
                    Setting::SetSaveSlot(slot) if released => {
                        self.config.deck.save_slot = slot;
                        self.trigger_event(EmulationEvent::SetSaveSlot(slot));
                        self.add_message(format!("Changed Save Slot to {slot}"));
                    }
                    Setting::ToggleFullscreen if released => {
                        self.config.fullscreen = !self.config.fullscreen;
                        self.window.set_fullscreen(
                            self.config
                                .fullscreen
                                .then_some(Fullscreen::Borderless(None)),
                        );
                    }
                    Setting::ToggleVsync if released => {
                        self.config.vsync = !self.config.vsync;
                        self.trigger_event(RendererEvent::SetVSync(self.config.vsync));
                    }
                    Setting::ToggleVideoFilter(filter) if released => {
                        self.config.deck.filter = if self.config.deck.filter == filter {
                            VideoFilter::Pixellate
                        } else {
                            filter
                        };
                        self.trigger_event(EmulationEvent::SetVideoFilter(self.config.deck.filter));
                    }
                    Setting::ToggleAudio if released => {
                        self.config.audio_enabled = !self.config.audio_enabled;
                        self.trigger_event(EmulationEvent::SetAudioEnabled(
                            self.config.audio_enabled,
                        ));
                    }
                    Setting::ToggleApuChannel(channel) if released => {
                        self.trigger_event(EmulationEvent::ToggleApuChannel(channel));
                    }
                    Setting::IncSpeed if released => {
                        self.config.frame_speed = self.config.frame_speed.increment();
                        self.set_speed(self.config.frame_speed);
                    }
                    Setting::DecSpeed if released => {
                        self.config.frame_speed = self.config.frame_speed.decrement();
                        self.set_speed(self.config.frame_speed);
                    }
                    Setting::FastForward if !repeat => self.set_speed(if released {
                        FrameSpeed::default()
                    } else {
                        FrameSpeed::X200
                    }),
                    _ => (),
                },
                Action::Joypad(button) if !repeat => {
                    self.trigger_event(EmulationEvent::Joypad((player, button, state)));
                }
                Action::ZapperTrigger if self.config.deck.zapper => {
                    self.trigger_event(EmulationEvent::ZapperTrigger);
                }
                Action::Debug(action) => match action {
                    // DebugAction::ToggleCpuDebugger if released => self.toggle_debugger()?,
                    // DebugAction::TogglePpuDebugger if released => self.toggle_ppu_viewer()?,
                    // DebugAction::ToggleApuDebugger if released => self.toggle_apu_viewer()?,
                    // DebugAction::StepInto if released || repeat => self.debug_step_into()?,
                    // DebugAction::StepOver if released || repeat => self.debug_step_over()?,
                    // DebugAction::StepOut if released || repeat => self.debug_step_out()?,
                    // DebugAction::StepFrame if released || repeat => self.debug_step_frame()?,
                    // DebugAction::StepScanline if released || repeat => self.debug_step_scanline()?,
                    DebugAction::IncScanline if released || repeat => {
                        // TODO: add ppu viewer
                        // if let Some(ref mut viewer) = self.ppu_viewer {
                        // TODO: check keydown
                        // let increment = if s.keymod_down(ModifiersState::SHIFT) { 10 } else { 1 };
                        // viewer.inc_scanline(increment);
                    }
                    DebugAction::DecScanline if released || repeat => {
                        // TODO: add ppu viewer
                        // if let Some(ref mut viewer) = self.ppu_viewer {
                        // TODO: check keydown
                        // let decrement = if s.keymod_down(ModifiersState::SHIFT) { 10 } else { 1 };
                        // viewer.dec_scanline(decrement);
                    }
                    _ => (),
                },
                _ => (),
            }
        }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        key_map!(map, One, ShiftRight, SHIFT, JoypadBtn::Select); // Required because shift is also a modifier
        key_map!(map, One, ShiftLeft, SHIFT, JoypadBtn::Select); // Required because shift is also a modifier
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
        key_map!(map, One, KeyV, SHIFT, Feature::ToggleReplayRecord);
        key_map!(map, One, KeyR, SHIFT, Feature::ToggleAudioRecord);
        key_map!(map, One, KeyM, CONTROL, Setting::ToggleAudio);
        key_map!(
            map,
            One,
            Numpad1,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Pulse1)
        );
        key_map!(
            map,
            One,
            Numpad2,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Pulse2)
        );
        key_map!(
            map,
            One,
            Numpad3,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Triangle)
        );
        key_map!(
            map,
            One,
            Numpad4,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Noise)
        );
        key_map!(
            map,
            One,
            Numpad5,
            SHIFT,
            Setting::ToggleApuChannel(Channel::Dmc)
        );
        key_map!(map, One, Enter, CONTROL, Setting::ToggleFullscreen);
        key_map!(map, One, KeyV, CONTROL, Setting::ToggleVsync);
        key_map!(
            map,
            One,
            KeyN,
            CONTROL,
            Setting::ToggleVideoFilter(VideoFilter::Ntsc)
        );
        key_map!(
            map,
            One,
            KeyD,
            SHIFT,
            DebugAction::ToggleDebugger(Debugger::Cpu)
        );
        key_map!(
            map,
            One,
            KeyP,
            SHIFT,
            DebugAction::ToggleDebugger(Debugger::Ppu)
        );
        key_map!(
            map,
            One,
            KeyA,
            SHIFT,
            DebugAction::ToggleDebugger(Debugger::Apu)
        );
        key_map!(map, One, KeyC, DebugAction::Step(Step::Into));
        key_map!(map, One, KeyO, DebugAction::Step(Step::Over));
        key_map!(map, One, KeyO, SHIFT, DebugAction::Step(Step::Out));
        key_map!(map, One, KeyL, SHIFT, DebugAction::Step(Step::Scanline));
        key_map!(map, One, KeyF, SHIFT, DebugAction::Step(Step::Frame));
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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum NesState {
    Quit,
    TogglePause,
    SoftReset,
    HardReset,
    MapperRevision(MapperRevision),
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Feature {
    ToggleReplayRecord,
    ToggleAudioRecord,
    Rewind,
    TakeScreenshot,
    SaveState,
    LoadState,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Setting {
    SetSaveSlot(u8),
    ToggleFullscreen,
    ToggleVsync,
    ToggleVideoFilter(VideoFilter),
    SetVideoFilter(VideoFilter),
    SetNesFormat(NesRegion),
    ToggleAudio,
    ToggleApuChannel(Channel),
    FastForward,
    IncSpeed,
    DecSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum Debugger {
    Cpu,
    Ppu,
    Apu,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub enum Step {
    Into,
    Out,
    Over,
    Scanline,
    Frame,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum DebugAction {
    ToggleDebugger(Debugger),
    Step(Step),
    IncScanline,
    DecScanline,
}

// impl Nes {
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

//
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

//
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
//     fn get_controller_player(&self, device_id: DeviceId) -> Option<Slot> {
//         self.controllers.iter().enumerate().find_map(|(player, id)| {
//             (*id == Some(device_id)).then_some(Slot::try_from(player).expect("valid player index"))
//         })
//     }

//     fn debug_step_into(&mut self) {
//         self.pause_play(PauseMode::Manual);
//         if let Err(err) = self.control_deck.clock_instr() {
//             self.handle_emulation_error(&err);
//         }
//     }

//     fn next_instr(&mut self) -> Instr {
//         let pc = self.control_deck.cpu().pc();
//         let opcode = self.control_deck.cpu().peek(pc, Access::Dummy);
//         Cpu::INSTRUCTIONS[opcode as usize]
//     }

//     fn debug_step_over(&mut self) {
//         self.pause_play(PauseMode::Manual);
//         let instr = self.next_instr();
//         if let Err(err) = self.control_deck.clock_instr() {
//             self.handle_emulation_error(&err);
//         }
//         if instr.op() == Operation::JSR {
//             let rti_addr = self.control_deck.cpu().peek_stack_u16().wrapping_add(1);
//             while self.control_deck.cpu().pc() != rti_addr {
//                 if let Err(err) = self.control_deck.clock_instr() {
//                     self.handle_emulation_error(&err);
//                     break;
//                 }
//             }
//         }
//     }

//     fn debug_step_out(&mut self) {
//         let mut instr = self.next_instr();
//         while !matches!(instr.op(), Operation::RTS | Operation::RTI) {
//             if let Err(err) = self.control_deck.clock_instr() {
//                 self.handle_emulation_error(&err);
//                 break;
//             }
//             instr = self.next_instr();
//         }
//         if let Err(err) = self.control_deck.clock_instr() {
//             self.handle_emulation_error(&err);
//         }
//     }

//     fn debug_step_frame(&mut self) {
//         self.pause_play(PauseMode::Manual);
//         if let Err(err) = self.control_deck.clock_frame() {
//             self.handle_emulation_error(&err);
//         }
//     }

//     fn debug_step_scanline(&mut self) {
//         self.pause_play(PauseMode::Manual);
//         if let Err(err) = self.control_deck.clock_scanline() {
//             self.handle_emulation_error(&err);
//         }
//     }
// }
