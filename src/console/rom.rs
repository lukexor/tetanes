use byteorder::{LittleEndian, ReadBytesExt};
use std::{error::Error, fmt, fs::File, io::Read, path::PathBuf};

const INES_FILE_MAGIC: [u8; 4] = *b"NES\x1a";
const PRG_ROM_SIZE: usize = 16384;
const TRAINER_SIZE: usize = 512;
const SRAM_SIZE: usize = 8192;

/// Mirror options
pub enum Mirror {
    OneScreenLower,
    OneScreenUpper,
    Vertical,
    Horizontal,
}

/// An iNES File Header
///
/// 16 bytes comprise the iNES file header specification
///   0-3: $4E $45 $53 $1A ("NES" followed by MS-DOS end-of-file)
///   4: Size of PRG ROM in 16 KB units
///   5: Size of CHR ROM in 8 KB units (Value 0 means the board uses CHR RAM)
///   6: Flags 6 - Mapper, mirroring, battery, trainer (if present)
///   7: Flags 7 - Mapper, VS/Playchoice, NES 2.0
///   8: Flags 8 - PRG-RAM size (rarely used extension)
///   9: Flags 9 - TV system (rarely used extension)
///   10: Flags 10 - TV system, PRG-RAM presence (unofficial, rarely used extension)
///   11-15: Unused padding (should be filled with zero, but some rippers put their
///        name across bytes 7-15)
///
struct INesHeader {
    magic: [u8; 4],   // ines magic number: 'N' 'E' 'S' '\x1a'
    prg_rom_size: u8, // number of PRG-ROM banks (16KB each)
    chr_rom_size: u8, // number of CHR-ROM banks (8KB each)
    flags_6: u8,      // control bits - flags 6
    flags_7: u8,      // control bits - flags 7
    prg_ram_size: u8, // PRG-RAM size (x 8KB)
    flags_9: u8,      // TV system (rarely used extension)
    flags_10: u8,     // TV system, PRG-RAM presence (unofficial, rarely used extension)
    zero: [u8; 5],    // Unused padding
}

/// Represents an iNES Rom file '.nes'
///
/// http://wiki.nesdev.com/w/index.php/INES
/// http://nesdev.com/NESDoc.pdf (page 28)
pub struct Rom {
    header: INesHeader, // TODO: Add NES 2.0 support
    pub prg: Vec<u8>,   // PRG-ROM banks - Program ROM
    pub chr: Vec<u8>,   // CHR-ROM banks - Pattern Tables / Character ROM
}

impl Rom {
    /// Attempts to return a valid Rom instance based on the passed in rom string
    ///
    /// # Arguments
    ///
    /// * `rom` - A string slice that holds the path to a valid '.nes' file
    ///
    /// # Errors
    ///
    /// If the rom file is not a valid '.nes' file, or there are insufficient permissions to read
    /// the file, then an error is returned.
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        let mut rom_file = File::open(PathBuf::from(rom))?;
        let mut header = [0u8; 16];
        rom_file.read_exact(&mut header)?;

        let header = INesHeader {
            magic: [header[0], header[1], header[2], header[3]],
            prg_rom_size: header[4],
            chr_rom_size: header[5],
            flags_6: header[6],
            flags_7: header[7],
            prg_ram_size: header[8],
            flags_9: header[9],
            flags_10: header[10],
            zero: [0; 5],
        };

        if header.magic != *b"NES\x1a" {
            return Err("invalid .nes file".into());
        }

        let mut prg = vec![0u8; (header.prg_rom_size as usize) * PRG_ROM_SIZE];
        rom_file.read_exact(&mut prg)?;
        let mut chr = vec![0u8; (header.chr_rom_size as usize) * SRAM_SIZE];
        rom_file.read_exact(&mut chr)?;

        Ok(Self { header, prg, chr })
    }

    pub fn mapper(&self) -> u8 {
        self.header.mapper()
    }

    pub fn trainer(&self) -> bool {
        self.header.trainer()
    }
}

impl INesHeader {
    pub fn mapper(&self) -> u8 {
        (self.flags_7 & 0xF0) | (self.flags_6 >> 4)
    }

    pub fn trainer(&self) -> bool {
        (self.flags_6 & 0x04) != 0
    }
}

impl fmt::Display for INesHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "PRG: {} KB, CHR: {} KB, Mapper: {}",
            self.prg_rom_size as usize * PRG_ROM_SIZE,
            self.chr_rom_size as usize * SRAM_SIZE,
            self.mapper(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROM1: &str = "roms/Zelda II - The Adventure of Link (USA).nes";
    const ROM2: &str = "roms/Super Mario Bros. (World).nes";
    const ROM3: &str = "roms/Gauntlet (USA).nes";

    #[test]
    fn test_load_rom() {
        let rom_path = PathBuf::from(ROM1);
        let rom = Rom::new(&rom_path).expect("valid rom");
        assert_eq!(rom.prg.len(), 131_072);
        assert_eq!(rom.chr.len(), 131_072);
        assert_eq!(rom.mapper(), 1);
        assert_eq!(rom.trainer(), false);

        let rom_path = PathBuf::from(ROM2);
        let rom = Rom::new(&rom_path).expect("valid rom");
        assert_eq!(rom.prg.len(), 32_768);
        assert_eq!(rom.chr.len(), 8_192);
        assert_eq!(rom.mapper(), 0);
        assert_eq!(rom.trainer(), false);

        let rom_path = PathBuf::from(ROM3);
        let rom = Rom::new(&rom_path).expect("valid rom");
        assert_eq!(rom.prg.len(), 131_072);
        assert_eq!(rom.chr.len(), 65_536);
        assert_eq!(rom.mapper(), 4);
        assert_eq!(rom.trainer(), false);
    }
}
