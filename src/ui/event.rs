use crate::{
    ui::{settings::DEFAULT_SPEED, Message, Ui, REWIND_TIMER},
    util, NesResult,
};
use pix_engine::{
    event::{Axis, Button, Key, Mouse, PixEvent},
    StateData,
};

const GAMEPAD_AXIS_DEADZONE: i16 = 8000;

impl Ui {
    fn rewind(&mut self) -> NesResult<()> {
        match self.rewind_queue.pop_back() {
            Some(slot) => {
                self.rewind_timer = REWIND_TIMER;
                self.messages
                    .push(Message::new(format!("Rewind Slot {}", slot)));
                self.rewind_save = slot + 1;
                self.console.load_state(slot)
            }
            None => Ok(()),
        }
    }

    pub(super) fn poll_events(&mut self, data: &mut StateData) -> NesResult<()> {
        let turbo = self.turbo_clock < 3;
        self.clock_turbo(turbo);
        for event in data.poll() {
            match event {
                PixEvent::Focus(focus) => {
                    if focus {
                        if !self.focused {
                            self.focused = true;
                            self.paused(false);
                        }
                    } else if !self.paused {
                        self.focused = false;
                        self.paused(true);
                    }
                }
                PixEvent::KeyPress(key, pressed, repeat) => {
                    self.handle_key_event(key, pressed, repeat, turbo, data)?;
                }
                PixEvent::GamepadBtn(which, btn, pressed) => match btn {
                    Button::Guide => self.paused(!self.paused),
                    Button::Back if pressed => self.rewind()?,
                    Button::LeftShoulder if pressed => self.change_speed(-0.25),
                    Button::RightShoulder if pressed => self.change_speed(0.25),
                    _ => self.handle_gamepad_button(which, btn, pressed, turbo)?,
                },
                PixEvent::GamepadAxis(which, axis, value) => {
                    self.handle_gamepad_axis(which, axis, value)?
                }
                _ => (),
            }
        }
        Ok(())
    }

    fn clock_turbo(&mut self, turbo: bool) {
        let mut input = self.input.borrow_mut();
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
        key: Key,
        pressed: bool,
        repeat: bool,
        turbo: bool,
        data: &mut StateData,
    ) -> NesResult<()> {
        // Keydown or Keyup
        match key {
            Key::Ctrl => self.ctrl = pressed,
            Key::LShift => self.shift = pressed,
            _ if !self.ctrl && !self.shift => self.handle_input_event(key, pressed, turbo),
            _ => (),
        }

        if pressed {
            match key {
                Key::Comma => self.rewind()?,
                // Key::Period => self.rewind_forward()?,
                // Debug =======================================================================
                Key::C if self.settings.debug => {
                    let _ = self.console.clock();
                }
                Key::S if self.settings.debug => {
                    let prev_scanline = self.console.cpu.mem.ppu.scanline;
                    let mut scanline = prev_scanline;
                    while scanline == prev_scanline {
                        let _ = self.console.clock();
                        scanline = self.console.cpu.mem.ppu.scanline;
                    }
                }
                Key::F if self.settings.debug => self.console.clock_frame(),
                Key::D if !self.ctrl && self.settings.debug => {
                    self.active_debug = !self.active_debug
                }
                _ => (),
            }
            if !repeat {
                // Keydown
                if self.ctrl {
                    match key {
                        // UI ==========================================================================
                        Key::Return => {
                            self.settings.fullscreen = !self.settings.fullscreen;
                            data.fullscreen(self.settings.fullscreen);
                        }
                        Key::V if self.shift => {
                            self.settings.vsync = !self.settings.vsync;
                            data.vsync(self.settings.vsync);
                        }
                        Key::V if !self.shift => eprintln!("Recording not implemented"), // TODO
                        Key::M => self.settings.sound_enabled = !self.settings.sound_enabled,
                        // Open
                        Key::O => eprintln!("Open Dialog not implemented"), // TODO
                        // Reset
                        Key::R => {
                            self.paused = false;
                            self.console.reset();
                        }
                        // Power Cycle
                        Key::P => {
                            self.paused = false;
                            self.console.power_cycle();
                        }
                        // Change speed
                        Key::Minus => self.change_speed(-0.25),
                        Key::Equals => self.change_speed(0.25),
                        // Save/Load
                        Key::S => {
                            if self.settings.save_enabled {
                                self.console.save_state(self.settings.save_slot)?;
                                self.messages.push(Message::new(format!(
                                    "Saved Slot {}",
                                    self.settings.save_slot
                                )));
                            } else {
                                self.messages
                                    .push(Message::new("Saved States Disabled".into()));
                            }
                        }
                        Key::L => {
                            if self.settings.save_enabled {
                                self.console.load_state(self.settings.save_slot)?;
                                self.messages.push(Message::new(format!(
                                    "Loaded Slot {}",
                                    self.settings.save_slot
                                )));
                            } else {
                                self.messages
                                    .push(Message::new("Saved States Disabled".into()));
                            }
                        }
                        Key::Num1 => self.settings.save_slot = 1,
                        Key::Num2 => self.settings.save_slot = 2,
                        Key::Num3 => self.settings.save_slot = 3,
                        Key::Num4 => self.settings.save_slot = 4,
                        // Debug =======================================================================
                        Key::D => self.toggle_debug(data),
                        _ => (),
                    }
                } else {
                    match key {
                        // UI ==========================================================================
                        Key::Escape => self.paused(!self.paused), // TODO menu
                        // Fast-forward
                        Key::Space => {
                            self.settings.speed = 2.0;
                            self.console.set_speed(self.settings.speed);
                        }
                        // Utilities ===================================================================
                        Key::F9 => eprintln!("Toggle Logging Setting not implemented"), // TODO
                        Key::F10 => util::screenshot(&self.console.frame()),
                        _ => (),
                    }
                }
            }
        } else {
            // Keyup
            if let Key::Space = key {
                self.settings.speed = DEFAULT_SPEED;
                self.console.set_speed(self.settings.speed);
            }
        }
        Ok(())
    }

    fn handle_input_event(&mut self, key: Key, pressed: bool, turbo: bool) {
        let mut input = self.input.borrow_mut();
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
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.down = false;
                }
                input.gamepad1.up = pressed;
            }
            Key::Down => {
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.up = false;
                }
                input.gamepad1.down = pressed;
            }
            Key::Left => {
                if !self.settings.concurrent_dpad && pressed {
                    input.gamepad1.right = false;
                }
                input.gamepad1.left = pressed;
            }
            Key::Right => {
                if !self.settings.concurrent_dpad && pressed {
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
        let mut input = self.input.borrow_mut();
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
        let mut input = self.input.borrow_mut();
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
            _ => (),
        }
        Ok(())
    }
}
