use egui::{load::SizedTexture, TextureId, Vec2};

pub struct Texture {
    pub label: Option<&'static str>,
    pub texture: wgpu::Texture,
    pub size: wgpu::Extent3d,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub aspect_ratio: f32,
}

impl Texture {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        aspect_ratio: f32,
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

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label,
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
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

        Self {
            label,
            texture,
            size,
            view,
            sampler,
            aspect_ratio,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32, aspect_ratio: f32) {
        self.size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        self.aspect_ratio = aspect_ratio;
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

    pub fn sized_texture(&self, texture_id: TextureId) -> SizedTexture {
        SizedTexture::new(
            texture_id,
            Vec2 {
                x: self.size.width as f32 * self.aspect_ratio,
                y: self.size.height as f32,
            },
        )
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
