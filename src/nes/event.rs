use super::{Nes, NesResult};
use crate::{common::create_png, input::GamepadBtn};
use chrono::prelude::{DateTime, Local};
use pix_engine::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

// TODO
// const GAMEPAD_TRIGGER_PRESS: i16 = 32_700;
// const GAMEPAD_AXIS_DEADZONE: i16 = 10_000;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct KeyBind {
    pub(crate) key: Key,
    pub(crate) keymod: KeyMod,
    pub(crate) action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyBindings(HashMap<(Key, KeyMod), Action>);

impl KeyBindings {
    pub(crate) fn with_config<P: AsRef<Path>>(config: P) -> NesResult<Self> {
        let config = config.as_ref();
        let file = BufReader::new(File::open(config)?);

        let keybinds: Vec<KeyBind> = serde_json::from_reader(file).unwrap();
        let mut bindings = HashMap::new();
        for bind in keybinds {
            bindings.insert((bind.key, bind.keymod), bind.action);
        }

        Ok(Self(bindings))
    }
}

impl Deref for KeyBindings {
    type Target = HashMap<(Key, KeyMod), Action>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KeyBindings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Action {
    Nes(NesState),
    Menu(Menu),
    Feature(Feature),
    Setting(Setting),
    Debug(DebugAction),
    Gamepad(GamepadBtn),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum NesState {
    TogglePause,
    Quit,
    Reset,
    PowerCycle,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Menu {
    Help,
    Config,
    LoadRom,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Feature {
    ToggleRecording,
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
    ToggleNtsc,
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
    ToggleDebugger,
    ToggleNtViewer,
    TogglePpuViewer,
    StepInto,
    StepOver,
    StepOut,
    StepFrame,
    StepScanline,
    IncScanline,
    DecScanline,
}

impl Nes {
    pub(super) fn handle_key_pressed(
        &mut self,
        s: &mut PixState,
        event: KeyEvent,
    ) -> PixResult<()> {
        self.handle_key_event(s, event)
    }

    pub(crate) fn handle_key_released(
        &mut self,
        s: &mut PixState,
        event: KeyEvent,
    ) -> PixResult<()> {
        self.handle_key_event(s, event)
    }

    fn handle_key_event(&mut self, s: &mut PixState, event: KeyEvent) -> PixResult<()> {
        use Action::*;
        if let Some(binding) = self
            .config
            .bindings
            .get(&(event.key, event.keymod))
            .copied()
        {
            if event.repeat {
                return Ok(());
            }
            match binding {
                Setting(setting) => self.handle_setting(s, setting, event.pressed)?,
                Gamepad(button) => self.handle_gamepad_pressed(s, button, event.pressed)?,
                _ => (), // Invalid action
            }
        }
        Ok(())
    }

    fn handle_gamepad_pressed(
        &mut self,
        s: &mut PixState,
        button: GamepadBtn,
        pressed: bool,
    ) -> PixResult<()> {
        if !s.focused() {
            return Ok(());
        }

        use GamepadBtn::*;
        let mut gamepad = self.control_deck.get_gamepad1_mut();
        if !self.config.concurrent_dpad && pressed {
            match button {
                Left => gamepad.right = false,
                Right => gamepad.left = false,
                Up => gamepad.down = false,
                Down => gamepad.up = false,
                _ => (),
            }
        }
        match button {
            Left => gamepad.left = pressed,
            Right => gamepad.right = pressed,
            Up => gamepad.up = pressed,
            Down => gamepad.down = pressed,
            A => gamepad.a = pressed,
            B => gamepad.b = pressed,
            TurboA => {
                gamepad.turbo_a = pressed;
                if !pressed {
                    gamepad.a = pressed;
                }
            }
            TurboB => {
                gamepad.turbo_b = pressed;
                if !pressed {
                    gamepad.b = pressed;
                }
            }
            Select => gamepad.select = pressed,
            Start => gamepad.start = pressed,
            Zapper => todo!("zapper"),
        };
        Ok(())
    }

    fn handle_setting(
        &mut self,
        _s: &mut PixState,
        setting: Setting,
        pressed: bool,
    ) -> NesResult<()> {
        use Setting::*;
        match setting {
            FastForward => {
                if pressed {
                    self.set_speed(2.0);
                } else {
                    self.set_speed(1.0);
                }
            }
            IncSpeed => self.change_speed(0.25),
            DecSpeed => self.change_speed(-0.25),
            _ => (),
        }
        Ok(())
    }

    /// Takes a screenshot and saves it to the current directory as a `.png` file
    ///
    /// # Arguments
    ///
    /// * `pixels` - An array of pixel data to save in `.png` format
    ///
    /// # Errors
    ///
    /// It's possible for this method to fail, but instead of erroring the program,
    /// it'll simply log the error out to STDERR
    // TODO Scale screenshot to current width/height
    // TODO Screenshot the currently focused window
    pub(crate) fn _screenshot(&mut self) -> NesResult<String> {
        let datetime: DateTime<Local> = Local::now();
        let mut png_path = PathBuf::from(
            datetime
                .format("Screen_Shot_%Y-%m-%d_at_%H_%M_%S")
                .to_string(),
        );
        let pixels = self.control_deck.get_frame();
        png_path.set_extension("png");
        println!("Saved screenshot: {:?}", png_path);
        create_png(&png_path, pixels)
    }
}

// TODO
//    /// Handles keyrepeats
//    pub(super) fn handle_keyrepeat(&mut self, key: Key) {
//        self.held_keys.insert(key as u8, true);
//        let c = self.is_key_held(Key::LCtrl);
//        let d = self.config.debug;
//        match key {
//            // No modifiers
//            // Step/Step Into
//            Key::C if d => {
//                let _ = self.clock();
//            }
//            // Step Frame
//            Key::F if d => self.clock_frame(),
//            // Step Scanline
//            Key::S if d && !c => {
//                let prev_scanline = self.cpu.bus.ppu.scanline;
//                let mut scanline = prev_scanline;
//                while scanline == prev_scanline {
//                    let _ = self.clock();
//                    scanline = self.cpu.bus.ppu.scanline;
//                }
//            }
//            _ => {
//                let _ = self.handle_scanline_key(key);
//            }
//        }
//    }

//    /// Checks for focus in debug windows and scrolls the scanline checking indicator up or down
//    /// Returns true if key was handled, or false if it was not
//    fn handle_scanline_key(&mut self, key: Key) -> bool {
//        match key {
//            // Nametable/PPU Viewer Shortcuts
//            Key::Up => {
//                if self.focused_window.is_some() {
//                    if self.focused_window == self.nt_viewer_window {
//                        self.set_nt_scanline(self.nt_scanline.saturating_sub(1));
//                        return true;
//                    } else if self.focused_window == self.ppu_viewer_window {
//                        self.set_pat_scanline(self.pat_scanline.saturating_sub(1));
//                        return true;
//                    }
//                }
//                false
//            }
//            Key::Down => {
//                if self.focused_window.is_some() {
//                    if self.focused_window == self.nt_viewer_window {
//                        self.set_nt_scanline(self.nt_scanline + 1);
//                        return true;
//                    } else if self.focused_window == self.ppu_viewer_window {
//                        self.set_pat_scanline(self.pat_scanline + 1);
//                        return true;
//                    }
//                }
//                false
//            }
//            _ => false,
//        }
//    }

//    /// Handles keydown events
//    // TODO: This is getting to be a large function - should refactor to break it
//    // up and also restructure keybind referencing to allow customization
//    // TODO: abstract out window-specific focused keypresses
//    #[allow(clippy::cognitive_complexity)]
//    pub(super) fn handle_keydown(&mut self, s: &mut PixState, key: Key) -> NesResult<()> {
//        self.held_keys.insert(key as u8, true);
//        let c = self.is_key_held(Key::LCtrl);
//        let shift = self.is_key_held(Key::LShift);
//        let d = self.config.debug;
//        match key {
//            // No modifiers
//            Key::Escape => {
//                // TODO close top menu
//                self.paused(!self.paused);
//            }
//            Key::R if !c => self.rewind(),
//            Key::F1 => {
//                // TODO open help menu
//                self.paused(true);
//                self.add_message("Help Menu not implemented");
//            }
//            // Step/Step Into
//            Key::C if d => {
//                if self.clock() == 0 {
//                    self.clock();
//                }
//            }
//            // Step Over
//            Key::O if !c && d => {
//                let instr = self.cpu.next_instr();
//                if self.clock() == 0 {
//                    self.clock();
//                }
//                if instr.op() == JSR {
//                    let mut op = self.cpu.instr.op();
//                    // TODO disable breakpoints here so 'step over' doesn't break?
//                    while op != RTS {
//                        let _ = self.clock();
//                        op = self.cpu.instr.op();
//                    }
//                }
//            }
//            // Step Out
//            Key::O if c && d => {
//                let mut op = self.cpu.instr.op();
//                while op != RTS {
//                    let _ = self.clock();
//                    op = self.cpu.instr.op();
//                }
//            }
//            // Toggle Active Debug
//            Key::D if d && !c => self.active_debug = !self.active_debug,
//            // Step Frame
//            Key::F if d => self.clock_frame(),
//            // Step Scanline
//            Key::S if d && !c => {
//                let prev_scanline = self.cpu.bus.ppu.scanline;
//                let mut scanline = prev_scanline;
//                while scanline == prev_scanline {
//                    let _ = self.clock();
//                    scanline = self.cpu.bus.ppu.scanline;
//                }
//            }
//            // Ctrl
//            Key::Num1 if c => self.set_save_slot(1),
//            Key::Num2 if c => self.set_save_slot(2),
//            Key::Num3 if c => self.set_save_slot(3),
//            Key::Num4 if c => self.set_save_slot(4),
//            Key::Return if c => {
//                self.config.fullscreen = !self.config.fullscreen;
//                s.fullscreen(self.config.fullscreen);
//            }
//            Key::C if c => {
//                // TODO open config menu
//                self.paused(true);
//            }
//            Key::D if c => self.toggle_debug(s)?,
//            Key::S if c => {
//                let rewind = false;
//                self.save_state(self.config.save_slot, rewind);
//            }
//            Key::L if c => {
//                let rewind = false;
//                self.load_state(self.config.save_slot, rewind);
//            }
//            Key::M if c => {
//                self.config.sound_enabled = !self.config.sound_enabled;
//                if self.config.sound_enabled {
//                    self.add_message("Sound Enabled");
//                } else {
//                    self.add_message("Sound Disabled");
//                }
//            }
//            Key::N if c => self.cpu.bus.ppu.ntsc_video = !self.cpu.bus.ppu.ntsc_video,
//            Key::O if c => {
//                // TODO open rom menu
//                self.paused(true);
//            }
//            Key::Q if c => self.should_close = true,
//            Key::R if c => {
//                self.paused(false);
//                self.reset();
//                self.add_message("Reset");
//            }
//            Key::P if c && !shift => {
//                self.paused(false);
//                self.power_cycle();
//                self.add_message("Power Cycled");
//            }
//            Key::V if c => {
//                self.config.vsync = !self.config.vsync;
//                todo!("toggle vsync");
//                // s.vsync(self.config.vsync)?;
//                // if self.config.vsync {
//                //     self.add_message("Vsync Enabled");
//                // } else {
//                //     self.add_message("Vsync Disabled");
//                // }
//            }
//            // Shift
//            Key::Num1 if shift => self.cpu.bus.apu.toggle_pulse1(),
//            Key::Num2 if shift => self.cpu.bus.apu.toggle_pulse2(),
//            Key::Num3 if shift => self.cpu.bus.apu.toggle_triangle(),
//            Key::Num4 if shift => self.cpu.bus.apu.toggle_noise(),
//            Key::Num5 if shift => self.cpu.bus.apu.toggle_dmc(),
//            Key::N if shift => self.toggle_nt_viewer(s)?,
//            Key::P if shift => self.toggle_ppu_viewer(s)?,
//            Key::V if shift => {
//                self.recording = !self.recording;
//                if self.recording {
//                    self.add_message("Recording Started");
//                } else {
//                    self.add_message("Recording Stopped");
//                    self.save_replay()?;
//                }
//            }
//            // F# Keys
//            Key::F9 => {} // TODO change log level
//            Key::F10 => match self.screenshot() {
//                Ok(s) => self.add_message(&s),
//                Err(e) => self.add_message(&e.to_string()),
//            },
//            _ => {
//                let handled = self.handle_scanline_key(key);
//                if !handled {
//                    self.handle_input_event(key, true);
//                }
//            }
//        }
//        Ok(())
//    }

//    /// Handles controller gamepad button events
//    fn handle_gamepad_button(&mut self, event: Event) -> NesResult<()> {
//        // Gamepad events only apply to the main window
//        if self.focused_window != Some(self.nes_window) {
//            return Ok(());
//        }
//        let (controller_id, button, pressed) = match event {
//            Event::ControllerDown {
//                controller_id,
//                button,
//            } => (controller_id, button, true),
//            Event::ControllerUp {
//                controller_id,
//                button,
//            } => (controller_id, button, false),
//            _ => return Ok(()),
//        };

//        let input = &mut self.cpu.bus.input;
//        let mut gamepad = match controller_id {
//            0 => &mut input.gamepad1,
//            1 => &mut input.gamepad2,
//            _ => panic!("invalid gamepad id: {}", controller_id),
//        };
//        match button {
//            Button::Guide if pressed => self.paused(!self.paused),
//            Button::LeftShoulder if pressed => self.change_speed(-0.25),
//            Button::RightShoulder if pressed => self.change_speed(0.25),
//            Button::A => {
//                gamepad.a = pressed;
//            }
//            Button::B => gamepad.b = pressed,
//            Button::X => {
//                gamepad.turbo_a = pressed;
//                gamepad.a = self.turbo && pressed;
//            }
//            Button::Y => {
//                gamepad.turbo_b = pressed;
//                gamepad.b = self.turbo && pressed;
//            }
//            Button::Back => gamepad.select = pressed,
//            Button::Start => gamepad.start = pressed,
//            Button::DPadUp => gamepad.up = pressed,
//            Button::DPadDown => gamepad.down = pressed,
//            Button::DPadLeft => gamepad.left = pressed,
//            Button::DPadRight => gamepad.right = pressed,
//            _ => {}
//        }
//        Ok(())
//    }

//    /// Handle controller gamepad joystick events
//    fn handle_gamepad_axis(&mut self, event: Event) -> NesResult<()> {
//        // Gamepad events only apply to the main window
//        if self.focused_window != Some(self.nes_window) {
//            return Ok(());
//        }
//        if let Event::ControllerAxisMotion {
//            controller_id,
//            axis,
//            value,
//        } = event
//        {
//            let input = &mut self.cpu.bus.input;
//            let mut gamepad = match controller_id {
//                0 => &mut input.gamepad1,
//                1 => &mut input.gamepad2,
//                _ => panic!("invalid gamepad id: {}", controller_id),
//            };
//            match axis {
//                // Left/Right
//                Axis::LeftX => {
//                    if value < -GAMEPAD_AXIS_DEADZONE {
//                        gamepad.left = true;
//                    } else if value > GAMEPAD_AXIS_DEADZONE {
//                        gamepad.right = true;
//                    } else {
//                        gamepad.left = false;
//                        gamepad.right = false;
//                    }
//                }
//                // Down/Up
//                Axis::LeftY => {
//                    if value < -GAMEPAD_AXIS_DEADZONE {
//                        gamepad.up = true;
//                    } else if value > GAMEPAD_AXIS_DEADZONE {
//                        gamepad.down = true;
//                    } else {
//                        gamepad.up = false;
//                        gamepad.down = false;
//                    }
//                }
//                Axis::TriggerLeft if value > GAMEPAD_TRIGGER_PRESS => {
//                    let rewind = false;
//                    self.save_state(self.config.save_slot, rewind);
//                }
//                Axis::TriggerRight if value > GAMEPAD_TRIGGER_PRESS => {
//                    let rewind = false;
//                    self.load_state(self.config.save_slot, rewind);
//                }
//                _ => (),
//            }
//        }
//        Ok(())
//    }
