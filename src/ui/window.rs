use crate::console::Image;
use crate::console::{SAMPLE_RATE, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::Result;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::FullscreenType;
use sdl2::{video, EventPump};

const DEFAULT_TITLE: &str = "RustyNES";

pub struct Window {
    pub event_pump: Option<EventPump>,
    canvas: Canvas<video::Window>,
    texture: Texture<'static>,
    pub audio_device: AudioQueue<f32>,
    _texture_creator: TextureCreator<video::WindowContext>,
}

impl Window {
    pub fn with_scale(scale: u32) -> Result<Self> {
        let context = sdl2::init().expect("sdl context");

        // Window/Graphics
        let video_sub = context.video().expect("sdl video subsystem");
        let window = video_sub
            .window(
                DEFAULT_TITLE,
                SCREEN_WIDTH as u32 * scale,
                SCREEN_HEIGHT as u32 * scale,
            )
            .position_centered()
            .build()
            .expect("sdl window");
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

        // Audio
        let audio_sub = context.audio().expect("sdl audio");
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .expect("sdl audio queue");
        audio_device.resume();

        // Input
        let event_pump = Some(context.event_pump().expect("sdl event_pump"));

        Ok(Self {
            event_pump,
            canvas,
            texture,
            audio_device,
            _texture_creator: texture_creator,
        })
    }

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

    pub fn enqueue_audio(&mut self, samples: &mut Vec<f32>) {
        let slice = samples.as_slice();
        self.audio_device.queue(&slice);
        // Keep audio in sync
        loop {
            let latency = self.audio_device.size() as f32 / SAMPLE_RATE as f32;
            if latency <= 1.0 {
                break;
            }
        }
        samples.clear();
    }

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
