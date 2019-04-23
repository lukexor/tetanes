use super::rom::Rom;

// pub trait Mapper {
//     fn name(&self) -> &'static str;
//     fn read(&self, rom: &Rom, addr: u16) -> u8;
// }

// pub fn new_mapper(mapper: u8, prg_size: usize) -> Result<Box<Mapper>, Box<Error>> {
//     match mapper {
//         // 0 | 2 => Ok(Box::new(Mapper2::new(prg_size))),
//         1 => Ok(Box::new(Mapper1::new(prg_size))),
//         // 3 => Ok(Box::new(Mapper3::new(prg_size))),
//         // 4 => Ok(Box::new(Mapper4::new(prg_size))),
//         // 7 => Ok(Box::new(Mapper7::new())),
//         _ => Err(format!("unsupported mapper number: {}", mapper).into()),
//     }
// }

fn bank_offset(size: usize, mut index: isize, offset: isize) -> usize {
    if index >= 0x80 {
        index -= 0x100;
    }
    index %= size as isize / offset;
    let mut offset = index * offset;
    if offset < 0 {
        offset += size as isize;
    }
    offset as usize
}

#[derive(Default, Debug)]
pub struct Mapper1 {
    shift_register: u8,
    control: u8,
    prg_mode: u8,
    chr_mode: u8,
    prg_bank: u8,
    chr_bank0: u8,
    chr_bank1: u8,
    prg_offsets: [usize; 2],
    chr_offsets: [usize; 2],
}

impl Mapper1 {
    pub fn new(prg_size: usize) -> Self {
        Self {
            shift_register: 0x10,
            prg_offsets: [0, bank_offset(prg_size, -1, 0x4000)],
            ..Default::default()
        }
    }

    pub fn read(&self, rom: &Rom, addr: u16) -> u8 {
        match addr {
            0x0000...0x2000 => {
                let bank = (addr / 0x1000) as usize;
                let offset = (addr % 0x1000) as usize;
                rom.chr[self.chr_offsets[bank] + offset]
            }
            // 0x6000...0x7FFF => rom.sram[addr as usize - 0x6000],
            0x8000...0xFFFF => {
                let addr = addr - 0x8000;
                let bank = (addr / 0x4000) as usize;
                let offset = (addr % 0x4000) as usize;
                rom.prg[self.prg_offsets[bank] + offset]
            }
            _ => panic!("unhandled mapper1 read at address: 0x{:04X}", addr),
        }
    }

    pub fn write(&mut self, rom: &mut Rom, addr: u16, val: u8) {
        match addr {
            0x0000...0x2000 => {
                let bank = (addr / 0x1000) as usize;
                let offset = (addr % 0x1000) as usize;
                rom.chr[self.chr_offsets[bank] + offset] = val;
            }
            // 0x6000...0x7FFF => rom.sram[addr as usize - 0x6000] = val,
            0x8000...0xFFFF => {
                if val & 0x80 == 0x80 {
                    self.shift_register = 0x10;
                    self.write_control(rom, self.control | 0x0C);
                    self.update_offsets(rom);
                } else {
                    let complete = self.shift_register & 1 == 1;
                    self.shift_register >>= 1;
                    self.shift_register |= (val & 1) << 4;
                    if complete {
                        match addr {
                            0x0000...0x9FFF => self.write_control(rom, self.shift_register),
                            0xA000...0xBFFF => self.chr_bank0 = self.shift_register,
                            0xC000...0xDFFF => self.chr_bank1 = self.shift_register,
                            0xE000...0xFFFF => self.prg_bank = self.shift_register & 0x0F,
                        }
                        self.update_offsets(rom);
                        self.shift_register = 0x10;
                    }
                }
            }
            _ => panic!("unhandled mapper1 write at address: 0x{:04X}", addr),
        }
    }

    fn update_offsets(&mut self, rom: &mut Rom) {
        let prg_size = rom.prg.len();
        let chr_size = rom.chr.len();
        match self.prg_mode {
            0 | 1 => {
                self.prg_offsets[0] =
                    bank_offset(prg_size, (self.prg_bank & 0xFE) as isize, 0x4000);
                self.prg_offsets[1] =
                    bank_offset(prg_size, (self.prg_bank | 0x01) as isize, 0x4000);
            }
            2 => {
                self.prg_offsets[0] = 0;
                self.prg_offsets[1] = bank_offset(prg_size, self.prg_bank as isize, 0x4000);
            }
            3 => {
                self.prg_offsets[0] = bank_offset(prg_size, self.prg_bank as isize, 0x4000);
                self.prg_offsets[1] = bank_offset(prg_size, -1, 0x4000);
            }
            _ => panic!("invalid prg_mode {}", self.prg_mode),
        }
        match self.chr_mode {
            0 => {
                self.chr_offsets[0] =
                    bank_offset(chr_size, (self.chr_bank0 & 0xFE) as isize, 0x1000);
                self.chr_offsets[1] =
                    bank_offset(chr_size, (self.chr_bank0 | 0x01) as isize, 0x1000);
            }
            1 => {
                self.chr_offsets[0] = bank_offset(chr_size, self.chr_bank0 as isize, 0x1000);
                self.chr_offsets[1] = bank_offset(chr_size, self.chr_bank1 as isize, 0x1000);
            }
            _ => panic!("invalid chr_mode {}", self.chr_mode),
        }
    }

    fn write_control(&mut self, rom: &mut Rom, val: u8) {
        self.control = val;
        self.chr_mode = (val >> 4) & 1;
        self.prg_mode = (val >> 2) & 3;
        // rom.mirror = match val & 3 {
        //     0 => 2,
        //     1 => 3,
        //     2 => 1,
        //     3 => 0,
        //     _ => panic!("invalid mirror mode {}", val & 3),
        // }
    }
}

// impl Mapper for Mapper1 {
//     fn name(&self) -> &'static str {
//         "Mapper1"
//     }

//     fn read(&self, rom: &Rom, addr: u16) -> u8 {
//         let addr = addr - 0x8000;
//         let prg_bank = (addr / 0x4000) as usize;
//         let prg_offset = (addr % 0x4000) as usize;
//         rom.prg[self.prg_offsets[prg_bank] + prg_offset]
//     }
// }

// #[derive(Default, Debug)]
// struct Mapper2 {
//     prg_banks: usize,
//     prg_bank1: usize,
//     prg_bank2: usize,
// }

// impl Mapper2 {
//     fn new(prg_size: usize) -> Self {
//         let prg_banks = prg_size / 0x4000;
//         Self {
//             prg_banks,
//             prg_bank2: (prg_banks - 1),
//             ..Default::default()
//         }
//     }
// }

// impl Mapper for Mapper2 {
//     fn name(&self) -> &'static str {
//         "Mapper2"
//     }

//     fn read(&self, _rom: &Rom, _addr: u16) -> u8 {
//         unimplemented!();
//     }
// }

// #[derive(Default, Debug)]
// struct Mapper3 {
//     chr_bank: usize,
//     prg_bank1: usize,
//     prg_bank2: usize,
// }

// impl Mapper3 {
//     fn new(prg_size: usize) -> Self {
//         let prg_banks = prg_size / 0x4000;
//         Self {
//             prg_bank2: (prg_banks - 1) as usize,
//             ..Default::default()
//         }
//     }
// }

// impl Mapper for Mapper3 {
//     fn name(&self) -> &'static str {
//         "Mapper3"
//     }

//     fn read(&self, _rom: &Rom, _addr: u16) -> u8 {
//         unimplemented!();
//     }
// }

// #[derive(Default, Debug)]
// struct Mapper4 {
//     register: u8,
//     registers: [u8; 8],
//     prg_mode: u8,
//     chr_mode: u8,
//     prg_offsets: [usize; 4],
//     chr_offsets: [usize; 8],
//     reload: u8,
//     counter: u8,
//     irq_enable: bool,
// }

// impl Mapper4 {
//     fn new(prg_size: usize) -> Self {
//         Self {
//             prg_offsets: [
//                 prg_bank_offset(prg_size, 0, 0x2000),
//                 prg_bank_offset(prg_size, 1, 0x2000),
//                 prg_bank_offset(prg_size, -2, 0x2000),
//                 prg_bank_offset(prg_size, -1, 0x2000),
//             ],
//             ..Default::default()
//         }
//     }
// }

// impl Mapper for Mapper4 {
//     fn name(&self) -> &'static str {
//         "Mapper4"
//     }

//     fn read(&self, _rom: &Rom, _addr: u16) -> u8 {
//         unimplemented!();
//     }
// }

// #[derive(Default, Debug)]
// struct Mapper7 {
//     prg_bank: usize,
// }

// impl Mapper7 {
//     fn new() -> Self {
//         Self {
//             ..Default::default()
//         }
//     }
// }

// impl Mapper for Mapper7 {
//     fn name(&self) -> &'static str {
//         "Mapper7"
//     }

//     fn read(&self, _rom: &Rom, _addr: u16) -> u8 {
//         unimplemented!();
//     }
// }
