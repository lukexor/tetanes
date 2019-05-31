//! Window Management using SDL2

use crate::console::Image;
use crate::console::{SAMPLE_RATE, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::util::Result;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{self, FullscreenType};
use sdl2::{EventPump, GameControllerSubsystem};

const DEFAULT_TITLE: &str = "RustyNES";

/// A Window instance
pub struct Window {
    pub controller_sub: GameControllerSubsystem,
    audio_device: AudioQueue<f32>,
    canvas: Canvas<video::Window>,
    texture: Texture<'static>,
    _texture_creator: TextureCreator<video::WindowContext>,
}

impl Window {
    /// Creates a new Window instance containing the necessary window, audio, and input components
    /// used by the UI
    pub fn init(scale: usize, fullscreen: bool) -> Result<(Self, EventPump)> {
        let context = sdl2::init().expect("sdl context");

        // Set up window canvas
        let video_sub = context.video().expect("sdl video subsystem");
        let mut window_builder = video_sub.window(
            DEFAULT_TITLE,
            (SCREEN_WIDTH * scale) as u32,
            (SCREEN_HEIGHT * scale) as u32,
        );
        window_builder.position_centered();
        if fullscreen {
            window_builder.fullscreen();
        }
        let window = window_builder.build().expect("sdl window");
        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .expect("sdl canvas");
        let texture_creator = canvas.texture_creator();
        let texture_creator_ptr = &texture_creator as *const TextureCreator<video::WindowContext>;
        let texture = unsafe { &*texture_creator_ptr }
            .create_texture_streaming(
                PixelFormatEnum::RGB24,
                SCREEN_WIDTH as u32,
                SCREEN_HEIGHT as u32,
            )
            .expect("sdl texture");

        // Set up Audio
        let audio_sub = context.audio().expect("sdl audio");
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .expect("sdl audio queue");
        audio_device.resume();

        // Set up Input event pump
        let event_pump = context.event_pump().expect("sdl event_pump");
        let controller_sub = context.game_controller().expect("sdl controller_sub");

        let window = Self {
            controller_sub,
            audio_device,
            canvas,
            texture,
            _texture_creator: texture_creator,
        };
        Ok((window, event_pump))
    }

    /// Updates the Window canvas texture with the passed in pixel data
    pub fn render(&mut self, pixels: &Image) {
        self.texture
            .update(None, pixels, SCREEN_WIDTH * 3)
            .expect("texture update");
        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, None)
            .expect("canvas copy");
        self.canvas.present();
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
    pub fn toggle_fullscreen(&mut self) {
        let state = self.canvas.window().fullscreen_state();
        if state == FullscreenType::Off {
            self.canvas
                .window_mut()
                .set_fullscreen(video::FullscreenType::True)
                .expect("toggled fullscreen on");
        } else {
            self.canvas
                .window_mut()
                .set_fullscreen(video::FullscreenType::Off)
                .expect("toggled fullscreen off");
        }
    }
}
