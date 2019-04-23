use super::console::Console;

const MIRROR_LOOKUP: [[u8; 4]; 5] = [
    [0, 0, 1, 1],
    [0, 1, 0, 1],
    [0, 0, 0, 0],
    [1, 1, 1, 1],
    [0, 1, 2, 3],
];

pub fn read_byte(c: &mut Console, addr: u16) -> u8 {
    // println!("reading from 0x{:04X}", addr);
    match addr {
        0x0000...0x1FFF => c.ram[(addr % 0x0800) as usize],
        0x2000...0x3FFF => read_ppu_register(c, 0x2000 + addr % 8),
        0x4000...0x4013 => c.apu.read_register(addr),
        0x4014 => read_ppu_register(c, addr),
        0x4015 => c.apu.read_register(addr),
        0x4016 => unimplemented!("c.controller1.read_byte()"),
        0x4017 => unimplemented!("c.controller2.read_byte()"),
        0x4018...0x5FFF => unimplemented!("I/O registers"),
        0x6000..=0xFFFF => read_mapper(c, addr),
    }
}

pub fn read16(c: &mut Console, addr: u16) -> u16 {
    let lo = u16::from(read_byte(c, addr));
    let hi = u16::from(read_byte(c, addr + 1));
    hi << 8 | lo
}

pub fn write_byte(c: &mut Console, addr: u16, val: u8) {
    match addr {
        0x0000...0x1FFF => c.ram[(addr % 0x8000) as usize] = val,
        0x2000...0x3FFF => write_ppu_register(c, 0x2000 + addr % 8, val),
        0x4000...0x4013 => c.apu.write_register(addr, val),
        0x4014 => write_ppu_register(c, addr, val),
        0x4015 => c.apu.write_register(addr, val),
        0x4016 => unimplemented!("c.controllers.write_byte(addr, val)"),
        0x4017 => c.apu.write_register(addr, val),
        0x4018...0x5FFF => unimplemented!("I/O registeres"),
        0x6000..=0xFFFF => unimplemented!("c.mapper.write_register(addr, val)"),
    }
}

pub fn read_ppu_register(c: &mut Console, addr: u16) -> u8 {
    match addr {
        0x2002 => {
            let mut result = c.ppu.register & 0x1F;
            result |= c.ppu.flag_sprite_overflow << 5;
            result |= c.ppu.flag_sprite_zero_hit << 6;
            if c.ppu.nmi_occurred {
                result |= 1 << 7;
            }
            c.ppu.nmi_occurred = false;
            c.ppu.nmi_change();
            c.ppu.w = 0;
            result
        }
        0x2004 => c.ppu.oam_data[c.ppu.oam_address as usize],
        0x2007 => {
            let mut val = read_ppu(c, c.ppu.v);
            if c.ppu.v % 0x4000 < 0x3F00 {
                std::mem::swap(&mut c.ppu.buffered_data, &mut val)
            } else {
                c.ppu.buffered_data = read_ppu(c, c.ppu.v - 0x1000);
            }
            if c.ppu.flag_increment {
                c.ppu.v += 32;
            } else {
                c.ppu.v += 1;
            }
            val
        }
        _ => panic!("unhandled PPU register read at address 0x{:04X}", addr),
    }
}

pub fn write_ppu_register(c: &mut Console, addr: u16, val: u8) {
    c.ppu.register = val;
    match addr {
        0x2000 => c.ppu.write_control(val),
        0x2001 => c.ppu.write_mask(val),
        0x2003 => c.ppu.oam_address = val,
        0x2004 => {
            // write oam data
            c.ppu.oam_data[c.ppu.oam_address as usize] = val;
            c.ppu.oam_address += 1;
        }
        0x2005 => {
            // write scroll
            if c.ppu.w == 0 {
                c.ppu.t = (c.ppu.t & 0xFFE0) | (u16::from(val) >> 3);
                c.ppu.x = val & 0x07;
                c.ppu.w = 1;
            } else {
                c.ppu.t = (c.ppu.t & 0x8FFF) | ((u16::from(val) & 0x07) << 12);
                c.ppu.t = (c.ppu.t & 0xFC1F) | ((u16::from(val) & 0xF8) << 2);
                c.ppu.w = 0;
            }
        }
        0x2006 => {
            // write address
            if c.ppu.w == 0 {
                c.ppu.t = (c.ppu.t & 0x80FF) | ((u16::from(val) & 0x3F) << 8);
                c.ppu.w = 1;
            } else {
                c.ppu.t = (c.ppu.t & 0xFF00) | u16::from(val);
                c.ppu.v = c.ppu.t;
                c.ppu.w = 0;
            }
        }
        0x2007 => {
            // write data
            write_ppu(c, c.ppu.v, val);
            if c.ppu.flag_increment {
                c.ppu.v += 32;
            } else {
                c.ppu.v += 1;
            }
        }
        0x4014 => {
            let mut addr = u16::from(val) << 8;
            for _ in 0..256 {
                c.ppu.oam_data[c.ppu.oam_address as usize] = read_byte(c, addr);
                c.ppu.oam_address += 1;
                addr += 1;
            }
            c.cpu.stall += 513;
            if c.cpu.cycles % 2 == 1 {
                c.cpu.stall += 1;
            }
        }
        _ => panic!("unhandled PPU register write at address 0x{:04X}", addr),
    }
}

pub fn read_ppu(c: &mut Console, mut addr: u16) -> u8 {
    addr %= 0x4000;
    match addr {
        0x0000...0x1FFF => read_mapper(c, addr),
        0x2000...0x3EFF => c
            .ppu
            .name_table_data(mirror_address(c.cartridge.mirror, addr)),
        0x3F00...0x4000 => c.ppu.read_palette(addr % 32),
        _ => panic!("unhandled PPU memory read at addr 0x{:04X}", addr),
    }
}

pub fn write_ppu(c: &mut Console, mut addr: u16, val: u8) {
    addr %= 0x4000;
    match addr {
        0x0000...0x1FFF => write_mapper(c, addr, val),
        0x2000...0x3EFF => c
            .ppu
            .set_name_table_data(mirror_address(c.cartridge.mirror, addr), val),
        0x3F00...0x4000 => c.ppu.write_palette(addr % 32, val),
        _ => panic!("unhandled PPU memory write at addr 0x{:04X}", addr),
    }
}

pub fn read_mapper(c: &mut Console, addr: u16) -> u8 {
    match addr {
        0x8000...0xFFFF => c.mapper.read(&c.cartridge, addr),
        _ => panic!("unhandled mapper memory read at addr 0x{:04X}", addr),
    }
}

pub fn write_mapper(c: &mut Console, addr: u16, val: u8) {
    unimplemented!();
}

// read16bug emulates a 6502 bug that caused the low byte to wrap without
// incrementing the high byte
pub fn read16bug(c: &mut Console, addr: u16) -> u16 {
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
    write_byte(c, 0x100 | u16::from(c.cpu.sp), val);
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

fn mirror_address(mode: u8, mut addr: u16) -> u16 {
    addr = (addr - 0x2000) % 0x1000;
    let table = addr / 0x0400;
    let offset = addr % 0x0400;
    0x2000 + u16::from(MIRROR_LOOKUP[mode as usize][table as usize]) * 0x0400 + offset
}
