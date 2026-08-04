//! NES cartridge implementation.

use crate::{
    common::NesRegion,
    fs,
    mapper::{self, Mapper},
    memory::RamState,
    memory::{Memory, MemoryLayout, Src},
    ppu::Mirroring,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Default CHR-RAM provided when a cart has no CHR-ROM.
const DEFAULT_CHR_RAM_SIZE: usize = 8 * 1024;

const PRG_ROM_BANK_SIZE: usize = 0x4000;
const CHR_ROM_BANK_SIZE: usize = 0x2000;

/// A `Result` from loading a cartridge.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors from loading a cartridge.
#[derive(Error, Debug)]
#[must_use]
pub enum Error {
    /// The iNES/NES 2.0 header did not describe a cartridge this crate can build.
    #[error("invalid nes header (found: ${value:04X} at byte: {byte}). {message}")]
    InvalidHeader {
        /// Which header byte was rejected.
        byte: u8,
        /// The value found there.
        value: u8,
        /// What was wrong with it.
        message: String,
    },
    /// The header named a mapper no board in the table serves.
    #[error("mapper: {0}")]
    InvalidMapper(#[from] mapper::Error),
    /// The ROM could not be read.
    #[error("{context}: {source:?}")]
    Io {
        /// What was being read when this happened.
        context: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl Error {
    /// Wraps an I/O error with a description of what was being read.
    pub fn io(source: std::io::Error, context: impl Into<String>) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// An NES cartridge.
#[derive(Debug)]
#[must_use]
pub struct Cart {
    /// The ROM's name, taken from the path it was loaded from.
    pub name: String,
    /// The iNES/NES 2.0 header it was built from.
    pub header: NesHeader,
    /// The region the cart runs in.
    pub region: NesRegion,
    /// How the cart's RAM was initialised at power-on.
    pub ram_state: RamState,
    /// The board serving this cart.
    pub mapper: Mapper,
    /// All of the cart's memory, and the page tables that address it.
    pub memory: Memory,
    /// Bytes of CHR-ROM; 0 for a cart with CHR-RAM instead.
    pub chr_rom_size: usize,
    /// Bytes of CHR-RAM; 0 for a cart with CHR-ROM instead.
    pub chr_ram_size: usize,
    /// Bytes of PRG-ROM.
    pub prg_rom_size: usize,
    /// Bytes of PRG-RAM, battery-backed or not.
    pub prg_ram_size: usize,
    /// What the game database knows about this ROM, matched by CRC.
    pub game_info: Option<GameInfo>,
}

impl Default for Cart {
    fn default() -> Self {
        Self::empty()
    }
}

impl Cart {
    /// Creates a cart with one PRG-ROM bank and one CHR-ROM bank of zeros, and no board.
    pub fn empty() -> Self {
        Self::empty_sized(PRG_ROM_BANK_SIZE, CHR_ROM_BANK_SIZE)
    }

    /// An empty `Cart` with the given ROM sizes. A `chr_rom_size` of zero yields CHR-RAM instead.
    pub fn empty_sized(prg_rom_size: usize, chr_rom_size: usize) -> Self {
        let chr_ram_size = if chr_rom_size == 0 {
            DEFAULT_CHR_RAM_SIZE
        } else {
            0
        };
        Self {
            name: "Empty Cart".to_string(),
            header: NesHeader::default(),
            region: NesRegion::default(),
            ram_state: RamState::default(),
            mapper: Mapper::none(),
            memory: Self::build_memory(
                0,
                prg_rom_size,
                0,
                chr_rom_size,
                chr_ram_size,
                false,
                RamState::default(),
            ),
            chr_rom_size,
            chr_ram_size,
            prg_rom_size,
            prg_ram_size: 0,
            game_info: None,
        }
    }

    /// Allocate page-table memory sized for a cart.
    ///
    /// `mapper_num` is needed before a [`Mapper`] exists, because a board that keeps battery state
    /// outside PRG-RAM needs room reserved for it here.
    fn build_memory(
        mapper_num: u16,
        prg_rom_size: usize,
        prg_ram_size: usize,
        chr_rom_size: usize,
        chr_ram_size: usize,
        four_screen: bool,
        ram_state: RamState,
    ) -> Memory {
        let mut memory = Memory::new(MemoryLayout {
            prg_rom: prg_rom_size,
            prg_ram: prg_ram_size.max(mapper::DEFAULT_PRG_RAM_SIZE),
            battery_ext: mapper::min_battery_ext(mapper_num),
            // `chr_ram_size` is only non-zero when there is no CHR-ROM, so exactly one of the two
            // backs the CHR region.
            chr: if chr_rom_size > 0 {
                chr_rom_size
            } else {
                chr_ram_size.max(DEFAULT_CHR_RAM_SIZE)
            },
            chr_writable: chr_rom_size == 0,
            ciram: if four_screen { 4 * 1024 } else { 2 * 1024 },
            // Boards with expansion RAM - MMC5's ExRAM, FK23C's CHR-RAM overlay - are a small
            // minority, but 8 KiB per cart is cheap enough that sizing it per board is not worth
            // the coupling of knowing the mapper before the memory exists.
            ex_ram: 8 * 1024,
        });
        memory.fill_ram(ram_state);
        memory
    }

    /// Load `Cart` from a ROM path.
    ///
    /// # Errors
    ///
    /// If the NES header is corrupted, the ROM file cannot be read, or the data does not match
    /// the header, then an error is returned.
    pub fn from_path<P: AsRef<Path>>(path: P, ram_state: RamState) -> Result<Self> {
        let path = path.as_ref();
        Self::from_rom(path.to_string_lossy(), &mut Self::open(path)?, ram_state)
    }

    /// Load a cart from a path *without* selecting a board. See [`Cart::from_rom_unmapped`].
    ///
    /// # Errors
    ///
    /// If the ROM cannot be read, its header is invalid, or the data does not match the header.
    pub fn from_path_unmapped<P: AsRef<Path>>(path: P, ram_state: RamState) -> Result<Self> {
        let path = path.as_ref();
        Self::from_rom_unmapped(path.to_string_lossy(), &mut Self::open(path)?, ram_state)
    }

    fn open(path: &Path) -> Result<BufReader<File>> {
        Ok(BufReader::new(File::open(path).map_err(|err| {
            Error::io(err, format!("failed to open rom {path:?}"))
        })?))
    }

    /// Load `Cart` from ROM data, selecting the board its mapper number calls for.
    ///
    /// Accepts both iNES and NES 2.0 headers. Most callers want
    /// [`ControlDeck::load_rom`](crate::control_deck::ControlDeck::load_rom) instead, which does
    /// this and installs the cart; reach for this to inspect a ROM without running it.
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use tetanes_core::{cart::Cart, memory::RamState};
    ///
    /// let rom = std::fs::read("some_awesome_game.nes")?;
    /// let cart = Cart::from_rom("some_awesome_game", &mut rom.as_slice(), RamState::Random)?;
    ///
    /// println!("mapper {} ({})", cart.mapper_num(), cart.mapper_board());
    /// println!("{} PRG-ROM bytes, battery: {}", cart.prg_rom().len(), cart.battery_backed());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// If the NES header is invalid, the ROM data does not match the header, or no board
    /// implements the cart's mapper, then an error is returned.
    pub fn from_rom<S, F>(name: S, rom_data: &mut F, ram_state: RamState) -> Result<Self>
    where
        S: ToString,
        F: Read,
    {
        let mut cart = Self::from_rom_unmapped(name, rom_data, ram_state)?;
        // Which board each mapper number selects lives with the boards themselves, in `mapper.rs`'s
        // `boards!` table, so that adding one does not mean editing this file too.
        cart.mapper = Mapper::from_cart(&mut cart)?;
        info!("loaded ROM `{cart}`");
        debug!("{cart:?}");
        Ok(cart)
    }

    /// Load a cart's ROM and metadata *without* selecting a board.
    ///
    /// The result holds [`Mapper::none`] and cannot be run. This exists for the tools that survey
    /// ROMs — `list_boards`, and `generate_db`, which builds the shipped CRC database — and which
    /// must not drop a cart merely because no board implements its mapper yet. Everything they read
    /// (`mapper_num`, `mapper_board`, `prg_rom`, mirroring, battery) comes from the header and the
    /// ROM itself, none of it from the board.
    ///
    /// # Errors
    ///
    /// If the NES header is invalid, or the ROM data does not match the header.
    pub fn from_rom_unmapped<S, F>(
        name: S,
        mut rom_data: &mut F,
        ram_state: RamState,
    ) -> Result<Self>
    where
        S: ToString,
        F: Read,
    {
        let name = name.to_string();
        let header = NesHeader::load(&mut rom_data)?;
        debug!("{header:?}");

        // Read into exactly-sized buffers first: the CRC lookup that picks the board needs the ROM
        // contents, and the board decides how much PRG-RAM to allocate, so the arena cannot be
        // built until afterwards.
        let prg_rom_size = (header.prg_rom_banks as usize) * PRG_ROM_BANK_SIZE;
        let mut prg_rom = vec![0u8; prg_rom_size];
        rom_data.read_exact(&mut prg_rom).map_err(|err| {
            if let std::io::ErrorKind::UnexpectedEof = err.kind() {
                Error::InvalidHeader {
                    byte: 4,
                    value: header.prg_rom_banks as u8,
                    message: format!(
                        "expected `{}` prg-rom banks ({prg_rom_size} total bytes)",
                        header.prg_rom_banks
                    ),
                }
            } else {
                Error::io(err, "failed to read prg-rom")
            }
        })?;

        let prg_ram_size = Self::calculate_ram_size(header.prg_ram_shift, 10)?;

        let chr_rom_size = (header.chr_rom_banks as usize) * CHR_ROM_BANK_SIZE;
        let mut chr_rom = vec![0u8; chr_rom_size];
        if chr_rom_size > 0 {
            rom_data.read_exact(&mut chr_rom).map_err(|err| {
                if let std::io::ErrorKind::UnexpectedEof = err.kind() {
                    Error::InvalidHeader {
                        byte: 5,
                        value: header.chr_rom_banks as u8,
                        message: format!(
                            "expected `{}` chr-rom banks ({prg_rom_size} total bytes)",
                            header.chr_rom_banks
                        ),
                    }
                } else {
                    Error::io(err, "failed to read chr-rom")
                }
            })?;
        }

        let chr_ram_size = if chr_rom_size > 0 {
            0
        } else {
            Self::calculate_ram_size(header.chr_ram_shift, 11)?
        };

        let crc32 = Self::rom_crc32(&prg_rom, &chr_rom);
        // Deliberately does not overwrite `header.mapper_num`: the header records what the ROM
        // itself claims, and `Cart::mapper_num` layers the database on top. Clobbering it made the
        // database self-referential, since `generate_db` could then only ever read back its own
        // previous answer.
        let game_info = Self::lookup_info(crc32);
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

        let mapper_num = game_info
            .as_ref()
            .map_or(header.mapper_num, |info| info.mapper_num);
        let mut memory = Self::build_memory(
            mapper_num,
            prg_rom_size,
            prg_ram_size.max(mapper::min_prg_ram(mapper_num)),
            chr_rom_size,
            chr_ram_size,
            header.flags & 0x08 == 0x08,
            ram_state,
        );
        memory.region_mut(Src::PrgRom)[..prg_rom_size].copy_from_slice(&prg_rom);
        if chr_rom_size > 0 {
            memory.region_mut(Src::Chr)[..chr_rom_size].copy_from_slice(&chr_rom);
        }
        memory.set_rom_crc32(crc32);

        let mut cart = Self {
            name,
            header,
            region,
            ram_state,
            mapper: Mapper::none(),
            memory,
            chr_rom_size,
            chr_ram_size,
            prg_rom_size,
            prg_ram_size,
            game_info,
        };
        // Header mirroring is the default for every board; only boards that override it - either
        // hard-wired or via a register - touch it again.
        cart.memory.set_mirroring(cart.mirroring());
        // The arena outlives the `Cart` - `Bus::load_cart` keeps only it and the board - so what
        // the console still needs to know about the cart travels with it.
        cart.memory.set_battery_backed(cart.battery_backed());
        Ok(cart)
    }

    /// The ROM's name.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the header is iNES rather than NES 2.0.
    #[must_use]
    pub const fn is_ines(&self) -> bool {
        matches!(
            self.header.variant,
            NesVariant::ArchaicINes | NesVariant::INes07 | NesVariant::INes
        )
    }

    /// Whether the header is NES 2.0.
    #[must_use]
    pub const fn is_nes2(&self) -> bool {
        matches!(self.header.variant, NesVariant::Nes2)
    }

    /// Returns whether this cartridge has battery-backed Save RAM.
    #[must_use]
    pub const fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
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

    /// The cart's PRG-ROM, exactly as the file contained it.
    ///
    /// [`Cart::memory`] pads each region out to whole pages, so this trims that padding back off
    /// for the callers that need the ROM itself - CRC lookups and the debugger.
    #[must_use]
    pub fn prg_rom(&self) -> &[u8] {
        &self.memory.region_ref(Src::PrgRom)[..self.prg_rom_size]
    }

    /// The cart's CHR-ROM, exactly as the file contained it. Empty for a CHR-RAM cart.
    #[must_use]
    pub fn chr_rom(&self) -> &[u8] {
        &self.memory.region_ref(Src::Chr)[..self.chr_rom_size]
    }

    /// Size of the cart's CHR region, or zero when no board is loaded.
    pub fn chr_size(&self) -> usize {
        if self.mapper.is_none() {
            0
        } else {
            self.memory.region_ref(Src::Chr).len()
        }
    }

    /// Total RAM a NES 2.0 size byte asks for, volatile plus battery-backed.
    ///
    /// Bytes 10 and 11 are each **two** nibbles - the low one volatile RAM, the high one
    /// battery-backed NVRAM - and each nibble is a shift, where the size is `64 << shift` and 0
    /// means none. Reading the byte whole instead made a cart declaring 8 KiB of PRG-NVRAM ask for
    /// `64 << 0x70`, which overflowed and was reported as a corrupt header, so **every NES 2.0
    /// cart with a battery save was rejected outright**.
    ///
    /// The two are summed because `Memory` gives a board one region per kind rather than one per
    /// volatility. No ROM in a 2722-cart library declares both halves - none is even NES 2.0 - so
    /// this is the conservative reading rather than a tested one.
    fn calculate_ram_size(byte: u8, header_byte: u8) -> Result<usize> {
        let shift_size = |shift: u8| {
            // `0xF` is reserved rather than a size, and is rejected while parsing the header.
            64usize
                .checked_shl(shift.into())
                .ok_or_else(|| Error::InvalidHeader {
                    byte: header_byte,
                    value: byte,
                    message: format!("invalid ram size shift `{shift}` in header"),
                })
        };

        let volatile = match byte & 0x0F {
            0 => 0,
            shift => shift_size(shift)?,
        };
        let non_volatile = match byte >> 4 {
            0 => 0,
            shift => shift_size(shift)?,
        };

        Ok(volatile + non_volatile)
    }

    /// CRC32 of the ROM itself: PRG-ROM, plus CHR-ROM when the cart has any.
    ///
    /// It identifies the game. The bundled database is keyed by it, and [`Cart::memory`] carries
    /// it so that a save state - which holds no ROM of its own - can be refused when it belongs
    /// to another game.
    fn rom_crc32(prg_rom: &[u8], chr_rom: &[u8]) -> u32 {
        let crc32 = fs::compute_crc32(prg_rom);
        if chr_rom.is_empty() {
            crc32
        } else {
            fs::compute_combine_crc32(crc32, chr_rom)
        }
    }

    fn lookup_info(crc32: u32) -> Option<GameInfo> {
        const GAME_DB: &[u8] = include_bytes!("../game_db.dat");

        let Ok(games) = fs::load_version::<Vec<GameInfo>>(GAME_DB, fs::GAME_DB_VERSION) else {
            error!("failed to load `game_db.dat`");
            return None;
        };

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

    /// Sets the region this cart runs in.
    pub const fn set_region(&mut self, region: NesRegion) {
        self.region = region;
    }
}

impl std::fmt::Display for Cart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(
            f,
            "{} - {}, CHR-ROM: {}K, CHR-RAM: {}K, PRG-ROM: {}K, PRG-RAM: {}K, Mirroring: {:?}, Battery: {}",
            self.name,
            self.mapper_board(),
            self.chr_rom_size / 0x0400,
            self.chr_ram_size / 0x0400,
            self.prg_rom_size / 0x0400,
            self.prg_ram_size / 0x0400,
            self.mirroring(),
            self.battery_backed(),
        )
    }
}

/// What the bundled game database knows about a ROM, matched by CRC32.
///
/// It exists because a fair number of ROMs carry a header that is wrong about their board, region
/// or submapper; a database entry overrides the header.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct GameInfo {
    /// CRC32 of the ROM's PRG and CHR data, which is what the database is keyed by.
    pub crc32: u32,
    /// The region the database says this ROM runs in.
    pub region: NesRegion,
    /// The mapper number the database says this ROM uses, overriding a wrong header.
    pub mapper_num: u16,
    /// The submapper number the database says this ROM uses.
    pub submapper_num: u8,
}

/// Which cartridge header format a ROM carries.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum NesVariant {
    /// The original iNES header, before byte 7 was assigned any meaning.
    #[default]
    ArchaicINes,
    /// iNES 0.7, whose byte 7 holds a mapper high nibble but no padding requirement.
    INes07,
    /// iNES, i.e. the format with bytes 8-15 required to be zero.
    INes,
    /// NES 2.0, which adds submappers, sized RAM fields and a region field.
    Nes2,
}

/// An `iNES` or `NES 2.0` formatted header representing hardware specs of a given NES cartridge.
///
/// <https://wiki.nesdev.org/w/index.php/INES>
/// <https://wiki.nesdev.org/w/index.php/NES_2.0>
/// <https://nesdev.org/NESDoc.pdf> (page 28)
#[derive(Default, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct NesHeader {
    /// Which of the four header formats this is.
    pub variant: NesVariant,
    /// The primary mapper number.
    pub mapper_num: u16,
    /// NES 2.0 submapper number.
    ///
    /// <https://wiki.nesdev.org/w/index.php/NES_2.0_submappers>
    pub submapper_num: u8,
    /// Mirroring, battery, trainer, VS Unisystem, PlayChoice-10 and the NES 2.0 marker.
    pub flags: u8,
    /// Number of 16K PRG-ROM banks.
    pub prg_rom_banks: u16,
    /// Number of 8K CHR-ROM banks.
    pub chr_rom_banks: u16,
    /// NES 2.0 PRG-RAM size, as a shift count.
    pub prg_ram_shift: u8,
    /// NES 2.0 CHR-RAM size, as a shift count.
    pub chr_ram_shift: u8,
    /// NES 2.0 NTSC/PAL indicator.
    pub tv_mode: u8,
    /// NES 2.0 VS System data.
    pub vs_data: u8,
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
            if chr_ram_shift & 0xF0 != 0 {
                // The high nibble alone, not `0xF0`, which the reserved-value check just above
                // already rejects. A `.sram` holds `Memory::sram` - PRG-RAM plus whatever the
                // board stages after it - and CHR-RAM is in neither, so a battery on it is not
                // persisted. Letting the cart run and saying so beats refusing to load it.
                warn!("battery-backed chr-ram is not persisted between sessions");
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
                            "unrecognized data found at header byte {i}. repair and try again"
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

    /// The board name conventionally associated with a mapper number, for display.
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
            .field("mapper_num", &format_args!("{:03}", self.mapper_num))
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

    /// A minimal NES 2.0 ROM: `ram_byte` is header byte 10, the PRG-RAM/PRG-NVRAM pair.
    fn nes2_rom(ram_byte: u8) -> Vec<u8> {
        let mut rom = vec![
            0x4E, 0x45, 0x53, 0x1A, // NES\x1a
            0x01, // 1 x 16K PRG-ROM
            0x01, // 1 x 8K CHR-ROM
            0x02, // mapper 0, battery-backed
            0x08, // NES 2.0
            0x00, 0x00, ram_byte, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        rom.resize(rom.len() + PRG_ROM_BANK_SIZE + CHR_ROM_BANK_SIZE, 0);
        rom
    }

    /// Header byte 10 is two nibbles - PRG-RAM low, battery-backed PRG-NVRAM high - and each has
    /// to be shifted on its own. Reading the byte whole asks for `64 << 0x70` on a cart with an
    /// 8 KiB battery save, which overflows and reads as a corrupt header, rejecting every NES 2.0
    /// cart that has a save.
    #[test]
    fn nes2_prg_nvram_is_the_high_nibble_of_byte_10() {
        let cases = [
            (0x00, 0),           // neither
            (0x07, 8 * 1024),    // 8K volatile PRG-RAM
            (0x70, 8 * 1024),    // 8K battery-backed, the high nibble on its own
            (0x77, 16 * 1024),   // both, summed
            (0x0E, 1024 * 1024), // the largest shift that is not the reserved value
        ];

        for (byte, expected) in cases {
            let cart = Cart::from_rom(
                "test.nes",
                &mut nes2_rom(byte).as_slice(),
                RamState::AllZeros,
            )
            .unwrap_or_else(|err| panic!("byte 10 = {byte:#04X} should load: {err}"));
            assert_eq!(cart.prg_ram_size, expected, "byte 10 = {byte:#04X}",);
        }
    }

    /// `0xF` is reserved in either nibble rather than a shift.
    #[test]
    fn nes2_rejects_the_reserved_ram_shift() {
        for byte in [0x0F, 0xF0] {
            let err = Cart::from_rom(
                "test.nes",
                &mut nes2_rom(byte).as_slice(),
                RamState::AllZeros,
            )
            .expect_err("reserved shift should be rejected");
            assert!(
                matches!(err, Error::InvalidHeader { byte: 10, .. }),
                "byte 10 = {byte:#04X} gave {err}"
            );
        }
    }
}
