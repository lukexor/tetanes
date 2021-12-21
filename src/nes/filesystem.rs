use super::{Nes, NesResult};
use anyhow::Context;
use std::{fs::File, io::BufReader, path::Path};

impl Nes {
    /// Searches for valid NES rom files ending in `.nes`
    pub(crate) fn find_roms(&mut self) -> NesResult<()> {
        self.roms.clear();
        if self.config.rom_path.is_file() {
            match self.config.rom_path.parent() {
                Some(parent) => self.config.rom_path = parent.to_path_buf(),
                None => return Ok(()),
            }
        }
        match self.config.rom_path.read_dir() {
            Ok(read_dir) => {
                read_dir
                    .filter_map(Result::ok)
                    .filter(|f| f.path().extension().unwrap_or_default() == "nes")
                    .for_each(|f| self.roms.push(f.path()));
                Ok(())
            }
            Err(_) => Ok(()),
        }
    }

    /// Loads a ROM cartridge into memory
    pub(crate) fn load_rom<P>(&mut self, path: P) -> NesResult<()>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let rom = File::open(&path).with_context(|| format!("failed to open rom {:?}", path))?;
        let mut rom = BufReader::new(rom);
        self.control_deck
            .load_rom(&path.to_string_lossy(), &mut rom)?;
        Ok(())
    }
}
