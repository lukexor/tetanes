//! NES cartridge implementation.

use crate::{
    common::{NesRegion, Regional},
    fs,
    mapper::{
        self, Axrom, BandaiFCG, Bf909x, Bnrom, Cnrom, ColorDreams, Dxrom76, Dxrom88, Dxrom95,
        Dxrom154, Dxrom206, Exrom, Fxrom, Gxrom, JalecoSs88006, Mapper, Mmc1Revision, Namco163,
        Nina003006, Nrom, Pxrom, SunsoftFme7, Sxrom, Txrom, Uxrom, Vrc6,
        m024_m026_vrc6::Revision as Vrc6Revision, m034_nina001::Nina001,
    },
    mem::{DynMemory, RamState},
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
    #[error("mapper: {0}")]
    InvalidMapper(#[from] mapper::Error),
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
pub struct GameInfo {
    pub crc32: u32,
    pub region: NesRegion,
    pub mapper_num: u16,
    pub submapper_num: u8,
}

/// An NES cartridge.
#[must_use]
pub struct Cart {
    name: String,
    header: NesHeader,
    region: NesRegion,
    ram_state: RamState,
    pub(crate) mapper: Mapper,
    pub(crate) chr_rom: DynMemory<u8>, // Character ROM
    pub(crate) chr_ram: DynMemory<u8>, // Character RAM
    pub(crate) prg_rom: DynMemory<u8>, // Program ROM
    pub(crate) prg_ram: DynMemory<u8>, // Program RAM
    pub(crate) ex_ram: DynMemory<u8>,  // Internal Extra RAM
    pub(crate) game_info: Option<GameInfo>,
}

impl Default for Cart {
    fn default() -> Self {
        Self::empty()
    }
}

impl Cart {
    pub fn empty() -> Self {
        let mut empty = Self {
            name: "Empty Cart".to_string(),
            header: NesHeader::default(),
            region: NesRegion::Ntsc,
            ram_state: RamState::default(),
            mapper: Mapper::none(),
            chr_rom: DynMemory::with_size(CHR_ROM_BANK_SIZE),
            chr_ram: DynMemory::new(),
            prg_rom: DynMemory::with_size(PRG_ROM_BANK_SIZE),
            prg_ram: DynMemory::new(),
            ex_ram: DynMemory::new(),
            game_info: None,
        };
        empty.mapper = Nrom::load(&mut empty).expect("valid empty mapper");
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
        Self::from_rom(path.to_string_lossy(), &mut rom, ram_state)
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
        debug!("{header:?}");

        let prg_rom_len = (header.prg_rom_banks as usize) * PRG_ROM_BANK_SIZE;
        let mut prg_rom = DynMemory::with_size(prg_rom_len);
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
        let prg_ram = DynMemory::with_size(prg_ram_size).with_ram_state(ram_state);

        let mut chr_rom = DynMemory::with_size((header.chr_rom_banks as usize) * CHR_ROM_BANK_SIZE);
        let chr_ram = if header.chr_rom_banks > 0 {
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
            DynMemory::new()
        } else {
            let chr_ram_size = Self::calculate_ram_size(header.chr_ram_shift)?;
            DynMemory::with_size(chr_ram_size).with_ram_state(ram_state)
        };

        let game_info = Self::lookup_info(&prg_rom, &chr_rom);
        let region = if matches!(header.variant, NesVariant::INes | NesVariant::Nes2) {
            match header.tv_mode {
                1 => NesRegion::Pal,
                3 => NesRegion::Dendy,
                _ => game_info
                    .as_ref()
                    .map(|info| info.region)
                    .unwrap_or_default(),
            }
        } else {
            game_info
                .as_ref()
                .map(|info| info.region)
                .unwrap_or_default()
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
            ex_ram: DynMemory::new(),
            game_info,
        };
        cart.mapper = match cart.header.mapper_num {
            0 => Nrom::load(&mut cart)?,
            1 => Sxrom::load(&mut cart, Mmc1Revision::BC)?,
            2 => Uxrom::load(&mut cart)?,
            3 => Cnrom::load(&mut cart)?,
            4 => Txrom::load(&mut cart)?,
            5 => Exrom::load(&mut cart)?,
            7 => Axrom::load(&mut cart)?,
            9 => Pxrom::load(&mut cart)?,
            10 => Fxrom::load(&mut cart)?,
            11 => ColorDreams::load(&mut cart)?,
            16 | 153 | 157 | 159 => BandaiFCG::load(&mut cart)?,
            18 => JalecoSs88006::load(&mut cart)?,
            19 | 210 => Namco163::load(&mut cart)?,
            24 => Vrc6::load(&mut cart, Vrc6Revision::A)?,
            26 => Vrc6::load(&mut cart, Vrc6Revision::B)?,
            34 => {
                // ≥ 16K implies NINA-001; ≤ 8K implies BNROM
                if cart.has_chr_rom() && cart.chr_rom.len() >= 0x4000 {
                    Nina001::load(&mut cart)?
                } else {
                    Bnrom::load(&mut cart)?
                }
            }
            66 => Gxrom::load(&mut cart)?,
            69 => SunsoftFme7::load(&mut cart)?,
            71 => Bf909x::load(&mut cart)?,
            76 => Dxrom76::load(&mut cart)?,
            79 | 113 | 146 => Nina003006::load(&mut cart)?,
            88 => Dxrom88::load(&mut cart)?,
            95 => Dxrom95::load(&mut cart)?,
            154 => Dxrom154::load(&mut cart)?,
            155 => Sxrom::load(&mut cart, Mmc1Revision::A)?,
            206 => Dxrom206::load(&mut cart)?,
            _ => Mapper::none(),
        };

        info!("loaded ROM `{cart}`");
        debug!("{cart:?}");
        Ok(cart)
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn chr_rom(&self) -> &[u8] {
        &self.chr_rom
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn chr_ram(&self) -> &[u8] {
        &self.chr_ram
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn prg_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn has_chr_rom(&self) -> bool {
        !self.chr_rom.is_empty()
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn has_chr_ram(&self) -> bool {
        !self.chr_ram.is_empty()
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn has_prg_ram(&self) -> bool {
        !self.prg_ram.is_empty()
    }

    #[must_use]
    pub const fn is_ines(&self) -> bool {
        matches!(
            self.header.variant,
            NesVariant::ArchaicINes | NesVariant::INes07 | NesVariant::INes
        )
    }

    #[must_use]
    pub const fn is_nes2(&self) -> bool {
        matches!(self.header.variant, NesVariant::Nes2)
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
    pub fn mapper_num(&self) -> u16 {
        self.game_info
            .as_ref()
            .map(|info| info.mapper_num)
            .unwrap_or(self.header.mapper_num)
    }

    /// Returns the Sub-Mapper number for this Cart.
    #[must_use]
    pub fn submapper_num(&self) -> u8 {
        self.game_info
            .as_ref()
            .map(|info| info.submapper_num)
            .unwrap_or(self.header.submapper_num)
    }

    /// Returns the Mapper and Board name for this Cart.
    #[must_use]
    pub fn mapper_board(&self) -> &'static str {
        NesHeader::mapper_board(self.mapper_num())
    }

    /// Allows mappers to add PRG-RAM.
    pub(crate) fn add_prg_ram(&mut self, capacity: usize) {
        self.prg_ram.resize(capacity);
    }

    /// Allows mappers to add CHR-RAM.
    pub(crate) fn add_chr_ram(&mut self, capacity: usize) {
        self.chr_ram.resize(capacity);
    }

    /// Allows mappers to add EX-RAM.
    pub(crate) fn add_exram(&mut self, capacity: usize) {
        self.ex_ram.resize(capacity);
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

    fn lookup_info(prg_rom: &[u8], chr: &[u8]) -> Option<GameInfo> {
        const GAME_DB: &[u8] = include_bytes!("../game_db.dat");

        let Ok(games) = fs::load_bytes::<Vec<GameInfo>>(GAME_DB) else {
            error!("failed to load `game_regions.dat`");
            return None;
        };

        let mut crc32 = fs::compute_crc32(prg_rom);
        if !chr.is_empty() {
            crc32 = fs::compute_combine_crc32(crc32, chr);
        }

        match games.binary_search_by(|game| game.crc32.cmp(&crc32)) {
            Ok(index) => {
                info!(
                    "found game matching crc: {crc32:#010X}. info: {:?}",
                    games[index]
                );
                Some(games[index].clone())
            }
            Err(_) => {
                info!("no game found matching crc: {crc32:#010X}");
                None
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
            .field("chr_rom", &self.chr_rom)
            .field("chr_ram", &self.chr_ram)
            .field("prg_rom", &self.prg_rom)
            .field("prg_ram", &self.prg_ram)
            .field("ex_ram", &self.ex_ram)
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
        }
        if (header[7] & 0x0C) == 0x04 {
            return Err(Error::InvalidHeader {
                byte: 7,
                value: header[7],
                message: "header is corrupted by `DiskDude!`. repair and try again".to_string(),
            });
        }
        if (header[7] & 0x0C) == 0x0C {
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
            }
            if chr_ram_shift & 0x0F == 0x0F || chr_ram_shift & 0xF0 == 0xF0 {
                return Err(Error::InvalidHeader {
                    byte: 11,
                    value: chr_ram_shift,
                    message: "invalid chr-ram size in header".to_string(),
                });
            }
            if chr_ram_shift & 0xF0 == 0xF0 {
                return Err(Error::InvalidHeader {
                    byte: 11,
                    value: chr_ram_shift,
                    message: "battery-backed chr-ram is currently not supported".to_string(),
                });
            }
            if header[14] > 0 || header[15] > 0 {
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
    pub const fn mapper_board(mapper_num: u16) -> &'static str {
        match mapper_num {
            0 => "Mapper 000 - NROM",
            1 => "Mapper 001 - SxROM/MMC1B/C",
            2 => "Mapper 002 - UxROM",
            3 => "Mapper 003 - CNROM",
            4 => "Mapper 004 - TxROM/MMC3/MMC6",
            5 => "Mapper 005 - ExROM/MMC5",
            6 => "Mapper 006 - FFE 1M/2M",
            7 => "Mapper 007 - AxROM",
            8 => "Mapper 008 - FFE 1M/2M", // Also Mapper 006 Submapper 4
            9 => "Mapper 009 - PxROM/MMC2",
            10 => "Mapper 010 - FxROM/MMC4",
            11 => "Mapper 011 - Color Dreams",
            12 => "Mapper 012 - Gouder/FFE 4M/MMC3",
            13 => "Mapper 013 - CPROM",
            14 => "Mapper 014 - UNL SL1632",
            15 => "Mapper 015 - K1029/30",
            16 => "Mapper 016 - Bandai FCG",
            17 => "Mapper 017 - FFE",
            18 => "Mapper 018 - Jaleco SS 88006",
            19 => "Mapper 019 - Namco 129/163",
            20 => "Mapper 020 - FDS",
            21 => "Mapper 021 - Vrc4a/Vrc4c",
            22 => "Mapper 022 - Vrc2a",
            23 => "Mapper 023 - Vrc4e",
            24 => "Mapper 024 - Vrc6a",
            25 => "Mapper 025 - Vrc4b",
            26 => "Mapper 026 - Vrc6b",
            27 => "Mapper 027 - Vrc4x",
            28 => "Mapper 028 - Action 53",
            29 => "Mapper 029 - Sealie Computing",
            30 => "Mapper 030 - UNROM 512",
            31 => "Mapper 031 - NSF",
            32 => "Mapper 032 - Irem G101",
            33 => "Mapper 033 - Taito TC0190",
            34 => "Mapper 034 - BNROM/NINA-001",
            35 => "Mapper 035 - JY Company",
            36 => "Mapper 036 - TXC 22000",
            37 => "Mapper 037 - MMC3 Multicart",
            38 => "Mapper 038 - UNL PCI556",
            39 => "Mapper 039 - Subor",
            40 => "Mapper 040 - NTDEC 2722",
            41 => "Mapper 041 - Caltron 6-in-1",
            42 => "Mapper 042",
            43 => "Mapper 043 - TONY-I/YS-612",
            44 => "Mapper 044 - MMC3 Multicart",
            45 => "Mapper 045 - MMC3 Multicart",
            46 => "Mapper 046 - Color Dreams",
            47 => "Mapper 047 - MMC3 Multicart",
            48 => "Mapper 048 - Taito TC0690",
            49 => "Mapper 049 - MMC Multicart",
            50 => "Mapper 050",
            51 => "Mapper 051",
            52 => "Mapper 052 - Realtec 8213/MMC Multicaart",
            53 => "Mapper 053 - Supervision",
            54 => "Mapper 054 - Novel Diamond",
            55 => "Mapper 055 - UNIF BTL-MARIO1-MALEE2",
            56 => "Mapper 056",
            57 => "Mapper 057",
            58 => "Mapper 058",
            59 => "Mapper 059 - BMC T3H53/D1038",
            60 => "Mapper 060",
            61 => "Mapper 061",
            62 => "Mapper 062",
            63 => "Mapper 063",
            64 => "Mapper 064 - RAMBO-1",
            65 => "Mapper 065 - Irem H3001",
            66 => "Mapper 066 - GxROM/MxROM",
            67 => "Mapper 067 - Sunsoft-3",
            68 => "Mapper 068 - Sunsoft-4",
            69 => "Mapper 069 - Sunsoft FME-7",
            70 => "Mapper 070 - Bandai",
            71 => "Mapper 071 - BF909x",
            72 => "Mapper 072 - Jaleco JF-17",
            73 => "Mapper 073 - Vrc3",
            74 => "Mapper 074",
            75 => "Mapper 075 - Vrc1",
            76 => "Mapper 076 - NAMCOT-108",
            77 => "Mapper 077",
            78 => "Mapper 078",
            79 => "Mapper 079 - NINA-03/06",
            80 => "Mapper 080 - Taito X1005",
            81 => "Mapper 081 - NTDEC 715021",
            82 => "Mapper 082 - Taito X1017",
            83 => "Mapper 083",
            84 => "Mapper 084",
            85 => "Mapper 085 - Vrc7",
            86 => "Mapper 086 - Jaleco JF-13",
            87 => "Mapper 087 - Jaleco JF-xx",
            88 => "Mapper 088",
            89 => "Mapper 089 - Sunsoft",
            90 => "Mapper 090 - JY Company",
            91 => "Mapper 091",
            92 => "Mapper 092",
            93 => "Mapper 093 - Sunsoft",
            94 => "Mapper 094 - UxROM",
            95 => "Mapper 095 - NAMCOT-3425",
            96 => "Mapper 096 - Oeka Kids",
            97 => "Mapper 097 - Irem TAM-S1",
            98 => "Mapper 098",
            99 => "Mapper 099 - Vs. System",
            100 => "Mapper 100",
            101 => "Mapper 101 - Jaleco JF-10",
            102 => "Mapper 102",
            103 => "Mapper 103",
            104 => "Mapper 104 - Golden Five",
            105 => "Mapper 105 - MMC1",
            106 => "Mapper 106",
            107 => "Mapper 107",
            108 => "Mapper 108",
            109 => "Mapper 109",
            110 => "Mapper 110",
            111 => "Mapper 111 - GTROM",
            112 => "Mapper 112",
            113 => "Mapper 113 - NINA-03/06",
            114 => "Mapper 114 - MMC3",
            115 => "Mapper 115 - MMC3",
            116 => "Mapper 116 - SOMARI-P",
            117 => "Mapper 117",
            118 => "Mapper 118 - TxSROM",
            119 => "Mapper 119 - TQROM",
            120 => "Mapper 120",
            121 => "Mapper 121 - MMC3",
            122 => "Mapper 122",
            123 => "Mapper 123 - MMC3",
            124 => "Mapper 124",
            125 => "Mapper 125 - UNL-LH32",
            126 => "Mapper 126 - MMC36",
            127 => "Mapper 127",
            128 => "Mapper 128",
            129 => "Mapper 129",
            130 => "Mapper 130",
            131 => "Mapper 131",
            132 => "Mapper 132 - TXC",
            133 => "Mapper 133 - Sachen 3009",
            134 => "Mapper 134 - MMC3",
            135 => "Mapper 135 - Sachen 8259A",
            136 => "Mapper 136 - Sachen 3011",
            137 => "Mapper 137 - Sachen 8259D",
            138 => "Mapper 138 - Sachen 8259B",
            139 => "Mapper 139 - Sachen 8259C",
            140 => "Mapper 140 - Jaleco JF-11/14",
            141 => "Mapper 141 - Sachen 8259A",
            142 => "Mapper 142 - Kaiser KS-7032",
            143 => "Mapper 143 - NROM",
            144 => "Mapper 144 - Color Dreams",
            145 => "Mapper 145 - Sachen SA-72007",
            146 => "Mapper 146 - NINA-03/06",
            147 => "Mapper 147 - Sachen 3018",
            148 => "Mapper 148 - Sachen SA-008-A/Tengen 800008",
            149 => "Mapper 149 - Sachen SA-0036",
            150 => "Mapper 150 - Sach SA-015/630",
            151 => "Mapper 151 - Vrc1",
            152 => "Mapper 152",
            153 => "Mapper 153 - Bandai FCG",
            154 => "Mapper 154 - NAMCOT-3453",
            155 => "Mapper 155 - SxROM/MMC1A",
            156 => "Mapper 156 - Daou",
            157 => "Mapper 157 - Bandai FCG",
            158 => "Mapper 158 - Tengen 800037",
            159 => "Mapper 159 - Bandai FCG",
            160 => "Mapper 160",
            161 => "Mapper 161",
            162 => "Mapper 162 - Wàixīng",
            163 => "Mapper 163 - Nánjīng",
            164 => "Mapper 164 - Dōngdá/Yànchéng",
            165 => "Mapper 165 - MMC3",
            166 => "Mapper 166 - Subor",
            167 => "Mapper 167 - Subor",
            168 => "Mapper 168 - Racermate",
            169 => "Mapper 169 - Yuxing",
            170 => "Mapper 170",
            171 => "Mapper 171 - Kaiser KS-7058",
            172 => "Mapper 172",
            173 => "Mapper 173",
            174 => "Mapper 174",
            175 => "Mapper 175 - Kaiser KS-7022",
            176 => "Mapper 176 - MMC3",
            177 => "Mapper 177 - Hénggé Diànzǐ",
            178 => "Mapper 178",
            179 => "Mapper 179",
            180 => "Mapper 180 - UNROM",
            181 => "Mapper 181",
            182 => "Mapper 182 - MMC3",
            183 => "Mapper 183",
            184 => "Mapper 184 - Sunsoft",
            185 => "Mapper 185 - CNROM",
            186 => "Mapper 186",
            187 => "Mapper 187 - Kǎshèng/MMC3",
            188 => "Mapper 188 - Bandai Karaoke",
            189 => "Mapper 189 - MMC3",
            190 => "Mapper 190 -",
            191 => "Mapper 191 - MMC3",
            192 => "Mapper 192 - Wàixīng",
            193 => "Mapper 193 - NTDEC TC-112",
            194 => "Mapper 194 - MMC3",
            195 => "Mapper 195 - Wàixīng/MMC3",
            196 => "Mapper 196 - MMC3",
            197 => "Mapper 197 - MMC3",
            198 => "Mapper 198 - MMC3",
            199 => "Mapper 199 - Wàixīng/MMC3",
            200 => "Mapper 200",
            201 => "Mapper 201 - NROM",
            202 => "Mapper 202",
            203 => "Mapper 203",
            204 => "Mapper 204",
            205 => "Mapper 205 - MMC3",
            206 => "Mapper 206 - DxROM",
            207 => "Mapper 207 - Taito X1-005",
            208 => "Mapper 208 - MMC3",
            209 => "Mapper 209 - JY Company",
            210 => "Mapper 210 - Namco",
            211 => "Mapper 211 - JyCompany",
            212 => "Mapper 212",
            213 => "Mapper 213",
            214 => "Mapper 214",
            215 => "Mapper 215 - MMC3",
            216 => "Mapper 216",
            217 => "Mapper 217 - MMC3",
            218 => "Mapper 218",
            219 => "Mapper 219 - Kǎshèng/MMC3",
            220 => "Mapper 220",
            221 => "Mapper 221 - NTDEC N625092",
            222 => "Mapper 222",
            223 => "Mapper 223",
            224 => "Mapper 224 - Jncota/MMC3",
            225 => "Mapper 225",
            226 => "Mapper 226",
            227 => "Mapper 227",
            228 => "Mapper 228- Active Enterprises",
            229 => "Mapper 229",
            230 => "Mapper 230",
            231 => "Mapper 231",
            232 => "Mapper 232 - BF909x",
            233 => "Mapper 233",
            234 => "Mapper 234 - Maxi 15 Multicart",
            235 => "Mapper 235",
            236 => "Mapper 236 - Realtec",
            237 => "Mapper 237",
            238 => "Mapper 238 - MMC3",
            239 => "Mapper 239",
            240 => "Mapper 240",
            241 => "Mapper 241 - BxROM",
            242 => "Mapper 242",
            243 => "Mapper 243 - Sachen SA-020A",
            244 => "Mapper 244",
            245 => "Mapper 245 - Wàixīng/MMC3",
            246 => "Mapper 246",
            247 => "Mapper 247",
            248 => "Mapper 248",
            249 => "Mapper 249 - MMC3",
            250 => "Mapper 250 - Nitra/MMC3",
            251 => "Mapper 251",
            252 => "Mapper 252 - Wàixīng",
            253 => "Mapper 253 - Wàixīng",
            254 => "Mapper 254 - MMC3",
            255 => "Mapper 255",
            _ => "Invalid Mapper",
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
