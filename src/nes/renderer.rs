use crate::{
    nes::{
        config::{Config, OVERSCAN_TRIM},
        event::{Event, RendererEvent},
        gui,
    },
    ppu::Ppu,
    profile,
    video::{Frame, FrameRecycle},
    NesResult,
};
use pixels::{
    wgpu::{PowerPreference, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{ops::Deref, sync::Arc};
use thingbuf::ThingBuf;
use winit::{
    dpi::LogicalSize,
    event::{Event as WinitEvent, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};

#[derive(Debug)]
#[must_use]
pub enum Message {
    NewFrame,
}

#[derive(Debug)]
pub struct BufferPool(Arc<ThingBuf<Frame, FrameRecycle>>);

impl BufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(1, FrameRecycle)))
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for BufferPool {
    type Target = Arc<ThingBuf<Frame, FrameRecycle>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for BufferPool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[derive(Debug)]
#[must_use]
pub struct Renderer {
    frame_pool: BufferPool,
    pub(crate) gui: gui::Gui,
    pixels: Pixels<'static>,
}

impl Renderer {
    /// Initializes the renderer in a platform-agnostic way.
    pub async fn initialize(
        event_loop: &EventLoop<Event>,
        window: Arc<Window>,
        frame_pool: BufferPool,
        config: &Config,
    ) -> NesResult<Self> {
        let mut window_size = window.inner_size();
        if window_size.width == 0 {
            let scale_factor = window.scale_factor();
            let (width, height) = config.dimensions();
            window_size = LogicalSize::new(width, height).to_physical(scale_factor);
        }
        let surface_texture =
            SurfaceTexture::new(window_size.width, window_size.height, Arc::clone(&window));
        let pixels = PixelsBuilder::new(Ppu::WIDTH, Ppu::HEIGHT, surface_texture)
            .request_adapter_options(RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                ..Default::default()
            })
            .enable_vsync(config.vsync)
            .build_async()
            .await?;
        let gui = gui::Gui::new(event_loop, Arc::clone(&window), &pixels);

        Ok(Self {
            frame_pool,
            gui,
            pixels,
        })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &WinitEvent<Event>) -> NesResult<()> {
        match event {
            WinitEvent::WindowEvent { event, .. } => {
                let _ = self.gui.on_event(event);
                if let WindowEvent::Resized(size) = event {
                    self.pixels.resize_surface(size.width, size.height)?
                }
            }
            WinitEvent::UserEvent(Event::Renderer(RendererEvent::SetVSync(enabled))) => {
                self.pixels.enable_vsync(*enabled);
            }
            _ => (),
        }
        Ok(())
    }

    /// Request redraw.
    pub fn request_redraw(&mut self, paused: bool, config: &mut Config) -> NesResult<()> {
        profile!();

        self.gui.prepare(paused, config);

        // Copy NES frame buffer
        if let Some(frame_buffer) = self.frame_pool.pop_ref() {
            let frame = self.pixels.frame_mut();
            if config.hide_overscan {
                let len = frame_buffer.len();
                frame[OVERSCAN_TRIM..len - OVERSCAN_TRIM]
                    .copy_from_slice(&frame_buffer[OVERSCAN_TRIM..len - OVERSCAN_TRIM]);
                frame[..OVERSCAN_TRIM].fill(0);
                frame[len - OVERSCAN_TRIM..].fill(0);
            } else {
                frame.copy_from_slice(&frame_buffer);
            }
        };

        Ok(self.pixels.render_with(|encoder, render_target, ctx| {
            self.gui.render(encoder, render_target, ctx);
            Ok(())
        })?)
    }

    pub fn toggle_menu(&mut self, menu: gui::Menu) {
        self.gui.toggle_menu(menu);
    }
}
