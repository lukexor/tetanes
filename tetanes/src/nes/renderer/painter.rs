use crate::nes::renderer::shader::{self, Shader};
use anyhow::{Context, anyhow};
use egui::{
    NumExt, ViewportId, ViewportIdMap, ViewportIdSet,
    ahash::HashMap,
    epaint::{self, Primitive, Vertex},
};
use std::{
    borrow::Cow,
    collections::hash_map::Entry,
    iter,
    num::{NonZeroU32, NonZeroU64},
    ops::{Deref, Range},
    sync::Arc,
};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

#[derive(Debug)]
#[must_use]
pub struct Surface {
    inner: wgpu::Surface<'static>,
    shader_resources: Option<shader::Resources>,
    width: u32,
    height: u32,
}

impl Surface {
    pub fn new(
        instance: &wgpu::Instance,
        window: Arc<Window>,
        size: PhysicalSize<u32>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: instance.create_surface(window)?,
            shader_resources: None,
            width: size.width,
            height: size.height,
        })
    }

    fn create_texture_view(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("surface_texture"),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn set_shader(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        uniform_bind_group_layout: &wgpu::BindGroupLayout,
        shader: Shader,
    ) {
        self.shader_resources = shader::Resources::new(
            device,
            format,
            self.create_texture_view(device, format),
            uniform_bind_group_layout,
            shader,
        );
    }
}

impl Deref for Surface {
    type Target = wgpu::Surface<'static>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug)]
#[must_use]
pub struct Painter {
    instance: wgpu::Instance,
    render_state: Option<RenderState>,
    surfaces: ViewportIdMap<Surface>,
}

impl Default for Painter {
    fn default() -> Self {
        let descriptor = if cfg!(all(target_arch = "wasm32", not(feature = "webgpu"))) {
            // TODO: WebGPU is still unsafe/experimental on Linux in Chrome and still nightly on
            // Firefox
            wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all().difference(wgpu::Backends::BROWSER_WEBGPU),
                ..Default::default()
            }
        } else {
            wgpu::InstanceDescriptor::default()
        };
        Self {
            instance: wgpu::Instance::new(&descriptor),
            render_state: None,
            surfaces: Default::default(),
        }
    }
}

impl Painter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_shader(&mut self, shader: Shader) {
        if let Some(render_state) = &mut self.render_state {
            render_state.shader = shader;
            for surface in self.surfaces.values_mut() {
                surface.set_shader(
                    &render_state.device,
                    render_state.format,
                    &render_state.uniform_bind_group_layout,
                    shader,
                );
            }
        }
    }

    pub async fn set_window(
        &mut self,
        viewport_id: ViewportId,
        window: Option<Arc<Window>>,
    ) -> anyhow::Result<()> {
        if let Some(window) = window {
            if let Entry::Vacant(entry) = self.surfaces.entry(viewport_id) {
                let size = window.inner_size();
                let mut surface = Surface::new(&self.instance, window, size)?;

                let render_state = match &mut self.render_state {
                    Some(render_state) => render_state,
                    None => {
                        let render_state = RenderState::create(&self.instance, &surface).await?;
                        self.render_state.get_or_insert(render_state)
                    }
                };

                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    render_state.resize_surface(&mut surface, width, height);
                }

                entry.insert(surface);
            }
        } else {
            self.surfaces.clear();
        }

        Ok(())
    }

    pub fn paint(
        &mut self,
        viewport_id: ViewportId,
        pixels_per_point: f32,
        clipped_primitives: &[epaint::ClippedPrimitive],
        textures_delta: &epaint::textures::TexturesDelta,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let Some(render_state) = &mut self.render_state else {
            return;
        };
        let Some(surface) = self.surfaces.get(&viewport_id) else {
            return;
        };

        let mut encoder =
            render_state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("encoder"),
                });

        // Upload all resources for the GPU.

        let size_in_pixels = [surface.width, surface.height];
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels,
            pixels_per_point,
        };

        for (id, image_delta) in &textures_delta.set {
            render_state.update_texture(*id, image_delta);
        }
        render_state.update_buffers(clipped_primitives, &screen_descriptor);

        let output_frame = match surface.get_current_texture() {
            Ok(frame) => frame,
            Err(err) => {
                if err != wgpu::SurfaceError::Outdated {
                    tracing::error!("failed to acquire next frame: {:?}", err);
                }
                return;
            }
        };

        {
            let view = match &surface.shader_resources {
                Some(shader) => &shader.view,
                None => &output_frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_state.render(&mut render_pass, clipped_primitives, &screen_descriptor);
        }

        if let Some(shader) = &surface.shader_resources {
            let view = &output_frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(0, 0, size_in_pixels[0], size_in_pixels[1]);
            render_pass.set_viewport(
                0.0,
                0.0,
                size_in_pixels[0] as f32,
                size_in_pixels[1] as f32,
                0.0,
                1.0,
            );
            render_pass.set_pipeline(&shader.render_pipeline);
            render_pass.set_bind_group(0, &render_state.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &shader.texture_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        for id in &textures_delta.free {
            render_state.textures.remove(id);
        }

        render_state.queue.submit(iter::once(encoder.finish()));

        output_frame.present();
    }

    pub const fn render_state(&self) -> Option<&RenderState> {
        self.render_state.as_ref()
    }

    pub const fn render_state_mut(&mut self) -> Option<&mut RenderState> {
        self.render_state.as_mut()
    }

    pub fn on_window_resized(&mut self, viewport_id: ViewportId, width: u32, height: u32) {
        if let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height))
            && let Some(surface) = self.surfaces.get_mut(&viewport_id)
                && let Some(render_state) = &mut self.render_state {
                    render_state.resize_surface(surface, width, height);
                }
    }

    pub fn retain_surfaces(&mut self, viewport_ids: &ViewportIdSet) {
        self.surfaces.retain(|id, _| viewport_ids.contains(id));
    }

    pub fn destroy(&mut self) {
        self.surfaces.clear();
        let _ = self.render_state.take();
    }
}

#[derive(Debug)]
#[must_use]
struct SlicedBuffer {
    buffer: wgpu::Buffer,
    slices: Vec<Range<usize>>,
    capacity: wgpu::BufferAddress,
}

#[derive(Debug)]
#[must_use]
pub struct RenderState {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,

    pipeline: wgpu::RenderPipeline,

    index_buffer: SlicedBuffer,
    vertex_buffer: SlicedBuffer,

    uniform_buffer: wgpu::Buffer,
    previous_uniform_buffer_content: UniformBuffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,

    shader: Shader,
    /// Map of egui texture IDs to textures and their associated bindgroups (texture view +
    /// sampler). The texture may be None if the `TextureId` is just a handle to a user-provided
    /// sampler.
    textures: HashMap<epaint::TextureId, (Option<wgpu::Texture>, wgpu::BindGroup)>,
    next_texture_id: u64,
    samplers: HashMap<epaint::textures::TextureOptions, wgpu::Sampler>,
}

impl RenderState {
    async fn create(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> anyhow::Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .context("failed to find suitable wgpu adapter")?;

        tracing::debug!("requested wgpu adapter: {:?}", adapter.get_info());

        let base_limits = if adapter.get_info().backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        };
        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("wgpu device"),
            // TODO: maybe CLEAR_TEXTURE?
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: 8192,
                ..base_limits
            },
            ..Default::default()
        };
        let mut connection = adapter.request_device(&device_descriptor).await;
        // Creating device may fail if adapter doesn't support the default cfg, so try to
        // recover with lower limits. Specifically max_texture_dimension_2d has a downlevel default
        // of 2048. egui_wgpu wants 8192 for 4k displays, but not all platforms support that yet.
        if let Err(err) = connection {
            tracing::error!("failed to create wgpu device: {err:?}, retrying with lower limits");
            connection = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 4096,
                        // Default Edge installed on Windows 10 is limited to 6 attachments,
                        // and we never need more than 1.
                        max_color_attachments: 6,
                        ..base_limits
                    },
                    ..device_descriptor
                })
                .await
        }

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| {
                // egui prefers these formats
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .unwrap_or_else(|| {
                tracing::warn!(format = ?capabilities.formats[0], "failling back to first available format");
                capabilities.formats[0]
            });

        let (device, queue) =
            connection.map_err(|err| anyhow!("failed to create wgpu device: {err:?}"))?;

        let shader_module_desc =
            wgpu::include_wgsl!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/gui.wgsl"));
        let shader_module = device.create_shader_module(shader_module_desc);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gui uniform buffer"),
            contents: bytemuck::cast_slice(&[UniformBuffer::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gui uniform bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(
                            std::mem::size_of::<UniformBuffer>() as _,
                        ),
                    },
                    count: None,
                }],
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gui uniform bind group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gui texture bind group layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gui pipeline layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gui pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    entry_point: Some("vs_main"),
                    module: &shader_module,
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 5 * 4,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        // 0: vec2 position
                        // 1: vec2 uv coordinates
                        // 2: uint color
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32],
                    }],
                    compilation_options: wgpu::PipelineCompilationOptions::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader_module,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Add,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default()
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            }
        );

        const INDEX_BUFFER_START_CAPACITY: wgpu::BufferAddress =
            (std::mem::size_of::<u32>() * 1024 * 3) as _;
        const VERTEX_BUFFER_START_CAPACITY: wgpu::BufferAddress =
            (std::mem::size_of::<Vertex>() * 1024) as _;

        let index_buffer = SlicedBuffer {
            buffer: Self::create_index_buffer(&device, INDEX_BUFFER_START_CAPACITY),
            slices: Vec::with_capacity(64),
            capacity: INDEX_BUFFER_START_CAPACITY,
        };
        let vertex_buffer = SlicedBuffer {
            buffer: Self::create_vertex_buffer(&device, VERTEX_BUFFER_START_CAPACITY),
            slices: Vec::with_capacity(64),
            capacity: VERTEX_BUFFER_START_CAPACITY,
        };

        Ok(Self {
            device,
            queue,
            format,

            pipeline,

            index_buffer,
            vertex_buffer,

            uniform_buffer,
            previous_uniform_buffer_content: Default::default(),
            uniform_bind_group,
            uniform_bind_group_layout,
            texture_bind_group_layout,

            shader: Shader::default(),
            textures: Default::default(),
            next_texture_id: 0,
            samplers: Default::default(),
        })
    }

    pub fn max_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    pub fn register_texture(
        &mut self,
        label: Option<&str>,
        view: &wgpu::TextureView,
        sampler_descriptor: wgpu::SamplerDescriptor<'_>,
    ) -> epaint::TextureId {
        let sampler = self.device.create_sampler(&sampler_descriptor);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let id = epaint::TextureId::User(self.next_texture_id);
        self.textures.insert(id, (None, bind_group));
        self.next_texture_id += 1;

        id
    }

    fn create_vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui vertex buffer"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            size,
            mapped_at_creation: false,
        })
    }

    fn create_index_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui index buffer"),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            size,
            mapped_at_creation: false,
        })
    }

    fn resize_surface(&self, surface: &mut Surface, width: NonZeroU32, height: NonZeroU32) {
        surface.width = width.get();
        surface.height = height.get();
        surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: width.get(),
                height: height.get(),
                // TODO: Support disabling vsync
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![self.format],
            },
        );
        surface.set_shader(
            &self.device,
            self.format,
            &self.uniform_bind_group_layout,
            self.shader,
        );
    }

    pub fn update_texture(&mut self, id: epaint::TextureId, image_delta: &epaint::ImageDelta) {
        let width = image_delta.image.width() as u32;
        let height = image_delta.image.height() as u32;

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let data_color32 = match &image_delta.image {
            epaint::ImageData::Color(image) => {
                assert_eq!(
                    width as usize * height as usize,
                    image.pixels.len(),
                    "Mismatch between texture size and texel count"
                );
                Cow::Borrowed(&image.pixels)
            }
            epaint::ImageData::Font(image) => {
                assert_eq!(
                    width as usize * height as usize,
                    image.pixels.len(),
                    "Mismatch between texture size and texel count"
                );
                Cow::Owned(image.srgba_pixels(None).collect::<Vec<egui::Color32>>())
            }
        };
        let data_bytes: &[u8] = bytemuck::cast_slice(data_color32.as_slice());

        let queue_write_data_to_texture = |texture, origin| {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin,
                    aspect: wgpu::TextureAspect::All,
                },
                data_bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                size,
            );
        };

        if let Some(pos) = image_delta.pos {
            // update the existing texture
            let (texture, _bind_group) = self
                .textures
                .get(&id)
                .expect("Tried to update a texture that has not been allocated yet.");
            let origin = wgpu::Origin3d {
                x: pos[0] as u32,
                y: pos[1] as u32,
                z: 0,
            };
            queue_write_data_to_texture(
                texture.as_ref().expect("Tried to update user texture."),
                origin,
            );
        } else {
            // allocate a new texture
            // Use same label for all resources associated with this texture id (no point in retyping the type)
            let label_str = format!("texture_{id:?}");
            let label = Some(label_str.as_str());
            let texture = {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label,
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb, // Minspec for wgpu WebGL emulation is WebGL2, so this should always be supported.
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
                })
            };
            let sampler = self
                .samplers
                .entry(image_delta.options)
                .or_insert_with(|| Self::create_sampler(image_delta.options, &self.device));

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label,
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
            let origin = wgpu::Origin3d::ZERO;
            queue_write_data_to_texture(&texture, origin);
            self.textures.insert(id, (Some(texture), bind_group));
        };
    }

    pub fn update_buffers(
        &mut self,
        paint_jobs: &[epaint::ClippedPrimitive],
        screen_descriptor: &ScreenDescriptor,
    ) {
        let screen_size_in_points = screen_descriptor.screen_size_in_points();

        let uniform_buffer_content = UniformBuffer {
            screen_size_in_points,
            _padding: Default::default(),
        };
        if uniform_buffer_content != self.previous_uniform_buffer_content {
            self.queue.write_buffer(
                &self.uniform_buffer,
                0,
                bytemuck::cast_slice(&[uniform_buffer_content]),
            );
            self.previous_uniform_buffer_content = uniform_buffer_content;
        }

        // Determine how many vertices & indices need to be rendered, and gather prepare callbacks
        // let mut callbacks = Vec::new();
        let (vertex_count, index_count) =
            paint_jobs.iter().fold((0, 0), |acc, clipped_primitive| {
                if let Primitive::Mesh(mesh) = &clipped_primitive.primitive {
                    (acc.0 + mesh.vertices.len(), acc.1 + mesh.indices.len())
                } else {
                    acc
                }
            });

        if index_count > 0 {
            self.index_buffer.slices.clear();

            let required_index_buffer_size = (std::mem::size_of::<u32>() * index_count) as u64;
            if self.index_buffer.capacity < required_index_buffer_size {
                // Resize index buffer if needed.
                self.index_buffer.capacity =
                    (self.index_buffer.capacity * 2).at_least(required_index_buffer_size);
                self.index_buffer.buffer =
                    Self::create_index_buffer(&self.device, self.index_buffer.capacity);
            }

            let index_buffer_staging = self.queue.write_buffer_with(
                &self.index_buffer.buffer,
                0,
                NonZeroU64::new(required_index_buffer_size).expect("valid index buffer size"),
            );

            let Some(mut index_buffer_staging) = index_buffer_staging else {
                panic!(
                    "Failed to create staging buffer for index data. Index count: {index_count}. Required index buffer size: {required_index_buffer_size}. Actual size {} and capacity: {} (bytes)",
                    self.index_buffer.buffer.size(),
                    self.index_buffer.capacity
                );
            };

            let mut index_offset = 0;
            for epaint::ClippedPrimitive { primitive, .. } in paint_jobs {
                if let Primitive::Mesh(mesh) = primitive {
                    let size = mesh.indices.len() * std::mem::size_of::<u32>();
                    let slice = index_offset..(size + index_offset);
                    index_buffer_staging[slice.clone()]
                        .copy_from_slice(bytemuck::cast_slice(&mesh.indices));
                    self.index_buffer.slices.push(slice);
                    index_offset += size;
                }
            }
        }

        if vertex_count > 0 {
            self.vertex_buffer.slices.clear();

            let required_vertex_buffer_size = (std::mem::size_of::<Vertex>() * vertex_count) as u64;
            if self.vertex_buffer.capacity < required_vertex_buffer_size {
                // Resize vertex buffer if needed.
                self.vertex_buffer.capacity =
                    (self.vertex_buffer.capacity * 2).at_least(required_vertex_buffer_size);
                self.vertex_buffer.buffer =
                    Self::create_vertex_buffer(&self.device, self.vertex_buffer.capacity);
            }

            let vertex_buffer_staging = self.queue.write_buffer_with(
                &self.vertex_buffer.buffer,
                0,
                NonZeroU64::new(required_vertex_buffer_size).expect("valid vertex buffer size"),
            );

            let Some(mut vertex_buffer_staging) = vertex_buffer_staging else {
                panic!(
                    "Failed to create staging buffer for vertex data. Vertex count: {vertex_count}. Required vertex buffer size: {required_vertex_buffer_size}. Actual size {} and capacity: {} (bytes)",
                    self.vertex_buffer.buffer.size(),
                    self.vertex_buffer.capacity
                );
            };

            let mut vertex_offset = 0;
            for epaint::ClippedPrimitive { primitive, .. } in paint_jobs {
                if let Primitive::Mesh(mesh) = primitive {
                    let size = mesh.vertices.len() * std::mem::size_of::<Vertex>();
                    let slice = vertex_offset..(size + vertex_offset);
                    vertex_buffer_staging[slice.clone()]
                        .copy_from_slice(bytemuck::cast_slice(&mesh.vertices));
                    self.vertex_buffer.slices.push(slice);
                    vertex_offset += size;
                }
            }
        }
    }

    pub fn render<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
        paint_jobs: &'rp [epaint::ClippedPrimitive],
        screen_descriptor: &ScreenDescriptor,
    ) {
        let pixels_per_point = screen_descriptor.pixels_per_point;
        let size_in_pixels = screen_descriptor.size_in_pixels;

        render_pass.set_scissor_rect(0, 0, size_in_pixels[0], size_in_pixels[1]);
        render_pass.set_viewport(
            0.0,
            0.0,
            size_in_pixels[0] as f32,
            size_in_pixels[1] as f32,
            0.0,
            1.0,
        );
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

        let mut index_buffer_slices = self.index_buffer.slices.iter();
        let mut vertex_buffer_slices = self.vertex_buffer.slices.iter();

        for epaint::ClippedPrimitive {
            clip_rect,
            primitive,
        } in paint_jobs
        {
            let rect = ScissorRect::new(clip_rect, pixels_per_point, size_in_pixels);

            if rect.width == 0 || rect.height == 0 {
                // Skip rendering zero-sized clip areas.
                if let Primitive::Mesh(_) = primitive {
                    // If this is a mesh, we need to advance the index and vertex buffer iterators:
                    index_buffer_slices.next();
                    vertex_buffer_slices.next();
                }
                continue;
            }

            render_pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);

            if let Primitive::Mesh(mesh) = primitive {
                // These expects should be valid because update_buffers inserts a slice for every
                // primitive
                let index_buffer_slice = index_buffer_slices
                    .next()
                    .expect("valid index buffer slice");
                let vertex_buffer_slice = vertex_buffer_slices
                    .next()
                    .expect("valid vertex buffer slice");

                if let Some((_texture, bind_group)) = self.textures.get(&mesh.texture_id) {
                    render_pass.set_bind_group(1, bind_group, &[]);
                    render_pass.set_index_buffer(
                        self.index_buffer
                            .buffer
                            .slice(index_buffer_slice.start as u64..index_buffer_slice.end as u64),
                        wgpu::IndexFormat::Uint32,
                    );
                    render_pass.set_vertex_buffer(
                        0,
                        self.vertex_buffer.buffer.slice(
                            vertex_buffer_slice.start as u64..vertex_buffer_slice.end as u64,
                        ),
                    );
                    render_pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
                } else {
                    tracing::warn!("Missing texture: {:?}", mesh.texture_id);
                }
            }
        }

        render_pass.set_scissor_rect(0, 0, size_in_pixels[0], size_in_pixels[1]);
    }

    fn create_sampler(
        options: epaint::textures::TextureOptions,
        device: &wgpu::Device,
    ) -> wgpu::Sampler {
        let mag_filter = match options.magnification {
            epaint::textures::TextureFilter::Nearest => wgpu::FilterMode::Nearest,
            epaint::textures::TextureFilter::Linear => wgpu::FilterMode::Linear,
        };
        let min_filter = match options.minification {
            epaint::textures::TextureFilter::Nearest => wgpu::FilterMode::Nearest,
            epaint::textures::TextureFilter::Linear => wgpu::FilterMode::Linear,
        };
        let address_mode = match options.wrap_mode {
            epaint::textures::TextureWrapMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            epaint::textures::TextureWrapMode::Repeat => wgpu::AddressMode::Repeat,
            epaint::textures::TextureWrapMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        };
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!(
                "gui sampler (mag: {mag_filter:?}, min {min_filter:?})"
            )),
            mag_filter,
            min_filter,
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            ..Default::default()
        })
    }
}

/// Uniform buffer used when rendering.
#[derive(Default, Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct UniformBuffer {
    screen_size_in_points: [f32; 2],
    // Uniform buffers need to be at least 16 bytes in WebGL.
    // See https://github.com/gfx-rs/wgpu/issues/2072
    _padding: [u32; 2],
}

impl PartialEq for UniformBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.screen_size_in_points == other.screen_size_in_points
    }
}

/// Information about the screen used for rendering.
pub struct ScreenDescriptor {
    /// Size of the window in physical pixels.
    pub size_in_pixels: [u32; 2],

    /// HiDPI scale factor (pixels per point).
    pub pixels_per_point: f32,
}

impl ScreenDescriptor {
    /// size in "logical" points
    fn screen_size_in_points(&self) -> [f32; 2] {
        [
            self.size_in_pixels[0] as f32 / self.pixels_per_point,
            self.size_in_pixels[1] as f32 / self.pixels_per_point,
        ]
    }
}

/// A Rect in physical pixel space, used for setting clipping rectangles.
struct ScissorRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl ScissorRect {
    fn new(clip_rect: &epaint::Rect, pixels_per_point: f32, target_size: [u32; 2]) -> Self {
        // Transform clip rect to physical pixels:
        let clip_min_x = pixels_per_point * clip_rect.min.x;
        let clip_min_y = pixels_per_point * clip_rect.min.y;
        let clip_max_x = pixels_per_point * clip_rect.max.x;
        let clip_max_y = pixels_per_point * clip_rect.max.y;

        // Round to integer:
        let clip_min_x = clip_min_x.round() as u32;
        let clip_min_y = clip_min_y.round() as u32;
        let clip_max_x = clip_max_x.round() as u32;
        let clip_max_y = clip_max_y.round() as u32;

        // Clamp:
        let clip_min_x = clip_min_x.clamp(0, target_size[0]);
        let clip_min_y = clip_min_y.clamp(0, target_size[1]);
        let clip_max_x = clip_max_x.clamp(clip_min_x, target_size[0]);
        let clip_max_y = clip_max_y.clamp(clip_min_y, target_size[1]);

        Self {
            x: clip_min_x,
            y: clip_min_y,
            width: clip_max_x - clip_min_x,
            height: clip_max_y - clip_min_y,
        }
    }
}
