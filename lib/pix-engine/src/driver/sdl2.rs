use crate::{
    draw::Rect,
    driver::{Driver, DriverOpts},
    event::PixEvent,
    pixel::ColorType,
    sprite::Sprite,
    PixEngineErr, PixEngineResult,
};
use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::GameController,
    event::{Event, WindowEvent},
    pixels::{Color, PixelFormatEnum},
    rect,
    render::{self, BlendMode, Canvas, Texture, TextureCreator},
    surface::Surface,
    video::{self, FullscreenType, WindowContext, WindowPos},
    EventPump, GameControllerSubsystem, Sdl,
};
use std::collections::HashMap;

pub const SAMPLE_RATE: i32 = 96_000; // in Hz

mod event;

pub(crate) struct Sdl2Driver {
    context: Sdl,
    window_id: u32,
    canvases: HashMap<u32, (Canvas<video::Window>, TextureCreator<WindowContext>)>,
    texture_maps: HashMap<String, TextureMap>,
    audio_device: AudioQueue<f32>,
    event_pump: EventPump,
    controller_sub: GameControllerSubsystem,
    controller1: Option<GameController>,
    controller2: Option<GameController>,
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
    pub(crate) fn new(opts: DriverOpts) -> PixEngineResult<Self> {
        let context = sdl2::init()?;

        // Set up the window
        let video_sub = context.video()?;
        let mut window_builder = video_sub.window(&opts.title, opts.width, opts.height);
        window_builder.position_centered().resizable();
        let window = window_builder.build()?;
        let window_id = window.id();

        // Set up canvas
        let mut canvas_builder = window.into_canvas().target_texture();
        if opts.vsync {
            canvas_builder = canvas_builder.present_vsync();
        }
        let mut canvas = canvas_builder.build()?;
        canvas.set_logical_size(opts.width, opts.height)?;

        // Event pump
        let event_pump = context.event_pump()?;
        let controller_sub = context.game_controller()?;

        // Primary screen texture
        let texture_creator = canvas.texture_creator();
        let screen_tex = texture_creator.create_texture_streaming(
            PixelFormatEnum::RGBA32,
            opts.width,
            opts.height,
        )?;
        let mut texture_maps = HashMap::new();
        texture_maps.insert(
            format!("screen{}", window_id),
            TextureMap {
                tex: screen_tex,
                format: PixelFormatEnum::RGBA32,
                channels: 4,
                pitch: (4 * opts.width) as usize,
                src: Some(rect::Rect::new(0, 0, opts.width, opts.height)),
                dst: Some(rect::Rect::new(0, 0, opts.width, opts.height)),
            },
        );

        // Set up Audio
        let audio_sub = context.audio()?;
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub.open_queue(None, &desired_spec)?;
        audio_device.resume();

        let mut canvases = HashMap::new();
        canvases.insert(window_id, (canvas, texture_creator));

        Ok(Self {
            context,
            window_id,
            canvases,
            audio_device,
            event_pump,
            controller_sub,
            controller1: None,
            controller2: None,
            texture_maps,
        })
    }
}

impl Driver for Sdl2Driver {
    fn fullscreen(&mut self, window_id: u32, val: bool) -> PixEngineResult<()> {
        if let Some((canvas, _)) = self.canvases.get_mut(&window_id) {
            let state = canvas.window().fullscreen_state();
            let mouse = self.context.mouse();
            let mode = if val && state == FullscreenType::Off {
                mouse.show_cursor(false);
                video::FullscreenType::True
            } else {
                mouse.show_cursor(true);
                video::FullscreenType::Off
            };
            canvas.window_mut().set_fullscreen(mode)?;
            Ok(())
        } else {
            Err(PixEngineErr::new(format!(
                "invalid window_id {}",
                window_id
            )))
        }
    }

    fn vsync(&mut self, window_id: u32, val: bool) -> PixEngineResult<()> {
        if let Some((canvas, texture_creator)) = self.canvases.get_mut(&window_id) {
            let title = canvas.window().title();
            let (width, height) = canvas.window().size();
            let (x, y) = canvas.window().position();
            let video_sub = canvas.window().subsystem();

            let mut window_builder = video_sub.window(&title, width, height);
            window_builder.position(x, y).resizable();
            let window = window_builder.build()?;

            // Set up canvas
            let mut canvas_builder = window.into_canvas().target_texture();
            if val {
                canvas_builder = canvas_builder.present_vsync();
            }
            let mut new_canvas = canvas_builder.build()?;
            new_canvas.set_logical_size(width, height)?;
            let new_texture_creator = new_canvas.texture_creator();
            let mut texture_maps = HashMap::new();
            for (name, map) in self.texture_maps.iter() {
                let tex = new_texture_creator.create_texture_streaming(
                    map.format,
                    map.src.expect("src").width(),
                    map.src.expect("src").height(),
                )?;
                texture_maps.insert(
                    name.to_string(),
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
            *canvas = new_canvas;
            *texture_creator = new_texture_creator;
            self.texture_maps = texture_maps;
        }
        Ok(())
    }

    fn load_icon(&mut self, path: &str) -> PixEngineResult<()> {
        let mut icon = Sprite::from_file(path)?;
        let width = icon.width();
        let height = icon.height();
        let pixels = icon.bytes_mut();
        for (_, (canvas, _)) in self.canvases.iter_mut() {
            let surface =
                Surface::from_data(pixels, width, height, width * 4, PixelFormatEnum::RGBA32);
            if let Ok(surface) = surface {
                canvas.window_mut().set_icon(surface);
            } else {
                return Err(PixEngineErr::new("failed to load icon"));
            }
        }
        Ok(())
    }

    fn window_id(&self) -> u32 {
        self.window_id
    }

    fn set_title(&mut self, window_id: u32, title: &str) -> PixEngineResult<()> {
        if let Some((canvas, _)) = self.canvases.get_mut(&window_id) {
            canvas.window_mut().set_title(title)?;
            Ok(())
        } else {
            Err(PixEngineErr::new(format!(
                "invalid window_id {}",
                window_id
            )))
        }
    }

    fn set_size(&mut self, window_id: u32, width: u32, height: u32) -> PixEngineResult<()> {
        if let Some((canvas, texture_creator)) = self.canvases.get_mut(&window_id) {
            canvas.set_logical_size(width, height)?;
            let window = canvas.window_mut();
            window.set_size(width, height)?;
            window.set_position(WindowPos::Centered, WindowPos::Centered);
            if let Some(map) = self.texture_maps.get_mut(&format!("screen{}", window_id)) {
                let tex = texture_creator.create_texture_streaming(map.format, width, height)?;
                map.tex = tex;
                map.pitch = (map.channels * width) as usize;
            }
            Ok(())
        } else {
            Err(PixEngineErr::new(format!(
                "invalid window_id {}",
                window_id
            )))
        }
    }

    fn poll(&mut self) -> PixEngineResult<Vec<PixEvent>> {
        let events: Vec<Event> = self.event_pump.poll_iter().collect();
        let mut pix_events: Vec<PixEvent> = Vec::new();
        for event in events {
            let pix_event = match event {
                Event::Quit { .. } => PixEvent::Quit,
                Event::AppTerminating { .. } => PixEvent::AppTerminating,
                Event::Window {
                    win_event,
                    window_id,
                    ..
                } => match win_event {
                    WindowEvent::Resized(..) | WindowEvent::SizeChanged(..) => PixEvent::Resized,
                    WindowEvent::FocusGained => PixEvent::Focus(window_id, true),
                    WindowEvent::FocusLost => PixEvent::Focus(window_id, false),
                    WindowEvent::Close => PixEvent::WinClose(window_id),
                    _ => PixEvent::None, // Ignore others
                },
                Event::ControllerDeviceAdded { which: id, .. } => {
                    match id {
                        0 => self.controller1 = Some(self.controller_sub.open(id)?),
                        1 => self.controller2 = Some(self.controller_sub.open(id)?),
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
                } => self.map_mouse(mouse_btn, x, y, true),
                Event::MouseButtonUp {
                    mouse_btn, x, y, ..
                } => self.map_mouse(mouse_btn, x, y, false),
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
                Event::MouseMotion { x, y, .. } => PixEvent::MouseMotion(x, y),
                Event::AppDidEnterBackground { .. } => PixEvent::Background(true),
                Event::AppDidEnterForeground { .. } => PixEvent::Background(false),
                _ => PixEvent::None, // Ignore others
            };
            if pix_event != PixEvent::None {
                pix_events.push(pix_event)
            }
        }
        Ok(pix_events)
    }

    fn clear(&mut self, window_id: u32) -> PixEngineResult<()> {
        if let Some((canvas, _)) = self.canvases.get_mut(&window_id) {
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
            canvas.clear();
            Ok(())
        } else {
            Err(PixEngineErr::new(format!(
                "invalid window_id {}",
                window_id
            )))
        }
    }

    fn present(&mut self) {
        for (_, (canvas, _)) in self.canvases.iter_mut() {
            canvas.present();
        }
    }

    fn create_texture(
        &mut self,
        window_id: u32,
        name: &str,
        color_type: ColorType,
        src: Rect,
        dst: Rect,
    ) -> PixEngineResult<()> {
        if let Some((_, texture_creator)) = self.canvases.get_mut(&window_id) {
            let (format, channels) = match color_type {
                ColorType::Rgb => (PixelFormatEnum::RGB24, 3),
                ColorType::Rgba => (PixelFormatEnum::RGBA32, 4),
            };
            let mut tex = texture_creator.create_texture_streaming(format, src.w, src.h)?;
            match color_type {
                ColorType::Rgb => tex.set_blend_mode(BlendMode::None),
                ColorType::Rgba => tex.set_blend_mode(BlendMode::Blend),
            }
            let _ = self.texture_maps.insert(
                name.to_string(),
                TextureMap {
                    tex,
                    format,
                    channels,
                    pitch: (channels * src.w) as usize,
                    src: Some(rect_to_sdl(src)),
                    dst: Some(rect_to_sdl(dst)),
                },
            );
            Ok(())
        } else {
            Err(PixEngineErr::new(format!(
                "invalid window_id {}",
                window_id
            )))
        }
    }

    fn copy_texture(&mut self, window_id: u32, name: &str, bytes: &[u8]) -> PixEngineResult<()> {
        if let Some(map) = self.texture_maps.get_mut(name) {
            map.tex.update(None, bytes, map.pitch)?;
            if let Some((canvas, _)) = self.canvases.get_mut(&window_id) {
                canvas.copy(&map.tex, map.src, map.dst)?;
                Ok(())
            } else {
                Err(PixEngineErr::new(format!(
                    "invalid window_id {}",
                    window_id
                )))
            }
        } else {
            Err(PixEngineErr::new(format!("invalid texture {}", name)))
        }
    }

    fn open_window(&mut self, title: &str, width: u32, height: u32) -> PixEngineResult<u32> {
        let video_sub = self.context.video()?;
        let mut window_builder = video_sub.window(title, width, height);
        window_builder.position(20, 40).resizable();
        let window = window_builder.build()?;

        // Set up canvas
        let canvas_builder = window.into_canvas().target_texture();
        let mut canvas = canvas_builder.build()?;
        canvas.set_logical_size(width, height)?;
        let window_id = canvas.window().id();

        let texture_creator = canvas.texture_creator();
        let screen_tex =
            texture_creator.create_texture_streaming(PixelFormatEnum::RGBA32, width, height)?;

        self.canvases.insert(window_id, (canvas, texture_creator));
        self.texture_maps.insert(
            format!("screen{}", window_id),
            TextureMap {
                tex: screen_tex,
                format: PixelFormatEnum::RGBA32,
                channels: 4,
                pitch: (4 * width) as usize,
                src: None,
                dst: None,
            },
        );
        Ok(window_id)
    }

    fn close_window(&mut self, window_id: u32) {
        let _ = self.canvases.remove(&window_id);
    }

    fn enqueue_audio(&mut self, samples: &[f32]) {
        while self.audio_device.size() > SAMPLE_RATE as u32 {}
        self.audio_device.queue(samples);
    }
}

impl From<video::WindowBuildError> for PixEngineErr {
    fn from(err: video::WindowBuildError) -> Self {
        Self::new(err.to_string())
    }
}

impl From<sdl2::IntegerOrSdlError> for PixEngineErr {
    fn from(err: sdl2::IntegerOrSdlError) -> Self {
        Self::new(err.to_string())
    }
}

impl From<render::TextureValueError> for PixEngineErr {
    fn from(err: render::TextureValueError) -> Self {
        Self::new(err.to_string())
    }
}

impl From<render::UpdateTextureError> for PixEngineErr {
    fn from(err: render::UpdateTextureError) -> Self {
        Self::new(err.to_string())
    }
}

impl From<std::ffi::NulError> for PixEngineErr {
    fn from(err: std::ffi::NulError) -> Self {
        Self::new(err.to_string())
    }
}
