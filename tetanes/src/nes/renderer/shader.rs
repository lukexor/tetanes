use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `VideoFilter`")]
pub struct ParseShaderError;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Shader {
    Default,
    #[default]
    CrtEasymode,
}

impl Shader {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::Default, Self::CrtEasymode]
    }
}

impl AsRef<str> for Shader {
    fn as_ref(&self) -> &str {
        match self {
            Self::Default => "Default",
            Self::CrtEasymode => "CRT Easymode",
        }
    }
}

impl TryFrom<usize> for Shader {
    type Error = ParseShaderError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Default,
            1 => Self::CrtEasymode,
            _ => return Err(ParseShaderError),
        })
    }
}

#[derive(Debug)]
#[must_use]
pub struct Resources {
    pub view: wgpu::TextureView,
    pub texture_bind_group: wgpu::BindGroup,
    pub render_pipeline: wgpu::RenderPipeline,
}

impl Resources {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view: wgpu::TextureView,
        uniform_bind_group_layout: &wgpu::BindGroupLayout,
        shader: Shader,
    ) -> Option<Self> {
        let shader_module_desc = match shader {
            Shader::Default => return None,
            Shader::CrtEasymode => wgpu::include_wgsl!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/crt-easymode.wgsl"
            )),
        };
        let shader_module = device.create_shader_module(shader_module_desc);

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nes frame bind group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shader pipeline layout"),
            bind_group_layouts: &[uniform_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Some(Self {
            view,
            texture_bind_group,
            render_pipeline,
        })
    }
}
