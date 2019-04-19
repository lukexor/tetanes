use super::cartridge::Cartridge;

pub trait Mapper {
    fn name(&self) -> &'static str;
    fn read(&self, cartridge: &Cartridge, addr: u16) -> u8;
}

#[derive(Default, Debug)]
pub struct Mapper1 {
    pub shift_register: u8,
    pub control: u8,
    pub prg_mode: u8,
    pub chr_mode: u8,
    pub prg_bank: u8,
    pub chr_bank0: u8,
    pub chr_bank1: u8,
    pub prg_offsets: [usize; 2],
    pub chr_offsets: [usize; 2],
}

impl Mapper for Mapper1 {
    fn name(&self) -> &'static str {
        "Mapper1"
    }

    fn read(&self, cartridge: &Cartridge, addr: u16) -> u8 {
        let addr = addr - 0x8000;
        let bank = (addr / 0x4000) as usize;
        let offset = (addr % 0x4000) as usize;
        cartridge.prg[self.prg_offsets[bank] + offset]
    }
}

#[derive(Default, Debug)]
pub struct Mapper2 {
    pub prg_banks: usize,
    pub prg_bank1: usize,
    pub prg_bank2: usize,
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
pub struct Mapper3 {
    pub chr_bank: usize,
    pub prg_bank1: usize,
    pub prg_bank2: usize,
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
pub struct Mapper4 {
    pub register: u8,
    pub registers: [u8; 8],
    pub prg_mode: u8,
    pub chr_mode: u8,
    pub prg_offsets: [usize; 4],
    pub chr_offsets: [usize; 8],
    pub reload: u8,
    pub counter: u8,
    pub irq_enable: bool,
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
pub struct Mapper7 {
    pub prg_bank: usize,
}

impl Mapper for Mapper7 {
    fn name(&self) -> &'static str {
        "Mapper7"
    }

    fn read(&self, _cartridge: &Cartridge, _addr: u16) -> u8 {
        unimplemented!();
    }
}
