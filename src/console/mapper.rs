use super::rom::{Rom, CHR_ROM_SIZE, PRG_ROM_SIZE};
use std::error::Error;

// TODO Implement MMC3 next

pub trait Mapper {
    fn readb(&mut self, addr: u16) -> u8;
    fn writeb(&mut self, addr: u16, val: u8);
    fn mirror(&self) -> u8;
    // fn prg_readb(&mut self, addr: u16) -> u8;
    // fn prg_writeb(&mut self, addr: u16, val: u8);
    // fn chr_readb(&mut self, addr: u16) -> u8;
    // fn chr_writeb(&mut self, addr: u16, val: u8);
}

/// Mirror options
enum Mirror {
    Horizontal,
    Vertical,
    OneScreenLower,
    OneScreenUpper,
    FourScreen,
}

pub fn new_mapper(rom: Rom) -> Result<Box<Mapper>, Box<Error>> {
    match rom.mapper() {
        0 | 2 => Ok(Box::new(Nrom::new(rom))),
        1 => Ok(Box::new(SxRom::new(rom))),
        // 3 => Ok(Box::new(Mapper3::new(rom))),
        // 4 => Ok(Box::new(Mapper4::new(rom))),
        // 7 => Ok(Box::new(Mapper7::new())),
        _ => Err(format!("unsupported mapper : {}", rom.mapper()).into()),
    }
}

/// SxRom

#[derive(Debug)]
enum SxRomPrgBankMode {
    Fused,    // Upper and lower banks are a single 32 KB, switchable bank
    FixFirst, // Fix lower bank, allowing upper bank to be switchable
    FixLast,  // Fix upper bank, allowing lower bank to be switchable
}

#[derive(Debug)]
enum SxRomChrBankMode {
    Fused,       // Upper and lower banks are a single 32 KB, switchable bank
    Independent, // Upper and lower banks can be switched
}

struct SxRom {
    rom: Rom,
    // Registers
    shift_reg: u8,
    // $8000-$9FFF
    // CPPMM
    // C : CHR ROM bank: 0 switch 8 KB; 1: switch two 4KB
    // P : PRG ROM bank: 0, 1: switch 32 KB at $8000
    //                   2: fix first bank at $8000, switch 16 KB at $C000
    //                   3: fix last bank at $C000, switch 16 KB at $8000
    // M : 0: one-screen, lower; 1: one-screen, upper;
    //     2: verticall 3: horizontal
    ctrl: u8,
    // $A000-$BFFF
    // Lower bank
    // CCCCC
    // Select 4 or 8 KB CHR bank at PPU $0000
    chr_bank0: u8,
    // $C000-$DFFF
    // Upper bank
    // CCCCC
    // Select 4 KB CHR bank at PPU $1000
    chr_bank1: u8,
    // $E000-$FFFF
    // RPPPP
    // R : PRG RAM chip enable: 0: enabled; 1: disabled
    // P : Select 16 KB PRG ROM bank
    prg_bank: u8,
    prg_offsets: [usize; 2],
    chr_offsets: [usize; 2],
    mirror: Mirror,
}

impl SxRom {
    pub fn new(rom: Rom) -> Self {
        Self {
            rom,
            shift_reg: 0,
            ctrl: 3 << 2,
            chr_bank0: 0,
            chr_bank1: 0,
            prg_bank: 0,
            prg_offsets: [0usize; 2],
            chr_offsets: [0usize; 2],
            mirror: Mirror::OneScreenLower,
        }
    }

    fn prg_bank_mode(&self) -> SxRomPrgBankMode {
        match (self.ctrl >> 2) & 3 {
            0 | 1 => SxRomPrgBankMode::Fused,
            2 => SxRomPrgBankMode::FixFirst,
            3 => SxRomPrgBankMode::FixLast,
            _ => panic!("not possible"),
        }
    }

    fn chr_bank_mode(&self) -> SxRomChrBankMode {
        match (self.ctrl >> 4) & 1 {
            0 => SxRomChrBankMode::Fused,
            1 => SxRomChrBankMode::Independent,
            _ => panic!("not possible"),
        }
    }

    fn update_offsets(&mut self) {
        let prg_size = self.rom.prg_rom.len();
        let chr_size = self.rom.chr_rom.len();
        match self.prg_bank_mode() {
            SxRomPrgBankMode::Fused => {
                self.prg_offsets[0] =
                    bank_offset(prg_size, (self.prg_bank & 0xFE) as isize, 0x4000);
                self.prg_offsets[1] =
                    bank_offset(prg_size, (self.prg_bank | 0x01) as isize, 0x4000);
            }
            SxRomPrgBankMode::FixFirst => {
                self.prg_offsets[0] = 0;
                self.prg_offsets[1] = bank_offset(prg_size, self.prg_bank as isize, 0x4000);
            }
            SxRomPrgBankMode::FixLast => {
                self.prg_offsets[0] = bank_offset(prg_size, self.prg_bank as isize, 0x4000);
                self.prg_offsets[1] = bank_offset(prg_size, -1, 0x4000);
            }
        }
        match self.chr_bank_mode() {
            SxRomChrBankMode::Fused => {
                self.chr_offsets[0] =
                    bank_offset(chr_size, (self.chr_bank0 & 0xFE) as isize, 0x1000);
                self.chr_offsets[1] =
                    bank_offset(chr_size, (self.chr_bank0 | 0x01) as isize, 0x1000);
            }
            SxRomChrBankMode::Independent => {
                self.chr_offsets[0] = bank_offset(chr_size, self.chr_bank0 as isize, 0x1000);
                self.chr_offsets[1] = bank_offset(chr_size, self.chr_bank1 as isize, 0x1000);
            }
        }
    }

    fn write_ctrl(&mut self, val: u8) {
        self.ctrl = val;
        self.mirror = match val & 3 {
            0 => Mirror::OneScreenLower,
            1 => Mirror::OneScreenUpper,
            2 => Mirror::Vertical,
            3 => Mirror::Horizontal,
            _ => panic!("invalid mirror mode {}", val & 3),
        }
    }
}

impl Mapper for SxRom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000...0x2000 => {
                let bank = (addr / 0x1000) as usize;
                let offset = (addr % 0x1000) as usize;
                self.rom.chr_rom[self.chr_offsets[bank] + offset]
            }
            0x6000...0x7FFF => self.rom.chr_ram[addr as usize - 0x6000],
            0x8000...0xFFFF => {
                let addr = addr - 0x8000;
                let bank = (addr / 0x4000) as usize;
                let offset = (addr % 0x4000) as usize;
                self.rom.prg_rom[self.prg_offsets[bank] + offset]
            }
            _ => panic!("unhandled mapper1 read at address: 0x{:04X}", addr),
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000...0x2000 => {
                let bank = (addr / 0x1000) as usize;
                let offset = (addr % 0x1000) as usize;
                self.rom.chr_ram[self.chr_offsets[bank] + offset] = val;
            }
            0x6000...0x7FFF => self.rom.chr_ram[addr as usize - 0x6000] = val,
            0x8000...0xFFFF => {
                if val & 0x80 == 0x80 {
                    self.shift_reg = 0x10;
                    self.write_ctrl(self.ctrl | 0x0C);
                    self.update_offsets();
                } else {
                    let complete = self.shift_reg & 1 == 1;
                    self.shift_reg >>= 1;
                    self.shift_reg |= (val & 1) << 4;
                    if complete {
                        match addr {
                            0x0000...0x9FFF => self.write_ctrl(self.shift_reg),
                            0xA000...0xBFFF => self.chr_bank0 = self.shift_reg,
                            0xC000...0xDFFF => self.chr_bank1 = self.shift_reg,
                            0xE000...0xFFFF => self.prg_bank = self.shift_reg & 0x0F,
                        }
                        self.update_offsets();
                        self.shift_reg = 0x10;
                    }
                }
            }
            _ => panic!("unhandled mapper1 write at address: 0x{:04X}", addr),
        }
    }

    fn mirror(&self) -> u8 {
        self.rom.mirror()
    }

    // fn prg_readb(&mut self, addr: u16) -> u8 {
    //     match addr {
    //         0x0000...0x7FFF => 0,
    //         0x8000...0xBFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 SxRomPrgBankMode::Fused => self.prg_bank & 0xFE,
    //                 SxRomPrgBankMode::FixFirst => 0,
    //                 SxRomPrgBankMode::FixLast => self.prg_bank,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)]
    //         }
    //         0xC000...0xFFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 SxRomPrgBankMode::Fused => (self.prg_bank & 0xFE) | 1,
    //                 SxRomPrgBankMode::FixFirst => self.prg_bank,
    //                 SxRomPrgBankMode::FixLast => (self.rom.prg_rom_size() - 1) as u8,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)]
    //         }
    //     }
    // }

    // fn prg_writeb(&mut self, addr: u16, val: u8) {
    //     match addr {
    //         0x0000...0x7FFF => (),
    //         0x8000...0xBFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 SxRomPrgBankMode::Fused => self.prg_bank & 0xFE,
    //                 SxRomPrgBankMode::FixFirst => 0,
    //                 SxRomPrgBankMode::FixLast => self.prg_bank,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)] = val;
    //         }
    //         0xC000...0xFFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 SxRomPrgBankMode::Fused => (self.prg_bank & 0xFE) | 1,
    //                 SxRomPrgBankMode::FixFirst => self.prg_bank,
    //                 SxRomPrgBankMode::FixLast => (self.rom.prg_rom_size() - 1) as u8,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)] = val;
    //         }
    //     }
    // }

    // fn chr_readb(&mut self, addr: u16) -> u8 {
    //     match addr {
    //         0x0000...0x2000 {
    //             let bank = match self.chr_bank_mode() {
    //                 SxRomChrBankMode::Fused => self.chr_bank0 & 0xFE,
    //                 SxRomChrBankMode::Independent => self.chr_bank0,
    //             };
    //             self.rom.chr_rom[(bank as usize
    //         }
    //         0x6000...0x7FFF => self.rom.chr_ram[addr as usize];
    //     }
    // }

    // fn chr_writeb(&mut self, addr: u16, val: u8) {}
}

fn bank_offset(size: usize, mut index: isize, offset: isize) -> usize {
    if index >= 0x80 {
        index -= 0x100;
    }
    // TODO Some roms causing chr size to be 0 here, find out why
    if size > 0 {
        index %= size as isize / offset;
    }
    let mut offset = index * offset;
    if offset < 0 {
        offset += size as isize;
    }
    offset as usize
}

/// NRom

struct Nrom {
    rom: Rom,
    prg_banks: u8,
    prg_bank1: u8,
    prg_bank2: u8,
}

impl Nrom {
    fn new(rom: Rom) -> Self {
        let prg_banks = (rom.prg_rom.len() / 0x4000) as u8;
        Self {
            rom,
            prg_banks,
            prg_bank1: 0,
            prg_bank2: prg_banks - 1,
        }
    }
}

impl Mapper for Nrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000...0x1FFF => self.rom.chr_rom[addr as usize],
            0x6000...0x7FFF => self.rom.chr_ram[addr as usize - 0x6000],
            0x8000...0xBFFF => {
                let index = u16::from(self.prg_bank1) * 0x4000 + (addr - 0x8000);
                self.rom.prg_rom[index as usize]
            }
            0xC000...0xFFFF => {
                let index = u16::from(self.prg_bank2) * 0x4000 + (addr - 0xC000);
                self.rom.prg_rom[index as usize]
            }
            _ => panic!("unhandled mapper2 read at address: 0x{:04X}", addr),
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000...0x1FFF => self.rom.chr_rom[addr as usize] = val,
            0x6000...0x7FFF => self.rom.chr_ram[addr as usize - 0x6000] = val,
            0x8000...0xFFFF => {
                self.prg_bank1 = val % self.prg_banks;
            }
            _ => panic!("unhandled mapper2 read at address: 0x{:04X}", addr),
        }
    }

    fn mirror(&self) -> u8 {
        self.rom.mirror()
    }
}
