use crate::{
    common::{NesRegion, Regional},
    mapper::{
        m024_m026_vrc6::Vrc6Revision, Axrom, Bf909x, Cnrom, Exrom, Gxrom, Mapper, Mmc1Revision,
        Nrom, Pxrom, Sxrom, Txrom, Uxrom, Vrc6,
    },
    mem::RamState,
    ppu::Mirroring,
    NesResult,
};
use anyhow::{anyhow, bail, Context};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

const PRG_ROM_BANK_SIZE: usize = 0x4000;
const CHR_ROM_BANK_SIZE: usize = 0x2000;

#[cfg(not(target_arch = "wasm32"))]
const GAME_DB: &[u8] = include_bytes!("../config/game_database.txt");

/// An NES cartridge.
#[derive(Default, Clone)]
#[must_use]
pub struct Cart {
    name: String,
    header: NesHeader,
    region: NesRegion,
    ram_state: RamState,
    pub(crate) mapper: Mapper,
    pub(crate) chr_rom: Vec<u8>, // Character ROM
    pub(crate) chr_ram: Vec<u8>, // Character RAM
    pub(crate) ex_ram: Vec<u8>,  // Internal Extra RAM
    pub(crate) prg_rom: Vec<u8>, // Program ROM
    pub(crate) prg_ram: Vec<u8>, // Program RAM
}

impl Cart {
    /// Load `Cart` from a ROM path.
    ///
    /// # Errors
    ///
    /// If the NES header is corrupted, the ROM file cannot be read, or the data does not match
    /// the header, then an error is returned.
    pub fn from_path<P: AsRef<Path>>(path: P, ram_state: RamState) -> NesResult<Self> {
        let path = path.as_ref();
        let mut rom = BufReader::new(
            File::open(path).with_context(|| format!("failed to open rom {:?}", path))?,
        );
        Self::from_rom(&path.to_string_lossy(), &mut rom, ram_state)
    }

    /// Load `Cart` from ROM data.
    ///
    /// # Errors
    ///
    /// If the NES header is invalid, or the ROM data does not match the header, then an error is
    /// returned.
    pub fn from_rom<S, F>(name: S, mut rom_data: &mut F, ram_state: RamState) -> NesResult<Self>
    where
        S: ToString,
        F: Read,
    {
        let name = name.to_string();
        let header = NesHeader::load(&mut rom_data)?;

        let mut prg_rom = vec![0x00; (header.prg_rom_banks as usize) * PRG_ROM_BANK_SIZE];
        rom_data.read_exact(&mut prg_rom).with_context(|| {
            let bytes_rem = rom_data
                .read_to_end(&mut prg_rom)
                .map_or_else(|_| "unknown".to_string(), |rem| rem.to_string());
            format!(
                "invalid rom header '{}'. prg-rom banks: {}. bytes remaining: {}",
                name, header.prg_rom_banks, bytes_rem
            )
        })?;

        let prg_ram_size = Self::calculate_ram_size(header.prg_ram_shift).context("prg_ram")?;
        let mut prg_ram = vec![0x00; prg_ram_size];
        RamState::fill(&mut prg_ram, ram_state);

        let mut chr_rom = vec![0x00; (header.chr_rom_banks as usize) * CHR_ROM_BANK_SIZE];
        rom_data.read_exact(&mut chr_rom).with_context(|| {
            let bytes_rem = rom_data
                .read_to_end(&mut chr_rom)
                .map_or_else(|_| "unknown".to_string(), |rem| rem.to_string());
            format!(
                "invalid rom header \"{}\". chr-rom banks: {}. bytes remaining: {}",
                name, header.chr_rom_banks, bytes_rem,
            )
        })?;

        let mut chr_ram = vec![];
        if chr_rom.is_empty() {
            let chr_ram_size = Self::calculate_ram_size(header.chr_ram_shift).context("chr_ram")?;
            chr_ram.resize(chr_ram_size, 0x00);
            RamState::fill(&mut chr_ram, ram_state);
        }

        #[cfg(not(target_arch = "wasm32"))]
        let region = {
            let mut hasher = DefaultHasher::new();
            prg_rom.hash(&mut hasher);
            let hash = hasher.finish();
            Self::lookup_region(hash)
        };
        #[cfg(target_arch = "wasm32")]
        let region = NesRegion::default();

        let mut cart = Self {
            name,
            header,
            region,
            ram_state,
            mapper: Mapper::none(),
            chr_rom,
            chr_ram,
            ex_ram: vec![],
            prg_rom,
            prg_ram,
        };
        cart.load_mapper()?;

        log::info!("Loaded `{}`", cart);
        log::debug!("{:?}", cart);
        Ok(cart)
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    #[must_use]
    pub fn chr_rom(&self) -> &[u8] {
        &self.chr_rom
    }

    #[inline]
    #[must_use]
    pub fn chr_ram(&self) -> &[u8] {
        &self.chr_ram
    }

    #[inline]
    #[must_use]
    pub fn prg_rom(&self) -> &[u8] {
        &self.prg_rom
    }

    #[inline]
    #[must_use]
    pub fn prg_ram(&self) -> &[u8] {
        &self.prg_ram
    }

    #[inline]
    #[must_use]
    pub fn has_chr_rom(&self) -> bool {
        !self.chr_rom.is_empty()
    }

    #[inline]
    #[must_use]
    pub fn has_prg_ram(&self) -> bool {
        !self.prg_ram.is_empty()
    }

    /// Returns whether this cartridge has battery-backed Save RAM.
    #[inline]
    #[must_use]
    pub const fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
    }

    /// Returns `RamState`.
    #[inline]
    pub const fn ram_state(&self) -> RamState {
        self.ram_state
    }

    /// Returns hardware configured `Mirroring`.
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

    /// Returns the Mapper number for this Cart.
    #[inline]
    #[must_use]
    pub const fn mapper_num(&self) -> u16 {
        self.header.mapper_num
    }

    /// Returns the Sub-Mapper number for this Cart.
    #[inline]
    #[must_use]
    pub const fn submapper_num(&self) -> u8 {
        self.header.submapper_num
    }

    /// Returns the Mapper and Board name for this Cart.
    #[inline]
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
    pub(crate) fn add_ex_ram(&mut self, capacity: usize) {
        self.ex_ram.resize(capacity, 0x00);
        RamState::fill(&mut self.ex_ram, self.ram_state);
    }

    fn load_mapper(&mut self) -> NesResult<()> {
        self.mapper = match self.header.mapper_num {
            0 => Nrom::load(self),
            1 => Sxrom::load(self, Mmc1Revision::BC),
            2 => Uxrom::load(self),
            3 => Cnrom::load(self),
            4 => Txrom::load(self),
            5 => Exrom::load(self),
            7 => Axrom::load(self),
            9 => Pxrom::load(self),
            24 => Vrc6::load(self, Vrc6Revision::A),
            26 => Vrc6::load(self, Vrc6Revision::B),
            66 => Gxrom::load(self),
            71 => Bf909x::load(self),
            155 => Sxrom::load(self, Mmc1Revision::A),
            _ => bail!("unimplemented mapper: {}", self.header.mapper_num),
        };
        Ok(())
    }

    fn calculate_ram_size(value: u8) -> NesResult<usize> {
        if value > 0 {
            64usize
                .checked_shl(value.into())
                .ok_or_else(|| anyhow!("invalid header ram size: ${:02X}", value))
        } else {
            Ok(0)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn lookup_region(lookup_hash: u64) -> NesRegion {
        use std::io::BufRead;

        let db = BufReader::new(GAME_DB);
        let lines: Vec<String> = db.lines().filter_map(Result::ok).collect();
        if let Ok(line) = lines.binary_search_by(|line| {
            let hash = line
                .split(',')
                .next()
                .map(|hash| hash.parse::<u64>().unwrap_or_default())
                .unwrap_or_default();
            hash.cmp(&lookup_hash)
        }) {
            let mut fields = lines[line].split(',').skip(1);
            if let Some(region) = fields.next() {
                return NesRegion::try_from(region).unwrap_or_default();
            }
        }
        NesRegion::default()
    }
}

impl Regional for Cart {
    #[inline]
    fn region(&self) -> NesRegion {
        self.region
    }

    #[inline]
    fn set_region(&mut self, region: NesRegion) {
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
            self.chr_rom.len() / 0x0400,
            self.chr_ram.len() / 0x0400,
            self.prg_rom.len() / 0x0400,
            self.prg_ram.len() / 0x0400,
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
            .field("ex_ram_len", &self.ex_ram.len())
            .field("prg_rom_len", &self.prg_rom.len())
            .field("prg_ram_len", &self.prg_ram.len())
            .finish()
    }
}

/// An `iNES` or `NES 2.0` formatted header representing hardware specs of a given NES cartridge.
///
/// <http://wiki.nesdev.com/w/index.php/INES>
/// <http://wiki.nesdev.com/w/index.php/NES_2.0>
/// <http://nesdev.com/NESDoc.pdf> (page 28)
#[derive(Default, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct NesHeader {
    pub version: u8,        // 1 for iNES or 2 for NES 2.0
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
    pub fn from_path<P: AsRef<Path>>(path: P) -> NesResult<Self> {
        let path = path.as_ref();
        let mut rom = BufReader::new(
            File::open(path).with_context(|| format!("failed to open rom {:?}", path))?,
        );
        Self::load(&mut rom)
    }

    /// Load `NesHeader` from ROM data.
    ///
    /// # Errors
    ///
    /// If the NES header is invalid, then an error is returned.
    pub fn load<F: Read>(rom_data: &mut F) -> NesResult<Self> {
        let mut header = [0u8; 16];
        rom_data.read_exact(&mut header)?;

        // Header checks
        if header[0..4] != *b"NES\x1a" {
            bail!("nes header signature not found");
        } else if (header[7] & 0x0C) == 0x04 {
            bail!("header is corrupted by `DiskDude!`. repair and try again");
        } else if (header[7] & 0x0C) == 0x0C {
            bail!("unrecognized header format. repair and try again");
        }

        let mut prg_rom_banks = u16::from(header[4]);
        let mut chr_rom_banks = u16::from(header[5]);
        // Upper 4 bits of flags 6 = D0..D3 and 7 = D4..D7
        let mut mapper_num = u16::from(((header[6] & 0xF0) >> 4) | (header[7] & 0xF0));
        // Lower 4 bits of flag 6 = D0..D3, upper 4 bits of flag 7 = D4..D7
        let flags = (header[6] & 0x0F) | ((header[7] & 0x0F) << 4);

        // NES 2.0 Format
        let mut version = 1; // Start off checking for iNES format
        let mut submapper_num = 0;
        let mut prg_ram_shift = 0;
        let mut chr_ram_shift = 0;
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
            prg_rom_banks |= u16::from(header[9] & 0x0F) << 8;
            // upper 4 bits of flag 9 = D8..D11 of chr_rom_size
            chr_rom_banks |= u16::from(header[9] & 0xF0) << 4;
            prg_ram_shift = header[10];
            chr_ram_shift = header[11];
            tv_mode = header[12];
            vs_data = header[13];

            if prg_ram_shift & 0x0F == 0x0F || prg_ram_shift & 0xF0 == 0xF0 {
                bail!("invalid prg-ram size in header");
            } else if chr_ram_shift & 0x0F == 0x0F || chr_ram_shift & 0xF0 == 0xF0 {
                bail!("invalid chr-ram size in header");
            } else if chr_ram_shift & 0xF0 == 0xF0 {
                bail!("battery-backed chr-ram is currently not supported");
            } else if header[14] > 0 || header[15] > 0 {
                bail!("unrecognized data found at header offsets 14-15");
            }
        } else {
            for (i, header) in header.iter().enumerate().take(16).skip(8) {
                if *header > 0 {
                    bail!(
                        "unrecogonized data found at header offset {}. repair and try again",
                        i,
                    );
                }
            }
        }

        // Trainer
        if flags & 0x04 == 0x04 {
            bail!("trained roms are currently not supported.");
        }

        Ok(Self {
            version,
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
            24 => "Mapper 024 - Vrc6a",
            26 => "Mapper 026 - Vrc6b",
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
            .field("version", &self.version)
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
                version: 1,
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
                version: 1,
                mapper_num: 1,
                flags: 0b0000_0000,
                prg_rom_banks: 8,
                chr_rom_banks: 0,
                ..NesHeader::default()
            },
        ),
    );
}
