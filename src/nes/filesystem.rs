use super::{Nes, NesResult};
use crate::{map_nes_err, nes_err};
use std::{fs::File, io::BufReader};

impl Nes {
    /// Searches for valid NES rom files ending in `.nes`
    ///
    /// If rom_path is a `.nes` file, uses that
    /// If no arg[1], searches current directory for `.nes` files
    pub(crate) fn find_roms(&mut self) -> NesResult<()> {
        use std::ffi::OsStr;
        let path = self.config.rom_path.to_owned();
        self.roms.clear();
        if path.is_dir() {
            path.read_dir()
                .map_err(|e| map_nes_err!("unable to read directory {:?}: {}", path, e))?
                .filter_map(|f| f.ok())
                .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
                .for_each(|f| self.roms.push(f.path()));
        } else if path.is_file() {
            self.roms.push(path.clone());
        } else {
            nes_err!("invalid path: {:?}", path)?;
        }
        if self.roms.is_empty() {
            nes_err!("no rom files found or specified in {:?}", path)
        } else {
            Ok(())
        }
    }

    /// Loads a ROM cartridge into memory
    pub(crate) fn load_rom(&mut self, rom_id: usize) -> NesResult<()> {
        if rom_id >= self.roms.len() {
            nes_err!("invalid rom_id")?;
        }
        let rom_path = &self.roms[rom_id];
        let rom = File::open(&self.roms[rom_id])
            .map_err(|e| map_nes_err!("unable to open file {:?}: {}", rom_path, e))?;
        let mut rom = BufReader::new(rom);
        self.control_deck
            .load_rom(&rom_path.to_string_lossy(), &mut rom)?;
        Ok(())
    }
}
