use crate::{
    nes::{
        config::{Config, OVERSCAN_TRIM},
        event::{EmulationEvent, Event, RendererEvent},
        renderer::{
            gui::{Gui, Menu, MSG_TIMEOUT},
            texture::Texture,
        },
        Nes,
    },
    platform::time::Instant,
    profile,
    video::{Frame, FrameRecycle},
    NesError, NesResult,
};
use anyhow::Context;
use crossbeam::channel::Sender;
use egui::{
    load::SizedTexture, ClippedPrimitive, SystemTheme, TexturesDelta, Vec2, ViewportCommand,
};
use std::{ops::Deref, sync::Arc};
use thingbuf::ThingBuf;
use tracing::{error, info};
use wgpu::util::DeviceExt;
use winit::{
    dpi::LogicalSize,
    event::WindowEvent,
    window::{Theme, Window},
};

pub mod gui;

pub mod texture {
    pub struct Texture {
        pub label: Option<&'static str>,
        pub texture: wgpu::Texture,
        pub size: wgpu::Extent3d,
        pub view: wgpu::TextureView,
        pub sampler: wgpu::Sampler,
    }

    impl Texture {
        pub fn new(
            device: &wgpu::Device,
            width: u32,
            height: u32,
            label: Option<&'static str>,
        ) -> Self {
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            Self {
                label,
                texture,
                size,
                view,
                sampler,
            }
        }

        pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
            self.size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            self.texture = device.create_texture(&wgpu::TextureDescriptor {
                label: self.label,
                size: self.size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            self.view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
        }

        pub fn update(&self, queue: &wgpu::Queue, bytes: &[u8]) {
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    aspect: wgpu::TextureAspect::All,
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                bytes,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.size.width),
                    rows_per_image: Some(self.size.height),
                },
                self.size,
            );
        }
    }
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
    frame_pool: BufferPool,
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
        event_tx: Sender<Event>,
        window: Arc<Window>,
        frame_pool: BufferPool,
        config: &Config,
    ) -> NesResult<Self> {
        let mut window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        if window_size.width == 0 {
            let scale_factor = window.scale_factor();
            let (width, height) = config.window_dimensions();
            window_size = LogicalSize::new(width, height).to_physical(scale_factor);
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
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("tetanes"),
                    required_features: wgpu::Features::empty(),
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    required_limits: if cfg!(target_arch = "wasm32") {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::downlevel_defaults()
                    },
                },
                None,
            )
            .await?;

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
            width: window_size.width,
            height: window_size.height,
            present_mode: if config.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let (width, height) = config.texture_dimensions();
        let texture = Texture::new(&device, width as u32, height as u32, Some("nes frame"));
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

        let state = Gui::new(
            event_tx,
            SizedTexture::new(
                egui_texture,
                Vec2 {
                    x: texture.size.width as f32,
                    y: texture.size.height as f32,
                },
            ),
        );

        Ok(Self {
            frame_pool,
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
            render_pipeline,
            vertex_buffer,
            bind_group_layout,
            bind_group,
            paint_jobs: vec![],
            textures: TexturesDelta::default(),
        })
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) {
        let _ = self.egui_state.on_window_event(window, event);
        match event {
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.screen_descriptor.size_in_pixels = [size.width, size.height];
                    self.surface_config.width = size.width;
                    self.surface_config.height = size.height;
                    self.surface.configure(&self.device, &self.surface_config);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.screen_descriptor.pixels_per_point = *scale_factor as f32;
            }
            WindowEvent::ThemeChanged(theme) => {
                self.ctx
                    .send_viewport_cmd(ViewportCommand::SetTheme(if *theme == Theme::Light {
                        SystemTheme::Light
                    } else {
                        SystemTheme::Dark
                    }));
            }
            _ => (),
        }
    }

    /// Handle event.
    pub fn on_event(&mut self, event: RendererEvent, config: &Config) {
        match event {
            RendererEvent::SetVSync(enabled) => {
                self.surface_config.present_mode = if enabled {
                    wgpu::PresentMode::AutoVsync
                } else {
                    wgpu::PresentMode::AutoNoVsync
                };
                self.surface.configure(&self.device, &self.surface_config);
            }
            RendererEvent::SetScale(_) => self.gui.resize_window(&self.ctx.style(), config),
            RendererEvent::Frame(duration) => self.gui.last_frame_duration = duration,
            RendererEvent::Pause(paused) => self.gui.paused = paused,
            RendererEvent::Menu(menu) => match menu {
                Menu::Config(_) => self.gui.config_open = !self.gui.config_open,
                Menu::Keybind(_) => self.gui.keybind_open = !self.gui.keybind_open,
                Menu::LoadRom => self.gui.load_rom_open = !self.gui.load_rom_open,
                Menu::About => self.gui.about_open = !self.gui.about_open,
            },
        }
    }

    fn resize_texture(&mut self, config: &Config) {
        let (width, height) = config.texture_dimensions();
        self.texture
            .resize(&self.device, width as u32, height as u32);
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
        self.gui.texture = SizedTexture::new(
            egui_texture,
            Vec2 {
                x: self.texture.size.width as f32,
                y: self.texture.size.height as f32,
            },
        );
    }

    /// Prepare.
    pub fn prepare(&mut self, window: &Window, config: &mut Config) {
        let raw_input = self.egui_state.take_egui_input(window);
        let output = self.ctx.run(raw_input, |ctx| {
            self.gui.ui(ctx, config);
        });
        self.screen_descriptor.pixels_per_point = output.pixels_per_point;

        self.textures.append(output.textures_delta);
        self.egui_state
            .handle_platform_output(window, output.platform_output);
        self.paint_jobs = self.ctx.tessellate(output.shapes, output.pixels_per_point);
    }

    /// Request redraw.
    pub fn request_redraw(&mut self, window: &Window, config: &mut Config) -> NesResult<()> {
        profile!();

        let prev_hide_overscan = config.hide_overscan;
        self.prepare(window, config);
        if prev_hide_overscan != config.hide_overscan {
            self.resize_texture(config);
        }

        let frame = self.surface.get_current_texture().or_else(|_| {
            self.surface.configure(&self.device, &self.surface_config);
            self.surface.get_current_texture()
        })?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        // Copy NES frame buffer
        if let Some(frame_buffer) = self.frame_pool.pop_ref() {
            self.texture.update(
                &self.queue,
                if config.hide_overscan {
                    let len = frame_buffer.len();
                    &frame_buffer[OVERSCAN_TRIM..len - OVERSCAN_TRIM]
                } else {
                    &frame_buffer
                },
            );
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("view"),
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
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

    pub fn on_error(&mut self, err: NesError) {
        self.trigger_event(EmulationEvent::Pause(true));
        error!("{err:?}");
        self.renderer.gui.error = Some(err.to_string());
    }
}
