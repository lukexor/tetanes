use crate::{
    nes::{
        config::{Config, OVERSCAN_TRIM},
        event::{DeckEvent, Event, RendererEvent},
        renderer::gui::{Gui, Menu, MSG_TIMEOUT},
        Nes,
    },
    platform::time::Instant,
    ppu::Ppu,
    profile,
    video::{Frame, FrameRecycle},
    NesError, NesResult,
};
use egui::{
    load::SizedTexture, ClippedPrimitive, Context, SystemTheme, TexturesDelta, Vec2,
    ViewportCommand, ViewportId,
};
use pixels::{
    wgpu::{
        FilterMode, LoadOp, Operations, PowerPreference, RenderPassColorAttachment,
        RenderPassDescriptor, RequestAdapterOptions, StoreOp, TextureViewDescriptor,
    },
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{ops::Deref, sync::Arc};
use thingbuf::ThingBuf;
use winit::{
    dpi::LogicalSize,
    event::{Event as WinitEvent, WindowEvent},
    event_loop::EventLoop,
    window::{Theme, Window},
};

pub mod gui;

#[derive(Debug)]
#[must_use]
pub enum Message {
    NewFrame,
}

#[derive(Debug)]
pub struct BufferPool(Arc<ThingBuf<Frame, FrameRecycle>>);

impl BufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(2, FrameRecycle)))
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

#[must_use]
pub struct Renderer {
    window: Arc<Window>,
    frame_pool: BufferPool,
    pixels: Pixels<'static>,
    gui: Gui,
    ctx: Context,
    egui_state: egui_winit::State,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
    renderer: egui_wgpu::Renderer,
    paint_jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,
}
impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("gui", &self.gui)
            .finish_non_exhaustive()
    }
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
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let ctx = Context::default();

        let egui_state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::default(),
            event_loop,
            Some(scale_factor),
            Some(pixels.device().limits().max_texture_dimension_2d as usize),
        );

        let texture = pixels.texture();
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let mut renderer =
            egui_wgpu::Renderer::new(pixels.device(), pixels.render_texture_format(), None, 1);
        let egui_texture =
            renderer.register_native_texture(pixels.device(), &texture_view, FilterMode::Nearest);
        let state = Gui::new(
            Arc::clone(&window),
            event_loop,
            SizedTexture::new(
                egui_texture,
                Vec2 {
                    x: window_size.width as f32,
                    y: window_size.height as f32,
                },
            ),
        );

        Ok(Self {
            window,
            frame_pool,
            pixels,
            gui: state,
            ctx,
            egui_state,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [window_size.width, window_size.height],
                pixels_per_point: scale_factor,
            },
            renderer,
            paint_jobs: vec![],
            textures: TexturesDelta::default(),
        })
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &WinitEvent<Event>) -> NesResult<()> {
        match event {
            WinitEvent::WindowEvent { event, .. } => {
                let _ = self.egui_state.on_window_event(&self.window, event);
                match event {
                    WindowEvent::Resized(size) => {
                        if size.width > 0 && size.height > 0 {
                            self.screen_descriptor.size_in_pixels = [size.width, size.height];
                            self.pixels.resize_surface(size.width, size.height)?;
                        }
                    }
                    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        self.screen_descriptor.pixels_per_point = *scale_factor as f32;
                    }
                    WindowEvent::ThemeChanged(theme) => {
                        self.ctx.send_viewport_cmd(ViewportCommand::SetTheme(
                            if *theme == Theme::Light {
                                SystemTheme::Light
                            } else {
                                SystemTheme::Dark
                            },
                        ));
                    }
                    _ => (),
                }
            }
            WinitEvent::UserEvent(Event::Renderer(event)) => match event {
                RendererEvent::SetVSync(enabled) => self.pixels.enable_vsync(*enabled),
                RendererEvent::SetScale(_) => {
                    // TODO
                    // self.state
                    //     .resize_window(&self.ctx.style(), &mut self.state.config);
                }
                RendererEvent::Frame(duration) => self.gui.last_frame_duration = *duration,
                RendererEvent::Menu(menu) => match menu {
                    Menu::Config(_) => self.gui.config_open = !self.gui.config_open,
                    Menu::Keybind(_) => self.gui.keybind_open = !self.gui.keybind_open,
                    Menu::LoadRom => self.gui.load_rom_open = !self.gui.load_rom_open,
                    Menu::About => self.gui.about_open = !self.gui.about_open,
                },
            },
            _ => (),
        }
        Ok(())
    }

    /// Prepare.
    pub fn prepare(&mut self, paused: bool, config: &mut Config) {
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

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let output = self.ctx.run(raw_input, |ctx| {
            self.gui.ui(ctx, paused, config);
        });

        self.textures.append(output.textures_delta);
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        self.paint_jobs = self
            .ctx
            .tessellate(output.shapes, self.screen_descriptor.pixels_per_point);
    }

    /// Request redraw.
    pub fn request_redraw(&mut self, paused: bool, config: &mut Config) -> NesResult<()> {
        profile!();

        self.prepare(paused, config);
        self.pixels.render_with(|encoder, render_target, ctx| {
            // ctx.scaling_renderer.render(encoder, render_target);

            for (id, image_delta) in &self.textures.set {
                self.renderer
                    .update_texture(&ctx.device, &ctx.queue, *id, image_delta);
            }
            self.renderer.update_buffers(
                &ctx.device,
                &ctx.queue,
                encoder,
                &self.paint_jobs,
                &self.screen_descriptor,
            );

            {
                let mut renderpass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("gui"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: render_target,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Load,
                            store: StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });

                self.renderer
                    .render(&mut renderpass, &self.paint_jobs, &self.screen_descriptor);
            }

            // Cleanup
            let textures = std::mem::take(&mut self.textures);
            for id in &textures.free {
                self.renderer.free_texture(id);
            }
            Ok(())
        })?;

        Ok(())
    }
}

impl Nes {
    pub fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        log::info!("{text}");
        self.renderer
            .gui
            .messages
            .push((text, Instant::now() + MSG_TIMEOUT));
    }

    pub fn on_error(&mut self, err: NesError) {
        self.send_event(DeckEvent::Pause(true));
        log::error!("{err:?}");
        self.renderer.gui.error = Some(err.to_string());
    }
}
