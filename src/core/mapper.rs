pub trait Mapper {
    fn mapper(&self);
}

impl Mapper for Mapper1 {
    fn mapper(&self) {}
}
impl Mapper for Mapper2 {
    fn mapper(&self) {}
}
impl Mapper for Mapper3 {
    fn mapper(&self) {}
}
impl Mapper for Mapper4 {
    fn mapper(&self) {}
}
impl Mapper for Mapper7 {
    fn mapper(&self) {}
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
    pub prg_offsets: [isize; 2],
    pub chr_offsets: [isize; 2],
}

#[derive(Default, Debug)]
pub struct Mapper2 {
    pub prg_banks: isize,
    pub prg_bank1: isize,
    pub prg_bank2: isize,
}

#[derive(Default, Debug)]
pub struct Mapper3 {
    pub chr_bank: isize,
    pub prg_bank1: isize,
    pub prg_bank2: isize,
}

#[derive(Default, Debug)]
pub struct Mapper4 {
    pub register: u8,
    pub registers: [u8; 8],
    pub prg_mode: u8,
    pub chr_mode: u8,
    pub prg_offsets: [isize; 4],
    pub chr_offsets: [isize; 8],
    pub reload: u8,
    pub counter: u8,
    pub irq_enable: bool,
}

#[derive(Default, Debug)]
pub struct Mapper7 {
    pub prg_bank: isize,
}
