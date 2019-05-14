use crate::console::memory::{Addr, Byte, Memory};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::EventPump;
use std::fmt;

// The "strobe state": the order in which the NES reads the buttons.
const STROBE_A: Byte = 0;
const STROBE_B: Byte = 1;
const STROBE_SELECT: Byte = 2;
const STROBE_START: Byte = 3;
const STROBE_UP: Byte = 4;
const STROBE_DOWN: Byte = 5;
const STROBE_LEFT: Byte = 6;
const STROBE_RIGHT: Byte = 7;

#[derive(Default, Debug)]
struct Gamepad {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    a: bool,
    b: bool,
    select: bool,
    start: bool,
    strobe_state: Byte,
}

impl Gamepad {
    fn next_state(&mut self) -> Byte {
        let state = match self.strobe_state {
            STROBE_A => self.a,
            STROBE_B => self.b,
            STROBE_SELECT => self.select,
            STROBE_START => self.start,
            STROBE_UP => self.up,
            STROBE_DOWN => self.down,
            STROBE_LEFT => self.left,
            STROBE_RIGHT => self.right,
            _ => panic!("invalid state {}", self.strobe_state),
        };
        self.strobe_state = (self.strobe_state + 1) & 7;
        state as Byte
    }
    fn reset(&mut self) {
        self.strobe_state = STROBE_A;
    }
}

pub struct Input {
    gamepad1: Gamepad,
    gamepad2: Gamepad,
    event_pump: EventPump,
}

impl Input {
    pub fn init(event_pump: EventPump) -> Self {
        Self {
            gamepad1: Gamepad::default(),
            gamepad2: Gamepad::default(),
            event_pump,
        }
    }

    pub fn poll_events(&mut self) {
        // for event in self.event_pump.poll_iter() {
        while let Some(event) = self.event_pump.poll_event() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    std::process::exit(0);
                }
                Event::KeyDown {
                    keycode: Some(key), ..
                } => self.handle_gamepad_event(key, true),
                Event::KeyUp {
                    keycode: Some(key), ..
                } => self.handle_gamepad_event(key, false),
                _ => (),
                // TODO Debugger, save/load, device added, record, menu, etc
            }
        }
    }

    fn handle_gamepad_event(&mut self, key: Keycode, down: bool) {
        match key {
            Keycode::Left => self.gamepad1.left = down,
            Keycode::Down => self.gamepad1.down = down,
            Keycode::Up => self.gamepad1.up = down,
            Keycode::Right => self.gamepad1.right = down,
            Keycode::Z => self.gamepad1.a = down,
            Keycode::X => self.gamepad1.b = down,
            Keycode::RShift => self.gamepad1.select = down,
            Keycode::Return => self.gamepad1.start = down,
            _ => {}
        }
    }
}

impl Memory for Input {
    fn readb(&mut self, addr: Addr) -> Byte {
        match addr {
            0x4016 => self.gamepad1.next_state(),
            0x4017 => self.gamepad2.next_state(),
            _ => 0,
        }
    }

    fn writeb(&mut self, addr: Addr, val: Byte) {
        if addr == 0x4016 {
            self.gamepad1.reset();
            self.gamepad2.reset();
        }
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Input {{ }} ")
    }
}
