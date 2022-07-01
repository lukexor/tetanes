use crate::{
    common::{Kind, Reset},
    ppu::Ppu,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Frame {
    count: u32,
    front_buffer: Vec<u16>,
    back_buffer: Vec<u16>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    pub fn new() -> Self {
        Self {
            count: 0,
            front_buffer: vec![0x00; Ppu::SIZE],
            back_buffer: vec![0x00; Ppu::SIZE],
        }
    }

    #[inline]
    pub fn increment(&mut self) {
        self.count += 1;
        std::mem::swap(&mut self.front_buffer, &mut self.back_buffer);
    }

    #[inline]
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> u16 {
        self.back_buffer[(x + (y << 8)) as usize]
    }

    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, color: u16) {
        self.back_buffer[(x + (y << 8)) as usize] = color;
    }

    #[must_use]
    pub fn pixel_brightness(&self, x: u32, y: u32) -> u32 {
        let pixel = self.pixel(x, y);
        let (red, green, blue) = Ppu::system_palette(pixel);
        u32::from(red) + u32::from(green) + u32::from(blue)
    }

    #[inline]
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.count
    }

    #[inline]
    #[must_use]
    pub fn buffer(&self) -> &[u16] {
        &self.front_buffer
    }
}

impl Reset for Frame {
    fn reset(&mut self, _kind: Kind) {
        self.count = 0;
        self.front_buffer.fill(0);
        self.back_buffer.fill(0);
    }
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame").field("count", &self.count).finish()
    }
}
