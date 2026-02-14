use crate::nes::renderer::painter::RenderState;
use egui::{TextureId, Vec2, load::SizedTexture};

#[derive(Debug)]
#[must_use]
pub struct Texture {
    pub label: Option<&'static str>,
    pub id: TextureId,
    pub texture: wgpu::Texture,
    pub size: Vec2,
    pub output_size: Vec2,
    pub view: wgpu::TextureView,
    pub aspect_ratio: f32,
}

impl Texture {
    pub fn new(
        render_state: &mut RenderState,
        size: Vec2,
        aspect_ratio: f32,
        label: Option<&'static str>,
    ) -> Self {
        let max_texture_side = render_state.max_texture_side() as f32;
        let texture = render_state
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label,
                size: wgpu::Extent3d {
                    width: size.x.min(max_texture_side) as u32,
                    height: size.y.min(max_texture_side) as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label,
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });
        let sampler_descriptor = wgpu::SamplerDescriptor {
            label: Some("sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        };
        let id = render_state.register_texture(label, &view, sampler_descriptor);

        Self {
            label,
            texture,
            size,
            output_size: Vec2 {
                x: size.x * aspect_ratio,
                y: size.y,
            },
            view,
            aspect_ratio,
            id,
        }
    }

    pub fn resize(&mut self, render_state: &mut RenderState, size: Vec2, aspect_ratio: f32) {
        *self = Self::new(render_state, size, aspect_ratio, self.label);
    }

    pub fn sized(&self) -> SizedTexture {
        SizedTexture::new(self.id, self.output_size)
    }

    pub fn update(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        self.update_partial(queue, bytes, Vec2::ZERO, self.size);
    }

    pub fn update_partial(&self, queue: &wgpu::Queue, bytes: &[u8], origin: Vec2, size: Vec2) {
        let size = wgpu::Extent3d {
            width: size.x as u32,
            height: size.y as u32,
            depth_or_array_layers: 1,
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                aspect: wgpu::TextureAspect::All,
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin.x as u32,
                    y: origin.y as u32,
                    z: 0,
                },
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );
    }
}
