//! A Disassembler for the C6502 CPU

use crate::console::cpu::{AddrMode, Operation, INSTRUCTIONS};
use crate::memory::Memory;

use AddrMode::*;
use Operation::*;

/// Disaassembles a single CPU instruction
pub fn disassemble(mem: &mut Memory, pc: u16, x: u8, y: u8) -> (u8, String) {
    let opcode = mem.readb(pc);
    let instr = &INSTRUCTIONS[opcode as usize];
    let addr_mode = instr.addr_mode();

    let mut num_bytes = 2;
    let string = match addr_mode {
        Immediate => format!("{:?} #${:02X}", instr, mem.readb(pc + 1)),
        ZeroPage => {
            let addr = mem.readb(pc + 1);
            let val = mem.readb(addr.into());
            format!("{:?} ${:02X} = {:02X}", instr, addr, val)
        }
        ZeroPageX => {
            let addr = mem.readb(pc + 1);
            let addrx = addr.wrapping_add(x);
            let val = mem.readb(addrx.into());
            format!("{:?} ${:02X},X @ {:02X} = {:02X}", instr, addr, addrx, val)
        }
        ZeroPageY => {
            let addr = mem.readb(pc + 1);
            let addry = addr.wrapping_add(y);
            let val = mem.readb(addry.into());
            format!("{:?} ${:02X},Y @ {:02X} = {:02X}", instr, addr, addry, val)
        }
        Absolute => {
            num_bytes += 1;
            let addr = mem.readw(pc + 1);
            if instr.op() == JMP || instr.op() == JSR {
                format!("{:?} ${:04X}", instr, addr)
            } else {
                let val = mem.readb(addr);
                format!("{:?} ${:04X} = {:02X}", instr, addr, val)
            }
        }
        AbsoluteX => {
            num_bytes += 1;
            let addr = mem.readw(pc + 1);
            let addrx = addr.wrapping_add(x.into());
            let val = mem.readb(addrx);
            format!("{:?} ${:04X},X @ {:04X} = {:02X}", instr, addr, addrx, val)
        }
        AbsoluteY => {
            num_bytes += 1;
            let addr = mem.readw(pc + 1);
            let addry = addr.wrapping_add(y.into());
            let val = mem.readb(addry);
            format!("{:?} ${:04X},Y @ {:04X} = {:02X}", instr, addr, addry, val)
        }
        Indirect => {
            num_bytes += 1;
            let addr = mem.readw(pc + 1);
            if instr.op() == JMP {
                let val = mem.readw_pagewrap(addr);
                format!("{:?} (${:04X}) = {:04X}", instr, addr, val)
            } else {
                format!("{:?} (${:04X})", instr, addr)
            }
        }
        IndirectX => {
            let addr_zp = mem.readb(pc + 1);
            let addr_zpx = addr_zp.wrapping_add(x);
            let addr = mem.readw_zp(addr_zpx);
            let val = mem.readb(addr);
            format!(
                "{:?} (${:02X},X) @ {:02X} = {:04X} = {:02X}",
                instr, addr_zp, addr_zpx, addr, val
            )
        }
        IndirectY => {
            let addr_zp = mem.readb(pc + 1);
            let addr = mem.readw_zp(addr_zp);
            let addry = addr.wrapping_add(y.into());
            let val = mem.readb(addry);
            format!(
                "{:?} (${:02X}),Y = {:04X} @ {:04X} = {:02X}",
                instr, addr_zp, addr, addry, val
            )
        }
        Relative => {
            let offset = 2 + mem.readb(pc + 1);
            let addr = if offset & 0x80 > 0 {
                // Result is negative signed number in twos complement
                let offset = !offset + 1;
                pc.wrapping_sub(offset.into())
            } else {
                pc.wrapping_add(offset.into())
            };
            format!("{:?} ${:04X}", instr, addr)
        }
        Accumulator => {
            num_bytes -= 1;
            format!("{:?} A", instr)
        }
        Implied => {
            num_bytes -= 1;
            format!("{:?}", instr)
        }
    };
    (num_bytes, string)
}
