//! An NES Cartridge Board

use crate::console::mapper;
use crate::console::memory::{Memory, Rom};
use crate::Result;
use failure::{format_err, Fail};
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const NES_HEADER_MAGIC: [u8; 4] = *b"NES\x1a";
pub const PRG_BANK_SIZE: usize = 0x4000; // 16K bytes
const CHR_BANK_SIZE: usize = 0x2000; // 8K bytes

/// Represents an NES Cartridge
///
/// http://wiki.nesdev.com/w/index.php/INES
/// http://wiki.nesdev.com/w/index.php/NES_2.0
/// http://nesdev.com/NESDoc.pdf (page 28)
pub struct Cartridge {
    pub title: String,
    pub board_type: BoardType,
    pub mirroring: Mirroring,
    pub battery: bool,
    pub num_prg_banks: usize,
    pub num_chr_banks: usize,
    pub prg_rom: Rom,
    pub chr_rom: Rom,
}

pub trait Board: Memory + Send {
    fn scanline_irq(&self) -> bool;
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

// http://wiki.nesdev.com/w/index.php/Mirroring#Nametable_Mirroring
#[derive(Debug, Eq, PartialEq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenA,
    SingleScreenB,
    // FourScreen, // Only ~3 games use 4-screen - maybe implement some day
}

use BoardType::*;
use Mirroring::*;

#[derive(Debug, Fail)]
pub enum CartridgeError {
    #[fail(display = "{}: {}", _0, _1)]
    Io(String, #[cause] io::Error),
    #[fail(display = "invalid `.nes` format: {:?}", _0)]
    InvalidFormat(PathBuf),
    #[fail(display = "invalid mapper: {}", _0)]
    InvalidMapper(u8),
    #[fail(display = "invalid mirroring: {}", _0)]
    InvalidMirroring(u8),
}

impl Cartridge {
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
    pub fn new<P: AsRef<Path> + fmt::Debug>(file: P) -> Result<Self> {
        let mut rom_file = std::fs::File::open(&file).map_err(|e| {
            CartridgeError::Io(
                format!("unable to open file {:?}", file.as_ref().to_path_buf()),
                e,
            )
        })?;

        let title = Self::extract_title(&file);
        let mut header = [0u8; 16];
        rom_file.read_exact(&mut header)?;

        let magic = [header[0], header[1], header[2], header[3]];
        if magic != NES_HEADER_MAGIC {
            Err(CartridgeError::InvalidFormat(file.as_ref().to_path_buf()))?;
        }

        let num_prg_banks = header[4] as usize;
        let mut prg_rom = vec![0u8; num_prg_banks * PRG_BANK_SIZE];
        rom_file.read_exact(&mut prg_rom)?;

        let num_chr_banks = header[5] as usize;
        let mut chr_rom = vec![0u8; num_chr_banks * CHR_BANK_SIZE];
        rom_file.read_exact(&mut chr_rom)?;

        // Upper 4 bits of byte 7 and upper 4 bits of byte 8
        let mapper = (header[7] & 0xF0) | (header[6] >> 4);
        let board_type = Cartridge::lookup_board_type(mapper)?;
        // First bit of byte 6 or 3rd bit overrides
        let mirroring = if (header[6] >> 3) & 1 == 1 {
            2
        } else {
            header[6] & 1
        };
        let mirroring = match mirroring {
            0 => Horizontal,
            1 => Vertical,
            _ => Err(CartridgeError::InvalidMirroring(mirroring))?,
        };

        Ok(Self {
            title,
            board_type,
            mirroring,
            battery: (header[6] >> 1) & 1 > 0,
            num_prg_banks,
            num_chr_banks,
            prg_rom: Rom::with_bytes(prg_rom),
            chr_rom: Rom::with_bytes(chr_rom),
        })
    }

    /// Attempts to return a valid Cartridge Board mapper for the given cartridge.
    /// Consumes the Cartridge instance in the process.
    pub fn load_board(self) -> Result<Arc<Mutex<Board>>> {
        match self.board_type {
            NROM => Ok(Arc::new(Mutex::new(mapper::Nrom::load(self)))),
            SxROM => Ok(Arc::new(Mutex::new(mapper::Sxrom::load(self)))),
            _ => Err(format_err!("unsupported mapper: {:?}", self.board_type))?,
        }
    }

    // Utility functions

    fn extract_title<P: AsRef<Path>>(file: P) -> String {
        let file_name = file
            .as_ref()
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("N/A"));
        let title_str = file_name.to_str().unwrap_or("N/A");
        title_str.to_string()
    }

    fn lookup_board_type(mapper: u8) -> Result<BoardType> {
        match mapper {
            0 => Ok(NROM),
            1 => Ok(SxROM),
            2 => Ok(UNROM),
            3 => Ok(CNROM),
            4 => Ok(TxROM),
            7 => Ok(AOROM),
            _ => Err(CartridgeError::InvalidMapper(mapper))?,
        }
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Cartridge {{ title: {}, board_type: {:?}, Mirroring: {:?}, Battery: {}",
            self.title, self.board_type, self.mirroring, self.battery,
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
                128,
                128,
                SxROM,
                Horizontal,
                true,
            ),
            (
                "roms/Super Mario Bros. (World).nes",
                "Super Mario Bros. (World)",
                32,
                8,
                NROM,
                Vertical,
                false,
            ),
            (
                "roms/Metroid (USA).nes",
                "Metroid (USA)",
                128,
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
                c.prg_rom.len() / 0x0400,
                rom.2,
                "PRG-ROM size matches for {}",
                c.title
            );
            assert_eq!(
                c.chr_rom.len() / 0x0400,
                rom.3,
                "CHR-ROM size matches for {}",
                c.title
            );
            assert_eq!(c.board_type, rom.4, "board_type matches for {}", c.title);
            assert_eq!(c.mirroring, rom.5, "mirroring matches for {}", c.title);
            assert_eq!(c.battery, rom.6, "battery matches for {}", c.title);
        }
    }

    #[test]
    fn test_invalid_cartridges() {
        let invalid_rom_tests = &[
            (
                "invalid_file.nes",
                "unable to open file \"invalid_file.nes\": No such file or directory (os error 2)",
            ),
            (
                "/tmp/unreadable.nes",
                "unable to open file \"/tmp/unreadable.nes\": Permission denied (os error 13)",
            ),
            (
                "roms/Family Trainer 9 - Fuuun Takeshi-jou 2 (Japan).nes",
                "invalid mapper: 66",
            ),
            ("roms/Gauntlet (USA).nes", "invalid mirroring: 2"),
        ];
        for test in invalid_rom_tests {
            let c = Cartridge::new(&PathBuf::from(test.0));
            assert!(c.is_err(), "invalid cartridge {}", test.0);
            assert_eq!(
                c.err().unwrap().to_string(),
                test.1,
                "error matches {}",
                test.0
            );
        }
    }
}
