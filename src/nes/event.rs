use crate::{
    common::{create_png, Clocked, Powered},
    cpu::Operation::*,
    logging::LogLevel,
    nes::{config::DEFAULT_SPEED, Nes, REWIND_TIMER},
    nes_err,
    serialization::Savable,
    NesResult,
};
use chrono::prelude::{DateTime, Local};
use pix_engine::{
    event::{Axis, Button, Key, Mouse, PixEvent},
    StateData,
};
use std::io::{BufWriter, Read, Write};

const GAMEPAD_TRIGGER_PRESS: i16 = 32_700;
const GAMEPAD_AXIS_DEADZONE: i16 = 10_000;

impl Nes {
    fn rewind(&mut self) {
        if self.config.save_enabled {
            if self.config.rewind_enabled {
                // If we saved too recently, ignore it and go back further
                if self.rewind_timer > 3.0 {
                    let _ = self.rewind_queue.pop_back();
                }
                if let Some(slot) = self.rewind_queue.pop_back() {
                    self.rewind_timer = REWIND_TIMER;
                    self.add_message(&format!("Rewind Slot {}", slot));
                    self.rewind_save = slot + 1;
                    self.load_state(slot);
                }
            } else {
                self.add_message("Rewind disabled");
            }
        } else {
            self.add_message("Savestates Disabled");
        }
    }

    pub(super) fn poll_events(&mut self, data: &mut StateData) -> NesResult<()> {
        let turbo = self.turbo_clock < 3;
        self.clock_turbo(turbo);
        let events = if self.playback && self.replay_frame < self.replay_buffer.len() {
            if let Some(events) = self.replay_buffer.get(self.replay_frame) {
                events.to_vec()
            } else {
                self.playback = false;
                data.poll()
            }
        } else {
            data.poll()
        };
        if self.recording && !self.playback {
            self.replay_buffer.push(Vec::new());
        }
        for event in events {
            match event {
                PixEvent::WinClose(window_id) => match Some(window_id) {
                    i if i == self.ppu_viewer_window => self.toggle_ppu_viewer(data)?,
                    i if i == self.nt_viewer_window => self.toggle_nt_viewer(data)?,
                    _ => (),
                },
                PixEvent::Focus(window_id, focus) => {
                    self.focused_window = if focus { window_id } else { 0 };
                }
                PixEvent::KeyPress(..) => self.handle_key_event(event, turbo, data)?,
                PixEvent::GamepadBtn(which, btn, pressed) => match btn {
                    Button::Guide if pressed => self.paused(!self.paused),
                    Button::LeftShoulder if pressed => self.change_speed(-0.25),
                    Button::RightShoulder if pressed => self.change_speed(0.25),
                    _ => {
                        if self.recording && !self.playback {
                            self.replay_buffer[self.replay_frame].push(event);
                        }
                        self.handle_gamepad_button(which, btn, pressed, turbo)?;
                    }
                },
                PixEvent::GamepadAxis(which, axis, value) => {
                    self.handle_gamepad_axis(which, axis, value)?
                }
                _ => (),
            }
        }
        self.replay_frame += 1;
        Ok(())
    }

    fn clock_turbo(&mut self, turbo: bool) {
        let mut input = &mut self.cpu.bus.input;
        if input.gamepad1.turbo_a {
            input.gamepad1.a = turbo;
        }
        if input.gamepad1.turbo_b {
            input.gamepad1.b = turbo;
        }
        if input.gamepad2.turbo_a {
            input.gamepad2.a = turbo;
        }
        if input.gamepad2.turbo_b {
            input.gamepad2.b = turbo;
        }
    }

    fn handle_key_event(
        &mut self,
        event: PixEvent,
        turbo: bool,
        data: &mut StateData,
    ) -> NesResult<()> {
        if self.recording && !self.playback {
            if let PixEvent::KeyPress(key, ..) = event {
                match key {
                    Key::A
                    | Key::S
                    | Key::Z
                    | Key::X
                    | Key::Return
                    | Key::RShift
                    | Key::Left
                    | Key::Right
                    | Key::Up
                    | Key::Down => {
                        self.replay_buffer[self.replay_frame].push(event);
                    }
                    _ => (),
                }
            }
        }
        match event {
            PixEvent::KeyPress(key, true, true) => self.handle_keyrepeat(key, data),
            PixEvent::KeyPress(key, true, false) => self.handle_keydown(key, turbo, data)?,
            PixEvent::KeyPress(key, false, ..) => self.handle_keyup(key, turbo),
            _ => (),
        }
        Ok(())
    }

    fn handle_keyrepeat(&mut self, key: Key, data: &mut StateData) {
        let c = data.get_key(Key::Ctrl).held;
        let d = self.config.debug;
        match key {
            // No modifiers
            // Step/Step Into
            Key::C if d => {
                let _ = self.clock();
            }
            // Step Frame
            Key::F if d => self.clock_frame(),
            // Step Scanline
            Key::S if d && !c => {
                let prev_scanline = self.cpu.bus.ppu.scanline;
                let mut scanline = prev_scanline;
                while scanline == prev_scanline {
                    let _ = self.clock();
                    scanline = self.cpu.bus.ppu.scanline;
                }
            }
            // Nametable/PPU Viewer Shortcuts
            Key::Up => {
                if Some(self.focused_window) == self.nt_viewer_window {
                    self.set_nt_scanline(self.nt_scanline.saturating_sub(1));
                } else {
                    self.set_pat_scanline(self.pat_scanline.saturating_sub(1));
                }
            }
            Key::Down => {
                if Some(self.focused_window) == self.nt_viewer_window {
                    self.set_nt_scanline(self.nt_scanline + 1);
                } else {
                    self.set_pat_scanline(self.pat_scanline + 1);
                }
            }
            _ => (),
        }
    }

    #[allow(clippy::cognitive_complexity)]
    fn handle_keydown(&mut self, key: Key, turbo: bool, data: &mut StateData) -> NesResult<()> {
        let c = data.get_key(Key::Ctrl).held;
        let s = data.get_key(Key::LShift).held;
        let d = self.config.debug;
        match key {
            // No modifiers
            Key::Escape => self.paused(!self.paused),
            Key::Space => self.change_speed(1.0),
            Key::Comma => self.rewind(),
            // Step/Step Into
            Key::C if d => {
                if self.clock() == 0 {
                    self.clock();
                }
            }
            // Step Over
            Key::O if !c && d => {
                let instr = self.cpu.next_instr();
                if self.clock() == 0 {
                    self.clock();
                }
                if instr.op() == JSR {
                    let mut op = self.cpu.instr.op();
                    // TODO disable breakpoints here so 'step over' doesn't break?
                    while op != RTS {
                        let _ = self.clock();
                        op = self.cpu.instr.op();
                    }
                }
            }
            // Step Out
            Key::O if c && d => {
                let mut op = self.cpu.instr.op();
                while op != RTS {
                    let _ = self.clock();
                    op = self.cpu.instr.op();
                }
            }
            // Toggle Active Debug
            Key::D if d && !c => self.active_debug = !self.active_debug,
            // Step Frame
            Key::F if d => self.clock_frame(),
            // Step Scanline
            Key::S if d && !c => {
                let prev_scanline = self.cpu.bus.ppu.scanline;
                let mut scanline = prev_scanline;
                while scanline == prev_scanline {
                    let _ = self.clock();
                    scanline = self.cpu.bus.ppu.scanline;
                }
            }
            // Ctrl
            Key::Num1 if c => self.config.save_slot = 1,
            Key::Num2 if c => self.config.save_slot = 2,
            Key::Num3 if c => self.config.save_slot = 3,
            Key::Num4 if c => self.config.save_slot = 4,
            Key::Minus if c => self.change_speed(-0.25),
            Key::Equals if c => self.change_speed(0.25),
            Key::Return if c => {
                self.config.fullscreen = !self.config.fullscreen;
                data.fullscreen(self.config.fullscreen)?;
            }
            Key::C if c => {
                self.menu = !self.menu;
                self.paused(self.menu);
            }
            Key::D if c => self.toggle_debug(data)?,
            Key::S if c => self.save_state(self.config.save_slot, false),
            Key::L if c => self.load_state(self.config.save_slot),
            Key::M if c => {
                if self.config.unlock_fps {
                    self.add_message("Sound disabled while FPS unlocked");
                } else {
                    self.config.sound_enabled = !self.config.sound_enabled;
                    if self.config.sound_enabled {
                        self.add_message("Sound Enabled");
                    } else {
                        self.add_message("Sound Disabled");
                    }
                }
            }
            Key::N if c => self.cpu.bus.ppu.ntsc_video = !self.cpu.bus.ppu.ntsc_video,
            Key::O if c => self.add_message("Open Dialog not implemented"), // TODO
            Key::Q if c => self.should_close = true,
            Key::R if c => {
                self.paused(false);
                self.reset();
                self.add_message("Reset");
            }
            Key::P if c && !s => {
                self.paused(false);
                self.power_cycle();
                self.add_message("Power Cycled");
            }
            Key::V if c => {
                self.config.vsync = !self.config.vsync;
                data.vsync(self.config.vsync)?;
                if self.config.vsync {
                    self.add_message("Vsync Enabled");
                } else {
                    self.add_message("Vsync Disabled");
                }
            }
            // Shift
            Key::N if s => self.toggle_nt_viewer(data)?,
            Key::P if s => self.toggle_ppu_viewer(data)?,
            Key::V if s => {
                self.recording = !self.recording;
                if self.recording {
                    self.add_message("Recording Started");
                } else {
                    self.add_message("Recording Stopped");
                    self.save_replay()?;
                }
            }
            // F# Keys
            Key::F9 => {
                self.config.log_level = LogLevel::increase(self.config.log_level);
                self.set_log_level(self.config.log_level, false);
            }
            Key::F10 => match self.screenshot() {
                Ok(s) => self.add_message(&s),
                Err(e) => self.add_message(&e.to_string()),
            },
            _ => {
                if Some(self.focused_window) == self.nt_viewer_window {
                    match key {
                        Key::Up => self.set_nt_scanline(self.nt_scanline.saturating_sub(1)),
                        Key::Down => self.set_nt_scanline(self.nt_scanline + 1),
                        _ => (),
                    }
                } else if Some(self.focused_window) == self.ppu_viewer_window {
                    match key {
                        Key::Up => self.set_pat_scanline(self.pat_scanline.saturating_sub(1)),
                        Key::Down => self.set_pat_scanline(self.pat_scanline + 1),
                        _ => (),
                    }
                } else {
                    self.handle_input_event(key, true, turbo);
                }
            }
        }
        Ok(())
    }

    fn handle_keyup(&mut self, key: Key, turbo: bool) {
        match key {
            Key::Space => {
                self.config.speed = DEFAULT_SPEED;
                self.cpu.bus.apu.set_speed(self.config.speed);
            }
            _ => self.handle_input_event(key, false, turbo),
        }
    }

    fn handle_input_event(&mut self, key: Key, pressed: bool, turbo: bool) {
        if self.focused_window != self.nes_window {
            return;
        }

        let mut input = &mut self.cpu.bus.input;
        match key {
            // Gamepad
            Key::Z => input.gamepad1.a = pressed,
            Key::X => input.gamepad1.b = pressed,
            Key::A => {
                input.gamepad1.turbo_a = pressed;
                input.gamepad1.a = turbo && pressed;
            }
            Key::S => {
                input.gamepad1.turbo_b = pressed;
                input.gamepad1.b = turbo && pressed;
            }
            Key::RShift => input.gamepad1.select = pressed,
            Key::Return => input.gamepad1.start = pressed,
            Key::Up => {
                if !self.config.concurrent_dpad && pressed {
                    input.gamepad1.down = false;
                }
                input.gamepad1.up = pressed;
            }
            Key::Down => {
                if !self.config.concurrent_dpad && pressed {
                    input.gamepad1.up = false;
                }
                input.gamepad1.down = pressed;
            }
            Key::Left => {
                if !self.config.concurrent_dpad && pressed {
                    input.gamepad1.right = false;
                }
                input.gamepad1.left = pressed;
            }
            Key::Right => {
                if !self.config.concurrent_dpad && pressed {
                    input.gamepad1.left = false;
                }
                input.gamepad1.right = pressed;
            }
            _ => (),
        }
    }

    fn handle_gamepad_button(
        &mut self,
        gamepad_id: i32,
        button: Button,
        pressed: bool,
        turbo: bool,
    ) -> NesResult<()> {
        if self.focused_window != self.nes_window {
            return Ok(());
        }

        let input = &mut self.cpu.bus.input;
        let mut gamepad = match gamepad_id {
            0 => &mut input.gamepad1,
            1 => &mut input.gamepad2,
            _ => panic!("invalid gamepad id: {}", gamepad_id),
        };
        match button {
            Button::A => {
                gamepad.a = pressed;
            }
            Button::B => gamepad.b = pressed,
            Button::X => {
                gamepad.turbo_a = pressed;
                gamepad.a = turbo && pressed;
            }
            Button::Y => {
                gamepad.turbo_b = pressed;
                gamepad.b = turbo && pressed;
            }
            Button::Back => gamepad.select = pressed,
            Button::Start => gamepad.start = pressed,
            Button::DPadUp => gamepad.up = pressed,
            Button::DPadDown => gamepad.down = pressed,
            Button::DPadLeft => gamepad.left = pressed,
            Button::DPadRight => gamepad.right = pressed,
            _ => {}
        }
        Ok(())
    }
    fn handle_gamepad_axis(&mut self, gamepad_id: i32, axis: Axis, value: i16) -> NesResult<()> {
        if self.focused_window != self.nes_window {
            return Ok(());
        }

        let input = &mut self.cpu.bus.input;
        let mut gamepad = match gamepad_id {
            0 => &mut input.gamepad1,
            1 => &mut input.gamepad2,
            _ => panic!("invalid gamepad id: {}", gamepad_id),
        };
        match axis {
            // Left/Right
            Axis::LeftX => {
                if value < -GAMEPAD_AXIS_DEADZONE {
                    gamepad.left = true;
                } else if value > GAMEPAD_AXIS_DEADZONE {
                    gamepad.right = true;
                } else {
                    gamepad.left = false;
                    gamepad.right = false;
                }
            }
            // Down/Up
            Axis::LeftY => {
                if value < -GAMEPAD_AXIS_DEADZONE {
                    gamepad.up = true;
                } else if value > GAMEPAD_AXIS_DEADZONE {
                    gamepad.down = true;
                } else {
                    gamepad.up = false;
                    gamepad.down = false;
                }
            }
            Axis::TriggerLeft if value > GAMEPAD_TRIGGER_PRESS => {
                self.save_state(self.config.save_slot, false)
            }
            Axis::TriggerRight if value > GAMEPAD_TRIGGER_PRESS => {
                self.load_state(self.config.save_slot)
            }
            _ => (),
        }
        Ok(())
    }

    pub fn save_replay(&mut self) -> NesResult<()> {
        use std::path::PathBuf;

        let datetime: DateTime<Local> = Local::now();
        let mut path = PathBuf::from(
            datetime
                .format("Recording_%Y-%m-%d_at_%H.%M.%S")
                .to_string(),
        );
        path.set_extension("dat");
        let file = std::fs::File::create(&path)?;
        let mut file = BufWriter::new(file);
        self.replay_buffer.save(&mut file)?;
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
    pub fn screenshot(&mut self) -> NesResult<String> {
        use std::path::PathBuf;

        let datetime: DateTime<Local> = Local::now();
        let mut png_path = PathBuf::from(
            datetime
                .format("Screen_Shot_%Y-%m-%d_at_%H_%M_%S")
                .to_string(),
        );
        let pixels = self.cpu.bus.ppu.frame();
        png_path.set_extension("png");
        create_png(&png_path, pixels)
    }
}

impl Savable for PixEvent {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        match *self {
            PixEvent::GamepadBtn(id, button, pressed) => {
                0u8.save(fh)?;
                id.save(fh)?;
                button.save(fh)?;
                pressed.save(fh)?;
            }
            PixEvent::GamepadAxis(id, axis, value) => {
                1u8.save(fh)?;
                id.save(fh)?;
                axis.save(fh)?;
                value.save(fh)?;
            }
            PixEvent::KeyPress(key, pressed, repeat) => {
                2u8.save(fh)?;
                key.save(fh)?;
                pressed.save(fh)?;
                repeat.save(fh)?;
            }
            _ => (),
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => {
                let mut id: i32 = 0;
                let mut btn = Button::default();
                let mut pressed = false;
                id.load(fh)?;
                btn.load(fh)?;
                pressed.load(fh)?;
                PixEvent::GamepadBtn(id, btn, pressed)
            }
            1 => {
                let mut id: i32 = 0;
                let mut axis = Axis::default();
                let mut value = 0;
                id.load(fh)?;
                axis.load(fh)?;
                value.load(fh)?;
                PixEvent::GamepadAxis(id, axis, value)
            }
            2 => {
                let mut key = Key::default();
                let mut pressed = false;
                let mut repeat = false;
                key.load(fh)?;
                pressed.load(fh)?;
                repeat.load(fh)?;
                PixEvent::KeyPress(key, pressed, repeat)
            }
            _ => return nes_err!("invalid PixEvent value"),
        };
        Ok(())
    }
}

impl Savable for Button {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Button::A,
            1 => Button::B,
            2 => Button::X,
            3 => Button::Y,
            4 => Button::Back,
            5 => Button::Start,
            6 => Button::Guide,
            7 => Button::DPadUp,
            8 => Button::DPadDown,
            9 => Button::DPadLeft,
            10 => Button::DPadRight,
            11 => Button::LeftStick,
            12 => Button::RightStick,
            13 => Button::LeftShoulder,
            14 => Button::RightShoulder,
            _ => nes_err!("invalid Button value")?,
        };
        Ok(())
    }
}

impl Savable for Axis {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Axis::LeftX,
            1 => Axis::RightX,
            2 => Axis::LeftY,
            3 => Axis::RightY,
            4 => Axis::TriggerLeft,
            5 => Axis::TriggerRight,
            _ => nes_err!("invalid Axis value")?,
        };
        Ok(())
    }
}

impl Savable for Key {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        let val: u8 = match *self {
            Key::A => 0, // Turbo A
            Key::S => 1, // Turbo B
            Key::X => 2, // A
            Key::Z => 3, // B
            Key::Left => 4,
            Key::Up => 5,
            Key::Down => 6,
            Key::Right => 7,
            Key::Return => 8, // Start
            Key::RShift => 9, // Select
            _ => return Ok(()),
        };
        val.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Key::A, // Turbo A
            1 => Key::S, // Turbo B
            2 => Key::X, // A
            3 => Key::Z, // B
            4 => Key::Left,
            5 => Key::Up,
            6 => Key::Down,
            7 => Key::Right,
            8 => Key::Return, // Start
            9 => Key::RShift, // Select
            _ => nes_err!("invalid Key value")?,
        };
        Ok(())
    }
}

impl Savable for Mouse {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Mouse::Left,
            1 => Mouse::Middle,
            2 => Mouse::Right,
            3 => Mouse::X1,
            4 => Mouse::X2,
            5 => Mouse::Unknown,
            _ => nes_err!("invalid Mouse value")?,
        };
        Ok(())
    }
}
