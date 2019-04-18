use super::*;
use byteorder::{LittleEndian, ReadBytesExt};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

/// Mirror options
pub enum Mirror {
    Horizontal,
    Vertical,
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
    pub _pad: u64,    // unused padding (necessary for properly reading ROM file)
}

/// Represents an iNES file '.nes'
///
/// http://wiki.nesdev.com/w/index.php/INES
/// http://nesdev.com/NESDoc.pdf (page 28)
pub struct Cartridge {
    pub prg: Vec<u8>,          // PRG-ROM banks - Program ROM
    pub chr: Vec<u8>,          // CHR-ROM banks - Pattern Tables / Character ROM
    pub sram: [u8; SRAM_SIZE], // Save RAM
    pub mapper: u8,            // mapper type
    pub mirror: u8,            // mirroring mode
    pub battery: u8,           // battery present
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
    pub fn new(rom: &str) -> Result<Self, Box<Error>> {
        let mut rom_file = File::open(PathBuf::from(rom))?;
        let header = Cartridge::load_file_header(&mut rom_file)?;

        // mapper lower four bits of mapper number
        // mapper2 upper four bits of mapper number
        let mapper1 = header.control1 >> 4;
        let mapper2 = header.control2 >> 4;
        let mapper = mapper1 | mapper2 << 4;

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
            let mut buffer: Vec<u8> = vec![0; 512];
            rom_file.read_exact(&mut buffer)?;
        }

        // read PRG-ROM data
        let mut prg: Vec<u8> = vec![0; (header.num_prg as usize) * PRG_ROM_SIZE];
        rom_file.read_exact(&mut prg)?;

        // read CHR-ROM data
        let mut chr: Vec<u8> = vec![0; (header.num_chr as usize) * SRAM_SIZE];
        rom_file.read_exact(&mut chr)?;

        // if 0, the board uses CHR RAM
        if header.num_chr == 0 {
            chr = vec![0; SRAM_SIZE];
        }

        let sram = [0; SRAM_SIZE];

        Ok(Cartridge {
            prg,
            chr,
            sram,
            mapper,
            mirror,
            battery,
        })
    }

    pub fn read(&self, address: u16) -> u8 {
        let index = ((address - 0x8000) % self.prg.len() as u16) as usize;
        self.prg[index]
    }

    /// Attempts to return a valid Mapper type based on the Cartridge data
    ///
    /// Tries to match to one of the mapper numbers located in the '.nes' file header.
    ///
    /// 0 or 2 : Mapper2
    /// 1      : Mapper1
    /// 3      : Mapper3
    /// 4      : Mapper4
    /// 7      : Mapper7
    ///
    /// # Errors
    ///
    /// If none of the above numbers match, an error is returned.
    pub fn get_mapper(&self) -> Result<Box<Mapper>, Box<Error>> {
        match self.mapper {
            0 | 2 => {
                let prg_banks = (self.prg.len() / 0x4000) as isize;
                Ok(Box::new(Mapper2 {
                    prg_banks,
                    prg_bank1: 0,
                    prg_bank2: (prg_banks - 1) as isize,
                }) as Box<Mapper>)
            }
            1 => {
                let mut mapper = Mapper1 {
                    shift_register: 0x10,
                    ..Default::default()
                };
                mapper.prg_offsets[1] = memory::prg_bank_offset(self, -1, 0x4000);
                Ok(Box::new(mapper) as Box<Mapper>)
            }
            3 => {
                let prg_banks = self.prg.len() / 0x4000;
                Ok(Box::new(Mapper3 {
                    chr_bank: 0,
                    prg_bank1: 0,
                    prg_bank2: (prg_banks - 1) as isize,
                }) as Box<Mapper>)
            }
            4 => {
                let mut mapper = Mapper4 {
                    ..Default::default()
                };
                mapper.prg_offsets[0] = memory::prg_bank_offset(self, 0, 0x2000);
                mapper.prg_offsets[1] = memory::prg_bank_offset(self, 1, 0x2000);
                mapper.prg_offsets[2] = memory::prg_bank_offset(self, -2, 0x2000);
                mapper.prg_offsets[3] = memory::prg_bank_offset(self, -1, 0x2000);
                Ok(Box::new(mapper) as Box<Mapper>)
            }
            7 => Ok(Box::new(Mapper7 { prg_bank: 0 }) as Box<Mapper>),
            _ => Err(format!("unsupported mapper: {}", self.mapper).into()),
        }
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
        let header = InesFileHeader {
            magic: file.read_u32::<LittleEndian>()?,            // 0-3
            num_prg: file.read_uint::<LittleEndian>(1)? as u8,  // 4
            num_chr: file.read_uint::<LittleEndian>(1)? as u8,  // 5
            control1: file.read_uint::<LittleEndian>(1)? as u8, // 6
            control2: file.read_uint::<LittleEndian>(1)? as u8, // 7
            num_ram: file.read_uint::<LittleEndian>(1)? as u8,  // 8
            _pad: file.read_uint::<LittleEndian>(7)?,           // 9-15
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

    #[test]
    fn test_load_file_header() {
        let rom_path = PathBuf::from("roms/Zelda II - The Adventure of Link (USA).nes");
        let mut rom_file = File::open(rom_path).expect("valid file");
        let file_header = Cartridge::load_file_header(&mut rom_file).expect("valid header");
        assert_eq!(file_header.magic, INES_FILE_MAGIC);

        let rom_path = PathBuf::from("roms/Super Mario Bros. (World).nes");
        let mut rom_file = File::open(rom_path).expect("valid file");
        let file_header = Cartridge::load_file_header(&mut rom_file).expect("valid header");
        assert_eq!(file_header.magic, INES_FILE_MAGIC);
    }

    #[test]
    fn test_load_cartridge() {
        let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
        let cartridge = Cartridge::new(rom).expect("valid cartridge");
        assert_eq!(cartridge.prg.len(), 131_072);
        assert_eq!(cartridge.chr.len(), 131_072);
        assert_eq!(cartridge.sram.len(), 8192);
        assert_eq!(cartridge.mapper, 1);
        assert_eq!(cartridge.mirror, 0);
        assert_eq!(cartridge.battery, 1);

        let rom = "roms/Super Mario Bros. (World).nes";
        let cartridge = Cartridge::new(rom).expect("valid cartridge");
        assert_eq!(cartridge.prg.len(), 32_768);
        assert_eq!(cartridge.chr.len(), 8_192);
        assert_eq!(cartridge.sram.len(), 8192);
        assert_eq!(cartridge.mapper, 0);
        assert_eq!(cartridge.mirror, 1);
        assert_eq!(cartridge.battery, 0);

        let rom = "roms/Gauntlet (USA).nes";
        let cartridge = Cartridge::new(rom).expect("valid cartridge");
        assert_eq!(cartridge.prg.len(), 131_072);
        assert_eq!(cartridge.chr.len(), 65_536);
        assert_eq!(cartridge.sram.len(), 8192);
        assert_eq!(cartridge.mapper, 4);
        assert_eq!(cartridge.mirror, 2);
        assert_eq!(cartridge.battery, 0);
    }
}
