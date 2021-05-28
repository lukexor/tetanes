use super::Nes;
use pix_engine::prelude::*;

const GAMEPAD_TRIGGER_PRESS: i16 = 32_700;
const GAMEPAD_AXIS_DEADZONE: i16 = 10_000;

impl Nes {
    pub(super) fn handle_key_pressed(
        &mut self,
        s: &mut PixState,
        key: Key,
        repeat: bool,
    ) -> PixResult<()> {
        if !s.focused() {
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn handle_key_released(&mut self, s: &mut PixState, key: Key) -> PixResult<()> {
        if !s.focused() {
            return Ok(());
        }
        Ok(())
    }
}
//    // /// This is called on every update loop to check for user events like quitting
//    // /// the application, pressing a button, resizing the window or clicking the mouse.
//    // pub(super) fn poll_events(&mut self, s: &mut PixState) -> NesResult<()> {
//    //     let turbo = self.clock_turbo();
//    //     let events = self.get_events(s);
//    //     if self.recording {
//    //         self.replay_buffer
//    //             .push(FrameEvent::new(self.frame, events.clone()));
//    //     }
//    //     let mut gamepad_events = Vec::new();
//    //     for event in events {
//    //         // Process system events
//    //         match event {
//    //             Event::Window {
//    //                 window_id,
//    //                 win_event,
//    //             } => match win_event {
//    //                 WindowEvent::FocusGained => self.focused_window = Some(window_id),
//    //                 WindowEvent::FocusLost => self.focused_window = None,
//    //                 WindowEvent::Close => match Some(window_id) {
//    //                     i if i == self.ppu_viewer_window => self.toggle_ppu_viewer(s)?,
//    //                     i if i == self.nt_viewer_window => self.toggle_nt_viewer(s)?,
//    //                     _ => (),
//    //                 },
//    //                 _ => (),
//    //             },
//    //             _ => (),
//    //         }

//    //         // Only process remaining events if we're focused
//    //         if !self.playback && self.focused_window.is_none() {
//    //             continue;
//    //         }
//    //         if Self::is_gamepad_event(&event) {
//    //             gamepad_events.push(event.clone());
//    //         }
//    //         match event {
//    //             Event::KeyDown { .. } => self.handle_key_event(event, turbo, s)?,
//    //             Event::MouseDown { .. } => self.handle_mouse_event(event)?,
//    //             Event::ControllerDown { .. } => self.handle_gamepad_button(event, turbo)?,
//    //             Event::ControllerAxisMotion { .. } => self.handle_gamepad_axis(event)?,
//    //             _ => (),
//    //         }
//    //     }
//    //     self.frame += 1;
//    //     Ok(())
//    // }

//    // fn get_events(&mut self, s: &mut PixState) -> Vec<Event> {
//    //     // Get the list events, either from the user or from a replay_buffer
//    //     let mut events: Vec<Event> = Vec::new();
//    //     if self.playback && !self.replay_buffer.is_empty() {
//    //         let frame_event = self.replay_buffer.pop().unwrap();
//    //         if frame_event.frame == self.frame {
//    //             events.extend(frame_event.events);
//    //         }
//    //     } else {
//    //         self.playback = false;
//    //         events.extend(s.poll_events());
//    //     };
//    //     events
//    // }

//    /// Turbo clock counts up every frame from 0-5
//    /// When it's less than 3, we toggle the button currently held down
//    /// This gives the effect of pressing a button quickly every 3 frames or
//    /// every ~48 milliseconds
//    pub(super) fn clock_turbo(&mut self) {
//        self.turbo = self.turbo_clock < 3;
//        let mut input = &mut self.cpu.bus.input;
//        if input.gamepad1.turbo_a {
//            input.gamepad1.a = self.turbo;
//        }
//        if input.gamepad1.turbo_b {
//            input.gamepad1.b = self.turbo;
//        }
//        if input.gamepad2.turbo_a {
//            input.gamepad2.a = self.turbo;
//        }
//        if input.gamepad2.turbo_b {
//            input.gamepad2.b = self.turbo;
//        }
//    }

//    /// Handles all mouse related events
//    #[allow(clippy::many_single_char_names)]
//    fn handle_mouse_event(&mut self, event: Event) -> NesResult<()> {
//        let (x, y, pressed) = match event {
//            Event::MouseDown {
//                button: Mouse::Left,
//                x,
//                y,
//            } => (x, y, true),
//            Event::MouseUp {
//                button: Mouse::Left,
//                x,
//                y,
//            } => (x, y, false),
//            _ => return Ok(()),
//        };

//        self.cpu.bus.input.zapper.triggered = pressed;
//        if pressed && x > 0 && x < self.width as i32 && y > 0 && y < self.height as i32 {
//            let x = x as u32 / self.config.scale;
//            let y = y as u32 / self.config.scale;
//            let frame = &self.cpu.bus.ppu.frame();
//            // Compute average brightness
//            let mut r = 0u16;
//            let mut g = 0u16;
//            let mut b = 0u16;
//            for x in x.saturating_sub(8)..x.saturating_add(8) {
//                for y in y.saturating_sub(8)..y.saturating_add(8) {
//                    let idx = 4 * (y * RENDER_WIDTH + x) as usize;
//                    r += u16::from(frame[idx]);
//                    g += u16::from(frame[idx + 1]);
//                    b += u16::from(frame[idx + 2]);
//                }
//            }
//            r /= 256;
//            g /= 256;
//            b /= 256;
//            let luminance = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) as u32;
//            self.cpu.bus.input.zapper.light_sense = luminance < 84;
//            self.zapper_decay = (luminance / 10) * 113;
//            // println!(
//            //     "lum: {}, sense: {}, decay: {}",
//            //     luminance, self.cpu.bus.input.zapper.light_sense, self.zapper_decay
//            // );
//        }
//        Ok(())
//    }

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
//            Key::Space => self.set_speed(2.0),
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
//            Key::Minus if c => self.change_speed(-0.25),
//            Key::Equals if c => self.change_speed(0.25),
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

//    /// Handles gamepad events from the keyboard.
//    // TODO: Update this to allow up to 4 players
//    pub(super) fn handle_input_event(&mut self, key: Key, pressed: bool) {
//        // Gamepad events only apply to the main window
//        if self.focused_window != Some(self.nes_window) {
//            return;
//        }
//        let mut input = &mut self.cpu.bus.input;
//        match key {
//            // Gamepad
//            Key::Z => input.gamepad1.a = pressed,
//            Key::X => input.gamepad1.b = pressed,
//            Key::A => {
//                input.gamepad1.turbo_a = pressed;
//                input.gamepad1.a = self.turbo && pressed;
//            }
//            Key::S => {
//                input.gamepad1.turbo_b = pressed;
//                input.gamepad1.b = self.turbo && pressed;
//            }
//            Key::RShift => input.gamepad1.select = pressed,
//            Key::Return => input.gamepad1.start = pressed,
//            Key::Up => {
//                if !self.config.concurrent_dpad && pressed {
//                    input.gamepad1.down = false;
//                }
//                input.gamepad1.up = pressed;
//            }
//            Key::Down => {
//                if !self.config.concurrent_dpad && pressed {
//                    input.gamepad1.up = false;
//                }
//                input.gamepad1.down = pressed;
//            }
//            Key::Left => {
//                if !self.config.concurrent_dpad && pressed {
//                    input.gamepad1.right = false;
//                }
//                input.gamepad1.left = pressed;
//            }
//            Key::Right => {
//                if !self.config.concurrent_dpad && pressed {
//                    input.gamepad1.left = false;
//                }
//                input.gamepad1.right = pressed;
//            }
//            _ => (),
//        }
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

//    /// Takes a screenshot and saves it to the current directory as a `.png` file
//    ///
//    /// # Arguments
//    ///
//    /// * `pixels` - An array of pixel data to save in `.png` format
//    ///
//    /// # Errors
//    ///
//    /// It's possible for this method to fail, but instead of erroring the program,
//    /// it'll simply log the error out to STDERR
//    // TODO Scale screenshot to current width/height
//    // TODO Screenshot the currently focused window
//    fn screenshot(&mut self) -> NesResult<String> {
//        let datetime: DateTime<Local> = Local::now();
//        let mut png_path = PathBuf::from(
//            datetime
//                .format("Screen_Shot_%Y-%m-%d_at_%H_%M_%S")
//                .to_string(),
//        );
//        let pixels = self.cpu.bus.ppu.frame();
//        png_path.set_extension("png");
//        println!("Saved screenshot: {:?}", png_path);
//        create_png(&png_path, pixels)
//    }

//    /// Helper function to get held keys
//    fn is_key_held(&self, key: Key) -> bool {
//        if let Some(held) = self.held_keys.get(&(key as u8)) {
//            *held
//        } else {
//            false
//        }
//    }

//    /// Helper function to determine if a keyboard event is a gamepad event
//    // TODO: When custom keybind abstraction is complete, update this to only
//    // match bindings that are tied to gamepad inputs
//    fn is_gamepad_event(event: &Event) -> bool {
//        match event {
//            Event::KeyDown { key: Some(key), .. } => match key {
//                Key::A
//                | Key::S
//                | Key::Z
//                | Key::X
//                | Key::Return
//                | Key::RShift
//                | Key::Left
//                | Key::Right
//                | Key::Up
//                | Key::Down => true,
//                _ => false,
//            },
//            Event::ControllerDown { button, .. } => match button {
//                Button::A
//                | Button::B
//                | Button::X
//                | Button::Y
//                | Button::Start
//                | Button::Back
//                | Button::DPadLeft
//                | Button::DPadRight
//                | Button::DPadUp
//                | Button::DPadDown => true,
//                _ => false,
//            },
//            Event::ControllerAxisMotion { axis, .. } => match axis {
//                Axis::LeftX | Axis::LeftY => true,
//                _ => false,
//            },
//            _ => false,
//        }
//    }
//}

//impl FrameEvent {
//    pub(super) fn new(frame: usize, events: Vec<Event>) -> Self {
//        Self { frame, events }
//    }
//}

//impl Savable for FrameEvent {
//    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
//        self.frame.save(fh)?;
//        self.events.save(fh)?;
//        Ok(())
//    }
//    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
//        self.frame.load(fh)?;
//        self.events.load(fh)?;
//        Ok(())
//    }
//}

//impl Default for FrameEvent {
//    fn default() -> Self {
//        Self {
//            frame: 0,
//            events: Vec::new(),
//        }
//    }
//}
