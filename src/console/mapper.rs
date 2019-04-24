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
        // 0 | 2 => Ok(Box::new(Mapper2::new(rom))),
        1 => Ok(Box::new(Mmc1::new(rom))),
        // 3 => Ok(Box::new(Mapper3::new(rom))),
        // 4 => Ok(Box::new(Mapper4::new(rom))),
        // 7 => Ok(Box::new(Mapper7::new())),
        _ => Err(format!("unsupported mapper : {}", rom.mapper()).into()),
    }
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

#[derive(Debug)]
enum Mmc1PrgBankMode {
    Fused,    // Upper and lower banks are a single 32 KB, switchable bank
    FixFirst, // Fix lower bank, allowing upper bank to be switchable
    FixLast,  // Fix upper bank, allowing lower bank to be switchable
}

#[derive(Debug)]
enum Mmc1ChrBankMode {
    Fused,       // Upper and lower banks are a single 32 KB, switchable bank
    Independent, // Upper and lower banks can be switched
}

pub struct Mmc1 {
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
    chr_ram: [u8; CHR_ROM_SIZE],
    mirror: Mirror,
}

impl Mmc1 {
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
            chr_ram: [0u8; CHR_ROM_SIZE],
            mirror: Mirror::OneScreenLower,
        }
    }

    fn prg_bank_mode(&self) -> Mmc1PrgBankMode {
        match (self.ctrl >> 2) & 3 {
            0 | 1 => Mmc1PrgBankMode::Fused,
            2 => Mmc1PrgBankMode::FixFirst,
            3 => Mmc1PrgBankMode::FixLast,
            _ => panic!("not possible"),
        }
    }

    fn chr_bank_mode(&self) -> Mmc1ChrBankMode {
        match (self.ctrl >> 4) & 1 {
            0 => Mmc1ChrBankMode::Fused,
            1 => Mmc1ChrBankMode::Independent,
            _ => panic!("not possible"),
        }
    }

    fn update_offsets(&mut self) {
        let prg_size = self.rom.prg_rom.len();
        let chr_size = self.rom.chr_rom.len();
        match self.prg_bank_mode() {
            Mmc1PrgBankMode::Fused => {
                self.prg_offsets[0] =
                    bank_offset(prg_size, (self.prg_bank & 0xFE) as isize, 0x4000);
                self.prg_offsets[1] =
                    bank_offset(prg_size, (self.prg_bank | 0x01) as isize, 0x4000);
            }
            Mmc1PrgBankMode::FixFirst => {
                self.prg_offsets[0] = 0;
                self.prg_offsets[1] = bank_offset(prg_size, self.prg_bank as isize, 0x4000);
            }
            Mmc1PrgBankMode::FixLast => {
                self.prg_offsets[0] = bank_offset(prg_size, self.prg_bank as isize, 0x4000);
                self.prg_offsets[1] = bank_offset(prg_size, -1, 0x4000);
            }
            _ => panic!("invalid prg_mode {:?}", self.prg_bank_mode()),
        }
        match self.chr_bank_mode() {
            Mmc1ChrBankMode::Fused => {
                self.chr_offsets[0] =
                    bank_offset(chr_size, (self.chr_bank0 & 0xFE) as isize, 0x1000);
                self.chr_offsets[1] =
                    bank_offset(chr_size, (self.chr_bank0 | 0x01) as isize, 0x1000);
            }
            Mmc1ChrBankMode::Independent => {
                self.chr_offsets[0] = bank_offset(chr_size, self.chr_bank0 as isize, 0x1000);
                self.chr_offsets[1] = bank_offset(chr_size, self.chr_bank1 as isize, 0x1000);
            }
            _ => panic!("invalid chr_mode {:?}", self.chr_bank_mode()),
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

impl Mapper for Mmc1 {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000...0x2000 => {
                let bank = (addr / 0x1000) as usize;
                let offset = (addr % 0x1000) as usize;
                self.rom.chr_rom[self.chr_offsets[bank] + offset]
            }
            0x6000...0x7FFF => self.chr_ram[addr as usize - 0x6000],
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
                self.rom.chr_rom[self.chr_offsets[bank] + offset] = val;
            }
            0x6000...0x7FFF => self.chr_ram[addr as usize - 0x6000] = val,
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
        match self.mirror {
            Mirror::Horizontal => 0,
            Mirror::Vertical => 1,
            Mirror::OneScreenLower => 2,
            Mirror::OneScreenUpper => 3,
            _ => panic!("not possible"),
        }
    }

    // fn prg_readb(&mut self, addr: u16) -> u8 {
    //     match addr {
    //         0x0000...0x7FFF => 0,
    //         0x8000...0xBFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 Mmc1PrgBankMode::Fused => self.prg_bank & 0xFE,
    //                 Mmc1PrgBankMode::FixFirst => 0,
    //                 Mmc1PrgBankMode::FixLast => self.prg_bank,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)]
    //         }
    //         0xC000...0xFFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 Mmc1PrgBankMode::Fused => (self.prg_bank & 0xFE) | 1,
    //                 Mmc1PrgBankMode::FixFirst => self.prg_bank,
    //                 Mmc1PrgBankMode::FixLast => (self.rom.prg_rom_size() - 1) as u8,
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
    //                 Mmc1PrgBankMode::Fused => self.prg_bank & 0xFE,
    //                 Mmc1PrgBankMode::FixFirst => 0,
    //                 Mmc1PrgBankMode::FixLast => self.prg_bank,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)] = val;
    //         }
    //         0xC000...0xFFFF => {
    //             let bank = match self.prg_bank_mode() {
    //                 Mmc1PrgBankMode::Fused => (self.prg_bank & 0xFE) | 1,
    //                 Mmc1PrgBankMode::FixFirst => self.prg_bank,
    //                 Mmc1PrgBankMode::FixLast => (self.rom.prg_rom_size() - 1) as u8,
    //             };
    //             self.rom.prg_rom[(bank as usize * PRG_ROM_SIZE) | ((addr & 0x3FFF) as usize)] = val;
    //         }
    //     }
    // }

    // fn chr_readb(&mut self, addr: u16) -> u8 {
    //     match addr {
    //         0x0000...0x2000 {
    //             let bank = match self.chr_bank_mode() {
    //                 Mmc1ChrBankMode::Fused => self.chr_bank0 & 0xFE,
    //                 Mmc1ChrBankMode::Independent => self.chr_bank0,
    //             };
    //             self.rom.chr_rom[(bank as usize
    //         }
    //         0x6000...0x7FFF => self.chr_ram[addr as usize];
    //     }
    // }

    // fn chr_writeb(&mut self, addr: u16, val: u8) {}
}
