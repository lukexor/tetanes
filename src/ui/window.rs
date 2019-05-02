use sdl2::{
    audio::{AudioQueue, AudioSpecDesired},
    controller::GameController,
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    render::{Canvas, Texture},
    video, AudioSubsystem, EventPump, GameControllerSubsystem, Sdl, VideoSubsystem,
};
use std::{error::Error, sync::mpsc};

const AUDIO_FREQUENCY: i32 = 44100;
const SAMPLES_PER_FRAME: u16 = 2048;
const DEFAULT_TITLE: &str = "NES";
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const SCALE: u32 = 3;

pub struct Window {
    context: Sdl,
    video_sub: VideoSubsystem,
    canvas: Canvas<video::Window>,
    // texture: Texture<'static>,
    audio_sub: AudioSubsystem,
    audio_device: AudioQueue<f32>,
    controller_sub: GameControllerSubsystem,
    controller1: Option<GameController>,
    controller2: Option<GameController>,
    event_pump: EventPump,
}

impl Window {
    pub fn new() -> Result<Self, Box<Error>> {
        let context = sdl2::init().expect("sdl context");

        // Window/Graphics
        let video_sub = context.video().expect("sdl video subsystem");
        let window = video_sub
            .window(DEFAULT_TITLE, WIDTH * SCALE, HEIGHT * SCALE)
            .position_centered()
            .build()
            .expect("sdl window");
        let mut canvas = window.into_canvas().build().expect("sdl canvas");

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
            // texture,
            audio_sub,
            audio_device,
            controller_sub,
            controller1: None,
            controller2: None,
            event_pump,
        })
    }

    pub fn render(&mut self, pixels: Vec<u8>) {
        let texture_creator = self.canvas.texture_creator();
        if pixels[0] != 0 {
            panic!("{:?}", &pixels[..10]);
        }
        let mut texture = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGB24, WIDTH, HEIGHT)
            .expect("sdl texture");
        texture
            .update(None, &pixels, WIDTH as usize)
            .expect("texture update");
        self.canvas.clear();
        self.canvas.copy(&texture, None, None).expect("canvas copy");
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
