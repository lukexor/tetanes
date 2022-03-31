//! Handles reading NES Cart headers and ROMs

use crate::{
    common::{Clocked, Powered},
    mapper::{
        m001_sxrom::Mmc1, Axrom, Bf909x, Cnrom, Empty, Exrom, Gxrom, MapRead, MapWrite, Mapped,
        MappedRead, MappedWrite, Mapper, MirroringType, Nrom, Pxrom, Sxrom, Txrom, Uxrom,
    },
    memory::{MemRead, MemWrite, Memory, RamState},
    ppu::Mirroring,
    NesResult,
};
use anyhow::{anyhow, Context};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::File,
    io::{BufReader, Read, Write},
    path::Path,
};

const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
const CHR_ROM_BANK_SIZE: usize = 8 * 1024;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum RomSize {
    S128, // 128 kilobits - not kilobytes
    S256,
    S512,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum ChrMode {
    Rom,
    Ram,
}

/// Represents an `iNES` or `NES 2.0` header
///
/// <http://wiki.nesdev.com/w/index.php/INES>
/// <http://wiki.nesdev.com/w/index.php/NES_2.0>
/// <http://nesdev.com/NESDoc.pdf> (page 28)
#[derive(Default, Copy, Clone)]
#[must_use]
pub struct NesHeader {
    pub version: u8,        // 1 for iNES or 2 for NES 2.0
    pub mapper_num: u16,    // The primary mapper number
    pub submapper_num: u8,  // NES 2.0 https://wiki.nesdev.com/w/index.php/NES_2.0_submappers
    pub flags: u8,          // Mirroring, Battery, Trainer, VS Unisystem, Playchoice-10, NES 2.0
    pub prg_rom_banks: u16, // Number of 16 KB PRG-ROM banks (Program ROM)
    pub chr_rom_banks: u16, // Number of 8 KB CHR-ROM banks (Character ROM)
    pub prg_ram_shift: u8,  // NES 2.0 PRG-RAM
    pub chr_ram_shift: u8,  // NES 2.0 CHR-RAM
    pub tv_mode: u8,        // NES 2.0 NTSC/PAL indicator
    pub vs_data: u8,        // NES 2.0 VS System data
}

/// Represents an NES Cart
#[derive(Default, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Cart {
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub header: NesHeader,
    #[serde(skip)]
    pub ram_state: RamState,
    #[serde(skip)]
    pub mirroring: Mirroring,
    #[serde(skip)]
    pub prg_rom: Memory, // Program ROM
    pub prg_ram: Memory, // Program RAM
    #[serde(skip)]
    pub chr: Memory, // Character ROM/RAM
    pub mapper: Mapper,
    pub open_bus: u8,
}

impl Cart {
    /// Creates an empty cartridge not loaded with any ROM
    #[inline]
    pub fn new() -> Self {
        Self {
            name: String::new(),
            ram_state: RamState::Random,
            header: NesHeader::new(),
            mirroring: Mirroring::default(),
            prg_rom: Memory::new(),
            prg_ram: Memory::new(),
            chr: Memory::new(),
            mapper: Empty.into(),
            open_bus: 0x00,
        }
    }

    /// Create a `Cart` from a ROM path.
    ///
    /// # Errors
    ///
    /// If the ROM can not be opened, or the NES header is corrupted, then an error is returned.
    #[inline]
    pub fn from_path<P: AsRef<Path>>(path: P) -> NesResult<Self> {
        let path = path.as_ref();
        let rom = File::open(path).with_context(|| format!("failed to open rom {:?}", path))?;
        let mut rom = BufReader::new(rom);
        Self::from_rom(&path.to_string_lossy(), &mut rom, RamState::AllZeros)
    }

    /// Creates a new Cart instance by reading in a `.nes` file
    ///
    /// # Arguments
    ///
    /// * `rom` - A String that that holds the path to a valid '.nes' file
    ///
    /// # Errors
    ///
    /// If the file is not a valid '.nes' file, or there are insufficient permissions to read the
    /// file, then an error is returned.
    pub fn from_rom<S, F>(name: &S, mut rom_data: &mut F, ram_state: RamState) -> NesResult<Self>
    where
        S: ToString,
        F: Read,
    {
        let name = name.to_string();
        let header = NesHeader::load(&mut rom_data)?;
        let prg_ram_size = Self::calculate_ram_size("prg", header.prg_ram_shift)?;
        let chr_ram_size = Self::calculate_ram_size("chr", header.chr_ram_shift)?;

        let mut prg_data = vec![0x00; (header.prg_rom_banks as usize) * PRG_ROM_BANK_SIZE];
        rom_data.read_exact(&mut prg_data).with_context(|| {
            let bytes_rem = rom_data
                .read_to_end(&mut prg_data)
                .map_or_else(|_| "unknown".to_string(), |rem| rem.to_string());
            format!(
                "invalid rom header \"{}\". prg-rom banks: {}. bytes remaining: {}",
                name, header.prg_rom_banks, bytes_rem
            )
        })?;
        let prg_rom = Memory::rom(prg_data);

        let mut chr_data = vec![0x00; (header.chr_rom_banks as usize) * CHR_ROM_BANK_SIZE];
        rom_data.read_exact(&mut chr_data).with_context(|| {
            let bytes_rem = rom_data
                .read_to_end(&mut chr_data)
                .map_or_else(|_| "unknown".to_string(), |rem| rem.to_string());
            format!(
                "invalid rom header \"{}\". chr-rom banks: {}. bytes remaining: {}",
                name, header.chr_rom_banks, bytes_rem,
            )
        })?;

        let chr = if chr_data.is_empty() {
            Memory::ram(chr_ram_size, ram_state)
        } else {
            Memory::rom(chr_data)
        };

        let mirroring = if header.flags & 0x08 == 0x08 {
            Mirroring::FourScreen
        } else {
            match header.flags & 0x01 {
                0 => Mirroring::Horizontal,
                1 => Mirroring::Vertical,
                _ => unreachable!("impossible mirroring"),
            }
        };

        let mut cart = Self {
            name,
            header,
            ram_state,
            mirroring,
            prg_rom,
            prg_ram: Memory::ram(prg_ram_size, ram_state),
            chr,
            mapper: Mapper::default(),
            open_bus: 0x00,
        };
        cart.mapper = match header.mapper_num {
            0 => Nrom::load(&mut cart),
            1 => Sxrom::load(&mut cart, Mmc1::BC),
            2 => Uxrom::load(&mut cart),
            3 => Cnrom::load(&mut cart),
            4 => Txrom::load(&mut cart),
            5 => Exrom::load(&mut cart),
            7 => Axrom::load(&mut cart),
            9 => Pxrom::load(&mut cart),
            66 => Gxrom::load(&mut cart),
            71 => Bf909x::load(&mut cart),
            155 => Sxrom::load(&mut cart, Mmc1::A),
            _ => return Err(anyhow!("unsupported mapper number: {}", header.mapper_num)),
        };

        info!("Loaded `{}`", cart);
        debug!("{:?}", cart);
        Ok(cart)
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The nametable mirroring mode defined in the header
    #[inline]
    pub fn mirroring(&self) -> Mirroring {
        match self.mapper.mirroring() {
            MirroringType::Hardware => self.mirroring,
            MirroringType::Software(mirroring) => mirroring,
        }
    }

    /// Save battery-backed RAM to disk.
    ///
    /// # Errors
    ///
    /// If any of the bytes in save RAM can't be saved, an error is returned.
    #[inline]
    pub fn save_sram<F: Write>(&self, f: &mut F) -> NesResult<()> {
        if self.battery_backed() {
            f.write_all(&self.prg_ram)?;
        }
        Ok(())
    }

    /// Load battery-backed RAM from disk.
    ///
    /// # Errors
    ///
    /// If the exact number of bytes in save file can't be read into memory, an error is returned.
    #[inline]
    pub fn load_sram<F: Read>(&mut self, f: &mut F) -> NesResult<()> {
        if self.battery_backed() {
            f.read_exact(&mut self.prg_ram)?;
        }
        Ok(())
    }

    #[inline]
    pub fn bus_read(&mut self, val: u8) {
        self.open_bus = val;
    }

    /// Returns whether this cartridge has battery-backed Save RAM
    #[inline]
    #[must_use]
    pub const fn battery_backed(&self) -> bool {
        self.header.flags & 0x02 == 0x02
    }

    #[must_use]
    pub const fn mapper_board(&self) -> &'static str {
        self.header.mapper_board()
    }
}

impl Cart {
    #[inline]
    fn calculate_ram_size(ram_type: &str, value: u8) -> NesResult<usize> {
        if value > 0 {
            64usize
                .checked_shl(value.into())
                .ok_or_else(|| anyhow!("invalid header {}-ram size: ${:02X}", ram_type, value))
        } else {
            Ok(0)
        }
    }
}

impl Mapped for Cart {
    #[inline]
    fn irq_pending(&self) -> bool {
        self.mapper.irq_pending()
    }

    #[inline]
    fn use_ciram(&self, addr: u16) -> bool {
        self.mapper.use_ciram(addr)
    }

    #[inline]
    fn nametable_page(&self, addr: u16) -> u16 {
        self.mapper.nametable_page(addr)
    }

    #[inline]
    fn ppu_addr(&mut self, addr: u16) {
        self.mapper.ppu_addr(addr);
    }

    #[inline]
    fn ppu_read(&mut self, addr: u16) {
        self.mapper.ppu_read(addr);
    }

    #[inline]
    fn ppu_write(&mut self, addr: u16, val: u8) {
        self.mapper.ppu_write(addr, val);
    }
}

impl MemRead for Cart {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        match self.mapper.map_read(addr) {
            MappedRead::Chr(addr) => self.chr.readw(addr),
            MappedRead::PrgRam(addr) => self.prg_ram.readw(addr),
            MappedRead::PrgRom(addr) => self.prg_rom.readw(addr),
            MappedRead::Data(data) => data,
            MappedRead::None => self.open_bus,
        }
    }

    #[inline]
    fn peek(&self, addr: u16) -> u8 {
        match self.mapper.map_peek(addr) {
            MappedRead::Chr(addr) => self.chr.peekw(addr),
            MappedRead::PrgRam(addr) => self.prg_ram.peekw(addr),
            MappedRead::PrgRom(addr) => self.prg_rom.peekw(addr),
            MappedRead::Data(data) => data,
            MappedRead::None => self.open_bus,
        }
    }
}

impl MemWrite for Cart {
    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        match self.mapper.map_write(addr, val) {
            MappedWrite::Chr(addr, val) if self.chr.writable() => self.chr.writew(addr, val),
            MappedWrite::PrgRam(addr, val) if self.prg_ram.writable() => {
                self.prg_ram.writew(addr, val);
            }
            MappedWrite::PrgRamProtect(protect) => self.prg_ram.write_protect(protect),
            _ => (),
        }
    }
}

impl Clocked for Cart {
    fn clock(&mut self) -> usize {
        self.mapper.clock()
    }
}

impl Powered for Cart {
    fn reset(&mut self) {
        self.mapper.reset();
    }
    fn power_cycle(&mut self) {
        self.mapper.power_cycle();
    }
}

impl fmt::Display for Cart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        write!(
            f,
            "{} - {}, CHR-{}: {}K, PRG-ROM: {}K, PRG-RAM: {}K, Mirroring: {:?}, Battery: {}",
            self.name,
            self.mapper_board(),
            if self.chr.writable() { "RAM" } else { "ROM" },
            self.chr.len() / 1024,
            self.prg_rom.len() / 1024,
            self.prg_ram.len() / 1024,
            self.mirroring(),
            self.battery_backed(),
        )
    }
}

impl fmt::Debug for Cart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
        f.debug_struct("Cart")
            .field("name", &self.name)
            .field("header", &self.header)
            .field("mirroring", &self.mirroring())
            .field("battery_backed", &self.battery_backed())
            .field("chr", &self.chr)
            .field("prg_rom", &self.prg_rom)
            .field("prg_ram", &self.prg_ram)
            .field("mapper", &self.mapper)
            .field("open_bus", &format_args!("${:02X}", &self.open_bus))
            .finish()
    }
}

impl NesHeader {
    /// Returns an empty `NesHeader` not loaded with any data
    const fn new() -> Self {
        Self {
            version: 0x01,
            mapper_num: 0x0000,
            submapper_num: 0x00,
            flags: 0x00,
            prg_rom_banks: 0x0000,
            chr_rom_banks: 0x0000,
            prg_ram_shift: 0x00,
            chr_ram_shift: 0x00,
            tv_mode: 0x00,
            vs_data: 0x00,
        }
    }

    /// Parses a slice of `u8` bytes and returns a valid `NesHeader` instance
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
            return Err(anyhow!("iNES header signature not found"));
        } else if (header[7] & 0x0C) == 0x04 {
            return Err(anyhow!(
                "header is corrupted by `DiskDude!`. repair and try again"
            ));
        } else if (header[7] & 0x0C) == 0x0C {
            return Err(anyhow!("unrecognized header format. repair and try again"));
        }

        let mut prg_rom_banks = u16::from(header[4]);
        let mut chr_rom_banks = u16::from(header[5]);
        // Upper 4 bits of flags 6 = D0..D3 and 7 = D4..D7
        let mut mapper_num = u16::from(((header[6] & 0xF0) >> 4) | (header[7] & 0xF0));
        // Lower 4 bits of flag 6 = D0..D3, upper 4 bits of flag 7 = D4..D7
        let flags = (header[6] & 0x0F) | ((header[7] & 0x0F) << 4);

        // NES 2.0 Format
        let mut version = 1; // Start off checking for iNES format v1
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
                return Err(anyhow!("invalid prg-ram size in header"));
            } else if chr_ram_shift & 0x0F == 0x0F || chr_ram_shift & 0xF0 == 0xF0 {
                return Err(anyhow!("invalid chr-ram size in header"));
            } else if chr_ram_shift & 0xF0 == 0xF0 {
                return Err(anyhow!("battery-backed chr-ram is currently not supported"));
            } else if header[14] > 0 || header[15] > 0 {
                return Err(anyhow!("unrecognized data found at header offsets 14-15"));
            }
        } else {
            for (i, header) in header.iter().enumerate().take(16).skip(8) {
                if *header > 0 {
                    return Err(anyhow!(
                        "unrecogonized data found at header offset {}. repair and try again",
                        i,
                    ));
                }
            }
        }

        // Trainer
        if flags & 0x04 == 0x04 {
            return Err(anyhow!("trained roms are currently not supported."));
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
            66 => "Mapper 066 - Gxrom",
            71 => "Mapper 071 - UNROM/CAMERICA/BF909X",
            155 => "Mapper 155 - SxROM/MMC1A",
            _ => "Unsupported Mapper",
        }
    }
}

impl fmt::Debug for NesHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), fmt::Error> {
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
            let c = Cart::from_rom(&data.0, &mut rom, RamState::AllZeros);
            assert!(c.is_ok(), "new cartridge {}", data.0);
            let c = c.unwrap();
            assert_eq!(
                c.header.prg_rom_banks, data.2,
                "PRG-ROM size matches for {}",
                data.0
            );
            assert_eq!(
                c.header.chr_rom_banks, data.3,
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
