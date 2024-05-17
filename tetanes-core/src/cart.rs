//! NES cartridge implementation.

use crate::{
    common::{NesRegion, Regional},
    fs,
    mapper::{
        m024_m026_vrc6::Revision as Vrc6Revision, m034_nina001::Nina001, Axrom, Bf909x, Bnrom,
        Cnrom, ColorDreams, Exrom, Gxrom, Mapper, Mmc1Revision, Nrom, Pxrom, Sxrom, Txrom, Uxrom,
        Vrc6,
    },
    mem::RamState,
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
use thiserror::Error;
use tracing::{debug, error, info};

const PRG_ROM_BANK_SIZE: usize = 0x4000;
const CHR_ROM_BANK_SIZE: usize = 0x2000;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[must_use]
pub enum Error {
    #[error("invalid nes header (found: ${value:04X} at byte: {byte}). {message}")]
    InvalidHeader {
        byte: u8,
        value: u8,
        message: String,
    },
    #[error("unimplemented mapper `{0}`")]
    UnimplementedMapper(u16),
    #[error("{context}: {source:?}")]
    Io {
        context: String,
        source: std::io::Error,
    },
}

impl Error {
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct GameRegion {
    pub crc32: u32,
    pub region: NesRegion,
}

/// An NES cartridge.
#[derive(Default)]
#[must_use]
pub struct Cart {
    name: String,
    header: NesHeader,
    region: NesRegion,
    ram_state: RamState,
    pub(crate) mapper: Mapper,
    pub(crate) chr_rom: Vec<u8>, // Character ROM
    pub(crate) chr_ram: Vec<u8>, // Character RAM
    pub(crate) prg_rom: Vec<u8>, // Program ROM
    pub(crate) prg_ram: Vec<u8>, // Program RAM
    pub(crate) ex_ram: Vec<u8>,  // Internal Extra RAM
}

impl Cart {
    pub fn empty() -> Self {
        let mut empty = Self {
            name: "Empty Cart".to_string(),
            header: NesHeader::default(),
            region: NesRegion::Ntsc,
            ram_state: RamState::default(),
            mapper: Mapper::none(),
            chr_rom: vec![0x00; CHR_ROM_BANK_SIZE],
            chr_ram: vec![],
            prg_rom: vec![0x00; PRG_ROM_BANK_SIZE],
            prg_ram: vec![],
            ex_ram: vec![],
        };
        empty.mapper = Nrom::load(&mut empty);
        empty
    }

    /// Load `Cart` from a ROM path.
    ///
    /// # Errors
    ///
    /// If the NES header is corrupted, the ROM file cannot be read, or the data does not match
    /// the header, then an error is returned.
    pub fn from_path<P: AsRef<Path>>(path: P, ram_state: RamState) -> Result<Self> {
        let path = path.as_ref();
        let mut rom = BufReader::new(
            File::open(path)
                .map_err(|err| Error::io(err, format!("failed to open rom {path:?}")))?,
        );
        Self::from_rom(&path.to_string_lossy(), &mut rom, ram_state)
    }

    /// Load `Cart` from ROM data.
    ///
    /// # Errors
    ///
    /// If the NES header is invalid, or the ROM data does not match the header, then an error is
    /// returned.
    pub fn from_rom<S, F>(name: S, mut rom_data: &mut F, ram_state: RamState) -> Result<Self>
    where
        S: ToString,
        F: Read,
    {
        let name = name.to_string();
        let header = NesHeader::load(&mut rom_data)?;

        let prg_rom_len = (header.prg_rom_banks as usize) * PRG_ROM_BANK_SIZE;
        let mut prg_rom = vec![0x00; prg_rom_len];
        rom_data.read_exact(&mut prg_rom).map_err(|err| {
            if let std::io::ErrorKind::UnexpectedEof = err.kind() {
                Error::InvalidHeader {
                    byte: 4,
                    value: header.prg_rom_banks as u8,
                    message: format!(
                        "expected `{}` prg-rom banks ({prg_rom_len} total bytes)",
                        header.prg_rom_banks
                    ),
                }
            } else {
                Error::io(err, "failed to read prg-rom")
            }
        })?;

        let prg_ram_size = Self::calculate_ram_size(header.prg_ram_shift)?;
        let mut prg_ram = vec![0x00; prg_ram_size];
        RamState::fill(&mut prg_ram, ram_state);

        let mut chr_rom = vec![0x00; (header.chr_rom_banks as usize) * CHR_ROM_BANK_SIZE];
        let mut chr_ram = vec![];
        if header.chr_rom_banks > 0 {
            rom_data.read_exact(&mut chr_rom).map_err(|err| {
                if let std::io::ErrorKind::UnexpectedEof = err.kind() {
                    Error::InvalidHeader {
                        byte: 5,
                        value: header.chr_rom_banks as u8,
                        message: format!(
                            "expected `{}` chr-rom banks ({prg_rom_len} total bytes)",
                            header.chr_rom_banks
                        ),
                    }
                } else {
                    Error::io(err, "failed to read chr-rom")
                }
            })?;
        } else {
            let chr_ram_size = Self::calculate_ram_size(header.chr_ram_shift)?;
            if chr_ram_size > 0 {
                chr_ram.resize(chr_ram_size, 0x00);
                RamState::fill(&mut chr_ram, ram_state);
            }
        }

        let region = if matches!(header.variant, NesVariant::INes | NesVariant::Nes2) {
            match header.tv_mode {
                1 => NesRegion::Pal,
                3 => NesRegion::Dendy,
                _ => Self::lookup_region(&prg_rom, &chr_rom),
            }
        } else {
            Self::lookup_region(&prg_rom, &chr_rom)
        };

        let mut cart = Self {
            name,
            header,
            region,
            ram_state,
            mapper: Mapper::none(),
            chr_rom,
            chr_ram,
            prg_rom,
            prg_ram,
            ex_ram: vec![],
        };
        cart.mapper = match cart.header.mapper_num {
            0 => Nrom::load(&mut cart),
            1 => Sxrom::load(&mut cart, Mmc1Revision::BC),
            2 => Uxrom::load(&mut cart),
            3 => Cnrom::load(&mut cart),
            4 => Txrom::load(&mut cart),
            5 => Exrom::load(&mut cart),
            7 => Axrom::load(&mut cart),
            9 => Pxrom::load(&mut cart),
            11 => ColorDreams::load(&mut cart),
            24 => Vrc6::load(&mut cart, Vrc6Revision::A),
            26 => Vrc6::load(&mut cart, Vrc6Revision::B),
            34 => {
                // ≥ 16K implies NINA-001; ≤ 8K implies BNROM
                if cart.has_chr_rom() && cart.chr_rom.len() >= 0x4000 {
                    Nina001::load(&mut cart)
                } else {
                    Bnrom::load(&mut cart)
                }
            }
            66 => Gxrom::load(&mut cart),
            71 => Bf909x::load(&mut cart),
            155 => Sxrom::load(&mut cart, Mmc1Revision::A),
            _ => return Err(Error::UnimplementedMapper(cart.header.mapper_num)),
        };

        info!("loaded ROM `{cart}`");
        debug!("{cart:?}");
        Ok(cart)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn chr_rom(&self) -> &[u8] {
        &self.chr_rom
    }

    #[must_use]
    pub fn chr_ram(&self) -> &[u8] {
        &self.chr_ram
    }

    #[must_use]
    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    #[must_use]
    pub fn prg_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    #[must_use]
    pub fn has_chr_rom(&self) -> bool {
        !self.chr_rom.is_empty()
    }

    #[must_use]
    pub fn has_chr_ram(&self) -> bool {
        !self.chr_ram.is_empty()
    }

    #[must_use]
    pub fn has_prg_ram(&self) -> bool {
        !self.prg_ram.is_empty()
    }

    /// Returns whether this cartridge has battery-backed Save RAM.
    #[must_use]
    pub const fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
    }

    /// Returns `RamState`.
    pub const fn ram_state(&self) -> RamState {
        self.ram_state
    }

    /// Returns hardware configured `Mirroring`.
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

    /// Returns the Mapper number for this Cart.
    #[must_use]
    pub const fn mapper_num(&self) -> u16 {
        self.header.mapper_num
    }

    /// Returns the Sub-Mapper number for this Cart.
    #[must_use]
    pub const fn submapper_num(&self) -> u8 {
        self.header.submapper_num
    }

    /// Returns the Mapper and Board name for this Cart.
    #[must_use]
    pub const fn mapper_board(&self) -> &'static str {
        self.header.mapper_board()
    }

    /// Allows mappers to add PRG-RAM.
    pub(crate) fn add_prg_ram(&mut self, capacity: usize) {
        self.prg_ram.resize(capacity, 0x00);
        RamState::fill(&mut self.prg_ram, self.ram_state);
    }

    /// Allows mappers to add CHR-RAM.
    pub(crate) fn add_chr_ram(&mut self, capacity: usize) {
        self.chr_ram.resize(capacity, 0x00);
        RamState::fill(&mut self.chr_ram, self.ram_state);
    }

    /// Allows mappers to add EX-RAM.
    pub(crate) fn add_exram(&mut self, capacity: usize) {
        self.ex_ram.resize(capacity, 0x00);
        RamState::fill(&mut self.ex_ram, self.ram_state);
    }

    fn calculate_ram_size(value: u8) -> Result<usize> {
        if value > 0 {
            64usize
                .checked_shl(value.into())
                .ok_or_else(|| Error::InvalidHeader {
                    byte: 11,
                    value,
                    message: "header ram size larger than 64".to_string(),
                })
        } else {
            Ok(0)
        }
    }

    fn lookup_region(prg_rom: &[u8], chr: &[u8]) -> NesRegion {
        const GAME_REGIONS: &[u8] = include_bytes!("../game_regions.dat");

        let Ok(games) = fs::load_bytes::<Vec<GameRegion>>(GAME_REGIONS) else {
            error!("failed to load `game_regions.dat`");
            return NesRegion::Ntsc;
        };

        let mut crc32 = fs::compute_crc32(prg_rom);
        if !chr.is_empty() {
            crc32 = fs::compute_combine_crc32(crc32, chr);
        }

        match games.binary_search_by(|game| game.crc32.cmp(&crc32)) {
            Ok(index) => {
                info!(
                    "found game matching crc: {crc32:#010X}. region: {}",
                    games[index].region
                );
                games[index].region
            }
            Err(_) => {
                info!("no game found matching crc: {crc32:#010X}",);
                NesRegion::Ntsc
            }
        }
    }
}

impl Regional for Cart {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
    }
}

impl std::fmt::Display for Cart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(
            f,
            "{} - {}, CHR-ROM: {}K, CHR-RAM: {}K, PRG-ROM: {}K, PRG-RAM: {}K, EX-RAM: {}K, Mirroring: {:?}, Battery: {}",
            self.name,
            self.mapper_board(),
            self.chr_rom.len() / 0x0400,
            self.chr_ram.len() / 0x0400,
            self.prg_rom.len() / 0x0400,
            self.prg_ram.len() / 0x0400,
            self.ex_ram.len() / 0x0400,
            self.mirroring(),
            self.battery_backed(),
        )
    }
}

impl std::fmt::Debug for Cart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Cart")
            .field("name", &self.name)
            .field("header", &self.header)
            .field("region", &self.region)
            .field("ram_state", &self.ram_state)
            .field("mapper", &self.mapper)
            .field("mirroring", &self.mirroring())
            .field("battery_backed", &self.battery_backed())
            .field("chr_rom_len", &self.chr_rom.len())
            .field("chr_ram_len", &self.chr_ram.len())
            .field("prg_rom_len", &self.prg_rom.len())
            .field("prg_ram_len", &self.prg_ram.len())
            .field("ex_ram_len", &self.ex_ram.len())
            .finish()
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum NesVariant {
    #[default]
    ArchaicINes,
    INes07,
    INes,
    Nes2,
}

/// An `iNES` or `NES 2.0` formatted header representing hardware specs of a given NES cartridge.
///
/// <http://wiki.nesdev.com/w/index.php/INES>
/// <http://wiki.nesdev.com/w/index.php/NES_2.0>
/// <http://nesdev.com/NESDoc.pdf> (page 28)
#[derive(Default, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct NesHeader {
    pub variant: NesVariant,
    pub mapper_num: u16,    // The primary mapper number
    pub submapper_num: u8,  // NES 2.0 https://wiki.nesdev.com/w/index.php/NES_2.0_submappers
    pub flags: u8,          // Mirroring, Battery, Trainer, VS Unisystem, Playchoice-10, NES 2.0
    pub prg_rom_banks: u16, // Number of 16KB PRG-ROM banks (Program ROM)
    pub chr_rom_banks: u16, // Number of 8KB CHR-ROM banks (Character ROM)
    pub prg_ram_shift: u8,  // NES 2.0 PRG-RAM
    pub chr_ram_shift: u8,  // NES 2.0 CHR-RAM
    pub tv_mode: u8,        // NES 2.0 NTSC/PAL indicator
    pub vs_data: u8,        // NES 2.0 VS System data
}

impl NesHeader {
    /// Load `NesHeader` from a ROM path.
    ///
    /// # Errors
    ///
    /// If the NES header is corrupted, the ROM file cannot be read, or the data does not match
    /// the header, then an error is returned.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let mut rom = BufReader::new(
            File::open(path)
                .map_err(|err| Error::io(err, format!("failed to open rom {path:?}")))?,
        );
        Self::load(&mut rom)
    }

    /// Load `NesHeader` from ROM data.
    ///
    /// # Errors
    ///
    /// If the NES header is invalid, then an error is returned.
    pub fn load<F: Read>(rom_data: &mut F) -> Result<Self> {
        let mut header = [0u8; 16];
        rom_data.read_exact(&mut header).map_err(|err| {
            if let std::io::ErrorKind::UnexpectedEof = err.kind() {
                Error::InvalidHeader {
                    byte: 0,
                    value: 0,
                    message: "expected 16-byte header".to_string(),
                }
            } else {
                Error::io(err, "failed to read nes header")
            }
        })?;

        // Header checks
        if header[0..4] != *b"NES\x1a" {
            return Err(Error::InvalidHeader {
                byte: 0,
                value: header[0],
                message: "nes header signature not found".to_string(),
            });
        } else if (header[7] & 0x0C) == 0x04 {
            return Err(Error::InvalidHeader {
                byte: 7,
                value: header[7],
                message: "header is corrupted by `DiskDude!`. repair and try again".to_string(),
            });
        } else if (header[7] & 0x0C) == 0x0C {
            return Err(Error::InvalidHeader {
                byte: 7,
                value: header[7],
                message: "unrecognized header format. repair and try again".to_string(),
            });
        }

        let mut prg_rom_banks = u16::from(header[4]);
        let mut chr_rom_banks = u16::from(header[5]);
        // Upper 4 bits of flags 6 = D0..D3 and 7 = D4..D7
        let mut mapper_num = u16::from(((header[6] & 0xF0) >> 4) | (header[7] & 0xF0));
        // Lower 4 bits of flag 6 = D0..D3, upper 4 bits of flag 7 = D4..D7
        let flags = (header[6] & 0x0F) | ((header[7] & 0x0F) << 4);

        // NES 2.0 Format
        let mut submapper_num = 0;
        let mut prg_ram_shift = 0;
        let mut chr_ram_shift = 0;
        let mut tv_mode = 0;
        let mut vs_data = 0;
        // If D2..D3 of flag 7 == 2, then NES 2.0 (supports bytes 0-15)
        let variant = if header[7] & 0x0C == 0x08 {
            // lower 4 bits of flag 8 = D8..D11 of mapper num
            mapper_num |= u16::from(header[8] & 0x0F) << 8;
            // upper 4 bits of flag 8 = D0..D3 of submapper
            submapper_num = (header[8] & 0xF0) >> 4;
            // lower 4 bits of flag 9 = D8..D11 of prg_rom_size
            prg_rom_banks |= u16::from(header[9] & 0x0F) << 8;
            // upper 4 bits of flag 9 = D8..D11 of chr_rom_size
            chr_rom_banks |= u16::from(header[9] & 0xF0) << 4;
            prg_ram_shift = header[10];
            chr_ram_shift = header[11];
            tv_mode = header[12];
            vs_data = header[13];

            if prg_ram_shift & 0x0F == 0x0F || prg_ram_shift & 0xF0 == 0xF0 {
                return Err(Error::InvalidHeader {
                    byte: 10,
                    value: prg_ram_shift,
                    message: "invalid prg-ram size in header".to_string(),
                });
            } else if chr_ram_shift & 0x0F == 0x0F || chr_ram_shift & 0xF0 == 0xF0 {
                return Err(Error::InvalidHeader {
                    byte: 11,
                    value: chr_ram_shift,
                    message: "invalid chr-ram size in header".to_string(),
                });
            } else if chr_ram_shift & 0xF0 == 0xF0 {
                return Err(Error::InvalidHeader {
                    byte: 11,
                    value: chr_ram_shift,
                    message: "battery-backed chr-ram is currently not supported".to_string(),
                });
            } else if header[14] > 0 || header[15] > 0 {
                return Err(Error::InvalidHeader {
                    byte: 14,
                    value: header[14],
                    message: "unrecognized data found at header offsets 14-15".to_string(),
                });
            }
            NesVariant::Nes2
        } else if header[7] & 0x0C == 0x04 {
            // If D2..D3 of flag 7 == 1, then archaic iNES (supports bytes 0-7)
            for (i, value) in header.iter().enumerate().take(16).skip(8) {
                if *value > 0 {
                    return Err(Error::InvalidHeader {
                        byte: i as u8,
                        value: *value,
                        message: format!(
                            "unrecogonized data found at header byte {i}. repair and try again"
                        ),
                    });
                }
            }
            NesVariant::ArchaicINes
        } else if header[7] & 0x0C == 00 && header[12..=15].iter().all(|v| *v == 0) {
            // If D2..D3 of flag 7 == 0 and bytes 12-15 are all 0, then iNES (supports bytes 0-9)
            NesVariant::INes
        } else {
            // Else iNES 0.7 or archaic iNES (supports mapper high nibble)
            NesVariant::INes07
        };

        // Trainer
        if flags & 0x04 == 0x04 {
            return Err(Error::InvalidHeader {
                byte: 6,
                value: header[6],
                message: "trained roms are currently not supported.".to_string(),
            });
        }

        Ok(Self {
            variant,
            mapper_num,
            submapper_num,
            flags,
            prg_rom_banks,
            chr_rom_banks,
            prg_ram_shift,
            chr_ram_shift,
            tv_mode,
            vs_data,
        })
    }

    #[must_use]
    pub const fn mapper_board(&self) -> &'static str {
        match self.mapper_num {
            0 => "Mapper 000 - NROM",
            1 => "Mapper 001 - SxROM/MMC1B/C",
            2 => "Mapper 002 - UxROM",
            3 => "Mapper 003 - CNROM",
            4 => "Mapper 004 - TxROM/MMC3/MMC6",
            5 => "Mapper 005 - ExROM/MMC5",
            7 => "Mapper 007 - AxROM",
            9 => "Mapper 009 - PxROM",
            11 => "Mapper 011 - Color Dreams",
            24 => "Mapper 024 - Vrc6a",
            26 => "Mapper 026 - Vrc6b",
            34 => "Mapper 034 - BNROM/NINA-001",
            66 => "Mapper 066 - GxROM/MxROM",
            71 => "Mapper 071 - Camerica/Codemasters/BF909x",
            155 => "Mapper 155 - SxROM/MMC1A",
            _ => "Unimplemented Mapper",
        }
    }
}

impl std::fmt::Debug for NesHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("NesHeader")
            .field("version", &self.variant)
            .field("mapper_num", &format_args!("{:03}", &self.mapper_num))
            .field("submapper_num", &self.submapper_num)
            .field("flags", &format_args!("0b{:08b}", &self.flags))
            .field("prg_rom_banks", &self.prg_rom_banks)
            .field("chr_rom_banks", &self.chr_rom_banks)
            .field("prg_ram_shift", &self.prg_ram_shift)
            .field("chr_ram_shift", &self.chr_ram_shift)
            .field("tv_mode", &self.tv_mode)
            .field("vs_data", &self.vs_data)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_headers {
        ($(($test:ident, $data:expr, $header:expr$(,)?)),*$(,)?) => {$(
            #[test]
            fn $test() {
                let header = NesHeader::load(&mut $data.as_slice()).expect("valid header");
                assert_eq!(header, $header);
            }
        )*};
    }

    #[rustfmt::skip]
    test_headers!(
        (
            mapper000_horizontal,
            [0x4E, 0x45, 0x53, 0x1A,
             0x02, 0x01, 0x01, 0x00,
             0x00, 0x00, 0x00, 0x00,
             0x00, 0x00, 0x00, 0x00],
            NesHeader {
                variant: NesVariant::INes,
                mapper_num: 0,
                flags: 0b0000_0001,
                prg_rom_banks: 2,
                chr_rom_banks: 1,
                ..NesHeader::default()
            },
        ),
        (
            mapper001_vertical,
            [0x4E, 0x45, 0x53, 0x1A,
             0x08, 0x00, 0x10, 0x00,
             0x00, 0x00, 0x00, 0x00,
             0x00, 0x00, 0x00, 0x00],
            NesHeader {
                variant: NesVariant::INes,
                mapper_num: 1,
                flags: 0b0000_0000,
                prg_rom_banks: 8,
                chr_rom_banks: 0,
                ..NesHeader::default()
            },
        ),
    );
}
