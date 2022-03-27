use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sprite {
    pub index: u8,
    pub x: u16,
    pub y: u16,
    pub tile_index: u16,
    pub tile_addr: u16,
    pub palette: u8,
    pub pattern: u32,
    pub has_priority: bool,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

impl Sprite {
    pub const fn new() -> Self {
        Self {
            index: 0u8,
            x: 0xFF,
            y: 0xFF,
            tile_index: 0xFF,
            tile_addr: 0xFF,
            palette: 0x07,
            pattern: 0u32,
            has_priority: true,
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
            .field("index", &self.index)
            .field("x", &self.x)
            .field("y", &self.y)
            .field("tile_index", &format_args!("${:04X}", &self.tile_index))
            .field("tile_addr", &format_args!("${:04X}", &self.tile_addr))
            .field("palette", &format_args!("${:02X}", &self.palette))
            .field("pattern", &format_args!("${:08X}", &self.pattern))
            .field("has_priority", &self.has_priority)
            .field("flip_horizontal", &self.flip_horizontal)
            .field("flip_vertical", &self.flip_vertical)
            .finish()
    }
}
