use byteorder::{LittleEndian, ReadBytesExt};
use std::{error::Error, fmt, fs::File, io::Read, path::PathBuf};

const INES_FILE_MAGIC: u32 = 0x1a53_454e;
const PRG_ROM_SIZE: usize = 16384;
const TRAINER_SIZE: usize = 512;
const SRAM_SIZE: usize = 8192;

/// Mirror options
pub enum Mirror {
    Horizontal,
    Vertical,
    Single0,
    Single1,
    Quad,
}

/// An iNES File Header
#[derive(Default, Debug)]
pub struct InesFileHeader {
    pub magic: u32,   // ines magic number
    pub num_prg: u8,  // number of PRG-ROM banks (16KB each)
    pub num_chr: u8,  // number of CHR-ROM banks (8KB each)
    pub control1: u8, // control bits
    pub control2: u8, // control bits
    pub num_ram: u8,  // PRG-RAM size (x 8KB)
}

/// Represents an iNES file '.nes'
///
/// http://wiki.nesdev.com/w/index.php/INES
/// http://nesdev.com/NESDoc.pdf (page 28)
pub struct Cartridge {
    pub prg: Vec<u8>,  // PRG-ROM banks - Program ROM
    pub chr: Vec<u8>,  // CHR-ROM banks - Pattern Tables / Character ROM
    pub sram: Vec<u8>, // Save RAM
    pub mapper: u8,    // mapper type
    pub mirror: u8,    // mirroring mode
    pub battery: u8,   // battery present
}

impl Cartridge {
    /// Attempts to return a valid Cartridge instance based on the passed in rom string
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
        let header = Cartridge::load_file_header(&mut rom_file)?;
        let mapper_lo = header.control1 >> 4;
        let mapper_hi = header.control2 >> 4;
        let mapper = mapper_lo | mapper_hi << 4;

        // nametable mirroring: bits 0 and 3
        // mirror1: 0: horizontal (vertical arrangement) (CIRAM A10 = PPU A11)
        //          1: vertical (horizontal arrangement) (CIRAM A10 = PPU A10)
        // mirror2: 1: Ignore mirroring control or above mirroring bit; instead provide
        //             four-screen VRAM
        let mirror1 = header.control1 & 1;
        let mirror2 = (header.control1 >> 3) & 1;
        let mirror = mirror1 | mirror2 << 1;

        // bit 1 - 1: Cartridge contains battery-backed PRG RAM ($6000-7FFF) or other persistent memory
        let battery = (header.control1 >> 1) & 1;

        // bit 2 - 1: 512-byte trainer at $7000-$71FF (stored before PRG data)
        if header.control1 & 4 == 4 {
            // TODO: implement trainer use?
            let mut buffer: Vec<u8> = vec![0; TRAINER_SIZE];
            rom_file.read_exact(&mut buffer)?;
        }

        // read PRG-ROM data
        let mut prg: Vec<u8> = vec![0; (header.num_prg as usize) * PRG_ROM_SIZE];
        rom_file.read_exact(&mut prg)?;

        // read CHR-ROM data
        let mut chr: Vec<u8> = vec![0; (header.num_chr as usize) * SRAM_SIZE];
        rom_file.read_exact(&mut chr)?;

        // if num_chr == 0, the board uses CHR RAM
        if header.num_chr == 0 {
            chr = vec![0; SRAM_SIZE];
        }

        let sram = vec![0; SRAM_SIZE];

        Ok(Self {
            prg,
            chr,
            sram,
            mapper,
            mirror,
            battery,
        })
    }

    pub fn read(&self, address: u16) -> u8 {
        let index = ((address - 0x8000) as usize % self.prg.len()) as usize;
        self.prg[index]
    }

    // Returns a valid iNES file header
    //
    // # Arguments
    //
    // * `file` - A mutable std::fs::File reference for the '.nes' file
    //
    // 16 bytes comprise the iNES file header specification
    //   0-3: $4E $45 $53 $1A ("NES" followed by MS-DOS end-of-file)
    //   4: Size of PRG ROM in 16 KB units
    //   5: Size of CHR ROM in 8 KB units (Value 0 means the board uses CHR RAM)
    //   6: Flags 6 - Mapper, mirroring, battery, trainer (if present)
    //   7: Flags 7 - Mapper, VS/Playchoice, NES 2.0
    //   8: Flags 8 - PRG-RAM size (rarely used extension)
    //   9: Flags 9 - TV system (rarely used extension)
    //   10: Flags 10 - TV system, PRG-RAM presence (unofficial, rarely used extension)
    //   11-15: Unused padding (should be filled with zero, but some rippers put their
    //        name across bytes 7-15)
    //
    // # Errors
    //
    // If the file does not have a valid "NES" header, the file is smaller than 16 bytes, or there
    // is some filesystem read issue, then an error is returned.
    // TODO: Add support for NES 2.0
    fn load_file_header(file: &mut File) -> Result<InesFileHeader, Box<Error>> {
        let magic = file.read_u32::<LittleEndian>()?; // 0-3
        let mut bytes = vec![0; 12]; // 4-15
        file.read_exact(&mut bytes)?;
        let header = InesFileHeader {
            magic,
            num_prg: bytes[0],
            num_chr: bytes[1],
            control1: bytes[2],
            control2: bytes[3],
            num_ram: bytes[4],
            // Remaining bytes are padding
        };
        // Check bytes 0-3 match "NES"
        match header.magic {
            INES_FILE_MAGIC => Ok(header),
            _ => Err("invalid .nes file".into()),
        }
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Cartridge {{ prg_size: {}, chr_size: {}, sram_size: {}, mapper: {}, mirror: {}, battery: {}",
            self.prg.len(), self.chr.len(), self.sram.len(), self.mapper, self.mirror, self.battery
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
    fn test_load_file_header() {
        let rom_path = PathBuf::from(ROM1);
        let mut rom_file = File::open(rom_path).expect("valid file");
        let file_header = Cartridge::load_file_header(&mut rom_file).expect("valid header");
        assert_eq!(file_header.magic, INES_FILE_MAGIC);

        let mut rom_file = File::open(ROM2).expect("valid file");
        let file_header = Cartridge::load_file_header(&mut rom_file).expect("valid header");
        assert_eq!(file_header.magic, INES_FILE_MAGIC);
    }

    #[test]
    fn test_load_cartridge() {
        let rom_path = PathBuf::from(ROM1);
        let cartridge = Cartridge::new(&rom_path).expect("valid cartridge");
        assert_eq!(cartridge.prg.len(), 131_072);
        assert_eq!(cartridge.chr.len(), 131_072);
        assert_eq!(cartridge.sram.len(), 8192);
        assert_eq!(cartridge.mapper, 1);
        assert_eq!(cartridge.mirror, 0);
        assert_eq!(cartridge.battery, 1);

        let rom_path = PathBuf::from(ROM2);
        let cartridge = Cartridge::new(&rom_path).expect("valid cartridge");
        assert_eq!(cartridge.prg.len(), 32_768);
        assert_eq!(cartridge.chr.len(), 8_192);
        assert_eq!(cartridge.sram.len(), 8192);
        assert_eq!(cartridge.mapper, 0);
        assert_eq!(cartridge.mirror, 1);
        assert_eq!(cartridge.battery, 0);

        let rom_path = PathBuf::from(ROM3);
        let cartridge = Cartridge::new(&rom_path).expect("valid cartridge");
        assert_eq!(cartridge.prg.len(), 131_072);
        assert_eq!(cartridge.chr.len(), 65_536);
        assert_eq!(cartridge.sram.len(), 8192);
        assert_eq!(cartridge.mapper, 4);
        assert_eq!(cartridge.mirror, 2);
        assert_eq!(cartridge.battery, 0);
    }
}
