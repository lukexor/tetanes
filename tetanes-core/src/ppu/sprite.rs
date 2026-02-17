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
#[repr(C)]
pub struct Sprite {
    pub flip_horizontal: bool,
    pub bg_priority: bool,
    pub x: u8,
    pub tile_lo: u8,
    pub tile_hi: u8,
    pub palette_offset: u8,
}

impl Sprite {
    pub const fn new() -> Self {
        Self {
            flip_horizontal: true,
            bg_priority: true,
            x: 0xFF,
            tile_lo: 0x00,
            tile_hi: 0x00,
            palette_offset: 0x07,
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
            .field("flip_horizontal", &self.flip_horizontal)
            .field("bg_priority", &self.bg_priority)
            .field("x", &self.x)
            .field("tile_lo", &format_args!("${:02X}", &self.tile_lo))
            .field("tile_hi", &format_args!("${:02X}", &self.tile_hi))
            .field("palette", &format_args!("${:02X}", &self.palette_offset))
            .finish()
    }
}
