//! Handles reading NES Cartridge headers and ROMs

use crate::{
    info,
    logging::{LogLevel, Loggable},
    map_nes_err,
    mapper::Mirroring,
    memory::Rom,
    nes_err,
    serialization::Savable,
    NesResult,
};
use std::{
    fmt,
    io::{BufReader, Read, Write},
};

const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
const CHR_ROM_BANK_SIZE: usize = 8 * 1024;

/// Represents an NES Cartridge
#[derive(Default)]
pub struct Cartridge {
    pub rom_file: String, // '.nes' rom file
    pub header: INesHeader,
    pub prg_rom: Rom, // Program ROM
    pub chr_rom: Rom, // Character ROM
    log_level: LogLevel,
}

impl Cartridge {
    /// Creates an empty cartridge not loaded with any ROM
    pub fn new() -> Self {
        Self {
            rom_file: String::new(),
            header: INesHeader::new(),
            prg_rom: Rom::init(PRG_ROM_BANK_SIZE),
            chr_rom: Rom::init(CHR_ROM_BANK_SIZE),
            log_level: LogLevel::default(),
        }
    }

    /// Creates a new Cartridge instance by reading in a `.nes` file
    ///
    /// # Arguments
    ///
    /// * `rom` - A String that that holds the path to a valid '.nes' file
    ///
    /// # Errors
    ///
    /// If the file is not a valid '.nes' file, or there are insufficient permissions to read the
    /// file, then an error is returned.
    pub fn from_rom(rom_file: &str) -> NesResult<Self> {
        use std::path::PathBuf;

        let rom_data = std::fs::File::open(&PathBuf::from(rom_file))
            .map_err(|e| map_nes_err!("unable to open file \"{}\": {}", rom_file, e))?;
        let mut rom_data = BufReader::new(rom_data);

        let mut header = [0u8; 16];
        rom_data.read_exact(&mut header)?;
        let header = INesHeader::from_bytes(&header)
            .map_err(|e| map_nes_err!("invalid rom \"{}\": {}", rom_file, e))?;

        let mut prg_rom = vec![0u8; (header.prg_rom_size as usize) * PRG_ROM_BANK_SIZE];
        rom_data.read_exact(&mut prg_rom).map_err(|e| {
            let bytes_rem = if let Ok(bytes) = rom_data.read_to_end(&mut prg_rom) {
                bytes.to_string()
            } else {
                "unknown".to_string()
            };

            map_nes_err!(
                "invalid rom \"{}\". PRG-ROM banks: {}. Bytes remaining: {}. Err: {}",
                rom_file,
                header.prg_rom_size,
                bytes_rem,
                e,
            )
        })?;
        let prg_rom = Rom::from_vec(prg_rom);

        let mut chr_rom = vec![0u8; (header.chr_rom_size as usize) * CHR_ROM_BANK_SIZE];
        rom_data.read_exact(&mut chr_rom).map_err(|e| {
            let bytes_rem = if let Ok(bytes) = rom_data.read_to_end(&mut chr_rom) {
                bytes.to_string()
            } else {
                "unknown".to_string()
            };

            map_nes_err!(
                "invalid rom \"{}\". CHR-ROM banks: {}. Bytes remaining: {}. Err: {}",
                rom_file,
                header.chr_rom_size,
                bytes_rem,
                e,
            )
        })?;
        let chr_rom = Rom::from_vec(chr_rom);

        let cart = Self {
            rom_file: rom_file.to_owned(),
            header,
            prg_rom,
            chr_rom,
            #[cfg(debug_assertions)]
            log_level: LogLevel::Debug,
            #[cfg(not(debug_assertions))]
            log_level: LogLevel::default(),
        };
        info!(
            cart,
            "Loaded `{}` - Mapper: {} - {}, PRG ROM: {}, CHR ROM: {}, Mirroring: {:?}",
            rom_file,
            cart.header.mapper_num,
            cart.mapper_board(),
            cart.header.prg_rom_size,
            cart.header.chr_rom_size,
            cart.mirroring(),
        );
        Ok(cart)
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

    pub fn mapper_board(&self) -> &'static str {
        match self.header.mapper_num {
            0 => "NROM",
            1 => "Sxrom/MMC1",
            2 => "UxROM",
            3 => "CNROM",
            4 => "TxROM/MMC3/MMC6",
            5 => "ExROM/MMC5",
            7 => "AxROM",
            9 => "PxROM",
            _ => "Unsupported Board",
        }
    }

    /// Returns whether this cartridge has battery-backed Save RAM
    pub fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
    }

    pub fn prg_ram_size(&self) -> NesResult<usize> {
        if self.header.prg_ram_size > 0 {
            if let Some(size) = 64usize.checked_shl(self.header.prg_ram_size.into()) {
                Ok(size)
            } else {
                nes_err!("invalid header PRG-RAM size")
            }
        } else {
            Ok(0)
        }
    }

    pub fn chr_ram_size(&self) -> NesResult<usize> {
        if self.header.chr_ram_size > 0 {
            if let Some(size) = 64usize.checked_shl(self.header.chr_ram_size.into()) {
                Ok(size)
            } else {
                nes_err!("invalid header CHR-RAM size")
            }
        } else {
            Ok(0)
        }
    }
}

impl Loggable for Cartridge {
    fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }
    fn log_level(&self) -> LogLevel {
        self.log_level
    }
}

impl Savable for Cartridge {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.prg_rom.save(fh)?;
        self.chr_rom.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
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
    fn from_bytes(header: &[u8; 16]) -> NesResult<Self> {
        // Header checks
        if header[0..4] != *b"NES\x1a" {
            return nes_err!("iNES header signature not found.");
        } else if (header[7] & 0x0C) == 0x04 {
            return nes_err!("Header is corrupted by \"DiskDude!\" - repair and try again.");
        } else if (header[7] & 0x0C) == 0x0C {
            return nes_err!("Unrecognized header format - repair and try again.");
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
                return nes_err!("Invalid PRG-RAM size in header.");
            } else if chr_ram_size & 0x0F == 0x0F || chr_ram_size & 0xF0 == 0xF0 {
                return nes_err!("Invalid CHR-RAM size in header.");
            } else if chr_ram_size & 0xF0 == 0xF0 {
                return nes_err!("Battery-backed CHR-RAM is currently not supported.");
            } else if header[14] > 0 || header[15] > 0 {
                return nes_err!("Unrecognized data found at header offsets 14-15.");
            }
        } else {
            for (i, header) in header.iter().enumerate().take(16).skip(8) {
                if *header > 0 {
                    return nes_err!(
                        "Unregonized data found at header offset {} - repair and try again.",
                        i,
                    );
                }
            }
        }

        // Trainer
        if flags & 0x04 == 0x04 {
            return nes_err!("Trained ROMs are currently not supported.");
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
    fn valid_cartridges() {
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
            let c = Cartridge::from_rom(&rom.0.to_string());
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
