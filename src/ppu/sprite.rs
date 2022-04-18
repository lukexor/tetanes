use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sprite {
    pub x: u32,
    pub y: u32,
    pub tile_lo: u8,
    pub tile_hi: u8,
    pub attr: u8,
    pub palette: u8,
    pub bg_priority: bool,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

impl Sprite {
    pub const fn new() -> Self {
        Self {
            x: 0xFF,
            y: 0xFF,
            tile_lo: 0x00,
            tile_hi: 0x00,
            attr: 0xFF,
            palette: 0x07,
            bg_priority: true,
            flip_horizontal: true,
            flip_vertical: true,
        }
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Sprite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sprite")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("palette", &format_args!("${:02X}", &self.palette))
            .field("has_priority", &self.bg_priority)
            .field("flip_horizontal", &self.flip_horizontal)
            .field("flip_vertical", &self.flip_vertical)
            .finish()
    }
}
