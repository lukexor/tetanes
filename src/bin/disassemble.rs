use std::{env, fmt::Write, path::PathBuf};
use structopt::StructOpt;
use tetanes::{
    cart::Cart,
    common::NesRegion,
    cpu::instr::{
        AddrMode::{ABS, ABX, ABY, ACC, IDX, IDY, IMM, IMP, IND, REL, ZP0, ZPX, ZPY},
        INSTRUCTIONS,
    },
    memory::RamState,
    NesResult,
};

fn main() -> NesResult<()> {
    env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();

    let opt = Opt::from_args();
    let cart = Cart::from_path(opt.path, NesRegion::default(), RamState::default())?;
    let mut addr = 0x0000;
    while addr <= cart.prg_rom.len() {
        log::info!("{}", disassemble(&mut addr, &cart.prg_rom));
    }

    Ok(())
}

#[derive(StructOpt, Debug)]
#[must_use]
struct Opt {
    #[structopt(help = "The NES ROM or a directory containing `.nes` ROM files.")]
    path: PathBuf,
}

pub fn disassemble(pc: &mut usize, rom: &[u8]) -> String {
    let opcode = rom[*pc];
    let instr = INSTRUCTIONS[opcode as usize];
    let mut bytes = Vec::with_capacity(3);
    let mut disasm = String::with_capacity(100);
    let _ = write!(disasm, "${:04X}:", pc);
    bytes.push(opcode);
    let mut addr = pc.wrapping_add(1);
    let mode = match instr.addr_mode() {
        IMM => {
            bytes.push(rom[addr]);
            addr = addr.wrapping_add(1);
            format!("#${:02X}", bytes[1])
        }
        ZP0 => {
            bytes.push(rom[addr]);
            addr = addr.wrapping_add(1);
            format!("${:02X}", bytes[1])
        }
        ZPX => {
            bytes.push(rom[addr]);
            addr = addr.wrapping_add(1);
            format!("${:02X},X", bytes[1])
        }
        ZPY => {
            bytes.push(rom[addr]);
            addr = addr.wrapping_add(1);
            format!("${:02X},Y", bytes[1])
        }
        ABS => {
            bytes.push(rom[addr]);
            bytes.push(rom[addr.wrapping_add(1)]);
            let lo = rom[addr] as usize;
            let hi = rom[addr.wrapping_add(1)] as usize;
            let abs_addr = (hi << 8) | lo;
            addr = addr.wrapping_add(2);
            format!("${:04X}", abs_addr)
        }
        ABX => {
            bytes.push(rom[addr]);
            bytes.push(rom[addr.wrapping_add(1)]);
            let lo = rom[addr] as usize;
            let hi = rom[addr.wrapping_add(1)] as usize;
            let abs_addr = (hi << 8) | lo;
            addr = addr.wrapping_add(2);
            format!("${:04X},X", abs_addr)
        }
        ABY => {
            bytes.push(rom[addr]);
            bytes.push(rom[addr.wrapping_add(1)]);
            let lo = rom[addr] as usize;
            let hi = rom[addr.wrapping_add(1)] as usize;
            let abs_addr = (hi << 8) | lo;
            addr = addr.wrapping_add(2);
            format!("${:04X},Y", abs_addr)
        }
        IND => {
            bytes.push(rom[addr]);
            bytes.push(rom[addr.wrapping_add(1)]);
            let lo = rom[addr] as usize;
            let hi = rom[addr.wrapping_add(1)] as usize;
            let abs_addr = (hi << 8) | lo;
            addr = addr.wrapping_add(2);
            format!("(${:04X})", abs_addr)
        }
        IDX => {
            bytes.push(rom[addr]);
            addr = addr.wrapping_add(1);
            format!("(${:02X},X)", bytes[1])
        }
        IDY => {
            bytes.push(rom[addr]);
            addr = addr.wrapping_add(1);
            format!("(${:02X})", bytes[1])
        }
        REL => {
            bytes.push(rom[addr]);
            let mut rel_addr = rom[addr].into();
            addr = addr.wrapping_add(1);
            if rel_addr & 0x80 == 0x80 {
                // If address is negative, extend sign to 16-bits
                rel_addr |= 0xFF00;
            }
            format!("${:04X}", addr.wrapping_add(rel_addr))
        }
        ACC | IMP => "".to_string(),
    };
    *pc = addr;
    for i in 0..3 {
        if i < bytes.len() {
            let _ = write!(disasm, "{:02X} ", bytes[i]);
        } else {
            disasm.push_str("   ");
        }
    }
    let _ = write!(disasm, "{:?} {}", instr, mode);
    disasm
}
