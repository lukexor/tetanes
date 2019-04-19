use super::{cartridge::Cartridge, console::Console};

pub fn prg_bank_offset(cartridge: &Cartridge, mut index: isize, offset: isize) -> usize {
    if index >= 0x80 {
        index -= 0x100;
    }
    index %= cartridge.prg.len() as isize / offset;
    let mut offset = index * offset;
    if offset < 0 {
        offset += cartridge.prg.len() as isize;
    }
    offset as usize
}

pub fn read_byte(c: &Console, addr: u16) -> u8 {
    // println!("reading from 0x{:04X}", addr);
    match addr {
        0x0000...0x1FFF => c.ram[(addr % 0x0800) as usize],
        0x2000...0x3FFF => unimplemented!("c.ppu.read_reg(0x2000 + {} % 8)", addr),
        0x4000...0x4013 => unimplemented!("c.apu.read_reg(addr, val)"),
        0x4014 => unimplemented!("c.ppu.read_reg(addr)"),
        0x4015 => unimplemented!("c.apu.read_reg(addr)"),
        0x4016 => unimplemented!("c.controller1.read_byte()"),
        0x4017 => unimplemented!("c.controller2.read_byte()"),
        0x4018...0x5FFF => unimplemented!("I/O registers"),
        0x6000..=0xFFFF => read_mapper(c, addr),
    }
}

pub fn read_mapper(c: &Console, addr: u16) -> u8 {
    match addr {
        0x8000...0xFFFF => c.mapper.read(&c.cartridge, addr),
        _ => panic!("unhandled mapper1 read at addr 0x{:04X}", addr),
    }
}

pub fn read16(c: &Console, addr: u16) -> u16 {
    let lo = u16::from(read_byte(c, addr));
    let hi = u16::from(read_byte(c, addr + 1));
    hi << 8 | lo
}

pub fn write(c: &mut Console, addr: u16, val: u8) {
    match addr {
        0x0000...0x1FFF => c.ram[(addr % 0x8000) as usize] = val,
        0x2000...0x3FFF => unimplemented!("c.ppu.write_reg(0x2000 + addr % 8, val)"),
        0x4000...0x4013 => unimplemented!("c.apu.write_reg(addr, val)"),
        0x4014 => unimplemented!("c.ppu.write_reg(addr, val)"),
        0x4015 => unimplemented!("c.apu.write_reg(addr, val)"),
        0x4016 => unimplemented!("c.controllers.write(addr, val)"),
        0x4017 => unimplemented!("c.apu.write_reg(addr, val)"),
        0x4018...0x5FFF => unimplemented!("I/O registeres"),
        0x6000..=0xFFFF => unimplemented!("c.mapper.write_reg(addr, val)"),
    }
}

// read16bug emulates a 6502 bug that caused the low byte to wrap without
// incrementing the high byte
pub fn read16bug(c: &Console, addr: u16) -> u16 {
    let lo = u16::from(read_byte(c, addr));
    let addr = (addr & 0xFF00) | u16::from(addr as u8 + 1);
    let hi = u16::from(read_byte(c, addr));
    hi << 8 | lo
}

/// Stack Functions

// Push byte to stack
pub fn push(c: &mut Console, val: u8) {
    // println!(
    //     "writing 0x{:04X} to stack (0x{:04X})",
    //     val,
    //     0x100 | u16::from(c.cpu.sp)
    // );
    write(c, 0x100 | u16::from(c.cpu.sp), val);
    c.cpu.sp -= 1;
}

// Pull byte from stack
pub fn pull(c: &mut Console) -> u8 {
    // println!(
    //     "pulling 0x{:04X} from stack (0x{:04X})",
    //     read_byte(c, 0x100 | u16::from(c.cpu.sp)),
    //     0x100 | u16::from(c.cpu.sp)
    // );
    c.cpu.sp += 1;
    read_byte(c, 0x100 | u16::from(c.cpu.sp))
}

// Push two bytes to stack
pub fn push16(c: &mut Console, val: u16) {
    let lo = (val & 0xFF) as u8;
    let hi = (val >> 8) as u8;
    push(c, hi);
    push(c, lo);
}

// Pull two bytes from stack
pub fn pull16(c: &mut Console) -> u16 {
    let lo = u16::from(pull(c));
    let hi = u16::from(pull(c));
    hi << 8 | lo
}
