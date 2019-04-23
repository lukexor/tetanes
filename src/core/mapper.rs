use super::cartridge::Cartridge;
use std::error::Error;

pub trait Mapper {
    fn name(&self) -> &'static str;
    fn read(&self, cartridge: &Cartridge, addr: u16) -> u8;
}

pub fn new_mapper(mapper: u8, prg_size: usize) -> Result<Box<Mapper>, Box<Error>> {
    match mapper {
        0 | 2 => Ok(Box::new(Mapper2::new(prg_size))),
        1 => Ok(Box::new(Mapper1::new(prg_size))),
        3 => Ok(Box::new(Mapper3::new(prg_size))),
        4 => Ok(Box::new(Mapper4::new(prg_size))),
        7 => Ok(Box::new(Mapper7::new())),
        _ => Err(format!("unsupported mapper number: {}", mapper).into()),
    }
}

fn prg_bank_offset(prg_size: usize, mut index: isize, offset: isize) -> usize {
    if index >= 0x80 {
        index -= 0x100;
    }
    index %= prg_size as isize / offset;
    let mut offset = index * offset;
    if offset < 0 {
        offset += prg_size as isize;
    }
    offset as usize
}

#[derive(Default, Debug)]
struct Mapper1 {
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
    fn new(prg_size: usize) -> Self {
        Self {
            shift_register: 0x10,
            prg_offsets: [0, prg_bank_offset(prg_size, -1, 0x4000)],
            ..Default::default()
        }
    }
}

impl Mapper for Mapper1 {
    fn name(&self) -> &'static str {
        "Mapper1"
    }

    fn read(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        let addr = addr - 0x8000;
        let prg_bank = (addr / 0x4000) as usize;
        let prg_offset = (addr % 0x4000) as usize;
        cartridge.prg[self.prg_offsets[prg_bank] + prg_offset]
    }
}

#[derive(Default, Debug)]
struct Mapper2 {
    prg_banks: usize,
    prg_bank1: usize,
    prg_bank2: usize,
}

impl Mapper2 {
    fn new(prg_size: usize) -> Self {
        let prg_banks = prg_size / 0x4000;
        Self {
            prg_banks,
            prg_bank2: (prg_banks - 1),
            ..Default::default()
        }
    }
}

impl Mapper for Mapper2 {
    fn name(&self) -> &'static str {
        "Mapper2"
    }

    fn read(&self, _cartridge: &Cartridge, _addr: u16) -> u8 {
        unimplemented!();
    }
}

#[derive(Default, Debug)]
struct Mapper3 {
    chr_bank: usize,
    prg_bank1: usize,
    prg_bank2: usize,
}

impl Mapper3 {
    fn new(prg_size: usize) -> Self {
        let prg_banks = prg_size / 0x4000;
        Self {
            prg_bank2: (prg_banks - 1) as usize,
            ..Default::default()
        }
    }
}

impl Mapper for Mapper3 {
    fn name(&self) -> &'static str {
        "Mapper3"
    }

    fn read(&self, _cartridge: &Cartridge, _addr: u16) -> u8 {
        unimplemented!();
    }
}

#[derive(Default, Debug)]
struct Mapper4 {
    register: u8,
    registers: [u8; 8],
    prg_mode: u8,
    chr_mode: u8,
    prg_offsets: [usize; 4],
    chr_offsets: [usize; 8],
    reload: u8,
    counter: u8,
    irq_enable: bool,
}

impl Mapper4 {
    fn new(prg_size: usize) -> Self {
        Self {
            prg_offsets: [
                prg_bank_offset(prg_size, 0, 0x2000),
                prg_bank_offset(prg_size, 1, 0x2000),
                prg_bank_offset(prg_size, -2, 0x2000),
                prg_bank_offset(prg_size, -1, 0x2000),
            ],
            ..Default::default()
        }
    }
}

impl Mapper for Mapper4 {
    fn name(&self) -> &'static str {
        "Mapper4"
    }

    fn read(&self, _cartridge: &Cartridge, _addr: u16) -> u8 {
        unimplemented!();
    }
}

#[derive(Default, Debug)]
struct Mapper7 {
    prg_bank: usize,
}

impl Mapper7 {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}

impl Mapper for Mapper7 {
    fn name(&self) -> &'static str {
        "Mapper7"
    }

    fn read(&self, _cartridge: &Cartridge, _addr: u16) -> u8 {
        unimplemented!();
    }
}
