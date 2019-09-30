use crate::{
    driver::{Driver, DriverOpts},
    event::PixEvent,
    input::Key,
    pixel::Sprite,
    Result,
};
use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::{Axis, Button, GameController},
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseUtil,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::{Canvas, Texture, TextureCreator},
    video::{self, FullscreenType, WindowContext},
    EventPump, GameControllerSubsystem, Sdl,
};
use std::collections::HashMap;

pub struct Sdl2Driver {
    title: String,
    width: u32,
    height: u32,
    context: Sdl,
    canvas: Canvas<video::Window>,
    // controller_sub: GameControllerSubsystem,
    // audio_device: AudioQueue<f32>,
    event_pump: EventPump,
    // gamepad1: Option<GameController>,
    // gamepad2: Option<GameController>,
    texture_creator: TextureCreator<WindowContext>,
    textures: HashMap<&'static str, Texture>,
}

impl Sdl2Driver {
    pub fn new(opts: DriverOpts) -> Self {
        let context = sdl2::init().unwrap();
        let video_sub = context.video().unwrap();

        let mut window_builder = video_sub.window("PixEngine", opts.width, opts.height);
        window_builder.position_centered().resizable();

        let mut window = window_builder.build().unwrap();
        if opts.fullscreen {
            context.mouse().show_cursor(false);
            window.set_fullscreen(FullscreenType::True).unwrap();
        }

        let mut canvas_builder = window.into_canvas().target_texture();
        if opts.vsync {
            canvas_builder = canvas_builder.present_vsync();
        }
        let mut canvas = canvas_builder.build().unwrap();
        canvas.set_logical_size(opts.width, opts.height).unwrap();

        let event_pump = context.event_pump().unwrap();
        let texture_creator = canvas.texture_creator();
        let screen_tex = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, opts.width, opts.height)
            .unwrap();
        let mut textures = HashMap::new();
        textures.insert("screen", screen_tex);
        Self {
            title: String::new(),
            width: opts.width,
            height: opts.height,
            context,
            canvas,
            event_pump,
            texture_creator,
            textures,
        }
    }

    fn map_key(&self, key: Keycode, pressed: bool) -> PixEvent {
        match key {
            Keycode::A => PixEvent::KeyPress(Key::A, pressed),
            Keycode::B => PixEvent::KeyPress(Key::B, pressed),
            Keycode::C => PixEvent::KeyPress(Key::C, pressed),
            Keycode::D => PixEvent::KeyPress(Key::D, pressed),
            Keycode::E => PixEvent::KeyPress(Key::E, pressed),
            Keycode::F => PixEvent::KeyPress(Key::F, pressed),
            Keycode::G => PixEvent::KeyPress(Key::G, pressed),
            Keycode::H => PixEvent::KeyPress(Key::H, pressed),
            Keycode::I => PixEvent::KeyPress(Key::I, pressed),
            Keycode::J => PixEvent::KeyPress(Key::J, pressed),
            Keycode::K => PixEvent::KeyPress(Key::K, pressed),
            Keycode::L => PixEvent::KeyPress(Key::L, pressed),
            Keycode::N => PixEvent::KeyPress(Key::N, pressed),
            Keycode::M => PixEvent::KeyPress(Key::M, pressed),
            Keycode::O => PixEvent::KeyPress(Key::O, pressed),
            Keycode::P => PixEvent::KeyPress(Key::P, pressed),
            Keycode::Q => PixEvent::KeyPress(Key::Q, pressed),
            Keycode::R => PixEvent::KeyPress(Key::R, pressed),
            Keycode::S => PixEvent::KeyPress(Key::S, pressed),
            Keycode::T => PixEvent::KeyPress(Key::T, pressed),
            Keycode::U => PixEvent::KeyPress(Key::U, pressed),
            Keycode::V => PixEvent::KeyPress(Key::V, pressed),
            Keycode::W => PixEvent::KeyPress(Key::W, pressed),
            Keycode::X => PixEvent::KeyPress(Key::X, pressed),
            Keycode::Y => PixEvent::KeyPress(Key::Y, pressed),
            Keycode::Z => PixEvent::KeyPress(Key::Z, pressed),
            Keycode::Num0 => PixEvent::KeyPress(Key::Num0, pressed),
            Keycode::Num1 => PixEvent::KeyPress(Key::Num1, pressed),
            Keycode::Num2 => PixEvent::KeyPress(Key::Num2, pressed),
            Keycode::Num3 => PixEvent::KeyPress(Key::Num3, pressed),
            Keycode::Num4 => PixEvent::KeyPress(Key::Num4, pressed),
            Keycode::Num5 => PixEvent::KeyPress(Key::Num5, pressed),
            Keycode::Num6 => PixEvent::KeyPress(Key::Num6, pressed),
            Keycode::Num7 => PixEvent::KeyPress(Key::Num7, pressed),
            Keycode::Num8 => PixEvent::KeyPress(Key::Num8, pressed),
            Keycode::Num9 => PixEvent::KeyPress(Key::Num9, pressed),
            Keycode::Kp0 => PixEvent::KeyPress(Key::Kp0, pressed),
            Keycode::Kp1 => PixEvent::KeyPress(Key::Kp1, pressed),
            Keycode::Kp2 => PixEvent::KeyPress(Key::Kp2, pressed),
            Keycode::Kp3 => PixEvent::KeyPress(Key::Kp3, pressed),
            Keycode::Kp4 => PixEvent::KeyPress(Key::Kp4, pressed),
            Keycode::Kp5 => PixEvent::KeyPress(Key::Kp5, pressed),
            Keycode::Kp6 => PixEvent::KeyPress(Key::Kp6, pressed),
            Keycode::Kp7 => PixEvent::KeyPress(Key::Kp7, pressed),
            Keycode::Kp8 => PixEvent::KeyPress(Key::Kp8, pressed),
            Keycode::Kp9 => PixEvent::KeyPress(Key::Kp9, pressed),
            Keycode::F1 => PixEvent::KeyPress(Key::F1, pressed),
            Keycode::F2 => PixEvent::KeyPress(Key::F2, pressed),
            Keycode::F3 => PixEvent::KeyPress(Key::F3, pressed),
            Keycode::F4 => PixEvent::KeyPress(Key::F4, pressed),
            Keycode::F5 => PixEvent::KeyPress(Key::F5, pressed),
            Keycode::F6 => PixEvent::KeyPress(Key::F6, pressed),
            Keycode::F7 => PixEvent::KeyPress(Key::F7, pressed),
            Keycode::F8 => PixEvent::KeyPress(Key::F8, pressed),
            Keycode::F9 => PixEvent::KeyPress(Key::F9, pressed),
            Keycode::F10 => PixEvent::KeyPress(Key::F10, pressed),
            Keycode::F11 => PixEvent::KeyPress(Key::F11, pressed),
            Keycode::F12 => PixEvent::KeyPress(Key::F12, pressed),
            Keycode::Left => PixEvent::KeyPress(Key::Left, pressed),
            Keycode::Up => PixEvent::KeyPress(Key::Up, pressed),
            Keycode::Down => PixEvent::KeyPress(Key::Down, pressed),
            Keycode::Right => PixEvent::KeyPress(Key::Right, pressed),
            Keycode::Tab => PixEvent::KeyPress(Key::Tab, pressed),
            Keycode::Insert => PixEvent::KeyPress(Key::Insert, pressed),
            Keycode::Delete => PixEvent::KeyPress(Key::Delete, pressed),
            Keycode::Home => PixEvent::KeyPress(Key::Home, pressed),
            Keycode::End => PixEvent::KeyPress(Key::End, pressed),
            Keycode::PageUp => PixEvent::KeyPress(Key::PageUp, pressed),
            Keycode::PageDown => PixEvent::KeyPress(Key::PageDown, pressed),
            Keycode::Escape => PixEvent::KeyPress(Key::Escape, pressed),
            Keycode::Backspace => PixEvent::KeyPress(Key::Backspace, pressed),
            Keycode::Return => PixEvent::KeyPress(Key::Return, pressed),
            Keycode::KpEnter => PixEvent::KeyPress(Key::KpEnter, pressed),
            Keycode::Pause => PixEvent::KeyPress(Key::Pause, pressed),
            Keycode::ScrollLock => PixEvent::KeyPress(Key::ScrollLock, pressed),
            Keycode::Plus => PixEvent::KeyPress(Key::Plus, pressed),
            Keycode::Minus => PixEvent::KeyPress(Key::Minus, pressed),
            Keycode::Period => PixEvent::KeyPress(Key::Period, pressed),
            Keycode::Underscore => PixEvent::KeyPress(Key::Underscore, pressed),
            Keycode::Equals => PixEvent::KeyPress(Key::Equals, pressed),
            Keycode::KpMultiply => PixEvent::KeyPress(Key::KpMultiply, pressed),
            Keycode::KpDivide => PixEvent::KeyPress(Key::KpDivide, pressed),
            Keycode::KpPlus => PixEvent::KeyPress(Key::KpPlus, pressed),
            Keycode::KpMinus => PixEvent::KeyPress(Key::KpMinus, pressed),
            Keycode::KpPeriod => PixEvent::KeyPress(Key::KpPeriod, pressed),
            Keycode::Backquote => PixEvent::KeyPress(Key::Backquote, pressed),
            Keycode::Exclaim => PixEvent::KeyPress(Key::Exclaim, pressed),
            Keycode::At => PixEvent::KeyPress(Key::At, pressed),
            Keycode::Hash => PixEvent::KeyPress(Key::Hash, pressed),
            Keycode::Dollar => PixEvent::KeyPress(Key::Dollar, pressed),
            Keycode::Percent => PixEvent::KeyPress(Key::Percent, pressed),
            Keycode::Caret => PixEvent::KeyPress(Key::Caret, pressed),
            Keycode::Ampersand => PixEvent::KeyPress(Key::Ampersand, pressed),
            Keycode::Asterisk => PixEvent::KeyPress(Key::Asterisk, pressed),
            Keycode::LeftParen => PixEvent::KeyPress(Key::LeftParen, pressed),
            Keycode::RightParen => PixEvent::KeyPress(Key::RightParen, pressed),
            Keycode::LeftBracket => PixEvent::KeyPress(Key::LeftBracket, pressed),
            Keycode::RightBracket => PixEvent::KeyPress(Key::RightBracket, pressed),
            Keycode::Backslash => PixEvent::KeyPress(Key::Backslash, pressed),
            Keycode::CapsLock => PixEvent::KeyPress(Key::CapsLock, pressed),
            Keycode::Semicolon => PixEvent::KeyPress(Key::Semicolon, pressed),
            Keycode::Colon => PixEvent::KeyPress(Key::Colon, pressed),
            Keycode::Quotedbl => PixEvent::KeyPress(Key::Quotedbl, pressed),
            Keycode::Quote => PixEvent::KeyPress(Key::Quote, pressed),
            Keycode::Less => PixEvent::KeyPress(Key::Less, pressed),
            Keycode::Comma => PixEvent::KeyPress(Key::Comma, pressed),
            Keycode::Greater => PixEvent::KeyPress(Key::Greater, pressed),
            Keycode::Question => PixEvent::KeyPress(Key::Question, pressed),
            Keycode::Slash => PixEvent::KeyPress(Key::Slash, pressed),
            Keycode::LShift | Keycode::RShift => PixEvent::KeyPress(Key::Shift, pressed),
            Keycode::Space => PixEvent::KeyPress(Key::Space, pressed),
            Keycode::LCtrl | Keycode::RCtrl => PixEvent::KeyPress(Key::Control, pressed),
            Keycode::LAlt | Keycode::RAlt => PixEvent::KeyPress(Key::Alt, pressed),
            Keycode::LGui | Keycode::RGui => PixEvent::KeyPress(Key::Meta, pressed),
            _ => PixEvent::None,
        }
    }
}

impl Driver for Sdl2Driver {
    fn setup() -> Result<()> {
        Ok(())
    }
    fn poll(&mut self) -> Vec<PixEvent> {
        let events: Vec<Event> = self.event_pump.poll_iter().collect();
        let mut pix_events: Vec<PixEvent> = Vec::new();
        for event in events {
            match event {
                Event::Quit { .. } => pix_events.push(PixEvent::Quit),
                Event::AppTerminating { .. } => pix_events.push(PixEvent::AppTerminating),
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => {
                    if !repeat {
                        pix_events.push(self.map_key(key, true));
                    }
                }
                Event::KeyUp {
                    keycode: Some(key), ..
                } => pix_events.push(self.map_key(key, false)),
                _ => (),
            }
        }
        pix_events
    }
    fn set_title(&mut self, title: &str) -> Result<()> {
        self.canvas.window_mut().set_title(title).unwrap();
        Ok(())
    }
    fn clear(&mut self) {
        self.canvas.clear();
    }
    fn update_frame(&mut self, sprite: &Sprite) {
        self.canvas.clear();
        self.textures
            .get_mut("screen")
            .unwrap()
            .update(None, &sprite.as_bytes(), (sprite.width() * 4) as usize)
            .unwrap();
        self.canvas
            .copy(
                &self.textures.get("screen").unwrap(),
                Rect::new(0, 0, self.width as u32, self.height as u32),
                None,
            )
            .unwrap();
        self.canvas.present();
    }
}
