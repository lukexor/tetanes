use egui::Rect;
use egui_wgpu::RenderState;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use thiserror::Error;

#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `VideoFilter`")]
pub struct ParseShaderError;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Shader {
    None,
    #[default]
    CrtEasymode,
}

impl Shader {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::None, Self::CrtEasymode]
    }
}

impl AsRef<str> for Shader {
    fn as_ref(&self) -> &str {
        match self {
            Self::None => "None",
            Self::CrtEasymode => "CRT Easymode",
        }
    }
}

impl TryFrom<usize> for Shader {
    type Error = ParseShaderError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::None,
            1 => Self::CrtEasymode,
            _ => return Err(ParseShaderError),
        })
    }
}

#[derive(Debug)]
#[must_use]
pub struct Renderer {
    rect: Rect,
}

impl Renderer {
    pub const fn new(rect: Rect) -> Self {
        Self { rect }
    }
}

impl egui_wgpu::CallbackTrait for Renderer {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(shader_res) = resources.get::<Resources>() {
            queue.write_buffer(
                &shader_res.size_uniform,
                0,
                bytemuck::cast_slice(&[self.rect.width(), self.rect.height(), 0.0, 0.0]),
            );
        }
        Vec::new()
    }

    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        resources: &'a egui_wgpu::CallbackResources,
    ) {
        if let Some(shader_res) = resources.get::<Resources>() {
            render_pass.set_pipeline(&shader_res.render_pipeline);
            render_pass.set_bind_group(0, &shader_res.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct Resources {
    bind_group: wgpu::BindGroup,
    size_uniform: wgpu::Buffer,
    render_pipeline: wgpu::RenderPipeline,
}

impl Resources {
    pub fn new(render_state: &RenderState, view: &wgpu::TextureView, shader: Shader) -> Self {
        let size_uniform_size = 16;
        let size_uniform = render_state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame Size Buffer"),
            size: size_uniform_size, // 16-byte minimum alignment, even though we only need 8 bytes
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            render_state
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(size_uniform_size),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
        let sampler = render_state
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
        let bind_group = render_state
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nes frame bind group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: size_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
        let pipeline_layout =
            render_state
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("pipeline layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let shader_module_desc = match shader {
            Shader::None => panic!("No shader selected"),
            Shader::CrtEasymode => wgpu::include_wgsl!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/crt-easymode.wgsl"
            )),
        };
        let shader = render_state.device.create_shader_module(shader_module_desc);

        let render_pipeline =
            render_state
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("render pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: "vs_main",
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: "fs_main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: render_state.target_format,
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

        Self {
            bind_group,
            size_uniform,
            render_pipeline,
        }
    }
}
