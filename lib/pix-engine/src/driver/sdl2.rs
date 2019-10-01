use crate::{
    driver::{Driver, DriverOpts},
    event::PixEvent,
    pixel::Sprite,
    Result,
};
use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::{Axis, Button, GameController},
    event::{Event, WindowEvent},
    // mouse::MouseUtil,
    mouse,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
    render::{Canvas, Texture, TextureCreator},
    surface::Surface,
    video::{self, FullscreenType, WindowContext},
    EventPump,
    GameControllerSubsystem,
    Sdl,
};
use std::collections::HashMap;

mod event;

pub(super) struct Sdl2Driver {
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
    pub(super) fn new(opts: DriverOpts) -> Self {
        let context = sdl2::init().unwrap();
        let video_sub = context.video().unwrap();

        let mut window_builder = video_sub.window("PixEngine", opts.width, opts.height);
        window_builder.position_centered().resizable();

        let mut window = window_builder.build().unwrap();
        if opts.fullscreen {
            context.mouse().show_cursor(false);
            window.set_fullscreen(FullscreenType::True).unwrap();
        }

        let mut pixels = opts.icon.to_bytes();
        if pixels.len() > 0 {
            let surface = Surface::from_data(
                &mut pixels,
                opts.icon.width() as u32,
                opts.icon.height() as u32,
                opts.icon.width() as u32 * 4,
                PixelFormatEnum::RGBA32,
            );
            if let Ok(surface) = surface {
                window.set_icon(surface);
            }
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
        // let rgb_tex = texture_creator
        //     .create_texture_streaming(PixelFormatEnum::RGB24, opts.width, opts.height)
        //     .unwrap();
        let mut textures = HashMap::new();
        textures.insert("screen", screen_tex);
        // textures.insert("rgb", rgb_tex);
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
}

impl Driver for Sdl2Driver {
    fn setup() -> Result<()> {
        Ok(())
    }
    fn poll(&mut self) -> Vec<PixEvent> {
        let events: Vec<Event> = self.event_pump.poll_iter().collect();
        let mut pix_events: Vec<PixEvent> = Vec::new();
        for event in events {
            let pix_event = match event {
                Event::Quit { .. } => PixEvent::Quit,
                Event::AppTerminating { .. } => PixEvent::AppTerminating,
                Event::Window { win_event, .. } => match win_event {
                    WindowEvent::Resized(..) | WindowEvent::SizeChanged(..) => PixEvent::Resized,
                    WindowEvent::FocusGained => PixEvent::Focus(true),
                    WindowEvent::FocusLost => PixEvent::Focus(false),
                    _ => PixEvent::None, // Ignore others
                },
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => {
                    if !repeat {
                        self.map_key(key, true)
                    } else {
                        PixEvent::None
                    }
                }
                Event::KeyUp {
                    keycode: Some(key), ..
                } => self.map_key(key, false),
                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => self.map_mouse(mouse_btn, x, y, true),
                Event::MouseButtonUp {
                    mouse_btn, x, y, ..
                } => self.map_mouse(mouse_btn, x, y, false),
                // Only really care about vertical scroll
                Event::MouseWheel { y, .. } => PixEvent::MouseWheel(y),
                Event::MouseMotion { x, y, .. } => PixEvent::MouseMotion(x, y),
                Event::AppDidEnterBackground { .. } => PixEvent::Background(true),
                Event::AppDidEnterForeground { .. } => PixEvent::Background(false),
                _ => PixEvent::None, // Ignore others
            };
            if pix_event != PixEvent::None {
                pix_events.push(pix_event)
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
        let tex = self.textures.get_mut("screen").unwrap();
        tex.update(None, &sprite.to_bytes(), (sprite.width() * 4) as usize)
            .unwrap();
        self.canvas.copy(&tex, None, None).unwrap();
        self.canvas.present();
    }
}
