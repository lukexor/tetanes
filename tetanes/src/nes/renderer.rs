use crate::nes::{
    config::Config,
    event::{EmulationEvent, NesEvent, RendererEvent},
    renderer::{
        gui::{Gui, Menu, MSG_TIMEOUT},
        texture::Texture,
    },
    Nes,
};
use anyhow::Context;
use egui::{
    load::SizedTexture, ClippedPrimitive, SystemTheme, TexturesDelta, Vec2, ViewportCommand,
};
use std::{ops::Deref, sync::Arc};
use tetanes_core::{ppu::Ppu, time::Instant, video::Frame};
use thingbuf::{Recycle, ThingBuf};
use tracing::{error, info};
use wgpu::util::DeviceExt;
use winit::{
    event::{Event, WindowEvent},
    event_loop::EventLoopProxy,
    window::{Theme, Window},
};

pub mod gui;
pub mod texture;

pub const OVERSCAN_TRIM: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Debug)]
#[must_use]
pub struct FrameRecycle;

impl Recycle<Frame> for FrameRecycle {
    fn new_element(&self) -> Frame {
        Frame::new()
    }

    fn recycle(&self, _frame: &mut Frame) {}
}

#[derive(Debug)]
pub struct BufferPool(Arc<ThingBuf<Frame, FrameRecycle>>);

impl BufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(2, FrameRecycle)))
    }

    pub fn push(&mut self, frame_buffer: &[u8]) -> bool {
        match self.0.push_ref() {
            Ok(mut frame) => {
                frame.clear();
                frame.extend_from_slice(frame_buffer);
                true
            }
            Err(_) => false,
        }
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
    config: Config,
    gui: Gui,
    ctx: egui::Context,
    egui_state: egui_winit::State,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
    renderer: egui_wgpu::Renderer,
    surface: wgpu::Surface<'static>,
    texture: Texture,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    resize_surface: bool,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
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
        event_proxy: EventLoopProxy<NesEvent>,
        window: Arc<Window>,
        frame_pool: BufferPool,
        config: Config,
    ) -> anyhow::Result<Self> {
        let mut window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        if window_size.width == 0 {
            let scale_factor = window.scale_factor();
            window_size = config
                .read(|cfg| cfg.texture_size())
                .to_physical(scale_factor);
        }

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(Arc::clone(&window))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("failed to request wgpu adapter")?;
        // WebGL doesn't support all of wgpu's features, so if
        // we're building for the web we'll have to disable some.
        let mut required_limits = if cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::downlevel_defaults()
        };
        // However, we do want to support the adapters max texture dimension for window size to
        // be maximized
        required_limits.max_texture_dimension_2d = adapter.limits().max_texture_dimension_2d;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("tetanes"),
                    required_features: wgpu::Features::empty(),
                    required_limits,
                },
                None,
            )
            .await?;

        let max_texture_dimension = device.limits().max_texture_dimension_2d;
        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_capabilities.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: window_size.width.min(max_texture_dimension),
            height: window_size.height.min(max_texture_dimension),
            present_mode: if config.read(|cfg| cfg.renderer.vsync) {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let texture_size = config.read(|cfg| cfg.texture_size());
        let texture = Texture::new(
            &device,
            texture_size.width.min(max_texture_dimension),
            texture_size.height.min(max_texture_dimension),
            Some("nes frame"),
        );
        let module = device.create_shader_module(wgpu::include_wgsl!("../../shaders/blit.wgsl"));

        let vertex_data: [[f32; 2]; 3] = [
            // One full-screen triangle
            [-1.0, -1.0],
            [3.0, -1.0],
            [-1.0, 3.0],
        ];
        let vertex_data_slice = bytemuck::cast_slice(&vertex_data);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: vertex_data_slice,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: (vertex_data_slice.len() / vertex_data.len()) as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nes frame bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            &window,
            Some(scale_factor),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let mut renderer = egui_wgpu::Renderer::new(&device, surface_config.format, None, 1);
        let egui_texture =
            renderer.register_native_texture(&device, &texture.view, wgpu::FilterMode::Nearest);

        let aspect_ratio = config.read(|cfg| cfg.deck.region.aspect_ratio());
        let state = Gui::new(
            Arc::clone(&window),
            event_proxy,
            SizedTexture::new(
                egui_texture,
                Vec2 {
                    x: texture.size.width as f32 * aspect_ratio,
                    y: texture.size.height as f32,
                },
            ),
            config.clone(),
        );

        Ok(Self {
            window,
            frame_pool,
            config,
            gui: state,
            ctx,
            egui_state,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [window_size.width, window_size.height],
                pixels_per_point: scale_factor,
            },
            renderer,
            surface,
            texture,
            device,
            queue,
            surface_config,
            resize_surface: false,
            render_pipeline,
            vertex_buffer,
            bind_group_layout,
            bind_group,
            paint_jobs: vec![],
            textures: TexturesDelta::default(),
        })
    }

    /// Handle event.
    pub fn on_event(&mut self, window: &Window, event: &Event<NesEvent>) {
        match event {
            Event::WindowEvent { event, .. } => {
                let _ = self.egui_state.on_window_event(window, event);
                match event {
                    WindowEvent::Resized(size) => {
                        if size.width > 0 && size.height > 0 {
                            let max_texture_dimension =
                                self.device.limits().max_texture_dimension_2d;
                            let width = size.width.min(max_texture_dimension);
                            let height = size.height.min(max_texture_dimension);
                            self.screen_descriptor.size_in_pixels = [width, height];
                            self.surface_config.width = width;
                            self.surface_config.height = height;
                            self.resize_surface = true;

                            let scale_factor = window.scale_factor() as f32;
                            let texture_size = self.config.read(|cfg| cfg.texture_size());
                            let scale = if size.width < size.height {
                                (width as f32 / scale_factor) / texture_size.width as f32
                            } else {
                                (height as f32 / scale_factor) / texture_size.height as f32
                            };
                            self.config.write(|cfg| {
                                cfg.renderer.scale = scale.floor();
                            });
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
            Event::UserEvent(event) => match event {
                NesEvent::Emulation(event) => match event {
                    EmulationEvent::ReplayRecord(recording) => {
                        self.gui.replay_recording = *recording;
                    }
                    EmulationEvent::AudioRecord(recording) => {
                        self.gui.audio_recording = *recording;
                    }
                    EmulationEvent::Pause(paused) => {
                        self.gui.paused = *paused;
                    }
                    _ => (),
                },
                NesEvent::Renderer(event) => match event {
                    RendererEvent::SetVSync(enabled) => {
                        self.surface_config.present_mode = if *enabled {
                            wgpu::PresentMode::AutoVsync
                        } else {
                            wgpu::PresentMode::AutoNoVsync
                        };
                        self.resize_surface = true;
                    }
                    RendererEvent::Frame => self.gui.frame_counter += 1,
                    RendererEvent::RomLoaded((title, region)) => {
                        self.gui.title = format!("{} :: {title}", Config::WINDOW_TITLE);
                        self.gui.cart_aspect_ratio = region.aspect_ratio();
                        self.gui.resize_window = true;
                        self.gui.resize_texture = true;
                    }
                    RendererEvent::Menu(menu) => match menu {
                        Menu::Config(_) => self.gui.preferences_open = !self.gui.preferences_open,
                        Menu::Keybind(_) => self.gui.keybinds_open = !self.gui.keybinds_open,
                        Menu::About => self.gui.about_open = !self.gui.about_open,
                    },
                },
                NesEvent::Ui(_) => (),
            },
            _ => (),
        }
    }

    fn resize_texture(&mut self) {
        let texture_size = self.config.read(|cfg| cfg.texture_size());
        self.texture
            .resize(&self.device, texture_size.width, texture_size.height);
        self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nes frame bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture.sampler),
                },
            ],
        });
        let egui_texture = self.renderer.register_native_texture(
            &self.device,
            &self.texture.view,
            wgpu::FilterMode::Nearest,
        );
        let region = self.config.read(|cfg| cfg.deck.region);
        let aspect_ratio = if region.is_auto() {
            self.gui.cart_aspect_ratio
        } else {
            region.aspect_ratio()
        };
        self.gui.texture = SizedTexture::new(
            egui_texture,
            Vec2 {
                x: self.texture.size.width as f32 * aspect_ratio,
                y: self.texture.size.height as f32,
            },
        );
    }

    /// Prepare.
    fn prepare(&mut self, window: &Window) {
        let raw_input = self.egui_state.take_egui_input(window);

        let output = self.ctx.run(raw_input, |ctx| {
            self.gui.ui(ctx);
        });

        self.screen_descriptor.pixels_per_point = output.pixels_per_point;
        self.textures.append(output.textures_delta);
        self.egui_state
            .handle_platform_output(window, output.platform_output);
        self.paint_jobs = self.ctx.tessellate(output.shapes, output.pixels_per_point);
    }

    /// Request redraw.
    pub fn request_redraw(&mut self, window: &Window) -> anyhow::Result<()> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        self.prepare(window);

        if self.resize_surface || self.gui.resize_window {
            self.surface.configure(&self.device, &self.surface_config);
            if self.gui.resize_window {
                let region = self.config.read(|cfg| cfg.deck.region);
                let aspect_ratio = if region.is_auto() {
                    self.gui.cart_aspect_ratio
                } else {
                    region.aspect_ratio()
                };
                let mut window_size = self.config.read(|cfg| cfg.window_size());
                window_size.width *= aspect_ratio;
                window_size.height += self.gui.menu_height;
                let _ = self.window.request_inner_size(window_size);
            }
            self.resize_surface = false;
            self.gui.resize_window = false;
        }
        if self.gui.resize_texture {
            self.resize_texture();
            self.gui.resize_texture = false;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("renderer"),
            });

        let frame = self.surface.get_current_texture().or_else(|_| {
            self.surface.configure(&self.device, &self.surface_config);
            self.surface.get_current_texture()
        })?;
        // Copy NES frame buffer
        if let Some(frame_buffer) = self.frame_pool.pop_ref() {
            self.texture.update(
                &self.queue,
                if self.config.read(|cfg| cfg.renderer.hide_overscan) {
                    let len = frame_buffer.len();
                    &frame_buffer[OVERSCAN_TRIM..len - OVERSCAN_TRIM]
                } else {
                    &frame_buffer
                },
            );
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gui"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });

        for (id, image_delta) in &self.textures.set {
            self.renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }
        self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.paint_jobs,
            &self.screen_descriptor,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            self.renderer
                .render(&mut render_pass, &self.paint_jobs, &self.screen_descriptor);
        }

        // Cleanup
        let textures = std::mem::take(&mut self.textures);
        for id in &textures.free {
            self.renderer.free_texture(id);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();

        Ok(())
    }
}

impl Nes {
    pub fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        info!("{text}");
        self.renderer
            .gui
            .messages
            .push((text, Instant::now() + MSG_TIMEOUT));
    }

    pub fn on_error(&mut self, err: anyhow::Error) {
        self.trigger_event(EmulationEvent::Pause(true));
        error!("{err:?}");
        self.renderer.gui.error = Some(err.to_string());
    }
}
