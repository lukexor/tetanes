use crate::{
    draw::Rect,
    driver::{Driver, DriverOpts},
    event::PixEvent,
    pixel::{self, ColorType},
    PixEngineErr, PixEngineResult,
};
use image::{DynamicImage, GenericImageView, Rgba};
use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::{Axis, Button, GameController},
    event::{Event, WindowEvent},
    mouse,
    pixels::{Color, PixelFormatEnum},
    rect::{self, Point},
    render::{BlendMode, Canvas, CanvasBuilder, Texture, TextureCreator},
    surface::Surface,
    video::{self, FullscreenType, WindowContext, WindowPos},
    EventPump, GameControllerSubsystem, Sdl,
};
use std::{collections::HashMap, path::Path};

pub const SAMPLE_RATE: i32 = 96_000; // in Hz

mod event;

pub(crate) struct Sdl2Driver {
    title: String,
    width: u32,
    height: u32,
    context: Sdl,
    canvas: Canvas<video::Window>,
    audio_device: AudioQueue<f32>,
    event_pump: EventPump,
    controller_sub: GameControllerSubsystem,
    controller1: Option<GameController>,
    controller2: Option<GameController>,
    texture_creator: TextureCreator<WindowContext>,
    texture_maps: HashMap<&'static str, TextureMap>,
    last_color: Color,
}

pub struct TextureMap {
    tex: Texture,
    format: PixelFormatEnum,
    channels: u32,
    pitch: usize,
    src: Option<rect::Rect>,
    dst: Option<rect::Rect>,
}

fn rect_to_sdl(rect: Rect) -> rect::Rect {
    rect::Rect::new(rect.x as i32, rect.y as i32, rect.w, rect.h)
}

impl Sdl2Driver {
    pub(crate) fn new(opts: DriverOpts) -> Self {
        let context = sdl2::init().expect("sdl2 context");

        // Set up the window
        let video_sub = context.video().expect("video sub");
        let mut window_builder = video_sub.window(&opts.title, opts.width, opts.height);
        window_builder.position_centered().resizable();
        let window = window_builder.build().expect("window builder");

        // Set up canvas
        let canvas_builder = window.into_canvas().target_texture();
        let mut canvas = canvas_builder.build().expect("canvas");
        canvas
            .set_logical_size(opts.width, opts.height)
            .expect("set logical size");

        // Event pump
        let event_pump = context.event_pump().expect("event pump");
        let controller_sub = context.game_controller().expect("sdl controller_sub");

        // Primary screen texture
        let texture_creator = canvas.texture_creator();
        let screen_tex = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, opts.width, opts.height)
            .expect("screen texture");
        let mut texture_maps = HashMap::new();
        texture_maps.insert(
            "screen",
            TextureMap {
                tex: screen_tex,
                format: PixelFormatEnum::RGBA32,
                channels: 4,
                pitch: (4 * opts.width) as usize,
                src: None,
                dst: None,
            },
        );

        // Set up Audio
        let audio_sub = context.audio().expect("audio subsystem");
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .expect("audio device");
        audio_device.resume();
        Self {
            title: opts.title.to_string(),
            width: opts.width,
            height: opts.height,
            context,
            canvas,
            audio_device,
            event_pump,
            controller_sub,
            controller1: None,
            controller2: None,
            texture_creator,
            texture_maps,
            last_color: Color::RGBA(0, 0, 0, 0),
        }
    }
}

impl Driver for Sdl2Driver {
    fn fullscreen(&mut self, val: bool) {
        let state = self.canvas.window().fullscreen_state();
        let mouse = self.context.mouse();
        let mode = if val && state == FullscreenType::Off {
            mouse.show_cursor(false);
            video::FullscreenType::True
        } else {
            mouse.show_cursor(true);
            video::FullscreenType::Off
        };
        self.canvas
            .window_mut()
            .set_fullscreen(mode)
            .expect("set fullscreen");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    fn vsync(&mut self, val: bool) {
        let video_sub = self.context.video().expect("video sub");
        let mut window_builder = video_sub.window(&self.title, self.width, self.height);
        window_builder.position_centered().resizable();
        let window = window_builder.build().expect("window builder");

        // Set up canvas
        let mut canvas_builder = window.into_canvas().target_texture();
        if val {
            canvas_builder = canvas_builder.present_vsync();
        }
        let mut canvas = canvas_builder.build().expect("canvas");
        canvas
            .set_logical_size(self.width, self.height)
            .expect("set logical size");

        let texture_creator = canvas.texture_creator();
        let screen_tex = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, self.width, self.height)
            .expect("screen texture");
        let mut texture_maps = HashMap::new();
        for (name, map) in &self.texture_maps {
            let (width, height) = if let Some(src) = map.src {
                (src.w as u32, src.h as u32)
            } else {
                (self.width, self.height)
            };
            let tex = texture_creator
                .create_texture_streaming(map.format, width, height)
                .expect("valid texture");
            texture_maps.insert(
                *name,
                TextureMap {
                    tex,
                    format: map.format,
                    channels: map.channels,
                    pitch: map.pitch,
                    src: map.src,
                    dst: map.dst,
                },
            );
        }
        self.canvas = canvas;
        self.texture_creator = texture_creator;
        self.texture_maps = texture_maps;
    }

    fn load_icon<P: AsRef<Path>>(&mut self, path: P) -> PixEngineResult<()> {
        let icon = pixel::load_from_file(path)?;
        let width = icon.width();
        let height = icon.height();
        let pixels = &mut icon.raw_pixels();
        let surface = Surface::from_data(pixels, width, height, width * 4, PixelFormatEnum::RGBA32);
        if let Ok(surface) = surface {
            self.canvas.window_mut().set_icon(surface);
            Ok(())
        } else {
            Err(PixEngineErr::new("Failed to load icon"))
        }
    }

    fn set_title(&mut self, title: &str) -> PixEngineResult<()> {
        self.canvas
            .window_mut()
            .set_title(title)
            .expect("set title");
        Ok(())
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.canvas
            .set_logical_size(width, height)
            .expect("set logical size");
        let window = self.canvas.window_mut();
        window.set_size(width, height).expect("set size");
        window.set_position(WindowPos::Centered, WindowPos::Centered);
        if let Some(map) = self.texture_maps.get_mut("screen") {
            let tex = self
                .texture_creator
                .create_texture_streaming(map.format, width, height)
                .expect("valid texture");
            map.tex = tex;
            map.pitch = (map.channels * width) as usize;
        }
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
                Event::ControllerDeviceAdded { which: id, .. } => {
                    match id {
                        0 => {
                            self.controller1 =
                                Some(self.controller_sub.open(id).expect("controller"))
                        }
                        1 => {
                            self.controller2 =
                                Some(self.controller_sub.open(id).expect("controller"))
                        }
                        _ => (),
                    }
                    PixEvent::None
                }
                Event::KeyDown {
                    keycode: Some(key),
                    repeat,
                    ..
                } => self.map_key(key, true, repeat),
                Event::KeyUp {
                    keycode: Some(key), ..
                } => self.map_key(key, false, false),
                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => self.map_mouse(mouse_btn, x as u32, y as u32, true),
                Event::MouseButtonUp {
                    mouse_btn, x, y, ..
                } => self.map_mouse(mouse_btn, x as u32, y as u32, false),
                Event::ControllerButtonDown { which, button, .. } => {
                    self.map_button(which, button, true)
                }
                Event::ControllerButtonUp { which, button, .. } => {
                    self.map_button(which, button, false)
                }
                Event::ControllerAxisMotion {
                    which, axis, value, ..
                } => self.map_axis(which, axis, value),
                // Only really care about vertical scroll
                Event::MouseWheel { y, .. } => PixEvent::MouseWheel(y),
                Event::MouseMotion { x, y, .. } => PixEvent::MouseMotion(x as u32, y as u32),
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

    fn clear(&mut self) {
        self.canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
        self.canvas.clear();
    }

    fn present(&mut self) {
        self.canvas.present();
    }

    fn create_texture(&mut self, name: &'static str, color_type: ColorType, src: Rect, dst: Rect) {
        if let Some(tex) = self.texture_maps.get(name) {
            return;
        }
        let (format, channels) = match color_type {
            ColorType::RGB => (PixelFormatEnum::RGB24, 3),
            ColorType::RGBA => (PixelFormatEnum::RGBA32, 4),
        };
        let mut tex = self
            .texture_creator
            .create_texture_streaming(format, src.w, src.h)
            .expect("valid texture");
        if color_type == ColorType::RGBA {
            tex.set_blend_mode(BlendMode::Blend);
        }
        let _ = self.texture_maps.insert(
            name,
            TextureMap {
                tex,
                format,
                channels,
                pitch: (channels * src.w) as usize,
                src: Some(rect_to_sdl(src)),
                dst: Some(rect_to_sdl(dst)),
            },
        );
    }

    fn update_texture(&mut self, name: &'static str, src: Rect, dst: Rect) {
        if let Some(tex) = self.texture_maps.get_mut(name) {
            tex.src = Some(rect_to_sdl(src));
            tex.dst = Some(rect_to_sdl(dst));
        }
    }

    fn copy_texture(&mut self, name: &str, bytes: &[u8]) {
        let map = self.texture_maps.get_mut(name).expect("valid texture");
        map.tex
            .update(None, bytes, map.pitch)
            .expect("update texture");
        self.canvas
            .copy(&map.tex, map.src, map.dst)
            .expect("copy texture");
    }

    fn copy_texture_dst(&mut self, name: &str, dst: Rect, bytes: &[u8]) {
        let dst = rect_to_sdl(dst);
        let map = self.texture_maps.get_mut(name).expect("valid texture");
        map.tex
            .update(None, bytes, map.pitch)
            .expect("update texture");
        self.canvas
            .copy(&map.tex, map.src, dst)
            .expect("copy texture");
    }

    fn draw_point(&mut self, x: u32, y: u32, p: Rgba<u8>) {
        let color = Color::RGBA(p[0], p[1], p[2], p[3]);
        if color != self.last_color {
            self.canvas
                .set_draw_color(Color::RGBA(p[0], p[1], p[2], p[3]));
        }
        let point = Point::new(x as i32, y as i32);
        self.canvas.draw_point(point).expect("draw point");
    }

    fn enqueue_audio(&mut self, samples: &[f32]) {
        while self.audio_device.size() > SAMPLE_RATE as u32 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.audio_device.queue(samples);
    }
}
