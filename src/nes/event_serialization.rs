use crate::{nes_err, serialization::Savable, NesResult};
use pix_engine::event::{Axis, Button, Key, Mouse, PixEvent};
use std::io::{Read, Write};

impl Savable for PixEvent {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        match *self {
            PixEvent::None => 0u8.save(fh)?,
            PixEvent::Quit => 1u8.save(fh)?,
            PixEvent::AppTerminating => 2u8.save(fh)?,
            PixEvent::GamepadBtn(id, button, pressed) => {
                3u8.save(fh)?;
                id.save(fh)?;
                button.save(fh)?;
                pressed.save(fh)?;
            }
            PixEvent::GamepadAxis(id, axis, value) => {
                4u8.save(fh)?;
                id.save(fh)?;
                axis.save(fh)?;
                value.save(fh)?;
            }
            PixEvent::KeyPress(key, pressed, repeat) => {
                5u8.save(fh)?;
                key.save(fh)?;
                pressed.save(fh)?;
                repeat.save(fh)?;
            }
            PixEvent::MousePress(mouse, x, y, pressed) => {
                6u8.save(fh)?;
                mouse.save(fh)?;
                x.save(fh)?;
                y.save(fh)?;
                pressed.save(fh)?;
            }
            PixEvent::MouseWheel(delta) => {
                7u8.save(fh)?;
                delta.save(fh)?;
            }
            PixEvent::MouseMotion(x, y) => {
                8u8.save(fh)?;
                x.save(fh)?;
                y.save(fh)?;
            }
            PixEvent::WinClose(id) => {
                9u8.save(fh)?;
                id.save(fh)?;
            }
            PixEvent::Resized => 10u8.save(fh)?,
            PixEvent::Focus(window_id, focused) => {
                11u8.save(fh)?;
                window_id.save(fh)?;
                focused.save(fh)?;
            }
            PixEvent::Background(is_background) => {
                12u8.save(fh)?;
                is_background.save(fh)?;
            }
        }
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => PixEvent::None,
            1 => PixEvent::Quit,
            2 => PixEvent::AppTerminating,
            3 => {
                let mut id: i32 = 0;
                let mut btn = Button::default();
                let mut pressed = false;
                id.load(fh)?;
                btn.load(fh)?;
                pressed.load(fh)?;
                PixEvent::GamepadBtn(id, btn, pressed)
            }
            4 => {
                let mut id: i32 = 0;
                let mut axis = Axis::default();
                let mut value = 0;
                id.load(fh)?;
                axis.load(fh)?;
                value.load(fh)?;
                PixEvent::GamepadAxis(id, axis, value)
            }
            5 => {
                let mut key = Key::default();
                let mut pressed = false;
                let mut repeat = false;
                key.load(fh)?;
                pressed.load(fh)?;
                repeat.load(fh)?;
                PixEvent::KeyPress(key, pressed, repeat)
            }
            6 => {
                let mut mouse = Mouse::default();
                let mut x = 0;
                let mut y = 0;
                let mut pressed = false;
                mouse.load(fh)?;
                x.load(fh)?;
                y.load(fh)?;
                pressed.load(fh)?;
                PixEvent::MousePress(mouse, x, y, pressed)
            }
            7 => {
                let mut delta = 0;
                delta.load(fh)?;
                PixEvent::MouseWheel(delta)
            }
            8 => {
                let mut x = 0;
                let mut y = 0;
                x.load(fh)?;
                y.load(fh)?;
                PixEvent::MouseMotion(x, y)
            }
            9 => {
                let mut id = 0;
                id.load(fh)?;
                PixEvent::WinClose(id)
            }
            10 => PixEvent::Resized,
            11 => {
                let mut window_id = 0;
                let mut focused = false;
                window_id.load(fh)?;
                focused.load(fh)?;
                PixEvent::Focus(window_id, focused)
            }
            12 => {
                let mut is_background = false;
                is_background.load(fh)?;
                PixEvent::Background(is_background)
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

// TODO: Make a macro for this so it's not so tedius and error prone
impl Savable for Key {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Key::A, // Turbo A
            1 => Key::B,
            2 => Key::C,
            3 => Key::D,
            4 => Key::E,
            5 => Key::F,
            6 => Key::G,
            7 => Key::H,
            8 => Key::I,
            9 => Key::J,
            10 => Key::K,
            11 => Key::L,
            12 => Key::M,
            13 => Key::N,
            14 => Key::O,
            15 => Key::P,
            16 => Key::Q,
            17 => Key::R,
            18 => Key::S, // Turbo B
            19 => Key::T,
            20 => Key::U,
            21 => Key::V,
            22 => Key::W,
            23 => Key::X, // A
            24 => Key::Y,
            25 => Key::Z, // B
            26 => Key::Num0,
            27 => Key::Num1,
            28 => Key::Num2,
            29 => Key::Num3,
            30 => Key::Num4,
            31 => Key::Num5,
            32 => Key::Num6,
            33 => Key::Num7,
            34 => Key::Num8,
            35 => Key::Num9,
            36 => Key::Kp0,
            37 => Key::Kp1,
            38 => Key::Kp2,
            39 => Key::Kp3,
            40 => Key::Kp4,
            41 => Key::Kp5,
            42 => Key::Kp6,
            43 => Key::Kp7,
            44 => Key::Kp8,
            45 => Key::Kp9,
            46 => Key::F1,
            47 => Key::F2,
            48 => Key::F3,
            49 => Key::F4,
            50 => Key::F5,
            51 => Key::F6,
            52 => Key::F7,
            53 => Key::F8,
            54 => Key::F9,
            55 => Key::F10,
            56 => Key::F11,
            57 => Key::F12,
            58 => Key::Left,
            59 => Key::Up,
            60 => Key::Down,
            61 => Key::Right,
            62 => Key::Tab,
            63 => Key::Insert,
            64 => Key::Delete,
            65 => Key::Home,
            66 => Key::End,
            67 => Key::PageUp,
            68 => Key::PageDown,
            69 => Key::Escape,
            70 => Key::Backspace,
            71 => Key::Return,
            72 => Key::KpEnter,
            73 => Key::Pause,
            74 => Key::ScrollLock,
            75 => Key::Plus,
            76 => Key::Minus,
            77 => Key::Period,
            78 => Key::Underscore,
            79 => Key::Equals,
            80 => Key::KpMultiply,
            81 => Key::KpDivide,
            82 => Key::KpPlus,
            83 => Key::KpMinus,
            84 => Key::KpPeriod,
            85 => Key::Backquote,
            86 => Key::Exclaim,
            87 => Key::At,
            88 => Key::Hash,
            89 => Key::Dollar,
            90 => Key::Percent,
            91 => Key::Caret,
            92 => Key::Ampersand,
            93 => Key::Asterisk,
            94 => Key::LeftParen,
            95 => Key::RightParen,
            96 => Key::LeftBracket,
            97 => Key::RightBracket,
            98 => Key::Backslash,
            99 => Key::CapsLock,
            100 => Key::Semicolon,
            101 => Key::Colon,
            102 => Key::Quotedbl,
            103 => Key::Quote,
            104 => Key::Less,
            105 => Key::Comma,
            106 => Key::Greater,
            107 => Key::Question,
            108 => Key::Slash,
            109 => Key::LShift,
            110 => Key::RShift,
            111 => Key::Space,
            112 => Key::Ctrl,
            113 => Key::Alt,
            114 => Key::Meta,
            115 => Key::Unknown,
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
