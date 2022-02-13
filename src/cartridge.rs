//! Handles reading NES Cartridge headers and ROMs

use crate::{map_nes_err, mapper::Mirroring, memory::Memory, nes_err, NesResult};
use log::info;
use std::{fmt, io::Read};

const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
const CHR_ROM_BANK_SIZE: usize = 8 * 1024;

/// Represents an `iNES` header
///
/// <http://wiki.nesdev.com/w/index.php/INES>
/// <http://wiki.nesdev.com/w/index.php/NES_2.0>
/// <http://nesdev.com/NESDoc.pdf> (page 28)
#[derive(Default, Debug, Copy, Clone)]
#[must_use]
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

/// Represents an NES Cartridge
#[derive(Default, Clone)]
#[must_use]
pub struct Cartridge {
    pub name: String, // '.nes' rom file
    pub header: INesHeader,
    pub prg_rom: Memory, // Program ROM
    pub chr_rom: Memory, // Character ROM
    pub prg_ram_size: Option<usize>,
    pub chr_ram_size: Option<usize>,
}

impl Cartridge {
    /// Creates an empty cartridge not loaded with any ROM
    pub fn new() -> Self {
        let consistent_ram = true;
        Self {
            name: String::new(),
            header: INesHeader::new(),
            prg_rom: Memory::new(consistent_ram),
            chr_rom: Memory::new(consistent_ram),
            prg_ram_size: None,
            chr_ram_size: None,
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
    pub fn from_rom<F: Read>(name: &str, mut rom_data: &mut F) -> NesResult<Self> {
        let header = INesHeader::load(&mut rom_data)
            .map_err(|e| map_nes_err!("invalid rom \"{}\": {}", name, e))?;

        let mut prg_rom = vec![0u8; (header.prg_rom_size as usize) * PRG_ROM_BANK_SIZE];
        rom_data.read_exact(&mut prg_rom).map_err(|e| {
            let bytes_rem = if let Ok(bytes) = rom_data.read_to_end(&mut prg_rom) {
                bytes.to_string()
            } else {
                "unknown".to_string()
            };

            map_nes_err!(
                "invalid rom \"{}\". PRG-ROM banks: {}. Bytes remaining: {}. Err: {}",
                name,
                header.prg_rom_size,
                bytes_rem,
                e,
            )
        })?;
        let prg_rom = Memory::rom_from_bytes(&prg_rom);

        let mut chr_rom = vec![0u8; (header.chr_rom_size as usize) * CHR_ROM_BANK_SIZE];
        rom_data.read_exact(&mut chr_rom).map_err(|e| {
            let bytes_rem = if let Ok(bytes) = rom_data.read_to_end(&mut chr_rom) {
                bytes.to_string()
            } else {
                "unknown".to_string()
            };

            map_nes_err!(
                "invalid rom \"{}\". CHR-ROM banks: {}. Bytes remaining: {}. Err: {}",
                name,
                header.chr_rom_size,
                bytes_rem,
                e,
            )
        })?;
        let chr_rom = Memory::rom_from_bytes(&chr_rom);

        let prg_ram_size = if header.prg_ram_size > 0 {
            let prg_ram_size = 64usize.checked_shl(header.prg_ram_size.into());
            if prg_ram_size.is_none() {
                return nes_err!("invalid header PRG-RAM size");
            }
            prg_ram_size
        } else {
            None
        };
        let chr_ram_size = if header.chr_ram_size > 0 {
            let chr_ram_size = 64usize.checked_shl(header.chr_ram_size.into());
            if chr_ram_size.is_none() {
                return nes_err!("invalid header CHR-RAM size");
            }
            chr_ram_size
        } else {
            None
        };

        let cart = Self {
            name: name.to_owned(),
            header,
            prg_rom,
            chr_rom,
            prg_ram_size,
            chr_ram_size,
        };
        info!(
            "Loaded `{}` - Mapper: {} - {}, PRG ROM: {}, CHR ROM: {}, Mirroring: {:?}, Battery: {}",
            name,
            cart.header.mapper_num,
            cart.mapper_board(),
            cart.header.prg_rom_size,
            cart.header.chr_rom_size,
            cart.mirroring(),
            cart.battery_backed(),
        );
        Ok(cart)
    }

    /// The nametable mirroring mode defined in the header
    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        if self.header.flags & 0x08 == 0x08 {
            Mirroring::FourScreen
        } else {
            match self.header.flags & 0x01 {
                0 => Mirroring::Horizontal,
                1 => Mirroring::Vertical,
                _ => unreachable!("impossible mirroring"),
            }
        }
    }

    #[must_use]
    pub const fn mapper_board(&self) -> &'static str {
        match self.header.mapper_num {
            0 => "Mapper 000 - NROM",
            1 => "Mapper 001 - SxROM/MMC1",
            2 => "Mapper 002 - UxROM",
            3 => "Mapper 003 - CNROM",
            4 => "Mapper 004 - TxROM/MMC3/MMC6",
            5 => "Mapper 005 - ExROM/MMC5",
            7 => "Mapper 007 - AxROM",
            9 => "Mapper 009 - PxROM",
            71 => "Mapper 071 - UxROM/CAMERICA",
            155 => "Mapper 155 - SxROM/MMC1A",
            _ => "Unsupported Mapper",
        }
    }

    /// Returns whether this cartridge has battery-backed Save RAM
    #[must_use]
    #[inline]
    pub const fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
    }
}

impl INesHeader {
    /// Returns an empty `INesHeader` not loaded with any data
    const fn new() -> Self {
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

    /// Parses a slice of `u8` bytes and returns a valid `INesHeader` instance
    ///
    /// # Errors
    ///
    /// If any header values are invalid, or if cart data doesn't match the header, then an error
    /// is returned.
    pub fn load<F: Read>(rom_data: &mut F) -> NesResult<Self> {
        let mut header = [0u8; 16];
        rom_data.read_exact(&mut header)?;

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
                return nes_err!("invalid PRG-RAM size in header");
            } else if chr_ram_size & 0x0F == 0x0F || chr_ram_size & 0xF0 == 0xF0 {
                return nes_err!("invalid CHR-RAM size in header");
            } else if chr_ram_size & 0xF0 == 0xF0 {
                return nes_err!("battery-backed CHR-RAM is currently not supported");
            } else if header[14] > 0 || header[15] > 0 {
                return nes_err!("unrecognized data found at header offsets 14-15");
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
            version,
            mapper_num,
            submapper_num,
            flags,
            prg_rom_size,
            chr_rom_size,
            prg_ram_size,
            chr_ram_size,
            tv_mode,
            vs_data,
        })
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
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

    // TODO: Move these to simple header test files instead
    #[test]
    #[ignore]
    fn valid_cartridges() {
        use std::{fs::File, io::BufReader};

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
        for data in rom_data {
            let rom = File::open(data.0).expect("valid file");
            let mut rom = BufReader::new(rom);
            let c = Cartridge::from_rom(data.0, &mut rom);
            assert!(c.is_ok(), "new cartridge {}", data.0);
            let c = c.unwrap();
            assert_eq!(
                c.header.prg_rom_size, data.2,
                "PRG-ROM size matches for {}",
                data.0
            );
            assert_eq!(
                c.header.chr_rom_size, data.3,
                "CHR-ROM size matches for {}",
                data.0
            );
            assert_eq!(
                c.header.mapper_num, data.4,
                "mapper num matches for {}",
                data.0
            );
            assert_eq!(
                c.header.flags & 0x01,
                data.5,
                "mirroring matches for {}",
                data.0
            );
            assert_eq!(
                c.header.flags & 0x02 == 0x02,
                data.6,
                "battery matches for {}",
                data.0
            );
        }
    }
}
