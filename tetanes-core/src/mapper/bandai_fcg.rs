//! `Bandai FCG` (Mappers 016, 153, 157, and 159).
//!
//! <https://www.nesdev.org/wiki/INES_Mapper_016>

use crate::{
    cart::Cart,
    common::{Clock, Regional, Reset, Sram},
    fs,
    mapper::{self, Map, Mapper, Mirroring},
    mem::{Banks, Memory},
    ppu::CIRam,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, path::Path};

/// `Bandai FCG` registers.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Regs {
    pub prg_page: u8,
    pub prg_bank_select: u8,
    pub prg_ram_enabled: bool,
    pub chr_regs: [u8; 8],
    pub irq_latch: u8,
    pub irq_counter: u16,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub irq_reload: u16,
}

/// Memory operation.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub enum MemoryOp {
    None,
    Read,
    Write,
    #[default]
    ReadWrite,
}

/// `Bandai FCG` (Mappers 016, 153, 157, and 159).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct BandaiFCG {
    pub chr: Memory<Box<[u8]>>,
    pub prg_rom: Memory<Box<[u8]>>,
    pub prg_ram: Memory<Box<[u8]>>,
    pub regs: Regs,
    pub has_chr_ram: bool,
    pub mirroring: Mirroring,
    pub mapper_num: u16,
    pub submapper_num: u8,
    pub barcode_reader: Option<BarcodeReader>,
    pub standard_eeprom: Option<Eeprom>,
    pub extra_eeprom: Option<Eeprom>,
    pub sram_access: MemoryOp,
    pub reg_access: MemoryOp,
    pub chr_banks: Banks,
    pub prg_rom_banks: Banks,
}

impl BandaiFCG {
    const PRG_WINDOW: usize = 16 * 1024;
    const PRG_RAM_SIZE: usize = 8 * 1024; // Mapper 153
    const CHR_ROM_WINDOW: usize = 1024;
    const CHR_RAM_SIZE: usize = 8 * 1024;

    pub fn load(
        cart: &Cart,
        chr_rom: Memory<Box<[u8]>>,
        prg_rom: Memory<Box<[u8]>>,
    ) -> Result<Mapper, mapper::Error> {
        let (chr, has_chr_ram) = cart.chr_rom_or_ram(chr_rom, Self::CHR_RAM_SIZE);
        let chr_window = if has_chr_ram {
            Self::CHR_RAM_SIZE
        } else {
            Self::CHR_ROM_WINDOW
        };
        let chr_banks = Banks::new(0x0000, 0x1FFF, chr.len(), chr_window)?;
        let prg_rom_banks = Banks::new(0x8000, 0xFFFF, prg_rom.len(), Self::PRG_WINDOW)?;
        let prg_ram = cart.prg_ram_or_default(Self::PRG_RAM_SIZE);
        let mut bandai_fcg = Self {
            chr,
            prg_rom,
            prg_ram,
            regs: Regs::default(),
            has_chr_ram,
            mirroring: cart.mirroring(),
            mapper_num: cart.mapper_num(),
            submapper_num: cart.submapper_num(),
            barcode_reader: None,
            standard_eeprom: None,
            extra_eeprom: None,
            sram_access: MemoryOp::default(),
            reg_access: MemoryOp::Write,
            chr_banks,
            prg_rom_banks,
        };

        // Mapper 157 is used for Datach Joint ROM System boards
        if bandai_fcg.mapper_num == 16 {
            // INES Mapper 016 submapper 4: FCG-1/2 ASIC, no serial EEPROM, banked CHR-ROM
            // INES Mapper 016 submapper 5: LZ93D50 ASIC and no or 256-byte serial EEPROM, banked
            // CHR-ROM

            // Add a 256 byte serial EEPROM (24C02)
            if matches!(bandai_fcg.submapper_num, 0 | 5) && bandai_fcg.prg_ram.len() == 256 {
                // Connect a 256-byte EEPROM for iNES roms, and when submapper 5 + 256 bytes of
                // save ram in header
                bandai_fcg.standard_eeprom = Some(Eeprom::new(EepromModel::X24C02));
            }
        } else if bandai_fcg.mapper_num == 157 {
            bandai_fcg.barcode_reader = Some(BarcodeReader::new());
            // Datach Joint ROM System
            //
            // It contains an internal 256-byte serial EEPROM (24C02) that is shared among all
            // Datach games.
            //
            // One game, Battle Rush: Build up Robot Tournament, has an additional external
            // 128-byte serial EEPROM (24C01) on the game cartridge.
            //
            // The NES 2.0 header's PRG-NVRAM field will only denote whether the game cartridge has
            // an additional 128-byte serial EEPROM
            if !cart.is_nes2() || bandai_fcg.prg_ram.len() == 128 {
                bandai_fcg.extra_eeprom = Some(Eeprom::new(EepromModel::X24C01));
            }

            // All mapper 157 games have an internal 256-byte EEPROM
            bandai_fcg.standard_eeprom = Some(Eeprom::new(EepromModel::X24C02));
        } else if bandai_fcg.mapper_num == 159 {
            // LZ93D50 with 128 byte serial EEPROM (24C01)
            bandai_fcg.standard_eeprom = Some(Eeprom::new(EepromModel::X24C01));
        }

        if bandai_fcg.mapper_num == 16 {
            if matches!(bandai_fcg.submapper_num, 0 | 4) {
                bandai_fcg.reg_access = MemoryOp::Read;
            }
            if matches!(bandai_fcg.submapper_num, 0 | 5) {
                bandai_fcg.sram_access = MemoryOp::Read;
            }
        } else {
            // For iNES Mapper 153 (with SRAM), the writeable ports must only be mirrored across
            // $8000-$FFFF. Mappers 157 and 159 do not need to support the FCG-1 and -2 and so
            // should only mirror the ports across $8000-$FFFF.
            if bandai_fcg.mapper_num == 153 {
                // Mapper 153 has regular save ram from $6000-$7FFF, need to remove the register for both read & writes
                bandai_fcg.sram_access = MemoryOp::None;
            } else {
                bandai_fcg.sram_access = MemoryOp::Read;
            }
        }

        let last_bank = bandai_fcg.prg_rom_banks.last();
        bandai_fcg.prg_rom_banks.set(1, last_bank);

        Ok(bandai_fcg.into())
    }

    fn write_chr_bank(&mut self, addr: u16, val: u8) {
        let bank = usize::from(addr & 0x07);
        self.regs.chr_regs[bank] = val;
        if self.mapper_num == 153 || self.prg_rom_banks.page_count() >= 0x20 {
            self.regs.prg_bank_select = 0;
            for reg in self.regs.chr_regs {
                self.regs.prg_bank_select |= (reg & 0x01) << 4;
            }
            self.prg_rom_banks
                .set(0, (self.regs.prg_page | self.regs.prg_bank_select).into());
            self.prg_rom_banks
                .set(1, 0x0F | usize::from(self.regs.prg_bank_select));
        } else if !self.has_chr_ram && self.mapper_num != 157 {
            self.chr_banks.set(bank, val.into());
        }

        if let Some(eeprom) = &mut self.extra_eeprom {
            if self.mapper_num == 157 && (addr & 0x0F) <= 3 {
                eeprom.write_scl((val >> 3) & 0x01)
            }
        }
    }

    fn write_prg_bank(&mut self, val: u8) {
        self.regs.prg_page = val & 0x0F;
        self.prg_rom_banks
            .set(0, (self.regs.prg_page | self.regs.prg_bank_select).into());
    }

    const fn write_mirroring(&mut self, val: u8) {
        self.mirroring = match val & 0b11 {
            0b00 => Mirroring::Vertical,
            0b01 => Mirroring::Horizontal,
            0b10 => Mirroring::SingleScreenA,
            _ => Mirroring::SingleScreenB,
        };
    }

    const fn write_irq_ctrl(&mut self, val: u8) {
        self.regs.irq_enabled = val & 0x01 == 0x01;

        // Wiki claims there is no reload value, however this seems to be the only way to make
        // Famicom Jump II - Saikyou no 7 Nin work properly
        if self.mapper_num != 16 || !matches!(self.submapper_num, 0 | 4) {
            // On the LZ93D50 (Submapper 5), writing to this register also copies the latch to the
            // actual counter.
            self.regs.irq_counter = self.regs.irq_reload;
        }

        self.regs.irq_pending = false;
    }

    fn write_irq_latch(&mut self, addr: u16, val: u8) {
        let (mask, val) = if addr & 0x0C == 0x0C {
            (0x00FF, u16::from(val) << 8)
        } else {
            (0xFF00, u16::from(val))
        };
        if self.mapper_num != 16 || !matches!(self.submapper_num, 0 | 4) {
            // On the LZ93D50 (Submapper 5), these registers instead modify a latch that will only
            // be copied to the actual counter when register $800A is written to.
            self.regs.irq_reload = (self.regs.irq_reload & mask) | val;
        } else {
            // On the FCG-1/2 (Submapper 4), writing to these two registers directly
            // modifies the counter itself; all such games therefore disable counting before
            // changing the counter value.
            self.regs.irq_counter = (self.regs.irq_counter & mask) | val;
        }
    }

    fn write_eeprom_ctrl(&mut self, val: u8) {
        let sda = (val & 0x40) >> 6;
        if let Some(eeprom) = &mut self.standard_eeprom {
            let scl = (val & 0x20) >> 5;
            eeprom.write(scl, sda);
        }
        if let Some(eeprom) = &mut self.extra_eeprom {
            eeprom.write_sda(sda);
        }
    }

    #[inline(always)]
    pub const fn prg_ram_enabled(&self) -> bool {
        self.mapper_num == 153 && self.regs.prg_ram_enabled
    }
}

impl Map for BandaiFCG {
    // Mapper 016
    //
    // PPU $0000..=$03FF 1K switchable CHR-ROM bank
    // PPU $0400..=$07FF 1K switchable CHR-ROM bank
    // PPU $0800..=$0BFF 1K switchable CHR-ROM bank
    // PPU $0c00..=$0FFF 1K switchable CHR-ROM bank
    // PPU $1000..=$13FF 1K switchable CHR-ROM bank
    // PPU $1400..=$17FF 1K switchable CHR-ROM bank
    // PPU $1800..=$1BFF 1K switchable CHR-ROM bank
    // PPU $1c00..=$1FFF 1K switchable CHR-ROM bank
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM bank, fixed to the last bank
    //
    // Mapper 153
    //
    // CPU $6000..=$7FFF 8K battery-backed WRAM
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM bank, fixed to the last bank
    // PPU $0000..=$1FFF 8K fixed CHR-ROM bank
    //
    // Mapper 157
    //
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM bank, fixed to the last bank
    // PPU $0000..=$1FFF 8K fixed CHR-ROM bank
    //
    // Mapper 159
    //
    // PPU $0000..=$03FF 1K switchable CHR-ROM bank
    // PPU $0400..=$07FF 1K switchable CHR-ROM bank
    // PPU $0800..=$0BFF 1K switchable CHR-ROM bank
    // PPU $0c00..=$0FFF 1K switchable CHR-ROM bank
    // PPU $1000..=$13FF 1K switchable CHR-ROM bank
    // PPU $1400..=$17FF 1K switchable CHR-ROM bank
    // PPU $1800..=$1BFF 1K switchable CHR-ROM bank
    // PPU $1c00..=$1FFF 1K switchable CHR-ROM bank
    // CPU $8000..=$BFFF 16K switchable PRG-ROM bank
    // CPU $C000..=$FFFF 16K PRG-ROM bank, fixed to the last bank

    /// Peek a byte from CHR-ROM/RAM at a given address.
    #[inline(always)]
    fn chr_peek(&self, addr: u16, ciram: &CIRam) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr[self.chr_banks.translate(addr)],
            0x2000..=0x3EFF => ciram.peek(addr, self.mirroring),
            _ => 0,
        }
    }

    /// Read a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_read(&mut self, addr: u16) -> u8 {
        if matches!(addr, 0x6000..=0x7FFF) {
            if !matches!(self.sram_access, MemoryOp::Read | MemoryOp::ReadWrite) {
                return 0;
            }

            let mut val = 0x00;
            if let Some(barcode_reader) = &mut self.barcode_reader {
                val |= barcode_reader.read();
            }
            if let (Some(eeprom1), Some(eeprom2)) =
                (&mut self.standard_eeprom, &mut self.extra_eeprom)
            {
                val |= (eeprom1.read() & eeprom2.read()) << 4;
            } else if let Some(eeprom) = &mut self.standard_eeprom {
                val |= eeprom.read() << 4;
            }

            val
        } else {
            self.prg_peek(addr)
        }
    }

    /// Peek a byte from PRG-ROM/RAM at a given address.
    #[inline(always)]
    fn prg_peek(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled() => self.prg_ram[usize::from(addr - 0x6000)],
            0x8000..=0xFFFF => self.prg_rom[self.prg_rom_banks.translate(addr)],
            _ => 0,
        }
    }

    /// Write a byte to CHR-RAM/CIRAM at a given address.
    #[inline(always)]
    fn chr_write(&mut self, addr: u16, val: u8, ciram: &mut CIRam) {
        match addr {
            0x0000..=0x1FFF => self.chr[self.chr_banks.translate(addr)] = val,
            0x2000..=0x3EFF => ciram.write(addr, val, self.mirroring),
            _ => (),
        }
    }

    /// Write a byte to PRG-RAM at a given address.
    #[inline(always)]
    fn prg_write(&mut self, addr: u16, val: u8) {
        match addr {
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                self.prg_ram[usize::from(addr - 0x6000)] = val;
            }
            0x6000..=0xFFFF => match addr & 0x0F {
                0x00..=0x07 => self.write_chr_bank(addr, val),
                0x08 => self.write_prg_bank(val),
                0x09 => self.write_mirroring(val),
                0x0A => self.write_irq_ctrl(val),
                0x0B..=0x0C => self.write_irq_latch(addr, val),
                0x0D => {
                    if self.mapper_num == 153 {
                        self.regs.prg_ram_enabled = (val & 0x20) == 0x20;
                    } else if matches!(self.sram_access, MemoryOp::Write | MemoryOp::ReadWrite) {
                        self.write_eeprom_ctrl(val);
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    /// Whether an IRQ is pending acknowledgement.
    fn irq_pending(&self) -> bool {
        self.regs.irq_pending
    }

    /// Returns the current [`Mirroring`] mode.
    #[inline(always)]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
}

impl Clock for BandaiFCG {
    fn clock(&mut self) {
        if let Some(barcode_reader) = &mut self.barcode_reader {
            barcode_reader.clock();
        }
        // Checking counter before decrementing seems to be the only way to get both Famicom Jump
        // II - Saikyou no 7 Nin (J) and Magical Taruruuto-kun 2 - Mahou Daibouken (J) to work
        // without glitches with the same code.
        if self.regs.irq_enabled {
            if self.regs.irq_counter == 0 {
                self.regs.irq_pending = true;
            }
            self.regs.irq_counter = self.regs.irq_counter.wrapping_sub(1);
        }
    }
}

impl Sram for BandaiFCG {
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        if let Some(eeprom) = &self.standard_eeprom {
            eeprom.save(&path)?;
        }
        if let Some(eeprom) = &self.extra_eeprom {
            eeprom.save(&path)?;
        }
        Ok(())
    }

    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        if let Some(eeprom) = &mut self.standard_eeprom {
            eeprom.load(&path)?;
        }
        if let Some(eeprom) = &mut self.extra_eeprom {
            eeprom.load(&path)?;
        }
        Ok(())
    }
}

impl Regional for BandaiFCG {}
impl Reset for BandaiFCG {}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct BarcodeReader {
    pub data: Box<[u8]>,
    pub master_clock: usize,
    pub insert_cycle: usize,
    pub new_barcode: u64,
    pub new_barcode_digit_count: u32,
}

impl BarcodeReader {
    pub fn new() -> Self {
        Self {
            data: Default::default(),
            master_clock: 0,
            insert_cycle: 0,
            new_barcode: 0,
            new_barcode_digit_count: 0,
        }
    }

    pub fn read(&self) -> u8 {
        let elapsed_cycles = self.master_clock - self.insert_cycle;
        let bit_number = elapsed_cycles / 1000;
        if bit_number < self.data.len() {
            self.data[bit_number]
        } else {
            0x00
        }
    }

    pub const fn input(&mut self, barcode: u64, digit_count: u32) {
        self.new_barcode = barcode;
        self.new_barcode_digit_count = digit_count;
    }

    pub fn barcode(&self) -> String {
        format!(
            "{:0>width$}",
            self.new_barcode,
            width = self.new_barcode_digit_count as usize
        )
    }

    pub fn init(&mut self) {
        self.insert_cycle = self.master_clock;

        static PREFIX_PARITY_TYPE: [[u8; 6]; 10] = [
            [8, 8, 8, 8, 8, 8],
            [8, 8, 0, 8, 0, 0],
            [8, 8, 0, 0, 8, 0],
            [8, 8, 0, 0, 0, 8],
            [8, 0, 8, 8, 0, 0],
            [8, 0, 0, 8, 8, 0],
            [8, 0, 0, 0, 8, 8],
            [8, 0, 8, 0, 8, 0],
            [8, 0, 8, 0, 0, 8],
            [8, 0, 0, 8, 0, 8],
        ];

        static DATA_LEFT_ODD: [[u8; 7]; 10] = [
            [8, 8, 8, 0, 0, 8, 0],
            [8, 8, 0, 0, 8, 8, 0],
            [8, 8, 0, 8, 8, 0, 0],
            [8, 0, 0, 0, 0, 8, 0],
            [8, 0, 8, 8, 8, 0, 0],
            [8, 0, 0, 8, 8, 8, 0],
            [8, 0, 8, 0, 0, 0, 0],
            [8, 0, 0, 0, 8, 0, 0],
            [8, 0, 0, 8, 0, 0, 0],
            [8, 8, 8, 0, 8, 0, 0],
        ];

        static DATA_LEFT_EVEN: [[u8; 7]; 10] = [
            [8, 0, 8, 8, 0, 0, 0],
            [8, 0, 0, 8, 8, 0, 0],
            [8, 8, 0, 0, 8, 0, 0],
            [8, 0, 8, 8, 8, 8, 0],
            [8, 8, 0, 0, 0, 8, 0],
            [8, 0, 0, 0, 8, 8, 0],
            [8, 8, 8, 8, 0, 8, 0],
            [8, 8, 0, 8, 8, 8, 0],
            [8, 8, 8, 0, 8, 8, 0],
            [8, 8, 0, 8, 0, 0, 0],
        ];

        static DATA_RIGHT: [[u8; 7]; 10] = [
            [0, 0, 0, 8, 8, 0, 8],
            [0, 0, 8, 8, 0, 0, 8],
            [0, 0, 8, 0, 0, 8, 8],
            [0, 8, 8, 8, 8, 0, 8],
            [0, 8, 0, 0, 0, 8, 8],
            [0, 8, 8, 0, 0, 0, 8],
            [0, 8, 0, 8, 8, 8, 8],
            [0, 8, 8, 8, 0, 8, 8],
            [0, 8, 8, 0, 8, 8, 8],
            [0, 0, 0, 8, 0, 8, 8],
        ];

        let barcode = self.barcode();
        let mut codes = Vec::new();
        for ch in barcode.chars() {
            codes.push(ch.to_digit(10).expect("valid barcode character") as usize);
        }

        let mut data = Vec::<u8>::with_capacity(256);

        data.extend([8; 33]);
        data.extend([0, 8, 0]);

        let mut sum = 0;
        if barcode.len() == 13 {
            for i in 0..6 {
                let odd = PREFIX_PARITY_TYPE[codes[0]][i] != 0;
                for j in 0..7 {
                    data.push(if odd {
                        DATA_LEFT_ODD[codes[i + 1]][j]
                    } else {
                        DATA_LEFT_EVEN[codes[i + 1]][j]
                    });
                }
            }

            data.extend([8, 0, 8, 0, 8]);

            for code in codes.iter().skip(7).take(5) {
                for code_data in DATA_RIGHT[*code].iter().take(7) {
                    data.push(*code_data);
                }
            }

            for (i, code) in codes.iter().enumerate().take(12) {
                sum += if (i & 1) == 1 { *code * 3 } else { *code };
            }
        } else {
            for code in codes.iter().take(4) {
                for code_data in DATA_LEFT_ODD[*code].iter().take(7) {
                    data.push(*code_data);
                }
            }

            data.extend([8, 0, 8, 0, 8]);

            for code in codes.iter().skip(4).take(3) {
                for code_data in DATA_RIGHT[*code].iter().take(7) {
                    data.push(*code_data);
                }
            }

            for (i, code) in codes.iter().enumerate().take(7) {
                sum += if (i & 1) == 1 { *code } else { *code * 3 };
            }
        }

        sum = (10 - (sum % 10)) % 10;

        for sum_data in DATA_RIGHT[sum].iter().take(7) {
            data.push(*sum_data);
        }

        data.extend([0, 8, 0]);
        data.extend([8; 32]);

        self.data = data.into();
    }
}

impl Clock for BarcodeReader {
    fn clock(&mut self) {
        self.master_clock += 1;
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub enum EepromModel {
    X24C01,
    X24C02,
}

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub enum EepromMode {
    #[default]
    Idle,
    Addr,
    Read,
    Write,
    SendAck,
    WaitAck,
    ChipAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Eeprom {
    pub model: EepromModel,
    pub mode: EepromMode,
    pub next_mode: EepromMode,
    pub chip_addr: u8,
    pub addr: u8,
    pub data: u8,
    pub counter: u8,
    pub output: u8,
    pub prev_scl: u8,
    pub prev_sda: u8,
    pub rom_data: Memory<Box<[u8]>>,
}

impl Eeprom {
    pub fn new(model: EepromModel) -> Self {
        let rom_size = match model {
            EepromModel::X24C01 => 128,
            EepromModel::X24C02 => 256,
        };
        Self {
            model,
            mode: EepromMode::default(),
            next_mode: EepromMode::default(),
            chip_addr: 0,
            addr: 0,
            data: 0,
            counter: 0,
            output: 0,
            prev_scl: 0,
            prev_sda: 0,
            rom_data: Memory::new(rom_size),
        }
    }

    pub const fn read(&self) -> u8 {
        self.output
    }

    pub fn write(&mut self, scl: u8, sda: u8) {
        match self.model {
            EepromModel::X24C01 => {
                if self.prev_scl > 0 && scl > 0 && sda < self.prev_sda {
                    // START is identified by a high to low transition of the SDA line while the
                    // clock SCL is *stable* in the high state
                    self.mode = EepromMode::Addr;
                    self.addr = 0;
                    self.counter = 0;
                    self.output = 1;
                } else if self.prev_scl > 0 && scl > 0 && sda > self.prev_sda {
                    // STOP is identified by a low to high transition of the SDA line while the
                    // clock SCL is *stable* in the high state
                    self.mode = EepromMode::Idle;
                    self.output = 1;
                } else if scl > self.prev_scl {
                    // Clock rise
                    match self.mode {
                        EepromMode::Addr => {
                            // To initiate a write operation, the master sends a start condition
                            // followed by a seven bit word address and a write bit.
                            match self.counter.cmp(&7) {
                                Ordering::Less => {
                                    if let Some(addr) = self.write_bit(self.addr, sda) {
                                        self.addr = addr;
                                    }
                                }
                                Ordering::Equal => {
                                    // 8th bit to determine if we're in read or write mode
                                    self.counter = 8;
                                    if sda > 0 {
                                        self.next_mode = EepromMode::Read;
                                        self.data = self.rom_data[usize::from(self.addr & 0x7F)];
                                    } else {
                                        self.next_mode = EepromMode::Write;
                                    }
                                }
                                _ => (),
                            }
                        }
                        EepromMode::Read => self.read_bit(),
                        EepromMode::Write => {
                            if let Some(data) = self.write_bit(self.data, sda) {
                                self.data = data;
                            }
                        }
                        EepromMode::SendAck => self.output = 0,
                        EepromMode::WaitAck if sda == 0 => {
                            // We expected an ack, but received something else, return to idle
                            // mode
                            self.next_mode = EepromMode::Idle;
                        }
                        _ => (),
                    }
                } else if scl < self.prev_scl {
                    // Clock fall
                    match self.mode {
                        EepromMode::Addr if self.counter == 8 => {
                            // After receiving the address, the X24C01 responds with an
                            // acknowledge, then waits for eight bits of data
                            self.mode = EepromMode::SendAck;
                            self.output = 1;
                        }
                        EepromMode::SendAck => {
                            // After sending an ack, move to the next mode of operation
                            self.mode = self.next_mode;
                            self.counter = 0;
                            self.output = 1;
                        }
                        EepromMode::Read if self.counter == 8 => {
                            // After sending all 8 bits, wait for an ack
                            self.mode = EepromMode::WaitAck;
                            self.addr = (self.addr + 1) & 0x7F;
                        }
                        EepromMode::Write if self.counter == 8 => {
                            // After receiving all 8 bits, send an ack and then wait
                            self.mode = EepromMode::SendAck;
                            self.next_mode = EepromMode::Idle;
                            self.rom_data[usize::from(self.addr & 0x7F)] = self.data;
                            self.addr = (self.addr + 1) & 0x7F;
                        }
                        _ => (),
                    }
                }

                self.prev_scl = scl;
                self.prev_sda = sda;
            }
            EepromModel::X24C02 => {
                if self.prev_scl > 0 && scl > 0 && sda < self.prev_sda {
                    // START is identified by a high to low transition of the SDA line while the
                    // clock SCL is *stable* in the high state
                    self.mode = EepromMode::ChipAddr;
                    self.counter = 0;
                    self.output = 1;
                } else if self.prev_scl > 0 && scl > 0 && sda > self.prev_sda {
                    // STOP is identified by a low to high transition of the SDA line while the
                    // clock SCL is *stable* in the high state
                    self.mode = EepromMode::Idle;
                    self.output = 1;
                } else if scl > self.prev_scl {
                    // Clock rise
                    match self.mode {
                        EepromMode::ChipAddr => {
                            if let Some(chip_addr) = self.write_bit(self.chip_addr, sda) {
                                self.chip_addr = chip_addr;
                            }
                        }
                        EepromMode::Addr => {
                            if let Some(addr) = self.write_bit(self.addr, sda) {
                                self.addr = addr;
                            }
                        }
                        EepromMode::Read => self.read_bit(),
                        EepromMode::Write => {
                            if let Some(data) = self.write_bit(self.data, sda) {
                                self.data = data;
                            }
                        }
                        EepromMode::SendAck => self.output = 0,
                        EepromMode::WaitAck if sda == 0 => {
                            self.next_mode = EepromMode::Read;
                            self.data = self.rom_data[usize::from(self.addr)];
                        }
                        _ => (),
                    }
                } else if scl < self.prev_scl {
                    // Clock fall
                    match self.mode {
                        // Upon a correct compare the X24C02 outputs an acknowledge on the SDA line
                        EepromMode::ChipAddr if self.counter == 8 => {
                            if (self.chip_addr & 0xA0) == 0xA0 {
                                self.mode = EepromMode::SendAck;
                                self.counter = 0;
                                self.output = 1;

                                // The last bit of the slave address defines the operation to
                                // be performed. When set to one a read operation is selected,
                                // when set to zero a write operations is selected
                                if (self.chip_addr & 0x01) == 0x01 {
                                    // Current Address Read
                                    // Upon receipt of the slave address with the R/W bit set
                                    // to one, the X24C02 issues an acknowledge and transmits
                                    // the eight bit word during the next eight clock cycles
                                    self.next_mode = EepromMode::Read;
                                    self.data = self.rom_data[usize::from(self.addr)];
                                } else {
                                    self.mode = EepromMode::Addr;
                                }
                            } else {
                                // This chip wasn't selected, go back to idle mode
                                self.mode = EepromMode::Idle;
                                self.counter = 0;
                                self.output = 1;
                            }
                        }
                        EepromMode::Addr if self.counter == 8 => {
                            // Finished receiving all 8 bits of the address, send an ack and then starting writing the value
                            self.mode = EepromMode::SendAck;
                            self.next_mode = EepromMode::Write;
                            self.counter = 0;
                            self.output = 1;
                        }
                        EepromMode::Read if self.counter == 8 => {
                            // After sending all 8 bits, wait for an ack
                            self.mode = EepromMode::WaitAck;
                            self.addr = self.addr.wrapping_add(1);
                        }
                        EepromMode::Write if self.counter == 8 => {
                            // After receiving all 8 bits, send an ack and then wait
                            self.mode = EepromMode::SendAck;
                            self.next_mode = EepromMode::Write;
                            self.counter = 0;
                            self.rom_data[usize::from(self.addr)] = self.data;
                            self.addr = self.addr.wrapping_add(1);
                        }
                        EepromMode::SendAck | EepromMode::WaitAck => {
                            self.mode = self.next_mode;
                            self.counter = 0;
                            self.output = 1;
                        }
                        _ => (),
                    }
                }

                self.prev_scl = scl;
                self.prev_sda = sda;
            }
        }
    }

    pub fn write_scl(&mut self, scl: u8) {
        self.write(scl, self.prev_sda);
    }

    pub fn write_sda(&mut self, sda: u8) {
        self.write(self.prev_scl, sda);
    }

    pub const fn write_bit(&mut self, dest: u8, val: u8) -> Option<u8> {
        if self.counter < 8 {
            let mask = !(1 << self.counter);
            let dest = (dest & mask) | (val << self.counter);
            self.counter += 1;
            Some(dest)
        } else {
            None
        }
    }

    pub const fn read_bit(&mut self) {
        if self.counter < 8 {
            self.output = if self.data & (1 << self.counter) > 0 {
                1
            } else {
                0
            };
            self.counter += 1;
        }
    }

    pub const fn sram_extension(&self) -> &str {
        match self.model {
            EepromModel::X24C01 => "eeprom128",
            EepromModel::X24C02 => "eeprom256",
        }
    }
}

impl Sram for Eeprom {
    fn save(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        let extension = self.sram_extension();
        fs::save(path.as_ref().with_extension(extension), &self.rom_data)
    }

    fn load(&mut self, path: impl AsRef<Path>) -> fs::Result<()> {
        let extension = self.sram_extension();
        fs::load(path.as_ref().with_extension(extension)).map(|data| self.rom_data = data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bandai_fcg_barcode_formatting() {
        let mut reader = BarcodeReader::new();
        reader.input(4902425679235, 13);
        assert_eq!(reader.barcode(), "4902425679235");

        // Test zero-padding for EAN-8
        reader.input(1234567, 8);
        assert_eq!(reader.barcode(), "01234567");
    }

    #[test]
    fn bandai_fcg_ean13_checksum() {
        // EAN-13: first 12 digits -> checksum is 13th
        // 490242567923 -> check digit 5
        let mut reader = BarcodeReader::new();
        reader.input(4902425679235, 13);
        reader.init();

        // The checksum is encoded in the last 7 data bits before the end guard
        // End guard is [0,8,0] + 32x8, so check digit encoding ends at len-35
        let check_digit_pattern = &reader.data[reader.data.len() - 35 - 7..reader.data.len() - 35];

        // DATA_RIGHT[5] = [0, 8, 8, 0, 0, 0, 8]
        assert_eq!(check_digit_pattern, &[0, 8, 8, 0, 0, 0, 8]);
    }

    #[test]
    fn bandai_fcg_ean13_structure() {
        let mut reader = BarcodeReader::new();
        reader.input(4902425679235, 13);
        reader.init();

        // EAN-13 total: 33 (quiet) + 3 (start) + 42 (left) + 5 (center) + 42 (right) + 3 (end) + 32 (quiet)
        // = 33 + 3 + 42 + 5 + 42 + 3 + 32 = 160
        assert_eq!(reader.data.len(), 160);

        // Start guard at index 33
        assert_eq!(&reader.data[33..36], &[0, 8, 0]);

        // Center guard at index 33 + 3 + 42 = 78
        assert_eq!(&reader.data[78..83], &[8, 0, 8, 0, 8]);

        // End guard at index 83 + 35 = 125 (after 5 digits * 7 bits)
        assert_eq!(&reader.data[125..128], &[0, 8, 0]);
    }

    #[test]
    fn bandai_fcg_ean8_structure() {
        let mut reader = BarcodeReader::new();
        // Valid EAN-8: 12345670 (checksum 0)
        reader.input(12345670, 8);
        reader.init();

        // EAN-8: 33 + 3 + 28 + 5 + 28 + 3 + 32 = 132
        assert_eq!(reader.data.len(), 132);
    }
}
