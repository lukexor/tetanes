use crate::Result;
use failure::format_err;
use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::GameController,
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    render::{Canvas, Texture, TextureCreator},
    video, AudioSubsystem, EventPump, GameControllerSubsystem, Sdl, VideoSubsystem,
};
use std::{error::Error, sync::mpsc};

const AUDIO_FREQUENCY: i32 = 44100;
const SAMPLES_PER_FRAME: u16 = 2048;
const DEFAULT_TITLE: &str = "NES";
const SCREEN_WIDTH: usize = 256;
const SCREEN_HEIGHT: usize = 240;
const SCREEN_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT * 3;
const DEFAULT_SCALE: u32 = 3;

pub struct Window {
    context: Sdl,
    video_sub: VideoSubsystem,
    canvas: Canvas<video::Window>,
    texture: Texture<'static>,
    audio_sub: AudioSubsystem,
    audio_device: AudioQueue<f32>,
    controller_sub: GameControllerSubsystem,
    controller1: Option<GameController>,
    controller2: Option<GameController>,
    event_pump: EventPump,
    _texture_creator: TextureCreator<video::WindowContext>,
}

impl Window {
    pub fn new() -> Result<Self> {
        Self::with_scale(DEFAULT_SCALE)
    }

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
        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .expect("sdl canvas");
        let texture_creator = canvas.texture_creator();
        let texture_creator_ptr = &texture_creator as *const TextureCreator<video::WindowContext>;
        let texture = unsafe { &*texture_creator_ptr }
            .create_texture_streaming(
                PixelFormatEnum::BGR24,
                SCREEN_WIDTH as u32,
                SCREEN_HEIGHT as u32,
            )
            .expect("sdl texture");

        // Audio
        let audio_sub = context.audio().expect("sdl audio");
        let desired_spec = AudioSpecDesired {
            freq: Some(AUDIO_FREQUENCY),
            channels: Some(1),
            samples: Some(SAMPLES_PER_FRAME),
        };
        let mut audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .expect("sdl audio queue");
        audio_device.resume();

        // Input
        let controller_sub = context.game_controller().expect("sdl controller");
        let event_pump = context.event_pump().expect("sdl event_pump");

        Ok(Window {
            context,
            video_sub,
            canvas,
            texture,
            audio_sub,
            audio_device,
            controller_sub,
            controller1: None,
            controller2: None,
            event_pump,
            _texture_creator: texture_creator,
        })
    }

    pub fn render(&mut self, pixels: &[u8; SCREEN_SIZE]) {
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
        if self.audio_device.size() <= (4 * SAMPLES_PER_FRAME).into() {
            self.audio_device.queue(&slice);
        }
        samples.clear();
    }

    pub fn poll_events(&mut self) {
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    std::process::exit(0);
                }
                _ => (),
                // TODO Debugger, save/load, device added, record, menu, etc
            }
        }
    }
}
