use crate::{
    nes::config::{Config, FRAME_TRIM_PITCH},
    ppu::Ppu,
    profile,
    video::Video,
    NesResult,
};
use crossbeam::channel::{self, Sender};
use pixels::{
    wgpu::{PowerPreference, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};
use thingbuf::{recycling::WithCapacity, ThingBuf};
use winit::{dpi::PhysicalSize, window::Window};

#[derive(Debug)]
#[must_use]
pub enum Message {
    NewFrame,
    SetVsync(bool),
    Resize(u32, u32),
    Terminate,
}

#[derive(Debug)]
#[must_use]
struct MultiThreaded {
    tx: Sender<Message>,
    handle: JoinHandle<NesResult<()>>,
}

impl MultiThreaded {
    fn spawn(pixels: Pixels, buffer_pool: BufferPool) -> NesResult<Self> {
        let (tx, rx) = channel::bounded::<Message>(64);
        Ok(Self {
            tx,
            handle: thread::Builder::new()
                .name("renderer".into())
                .spawn(move || Self::main(pixels, buffer_pool, rx))?,
        })
    }

    fn main(
        mut renderer: Pixels,
        buffer_pool: BufferPool,
        rx: channel::Receiver<Message>,
    ) -> NesResult<()> {
        let mut latest_frame = None;
        loop {
            while let Ok(msg) = rx.try_recv() {
                profile!();
                match msg {
                    // Only render the latest frame
                    Message::NewFrame => {
                        if let Some(frame) = buffer_pool.pop_ref() {
                            latest_frame = Some(frame);
                        }
                    }
                    Message::SetVsync(_enabled) => {
                        // TODO: feature not released yet: https://github.com/parasyte/pixels/pull/373
                        // pixels.enable_vsync(enabled),
                    }
                    Message::Resize(width, height) => renderer.resize_surface(width, height)?,
                    Message::Terminate => break,
                }
            }
            if let Some(ref frame) = latest_frame {
                Renderer::render_frame(&mut renderer, frame)?;
            }
        }
    }
}

type BufferPool = Arc<ThingBuf<Vec<u8>, WithCapacity>>;

#[derive(Debug)]
#[must_use]
enum Backend {
    SingleThreaded(Pixels),
    MultiThreaded(MultiThreaded),
}

#[derive(Debug)]
#[must_use]
pub struct Renderer {
    buffer_pool: BufferPool,
    backend: Backend,
}

impl Renderer {
    /// Initializes the renderer in a platform-agnostic way.
    pub async fn initialize(window: &Window, config: &Config) -> NesResult<Self> {
        let mut window_size = window.inner_size();
        if window_size.width == 0 {
            let (width, height) = config.get_dimensions();
            window_size = PhysicalSize::new(width, height);
        }
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        let pixels = PixelsBuilder::new(Ppu::WIDTH, Ppu::HEIGHT - 16, surface_texture)
            .request_adapter_options(RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                ..Default::default()
            })
            .enable_vsync(config.vsync)
            .build_async()
            .await?;

        let buffer_pool = Arc::new(ThingBuf::with_recycle(
            16,
            WithCapacity::new().with_min_capacity(Video::FRAME_SIZE),
        ));
        let backend = if config.threaded
            && thread::available_parallelism().map_or(false, |count| count.get() > 1)
        {
            Backend::MultiThreaded(MultiThreaded::spawn(pixels, Arc::clone(&buffer_pool))?)
        } else {
            Backend::SingleThreaded(pixels)
        };

        Ok(Self {
            buffer_pool,
            backend,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> NesResult<()> {
        match self.backend {
            Backend::SingleThreaded(ref mut pixels) => pixels.resize_surface(width, height)?,
            // TODO: re-use allocations
            Backend::MultiThreaded(MultiThreaded { ref tx, .. }) => {
                tx.try_send(Message::Resize(width, height))?;
            }
        }
        Ok(())
    }

    pub fn draw_frame(&mut self, frame_buffer: &[u8]) -> NesResult<()> {
        match self.backend {
            Backend::SingleThreaded(ref mut pixels) => Self::render_frame(pixels, frame_buffer),
            // TODO: re-use allocations
            Backend::MultiThreaded(MultiThreaded { ref tx, .. }) => {
                if let Ok(mut buffer_slot) = self.buffer_pool.push_ref() {
                    buffer_slot.extend_from_slice(frame_buffer);
                    tx.try_send(Message::NewFrame)?;
                }
                Ok(())
            }
        }
    }

    fn render_frame(renderer: &mut Pixels, frame_buffer: &[u8]) -> NesResult<()> {
        profile!();

        // Copy NES frame buffer
        let frame_buffer_len = frame_buffer.len();
        renderer
            .frame_mut()
            .copy_from_slice(&frame_buffer[FRAME_TRIM_PITCH..frame_buffer_len - FRAME_TRIM_PITCH]);

        // TODO: Render framerate
        // TODO: Draw zapper crosshair
        // if self.config.zapper {
        //     s.set_texture_target(texture_id)?;
        //     let (x, y) = self.control_deck.zapper_pos();
        //     s.stroke(Color::GRAY);
        //     s.line([x - 8, y, x + 8, y])?;
        //     s.line([x, y - 8, x, y + 8])?;
        //     s.clear_texture_target();
        // }
        // TODO: Render menus
        // TODO: Render debug windows
        // self.render_debugger(s)?;
        // self.render_ppu_viewer(s)?;
        //         match self.mode {
        //             Mode::Paused | Mode::PausedBg => {
        //                 if self.confirm_quit.is_some() {
        //                     if self.render_confirm_quit(s)? {
        //                         s.quit();
        //                     }
        //                 } else {
        //                     self.render_status(s, "Paused")?;
        //                 }
        //             }
        //             Mode::InMenu(menu) => self.render_menu(s, menu)?,
        //             Mode::Rewinding => {
        //                 self.render_status(s, "Rewinding")?;
        //                 self.rewind();
        //             }
        //             Mode::Playing => match self.replay.mode {
        //                 ReplayMode::Recording => self.render_status(s, "Recording Replay")?,
        //                 ReplayMode::Playback => self.render_status(s, "Replay Playback")?,
        //                 ReplayMode::Off => (),
        //             },
        //         }
        //         if (self.config.speed - 1.0).abs() > f32::EPSILON {
        //             self.render_status(s, &format!("Speed {:.2}", self.config.speed))?;
        //         }
        //         self.render_messages(s)?;

        Ok(renderer.render()?)
    }
}
