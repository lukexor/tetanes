use std::ops::{Deref, DerefMut};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ColorType {
    Rgb,
    Rgba,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Pixel(pub [u8; 4]);

impl Deref for Pixel {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Pixel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Pixel Constants

// White/Black/Blank
pub static WHITE: Pixel = Pixel([255, 255, 255, 255]);
pub static BLACK: Pixel = Pixel([0, 0, 0, 255]);
pub static TRANSPARENT: Pixel = Pixel([0, 0, 0, 0]);

// Gray
pub static GRAY: Pixel = Pixel([192, 192, 192, 255]);
pub static DARK_GRAY: Pixel = Pixel([128, 128, 128, 255]);
pub static VERY_DARK_GRAY: Pixel = Pixel([64, 64, 64, 255]);

// Red
pub static RED: Pixel = Pixel([255, 0, 0, 255]);
pub static DARK_RED: Pixel = Pixel([128, 0, 0, 255]);
pub static VERY_DARK_RED: Pixel = Pixel([64, 0, 0, 255]);

// Orange
pub static ORANGE: Pixel = Pixel([255, 128, 0, 255]);

// Yellow
pub static YELLOW: Pixel = Pixel([255, 255, 0, 255]);
pub static DARK_YELLOW: Pixel = Pixel([128, 128, 0, 255]);
pub static VERY_DARK_YELLOW: Pixel = Pixel([64, 64, 0, 255]);

// Green
pub static GREEN: Pixel = Pixel([0, 255, 0, 255]);
pub static DARK_GREEN: Pixel = Pixel([0, 128, 0, 255]);
pub static VERY_DARK_GREEN: Pixel = Pixel([0, 64, 0, 255]);

// Cyan
pub static CYAN: Pixel = Pixel([0, 255, 255, 255]);
pub static DARK_CYAN: Pixel = Pixel([0, 128, 128, 255]);
pub static VERY_DARK_CYAN: Pixel = Pixel([0, 64, 64, 255]);

// Blue
pub static BLUE: Pixel = Pixel([0, 0, 255, 255]);
pub static DARK_BLUE: Pixel = Pixel([0, 0, 128, 255]);
pub static VERY_DARK_BLUE: Pixel = Pixel([0, 0, 64, 255]);

// Magenta
pub static MAGENTA: Pixel = Pixel([255, 0, 255, 255]);
pub static DARK_MAGENTA: Pixel = Pixel([128, 0, 128, 255]);
pub static VERY_DARK_MAGENTA: Pixel = Pixel([64, 0, 64, 255]);
