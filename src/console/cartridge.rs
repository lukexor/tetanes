//! An NES Cartridge Board

use crate::console::{mapper, Memory};
use crate::Result;
use failure::Fail;
use std::cell::RefCell;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub const PRG_BANK_SIZE: usize = 0x4000; // 16K bytes
const CHR_BANK_SIZE: usize = 0x2000; // 8K bytes
const DEFAULT_PRG_RAM_SIZE: usize = 0x2000; // 8K bytes
const NES_HEADER_MAGIC: [u8; 4] = *b"NES\x1a";

/// Represents an NES Cartridge
///
/// http://wiki.nesdev.com/w/index.php/INES
/// http://wiki.nesdev.com/w/index.php/NES_2.0
/// http://nesdev.com/NESDoc.pdf (page 28)
pub struct Cartridge {
    pub header: INesHeader,
    pub mirroring: Mirroring,
    pub prg_rom: Vec<u8>,
    pub prg_ram: Vec<u8>,
    pub chr_rom: Vec<u8>,
}

#[derive(Debug)]
pub struct INesHeader {
    mapper_num: u16,
    flags: u8, // Mirroring, Battery, Trainer, VS Unisystem, Playchoice-10, NES 2.0
    pub prg_size: u16,
    pub chr_size: u16,
    version: u8,   // iNES or NES 2.0
    submapper: u8, // NES 2.0 only
    prg_ram: u8,   // NES 2.0 PRG-RAM
    chr_ram: u8,   // NES 2.0 CHR-RAM
    tv_mode: u8,   // NES 2.0 NTSC/PAL indicator
    vs_data: u8,   // NES 2.0 VS System data
}

// http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenA,
    SingleScreenB,
    FourScreen, // Only ~3 games use 4-screen - maybe implement some day
}

pub type BoardRef = Rc<RefCell<Board>>;

pub trait Board: Memory + Send {
    fn scanline_irq(&self) -> bool;
    fn mirroring(&self) -> Mirroring;
    fn step(&mut self);
}

#[derive(Debug, Eq, PartialEq)]
pub enum BoardType {
    AOROM, // mapper 7, ~9 games - Battle Toads, Double Dragon
    CNROM, // mapper 3, ~58 games - Paperboy
    NROM, // mapper 0, ~51 games - Bomberman, Donkey Kong, Donkey Kong 3, Galaga, Pac Man, Super Mario
    // Brothers
    SxROM, // mapper 1, ~200 games - A Boy and His Blob, Addams Family, Castlevania 2, Final
    // Fantasy, Maniac Mansion, Metroid, Zelda
    TxROM, // mapper 4, ~175 games - Kickle Cubicle, Krusty's Fun House, Super Mario Brothers 2/3
    UNROM, // mapper 2, ~82 games - Castlevania, Contra, Mega Man
}

use BoardType::*;
use Mirroring::*;

#[derive(Debug, Fail)]
pub enum CartErr {
    #[fail(display = "{}: {}", _0, _1)]
    Io(String, #[cause] io::Error),
    #[fail(display = "{}", _0)]
    InvalidHeader(String),
    #[fail(display = "Unsupported ROM: {}", _0)]
    Unsupported(String),
}

impl Cartridge {
    pub fn new() -> Self {
        Self {
            header: INesHeader::new(),
            mirroring: Mirroring::Horizontal,
            prg_rom: Vec::new(),
            prg_ram: Vec::new(),
            chr_rom: Vec::new(),
        }
    }

    /// Creates a new Cartridge instance by reading in a `.nes` file
    ///
    /// # Arguments
    ///
    /// * `file` - A string that holds the path to a valid '.nes' file
    ///
    /// # Errors
    ///
    /// If the file is not a valid '.nes' file, or there are insufficient permissions to read the
    /// file, then an error is returned.
    pub fn from_rom<P: AsRef<Path> + fmt::Debug>(rom: P) -> Result<Self> {
        let mut rom = std::fs::File::open(&rom)
            .map_err(|e| CartErr::Io(format!("unable to open file {:?}", rom), e))?;

        let mut header = [0u8; 16];
        rom.read_exact(&mut header)?;
        let header = INesHeader::from_bytes(&header)?;

        let mut prg_rom = vec![0u8; header.prg_size as usize * PRG_BANK_SIZE];
        rom.read_exact(&mut prg_rom)?;

        let mut chr_rom = vec![0u8; header.chr_size as usize * CHR_BANK_SIZE];
        rom.read_exact(&mut chr_rom)?;

        let mirroring = match header.flags & 0x01 {
            0 => Horizontal,
            1 => Vertical,
            _ => Err(CartErr::Unsupported(format!(
                "Unsupported mirroring: {}",
                header.flags & 0x03
            )))?,
        };

        // PRG-RAM
        let prg_ram = vec![0; 8 * 1024];

        let cartridge = Self {
            header,
            mirroring,
            prg_rom,
            prg_ram,
            chr_rom,
        };
        eprintln!("{:?}", cartridge);
        Ok(cartridge)
    }

    /// Attempts to return a valid Cartridge Board mapper for the given cartridge.
    /// Consumes the Cartridge instance in the process.
    pub fn load_board(self) -> Result<BoardRef> {
        match self.header.mapper_num {
            0 => Ok(Rc::new(RefCell::new(mapper::Nrom::load(self)))),
            1 => Ok(Rc::new(RefCell::new(mapper::Sxrom::load(self)))),
            3 => Ok(Rc::new(RefCell::new(mapper::Cnrom::load(self)))),
            _ => Err(CartErr::Unsupported(format!(
                "unsupported mapper number: {}",
                self.header.mapper_num
            )))?,
        }
    }
}

impl INesHeader {
    fn new() -> Self {
        Self {
            mapper_num: 0u16,
            flags: 0u8,
            prg_size: 0u16,
            chr_size: 0u16,
            version: 1u8,
            submapper: 0,
            prg_ram: 0,
            chr_ram: 0,
            tv_mode: 0,
            vs_data: 0,
        }
    }

    fn from_bytes(header: &[u8; 16]) -> Result<Self> {
        // Header checks
        if &header[0..4] != NES_HEADER_MAGIC {
            Err(CartErr::InvalidHeader(
                "iNES header signature not found.".to_string(),
            ))?;
        } else if (header[7] & 0x0C) == 0x04 {
            Err(CartErr::InvalidHeader(
                "Header is corrupted by \"DiskDude!\" - repair and try again.".to_string(),
            ))?;
        } else if (header[7] & 0x0C) == 0x0C {
            Err(CartErr::InvalidHeader(
                "Unrecognied header format - repair and try again.".to_string(),
            ))?;
        }

        let mut prg_size = u16::from(header[4]);
        let mut chr_size = u16::from(header[5]);
        // Upper 4 bits of flags 6 and 7
        let mut mapper_num = u16::from(((header[6] & 0xF0) >> 4) | (header[7] & 0xF0));
        // Lower 4 bits of flag 6, upper 4 bits of flag 7
        let flags = (header[6] & 0x0F) | ((header[7] & 0x0F) << 4);

        // NES 2.0 Format
        // If bits 2-3 of flag 7 are equal to 2
        let mut version = 1; // Start off checking for iNES format v1
        let mut submapper = 0;
        let mut prg_ram = 0;
        let mut chr_ram = 0;
        let mut tv_mode = 0;
        let mut vs_data = 0;
        if header[7] & 0x0C == 0x08 {
            version = 2;
            // lower 4 bits of flag 8 = bits 8-11 of mapper num
            // mapper_num |= u16::from((header[8] & 0x0F) << 8);
            // upper 4 bits of flag 8
            // submapper = (header[8] & 0xF0) >> 4;
            // lower 4 bits of flag 9 = bits 8-11 of prg_size
            // prg_size |= u16::from((header[9] & 0x0F) << 8);
            // upper 4 bits of flag 9 = bits 8-11 of chr_size
            // chr_size |= u16::from((header[9] & 0xF0) << 4);
            prg_ram = header[10];
            chr_ram = header[11];
            tv_mode = header[12];
            vs_data = header[13];

            if prg_ram & 0x0F == 0x0F || prg_ram & 0xF0 == 0xF0 {
                Err(CartErr::InvalidHeader(
                    "Invalid PRG RAM size in header.".to_string(),
                ))?;
            } else if chr_ram & 0x0F == 0x0F || chr_ram & 0xF0 == 0xF0 {
                Err(CartErr::InvalidHeader(
                    "Invalid CHR RAM size in header.".to_string(),
                ))?;
            } else if chr_ram & 0xF0 == 0xF0 {
                Err(CartErr::InvalidHeader(
                    "Battery-backed CHR RAM is not supported.".to_string(),
                ))?;
            } else if header[14] > 0 || header[15] > 0 {
                Err(CartErr::InvalidHeader(
                    "Unregonized data found at header offsets 14-15.".to_string(),
                ))?;
            }
        } else {
            for i in 8..16 {
                if header[i] > 0 {
                    Err(CartErr::InvalidHeader(format!(
                        "Unregonized data found at header offset {} - repair and try again.",
                        i
                    )))?;
                }
            }
        }

        // Trainer
        if flags & 0x04 == 0x04 {
            Err(CartErr::Unsupported(
                "Trained ROMs are not supported.".to_string(),
            ))?;
        }
        Ok(Self {
            mapper_num,
            flags,
            prg_size,
            chr_size,
            version,
            submapper,
            prg_ram,
            chr_ram,
            tv_mode,
            vs_data,
        })
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Cartridge {{ header: {:?}, Mirroring: {:?}, PRG-ROM: {}, CHR-ROM: {}, PRG-RAM: {}",
            self.header,
            self.mirroring,
            self.prg_rom.len(),
            self.chr_rom.len(),
            self.prg_ram.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const ROMS: &[&str] = &[
        "roms/Zelda II - The Adventure of Link (USA).nes",
        "roms/Super Mario Bros. (World).nes",
        "roms/Metroid (USA).nes",
        "roms/Gauntlet (USA).nes",
    ];

    #[test]
    fn test_valid_cartridges() {
        let rom_data = &[
            // (File, PRG, CHR, Mapper, Mirroring, Battery)
            (
                "roms/Zelda II - The Adventure of Link (USA).nes",
                "Zelda II - The Adventure of Link (USA)",
                8,
                16,
                SxROM,
                Horizontal,
                true,
            ),
            (
                "roms/Super Mario Bros. (World).nes",
                "Super Mario Bros. (World)",
                2,
                1,
                NROM,
                Vertical,
                false,
            ),
            (
                "roms/Metroid (USA).nes",
                "Metroid (USA)",
                8,
                0,
                SxROM,
                Horizontal,
                false,
            ),
        ];
        for rom in rom_data {
            let c = Cartridge::new(&PathBuf::from(rom.0));
            assert!(c.is_ok(), "new cartridge {}", rom.0);
            let c = c.unwrap();
            assert_eq!(c.title, rom.1, "title matches {}", rom.0);
            assert_eq!(
                c.num_prg_banks, rom.2,
                "PRG-ROM size matches for {}",
                c.title
            );
            assert_eq!(
                c.num_chr_banks, rom.3,
                "CHR-ROM size matches for {}",
                c.title
            );
            assert_eq!(c.mirroring, rom.5, "mirroring matches for {}", c.title);
            assert_eq!(c.battery, rom.6, "battery matches for {}", c.title);
        }
    }

    #[test]
    fn test_invalid_cartridges() {
        use std::fs;
        use std::fs::OpenOptions;

        // TODO Make these tests not rely on actual cartridges
        let invalid_rom_tests = &[
            (
                "invalid_file.nes",
                "unable to open file \"invalid_file.nes\": No such file or directory (os error 2)",
            ),
            (
                "roms/Family Trainer 9 - Fuuun Takeshi-jou 2 (Japan).nes",
                "unsupported mapper: 66",
            ),
            ("roms/Gauntlet (USA).nes", "unsupported mirroring: 2"),
        ];
        for test in invalid_rom_tests {
            let c = Cartridge::new(&PathBuf::from(test.0));
            assert!(c.is_err(), "invalid cartridge {}", test.0);
            assert_eq!(
                c.err().expect("valid cartridge error").to_string(),
                test.1,
                "error matches {}",
                test.0
            );
        }
    }
}
