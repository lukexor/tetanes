use crate::memory::Memory;
use crate::ui::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub type InputRef = Rc<RefCell<Input>>;

// The "strobe state": the order in which the NES reads the buttons.
const STROBE_A: u8 = 0;
const STROBE_B: u8 = 1;
const STROBE_SELECT: u8 = 2;
const STROBE_START: u8 = 3;
const STROBE_UP: u8 = 4;
const STROBE_DOWN: u8 = 5;
const STROBE_LEFT: u8 = 6;
const STROBE_RIGHT: u8 = 7;

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
    strobe_state: u8,
}

impl Gamepad {
    fn next_state(&mut self) -> u8 {
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
        state as u8
    }
    fn reset(&mut self) {
        self.strobe_state = STROBE_A;
    }
}

#[derive(Default)]
pub struct Input {
    gamepad1: Gamepad,
    gamepad2: Gamepad,
    event_pump: Option<EventPump>,
}

pub enum InputResult {
    Continue,
    Quit,
    // TODO Menu,
    Reset,
    // TODO Save,
    // TODO Load,
}

impl Input {
    pub fn new() -> Self {
        Self {
            gamepad1: Gamepad::default(),
            gamepad2: Gamepad::default(),
            event_pump: None,
        }
    }

    pub fn init(event_pump: EventPump) -> Self {
        Self {
            gamepad1: Gamepad::default(),
            gamepad2: Gamepad::default(),
            event_pump: Some(event_pump),
        }
    }

    pub fn poll_events(&mut self) -> InputResult {
        if let Some(event_pump) = &mut self.event_pump {
            if let Some(event) = event_pump.poll_event() {
                let result = match event {
                    Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    } => InputResult::Quit,
                    Event::KeyDown {
                        keycode: Some(Keycode::R),
                        ..
                    } => InputResult::Reset,
                    Event::KeyDown {
                        keycode: Some(key), ..
                    } => self.handle_gamepad_event(key, true),
                    Event::KeyUp {
                        keycode: Some(key), ..
                    } => self.handle_gamepad_event(key, false),
                    _ => InputResult::Continue,
                    // TODO Debugger, save/load, device added, record, menu, etc
                };
                return result;
            }
        }
        InputResult::Continue
    }

    fn handle_gamepad_event(&mut self, key: Keycode, down: bool) -> InputResult {
        match key {
            Keycode::Z => self.gamepad1.a = down,
            Keycode::X => self.gamepad1.b = down,
            Keycode::RShift => self.gamepad1.select = down,
            Keycode::Return => self.gamepad1.start = down,
            Keycode::Up => self.gamepad1.up = down,
            Keycode::Down => self.gamepad1.down = down,
            Keycode::Left => self.gamepad1.left = down,
            Keycode::Right => self.gamepad1.right = down,
            _ => {}
        }
        InputResult::Continue
    }
}

impl Memory for Input {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x4016 => self.gamepad1.next_state(),
            0x4017 => self.gamepad2.next_state(),
            _ => 0,
        }
    }

    fn writeb(&mut self, addr: u16, _val: u8) {
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
