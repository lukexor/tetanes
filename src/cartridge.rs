use std::{error::Error, fmt, fs::File, io::Read, path::PathBuf};

const NES_HEADER_MAGIC: [u8; 4] = *b"NES\x1a";
const PRG_ROM_BANK_SIZE: usize = 0x4000; // 16K bytes
const CHR_ROM_BANK_SIZE: usize = 0x2000; // 8K bytes

/// Represents an NES Cartridge
///
/// http://wiki.nesdev.com/w/index.php/INES
/// http://wiki.nesdev.com/w/index.php/NES_2.0
/// http://nesdev.com/NESDoc.pdf (page 28)
#[derive(Clone)]
pub struct Cartridge {
    title: String,
    mapper_num: u8,
    mirroring: u8, // 0 = vertical, 1 = horizontal, 3 = single-screen, 4 = four-screen
    battery: bool,
    pub prg_rom: Vec<u8>, // PRG-ROM - Program ROM
    pub chr_rom: Vec<u8>, // CHR-ROM - Pattern Tables / Character ROM
}

impl Cartridge {
    /// Attempts to return a valid Cartridge based on the passed in file path
    ///
    /// # Arguments
    ///
    /// * `file` - A string slice that holds the path to a valid '.nes' file
    ///
    /// # Errors
    ///
    /// If the file is not a valid '.nes' file, or there are insufficient permissions to read the
    /// file, then an error is returned.
    pub fn new(file: &PathBuf) -> Result<Self, Box<Error>> {
        let title = Self::extract_title(&file);
        let mut rom_file = File::open(PathBuf::from(file))?;
        let mut header = [0u8; 16];
        rom_file.read_exact(&mut header)?;

        let magic = [header[0], header[1], header[2], header[3]];
        if magic != NES_HEADER_MAGIC {
            return Err("invalid .nes file".into());
        }

        let prg_rom_size = header[4] as usize * PRG_ROM_BANK_SIZE;
        let mut prg_rom = vec![0u8; prg_rom_size];
        rom_file.read_exact(&mut prg_rom)?;

        let chr_rom_size = header[5] as usize * CHR_ROM_BANK_SIZE;
        let mut chr_rom = vec![0u8; chr_rom_size];
        rom_file.read_exact(&mut chr_rom)?;

        Ok(Self {
            title,
            // Upper 4 bits of byte 7 and upper 4 bits of byte 8
            mapper_num: (header[7] & 0xF0) | (header[6] >> 4),
            // 0 = vertical, 1 = horizontal, 4 = four-screen override
            mirroring: (header[6] & 1) | ((header[6] >> 3) & 1) << 2,
            battery: (header[6] >> 1) & 1 > 0,
            prg_rom,
            chr_rom,
        })
    }

    // Getters

    pub fn title(&self) -> &String {
        &self.title
    }

    pub fn prg_size(&self) -> usize {
        self.prg_rom.len()
    }

    pub fn chr_size(&self) -> usize {
        self.chr_rom.len()
    }

    pub fn mapper_num(&self) -> u8 {
        self.mapper_num
    }

    pub fn mirroring(&self) -> u8 {
        self.mirroring
    }

    pub fn battery(&self) -> bool {
        self.battery
    }

    // Utility

    fn extract_title(file: &PathBuf) -> String {
        let file_name = file.file_stem().unwrap_or(std::ffi::OsStr::new("N/A"));
        let title_str = file_name.to_str().unwrap_or("N/A");
        title_str.to_string()
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Cartridge {{ PRG-ROM: {}KB, CHR-ROM: {}KB, Mapper: {}, Mirroring: {}, Battery: {}",
            self.prg_rom.len() / 0x0400,
            self.chr_rom.len() / 0x0400,
            self.mapper_num,
            self.mirroring,
            self.battery,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROMS: &[&str] = &[
        "roms/Zelda II - The Adventure of Link (USA).nes",
        "roms/Super Mario Bros. (World).nes",
        "roms/Metroid (USA).nes",
        "roms/Gauntlet (USA).nes",
    ];

    #[test]
    fn test_cartridges() {
        let rom_data = &[
            // (PRG, CHR, Mapper, Mirroring, Battery)
            (
                "Zelda II - The Adventure of Link (USA)",
                128,
                128,
                1,
                0,
                true,
            ),
            ("Super Mario Bros. (World)", 32, 8, 0, 1, false),
            ("Metroid (USA)", 128, 0, 1, 0, false),
            ("Gauntlet (USA)", 128, 64, 4, 4, false),
        ];
        for i in 0..rom_data.len() {
            let c = Cartridge::new(&PathBuf::from(ROMS[i]));
            assert!(c.is_ok());
            let c = c.unwrap();
            assert_eq!(c.title(), rom_data[i].0);
            assert_eq!(c.prg_size() / 0x0400, rom_data[i].1);
            assert_eq!(c.chr_size() / 0x0400, rom_data[i].2);
            assert_eq!(c.mapper_num(), rom_data[i].3);
            assert_eq!(c.mirroring(), rom_data[i].4);
            assert_eq!(c.battery(), rom_data[i].5);
        }
    }
}
