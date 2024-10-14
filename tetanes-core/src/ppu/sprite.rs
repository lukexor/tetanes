//! PPU OAM Sprite implementation.
//!
//! See: <https://www.nesdev.org/wiki/PPU_OAM>

use serde::{Deserialize, Serialize};
use std::fmt;

/// PPU OAM Sprite entry.
///
/// See: <https://www.nesdev.org/wiki/PPU_OAM>
#[derive(Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Sprite {
    pub x: u32,
    pub y: u32,
    pub tile_addr: u16,
    pub tile_lo: u8,
    pub tile_hi: u8,
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
            tile_addr: 0x0000,
            tile_lo: 0x00,
            tile_hi: 0x00,
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
            .field("tile_addr", &format_args!("${:04X}", &self.tile_addr))
            .field("tile_lo", &format_args!("${:02X}", &self.tile_lo))
            .field("tile_hi", &format_args!("${:02X}", &self.tile_hi))
            .field("palette", &format_args!("${:02X}", &self.palette))
            .field("bg_priority", &self.bg_priority)
            .field("flip_horizontal", &self.flip_horizontal)
            .field("flip_vertical", &self.flip_vertical)
            .finish()
    }
}
