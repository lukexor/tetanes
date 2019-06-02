//! Window Management using SDL2

use crate::console::{Image, Rgb, RENDER_HEIGHT, RENDER_WIDTH, SAMPLE_RATE};
use crate::util::{self, Result};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{self, FullscreenType};
use sdl2::{EventPump, GameControllerSubsystem};

const WINDOW_WIDTH: u32 = 292; // 256 * 8/7 for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = (RENDER_HEIGHT - 16) as u32;

/// A Window instance
pub struct Window {
    width: u32,
    height: u32,
    pub controller_sub: GameControllerSubsystem,
    audio_device: AudioQueue<f32>,
    canvas: Canvas<video::Window>,
    overscan_src: Rect,
    overscan_dst: Rect,
    texture: Texture<'static>,
    default_bg_color: Color,
    _texture_creator: TextureCreator<video::WindowContext>,
}

impl Window {
    /// Creates a new Window instance containing the necessary window, audio, and input components
    /// used by the UI
    pub fn init(title: &str, scale: u32, fullscreen: bool) -> Result<(Self, EventPump)> {
        let context = sdl2::init().map_err(util::str_to_err)?;

        let width = WINDOW_WIDTH * scale;
        let height = WINDOW_HEIGHT * scale;

        // Window
        let video_sub = context.video().map_err(util::str_to_err)?;
        let mut window_builder = video_sub.window(title, width, height);
        window_builder.position_centered().resizable();
        if fullscreen {
            window_builder.fullscreen();
        }
        let window = window_builder.build()?;

        // Canvas
        let mut canvas = window.into_canvas().accelerated().present_vsync().build()?;
        canvas.set_logical_size(width, height)?;

        // Texture
        let texture_creator = canvas.texture_creator();
        let texture_creator_ptr = &texture_creator as *const TextureCreator<video::WindowContext>;
        let texture = unsafe { &*texture_creator_ptr }.create_texture_streaming(
            PixelFormatEnum::RGB24,
            RENDER_WIDTH as u32,
            RENDER_HEIGHT as u32,
        )?;

        // Set up Audio
        let audio_sub = context.audio().map_err(util::str_to_err)?;
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .map_err(util::str_to_err)?;
        audio_device.resume();

        // Set up Input event pump
        let event_pump = context.event_pump().map_err(util::str_to_err)?;
        let controller_sub = context.game_controller().map_err(util::str_to_err)?;

        let window = Self {
            width,
            height,
            controller_sub,
            audio_device,
            canvas,
            overscan_src: Rect::new(0, 8, width, height),
            overscan_dst: Rect::new(0, 0, width, height + 8),
            texture,
            default_bg_color: Color::RGB(0, 0, 0),
            _texture_creator: texture_creator,
        };
        Ok((window, event_pump))
    }

    /// Updates the Window canvas texture with the passed in pixel data
    pub fn render(&mut self, pixels: &Image) -> Result<()> {
        self.texture.update(None, pixels, RENDER_WIDTH * 3)?;
        self.canvas.clear();
        self.canvas
            .copy(&self.texture, self.overscan_src, self.overscan_dst)
            .map_err(util::str_to_err)?;
        self.canvas.present();
        Ok(())
    }

    pub fn render_bg(&mut self) -> Result<()> {
        self.canvas.clear();
        self.canvas.present();
        Ok(())
    }

    pub fn set_default_bg_color(&mut self, color: Rgb) {
        self.default_bg_color = Color::RGB(color.r(), color.g(), color.b());
        self.canvas.set_draw_color(self.default_bg_color);
    }

    /// Add audio samples to the audio queue
    pub fn enqueue_audio(&mut self, samples: &[f32]) {
        self.audio_device.queue(samples);
        // Keep audio in sync
        loop {
            let latency = self.audio_device.size() as f32 / SAMPLE_RATE as f32;
            if latency <= 1.0 {
                break;
            }
        }
    }

    /// Toggles fullscreen mode on the SDL2 window
    pub fn toggle_fullscreen(&mut self) -> Result<()> {
        let state = self.canvas.window().fullscreen_state();
        let mode = if state == FullscreenType::Off {
            video::FullscreenType::True
        } else {
            video::FullscreenType::Off
        };
        self.canvas.window_mut().maximize();
        self.canvas
            .window_mut()
            .set_fullscreen(mode)
            .map_err(util::str_to_err)?;
        self.canvas.window_mut().set_size(self.width, self.height)?;
        Ok(())
    }

    /// Sets the window title
    pub fn set_title(&mut self, title: &str) -> Result<()> {
        self.canvas.window_mut().set_title(title)?;
        Ok(())
    }
}
