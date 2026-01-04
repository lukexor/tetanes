//! PPU frame implementation.

use crate::{
    common::{Reset, ResetKind},
    ppu::Ppu,
};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// PPU frame.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
#[must_use]
pub struct Buffer(Vec<u16>);

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Buffer({} elements)", self.0.len())
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self(vec![0x00; Ppu::SIZE])
    }
}

impl Deref for Buffer {
    type Target = [u16];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// PPU frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Frame {
    pub count: u32,
    pub is_odd: bool,
    #[serde(skip)]
    pub buffer: Buffer,
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
            is_odd: false,
            buffer: Buffer::default(),
        }
    }

    #[inline(always)]
    pub const fn increment(&mut self) {
        self.count = self.count.wrapping_add(1);
        self.is_odd = self.count & 0x01 == 0x01;
    }

    #[inline(always)]
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> u16 {
        self.buffer[(x + (y << 8)) as usize]
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: u32, y: u32, color: u16) {
        self.buffer[(x + (y << 8)) as usize] = color;
    }

    #[must_use]
    pub fn pixel_brightness(&self, x: u32, y: u32) -> u32 {
        let pixel = self.pixel(x, y);
        let index = (pixel as usize) * 3;
        let red = Ppu::NTSC_PALETTE[index];
        let green = Ppu::NTSC_PALETTE[index + 1];
        let blue = Ppu::NTSC_PALETTE[index + 2];
        u32::from(red) + u32::from(green) + u32::from(blue)
    }

    #[inline(always)]
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.count
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_odd(&self) -> bool {
        self.is_odd
    }

    #[inline(always)]
    #[must_use]
    pub fn buffer(&self) -> &[u16] {
        &self.buffer
    }
}

impl Reset for Frame {
    fn reset(&mut self, _kind: ResetKind) {
        self.count = 0;
        self.buffer = Buffer::default();
    }
}
