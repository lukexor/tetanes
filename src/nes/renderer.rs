use crate::{
    nes::{
        self,
        config::{Config, FRAME_TRIM_PITCH},
    },
    ppu::Ppu,
    profile, NesResult,
};
use anyhow::Context;
use crossbeam::channel::{self, Sender};
use pixels::{
    wgpu::{PowerPreference, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::thread::{self, JoinHandle};
use winit::{dpi::PhysicalSize, window::Window};

#[derive(Debug)]
#[must_use]
pub enum Message {
    NewFrame(Vec<u8>),
    SetVsync(bool),
    Resize(u32, u32),
    Pause(bool),
    Terminate,
}

#[derive(Debug)]
#[must_use]
pub enum Renderer {
    SingleThreaded(Pixels),
    MultiThreaded {
        tx: Sender<Message>,
        handle: JoinHandle<()>,
    },
}

impl Renderer {
    /// Initializes the renderer in a platform-agnostic way.
    pub async fn initialize(
        window: &Window,
        config: &Config,
        nes_tx: Sender<nes::Message>,
    ) -> NesResult<Self> {
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

        if thread::available_parallelism().map_or(false, |count| count.get() > 1) {
            let (tx, rx) = channel::bounded::<Message>(64);
            Ok(Self::MultiThreaded {
                tx,
                handle: thread::Builder::new()
                    .name("renderer".into())
                    .spawn(move || Self::thread_main(pixels, rx, nes_tx))?,
            })
        } else {
            Ok(Self::SingleThreaded(pixels))
        }
    }

    pub fn redraw(&mut self, frame_buffer: &[u8]) {
        profile!();
        if let Err(err) = match self {
            Self::SingleThreaded(ref mut pixels) => Self::render_frame(pixels, frame_buffer),
            // TODO: re-use allocations
            Self::MultiThreaded { tx, .. } => tx
                .try_send(Message::NewFrame(frame_buffer.to_vec()))
                .context("failed to send new frame"),
        } {
            log::error!("error rendering frame: {err:?}");
        }
    }

    pub fn pause(&self) {
        if let Self::MultiThreaded { tx, .. } = self {
            if let Err(err) = tx.try_send(Message::Pause(true)) {
                log::error!("failed to send pause message {err:?}");
            }
        }
    }

    pub fn resume(&self) {
        if let Self::MultiThreaded { handle, .. } = self {
            handle.thread().unpark();
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

    fn thread_main(
        mut renderer: Pixels,
        render_rx: channel::Receiver<Message>,
        event_tx: channel::Sender<nes::Message>,
    ) {
        while let Ok(msg) = render_rx.recv() {
            profile!();
            match msg {
                Message::NewFrame(frame_buffer) => {
                    if let Err(err) = Self::render_frame(&mut renderer, &frame_buffer) {
                        log::error!("error rendering frame: {err:?}");
                        if let Err(err) = event_tx.try_send(nes::Message::Terminate) {
                            log::error!("failed to send terminate message to event_loop: {err:?}");
                        }
                        break;
                    }
                }
                Message::SetVsync(_enabled) => {
                    // TODO: feature not released yet: https://github.com/parasyte/pixels/pull/373
                    // pixels.enable_vsync(enabled),
                }
                Message::Resize(width, height) => {
                    if let Err(err) = renderer.resize_surface(width, height) {
                        log::error!("failed to resize render surface: {err:?}");
                    }
                }
                Message::Terminate => break,
                Message::Pause(true) => thread::park(),
                Message::Pause(false) => (),
            }
        }
    }
}
