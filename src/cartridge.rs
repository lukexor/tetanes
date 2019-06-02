//! Handles reading NES Cartridge headers and ROMs

use crate::mapper::Mirroring;
use crate::memory::{Rom, CHR_ROM_BANK_SIZE, PRG_ROM_BANK_SIZE};
use crate::serialization::Savable;
use crate::util::Result;
use failure::format_err;
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Represents an NES Cartridge
#[derive(Default)]
pub struct Cartridge {
    pub rom_file: PathBuf, // '.nes' rom file
    pub header: INesHeader,
    pub prg_rom: Rom, // Program ROM
    pub chr_rom: Rom, // Character ROM
}

impl Cartridge {
    /// Creates an empty cartridge not loaded with any ROM
    pub fn new() -> Self {
        Self {
            rom_file: PathBuf::new(),
            header: INesHeader::new(),
            prg_rom: Rom::init(PRG_ROM_BANK_SIZE),
            chr_rom: Rom::init(CHR_ROM_BANK_SIZE),
        }
    }

    /// Creates a new Cartridge instance by reading in a `.nes` file
    ///
    /// # Arguments
    ///
    /// * `rom` - An object that implements AsRef<Path> that holds the path to a valid '.nes' file
    ///
    /// # Errors
    ///
    /// If the file is not a valid '.nes' file, or there are insufficient permissions to read the
    /// file, then an error is returned.
    pub fn from_rom<P: AsRef<Path>>(rom_file: P) -> Result<Self> {
        let mut rom_data = std::fs::File::open(&rom_file).map_err(|e| {
            format_err!(
                "unable to open file \"{}\": {}",
                rom_file.as_ref().display(),
                e
            )
        })?;

        let mut header = [0u8; 16];
        rom_data.read_exact(&mut header)?;
        let header = INesHeader::from_bytes(&header)?;

        let mut prg_rom = vec![0u8; (header.prg_rom_size as usize) * PRG_ROM_BANK_SIZE];
        rom_data.read_exact(&mut prg_rom)?;
        let prg_rom = Rom::from_vec(prg_rom);

        let mut chr_rom = vec![0u8; (header.chr_rom_size as usize) * CHR_ROM_BANK_SIZE];
        rom_data.read_exact(&mut chr_rom)?;
        let chr_rom = Rom::from_vec(chr_rom);

        eprintln!(
            "Loaded `{}` - Mapper: {}, PRG ROM: {}, CHR ROM: {}",
            rom_file.as_ref().display(),
            header.mapper_num,
            header.prg_rom_size,
            header.chr_rom_size,
        );
        Ok(Self {
            rom_file: rom_file.as_ref().to_path_buf(),
            header,
            prg_rom,
            chr_rom,
        })
    }

    /// The nametable mirroring mode defined in the header
    pub fn mirroring(&self) -> Mirroring {
        if self.header.flags & 0x08 == 0x08 {
            Mirroring::FourScreen
        } else {
            match self.header.flags & 0x01 {
                0 => Mirroring::Horizontal,
                1 => Mirroring::Vertical,
                _ => panic!("impossible mirroring"),
            }
        }
    }

    /// Returns whether this cartridge has battery-backed Save RAM
    pub fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
    }

    pub fn prg_ram_size(&self) -> usize {
        if self.header.prg_ram_size > 0 {
            (64 << self.header.prg_ram_size) as usize
        } else {
            0
        }
    }
}

impl Savable for Cartridge {
    fn save(&self, fh: &mut Write) -> Result<()> {
        self.prg_rom.save(fh)?;
        self.chr_rom.save(fh)
    }
    fn load(&mut self, fh: &mut Read) -> Result<()> {
        self.prg_rom.load(fh)?;
        self.chr_rom.load(fh)
    }
}

/// Represents an iNES header
///
/// [http://wiki.nesdev.com/w/index.php/INES]()
/// [http://wiki.nesdev.com/w/index.php/NES_2.0]()
/// [http://nesdev.com/NESDoc.pdf (page 28)]()
#[derive(Default, Debug)]
pub struct INesHeader {
    pub version: u8,       // 1 for iNES or 2 for NES 2.0
    pub mapper_num: u16,   // The primary mapper number
    pub submapper_num: u8, // NES 2.0 https://wiki.nesdev.com/w/index.php/NES_2.0_submappers
    pub flags: u8,         // Mirroring, Battery, Trainer, VS Unisystem, Playchoice-10, NES 2.0
    pub prg_rom_size: u16, // Number of 16 KB PRG-ROM banks (Program ROM)
    pub chr_rom_size: u16, // Number of 8 KB CHR-ROM banks (Character ROM)
    pub prg_ram_size: u8,  // NES 2.0 PRG-RAM
    pub chr_ram_size: u8,  // NES 2.0 CHR-RAM
    pub tv_mode: u8,       // NES 2.0 NTSC/PAL indicator
    pub vs_data: u8,       // NES 2.0 VS System data
}

impl INesHeader {
    /// Returns an empty INesHeader not loaded with any data
    fn new() -> Self {
        Self {
            version: 1u8,
            mapper_num: 0u16,
            submapper_num: 0u8,
            flags: 0u8,
            prg_rom_size: 0u16,
            chr_rom_size: 0u16,
            prg_ram_size: 0u8,
            chr_ram_size: 0u8,
            tv_mode: 0u8,
            vs_data: 0u8,
        }
    }

    /// Parses a slice of `u8` bytes and returns a valid INesHeader instance
    fn from_bytes(header: &[u8; 16]) -> Result<Self> {
        // Header checks
        if header[0..4] != *b"NES\x1a" {
            Err(format_err!("iNES header signature not found."))?;
        } else if (header[7] & 0x0C) == 0x04 {
            Err(format_err!(
                "Header is corrupted by \"DiskDude!\" - repair and try again."
            ))?;
        } else if (header[7] & 0x0C) == 0x0C {
            Err(format_err!(
                "Unrecognied header format - repair and try again."
            ))?;
        }

        let mut prg_rom_size = u16::from(header[4]);
        let mut chr_rom_size = u16::from(header[5]);
        // Upper 4 bits of flags 6 = D0..D3 and 7 = D4..D7
        let mut mapper_num = u16::from(((header[6] & 0xF0) >> 4) | (header[7] & 0xF0));
        // Lower 4 bits of flag 6 = D0..D3, upper 4 bits of flag 7 = D4..D7
        let flags = (header[6] & 0x0F) | ((header[7] & 0x0F) << 4);

        // NES 2.0 Format
        let mut version = 1; // Start off checking for iNES format v1
        let mut submapper_num = 0;
        let mut prg_ram_size = 0;
        let mut chr_ram_size = 0;
        let mut tv_mode = 0;
        let mut vs_data = 0;
        // If D2..D3 of flag 7 == 2
        if header[7] & 0x0C == 0x08 {
            version = 2;
            // lower 4 bits of flag 8 = D8..D11 of mapper num
            mapper_num |= u16::from(header[8] & 0x0F) << 8;
            // upper 4 bits of flag 8 = D0..D3 of submapper
            submapper_num = (header[8] & 0xF0) >> 4;
            // lower 4 bits of flag 9 = D8..D11 of prg_rom_size
            prg_rom_size |= u16::from(header[9] & 0x0F) << 8;
            // upper 4 bits of flag 9 = D8..D11 of chr_rom_size
            chr_rom_size |= u16::from(header[9] & 0xF0) << 4;
            prg_ram_size = header[10];
            chr_ram_size = header[11];
            tv_mode = header[12];
            vs_data = header[13];

            if prg_ram_size & 0x0F == 0x0F || prg_ram_size & 0xF0 == 0xF0 {
                Err(format_err!("Invalid PRG-RAM size in header."))?;
            } else if chr_ram_size & 0x0F == 0x0F || chr_ram_size & 0xF0 == 0xF0 {
                Err(format_err!("Invalid CHR-RAM size in header."))?;
            } else if chr_ram_size & 0xF0 == 0xF0 {
                Err(format_err!(
                    "Battery-backed CHR-RAM is currently not supported."
                ))?;
            } else if header[14] > 0 || header[15] > 0 {
                Err(format_err!(
                    "Unrecognized data found at header offsets 14-15."
                ))?;
            }
        } else {
            for (i, header) in header.iter().enumerate().take(16).skip(8) {
                if *header > 0 {
                    Err(format_err!(
                        "Unregonized data found at header offset {} - repair and try again.",
                        i
                    ))?;
                }
            }
        }

        // Trainer
        if flags & 0x04 == 0x04 {
            Err(format_err!("Trained ROMs are currently not supported."))?;
        }
        Ok(Self {
            mapper_num,
            submapper_num,
            flags,
            prg_rom_size,
            chr_rom_size,
            version,
            prg_ram_size,
            chr_ram_size,
            tv_mode,
            vs_data,
        })
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "Cartridge {{ header: {:?}, PRG-ROM: {}, CHR-ROM: {}",
            self.header,
            self.prg_rom.len(),
            self.chr_rom.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_cartridges() {
        let rom_data = &[
            // (File, PRG, CHR, Mapper, Mirroring, Battery)
            (
                "roms/super_mario_bros.nes",
                "Super Mario Bros. (World)",
                2,
                1,
                0,
                1,
                false,
            ),
            ("roms/metroid.nes", "Metroid (USA)", 8, 0, 1, 0, false),
        ];
        for rom in rom_data {
            let c = Cartridge::from_rom(PathBuf::from(rom.0));
            assert!(c.is_ok(), "new cartridge {}", rom.0);
            let c = c.unwrap();
            assert_eq!(
                c.header.prg_rom_size, rom.2,
                "PRG-ROM size matches for {}",
                rom.0
            );
            assert_eq!(
                c.header.chr_rom_size, rom.3,
                "CHR-ROM size matches for {}",
                rom.0
            );
            assert_eq!(
                c.header.mapper_num, rom.4,
                "mapper num matches for {}",
                rom.0
            );
            assert_eq!(
                c.header.flags & 0x01,
                rom.5,
                "mirroring matches for {}",
                rom.0
            );
            assert_eq!(
                c.header.flags & 0x02 == 0x02,
                rom.6,
                "battery matches for {}",
                rom.0
            );
        }
    }
}
