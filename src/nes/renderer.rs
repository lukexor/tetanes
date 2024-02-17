use crate::{
    frame_begin,
    nes::{
        config::Config,
        event::{Event, RendererEvent},
        gui,
    },
    ppu::Ppu,
    profile,
    video::Video,
    NesResult,
};
use pixels::{
    wgpu::{PowerPreference, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{ops::Deref, sync::Arc};
use thingbuf::{recycling::WithCapacity, ThingBuf};
use winit::{
    dpi::PhysicalSize,
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
pub struct BufferPool(Arc<ThingBuf<Vec<u8>, WithCapacity>>);

impl BufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(
            1,
            WithCapacity::new().with_min_capacity(Video::FRAME_SIZE),
        )))
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for BufferPool {
    type Target = Arc<ThingBuf<Vec<u8>, WithCapacity>>;
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
    pub(crate) frame_buffer: BufferPool,
    pub(crate) gui: gui::Gui,
    pub(crate) pixels: Pixels<'static>,
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
            let (width, height) = config.get_dimensions();
            window_size = PhysicalSize::new(width, height);
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
            frame_buffer: frame_pool,
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
            WinitEvent::UserEvent(Event::Renderer(RendererEvent::ToggleVsync)) => {
                // TODO: Toggle vsync, not released yet/ See: https://github.com/parasyte/pixels/issues/372
            }
            _ => (),
        }
        Ok(())
    }

    /// Request redraw.
    pub fn request_redraw(&mut self, paused: bool, config: &mut Config) -> NesResult<()> {
        frame_begin!();
        profile!();

        self.gui.prepare(paused, config);

        // Copy NES frame buffer
        if let Some(frame_buffer) = self.frame_buffer.pop_ref() {
            let frame = self.pixels.frame_mut();
            frame.copy_from_slice(&frame_buffer[..]);
        };

        Ok(self.pixels.render_with(|encoder, render_target, ctx| {
            ctx.scaling_renderer.render(encoder, render_target);
            self.gui.render(encoder, render_target, ctx);
            Ok(())
        })?)
    }

    pub fn toggle_menu(&mut self, menu: gui::Menu) {
        self.gui.toggle_menu(menu);
    }
}
