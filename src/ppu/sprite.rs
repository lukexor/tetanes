use crate::{serialization::Savable, NesResult};
use std::io::{Read, Write};

#[derive(Default, Debug, Copy, Clone)]
#[must_use]
pub(super) struct Sprite {
    pub(super) index: u8,
    pub(super) x: u16,
    pub(super) y: u16,
    pub(super) tile_index: u16,
    pub(super) tile_addr: u16,
    pub(super) palette: u8,
    pub(super) pattern: u32,
    pub(super) has_priority: bool,
    pub(super) flip_horizontal: bool,
    pub(super) flip_vertical: bool,
}

impl Sprite {
    pub(super) const fn new() -> Self {
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

impl Savable for Sprite {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.index.save(fh)?;
        self.x.save(fh)?;
        self.y.save(fh)?;
        self.tile_index.save(fh)?;
        self.tile_addr.save(fh)?;
        self.palette.save(fh)?;
        self.pattern.save(fh)?;
        self.has_priority.save(fh)?;
        self.flip_horizontal.save(fh)?;
        self.flip_vertical.save(fh)?;
        Ok(())
    }

    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.index.load(fh)?;
        self.x.load(fh)?;
        self.y.load(fh)?;
        self.tile_index.load(fh)?;
        self.tile_addr.load(fh)?;
        self.palette.load(fh)?;
        self.pattern.load(fh)?;
        self.has_priority.load(fh)?;
        self.flip_horizontal.load(fh)?;
        self.flip_vertical.load(fh)?;
        Ok(())
    }
}
