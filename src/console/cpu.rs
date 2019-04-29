use super::{
    memory::{readb, writeb},
    Console,
};

pub const CPU_FREQUENCY: f64 = 1_789_773.0;

/// Interrupt Types
pub enum Interrupt {
    None,
    NMI,
    IRQ,
}

/// The Central Processing Unit
///
/// 7  bit  0
/// ---- ----
/// NVss DIZC
/// |||| ||||
/// |||| |||+- Carry
/// |||| ||+-- Zero
/// |||| |+--- Interrupt Disable
/// |||| +---- Decimal
/// ||++------ No CPU effect, see: the B flag
/// |+-------- Overflow
/// +--------- Negative
pub struct CPU {
    pub cycles: u64,             // number of cycles
    pub pc: u16,                 // program counter
    sp: u8,                      // stack pointer - stack is at $0100-$01FF
    acc: u8,                     // accumulator
    x: u8,                       // x register
    y: u8,                       // y register
    carry: bool,                 // carry flag
    zero: bool,                  // zero flag
    pub interrupt_disable: bool, // interrupt disable flag
    decimal: bool,               // decimal mode flag
    // b: u8,                    // break command flag
    // u: u8,                    // unused flag
    overflow: bool,           // overflow flag
    negative: bool,           // negative flag
    pub interrupt: Interrupt, // interrupt type to perform
    pub stall: isize,         // number of cycles to stall
    #[cfg(debug_assertions)]
    pub oplog: String,
}

impl CPU {
    pub fn new() -> Self {
        Self {
            cycles: 0,
            pc: 0,
            sp: 0,
            acc: 0,
            x: 0,
            y: 0,
            carry: false,
            zero: false,
            interrupt_disable: false,
            decimal: false,
            overflow: false,
            negative: false,
            interrupt: Interrupt::None,
            stall: 0,
            #[cfg(debug_assertions)]
            oplog: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.sp = 0xFD;
        self.set_flags(0x24);
        self.cycles = 7;
        self.stall = 6;
    }
}

/// PHP: Push Processor Status on Stack
pub fn php(c: &mut Console) {
    let flags = c.cpu.flags();
    push_stackb(c, flags | 0x10); // Set Decimal Mode Flag
}

pub fn execute(c: &mut Console, opcode: u8) {
    let (op, addr_mode, cycles, page_cycles) = OPCODES[opcode as usize];
    let (val, target, num_args, page_crossed) =
        decode_addr_mode(c, addr_mode, c.cpu.pc.wrapping_add(1), op);
    #[cfg(debug_assertions)]
    {
        if c.trace > 0 {
            print_instruction(c, op, opcode, 1 + num_args);
        }
    }
    c.cpu.pc = c.cpu.pc.wrapping_add(1 + num_args);
    c.cpu.cycles += cycles;
    if page_crossed {
        c.cpu.cycles += page_cycles;
    }

    let val = val as u8;
    match op {
        ADC => c.cpu.adc(val),
        AND => c.cpu.and(val),
        ASL => {
            let val = read_target(c, target);
            let wr = c.cpu.asl(val);
            write_target(c, target, wr);
        }
        BCC => c.cpu.bcc(val),
        BCS => c.cpu.bcs(val),
        BEQ => c.cpu.beq(val),
        BIT => c.cpu.bit(val),
        BMI => c.cpu.bmi(val),
        BNE => c.cpu.bne(val),
        BPL => c.cpu.bpl(val),
        // Break Interrupt
        BRK => {
            push_stackw(c, c.cpu.pc);
            php(c);
            c.cpu.sei();
            c.cpu.pc = readw(c, 0xFFFE);
        }
        BVC => c.cpu.bvc(val),
        BVS => c.cpu.bvs(val),
        CLC => c.cpu.clc(),
        CLD => c.cpu.cld(),
        CLI => c.cpu.cli(),
        CLV => c.cpu.clv(),
        CMP => c.cpu.cmp(val),
        CPX => c.cpu.cpx(val),
        CPY => c.cpu.cpy(val),
        DEC => {
            let val = read_target(c, target);
            let wr = c.cpu.dec(val);
            write_target(c, target, wr);
        }
        DEX => c.cpu.dex(),
        DEY => c.cpu.dey(),
        EOR => c.cpu.eor(val),
        INC => {
            let val = read_target(c, target);
            let wr = c.cpu.inc(val);
            write_target(c, target, wr);
        }
        INX => c.cpu.inx(),
        INY => c.cpu.iny(),
        JMP => c.cpu.jmp(target.unwrap()),
        JSR => {
            push_stackw(c, c.cpu.pc.wrapping_sub(1));
            c.cpu.jsr(target.unwrap());
        }
        LAX => c.cpu.lax(val),
        LDA => c.cpu.lda(val),
        LDX => c.cpu.ldx(val),
        LDY => c.cpu.ldy(val),
        LSR => {
            let val = read_target(c, target);
            let wr = c.cpu.lsr(val);
            write_target(c, target, wr);
        }
        NOP => c.cpu.nop(),
        ORA => c.cpu.ora(val),
        // PHA: Push A on Stack
        PHA => {
            push_stackb(c, c.cpu.acc);
        }
        PHP => php(c),
        // PLA: Pull A from Stack
        PLA => {
            c.cpu.acc = pop_stackb(c);
            c.cpu.update_acc();
        }
        PLP => {
            let status = pop_stackb(c);
            c.cpu.plp(status);
        }
        ROL => {
            let val = read_target(c, target);
            let wr = c.cpu.rol(val);
            write_target(c, target, wr);
        }
        ROR => {
            let val = read_target(c, target);
            let wr = c.cpu.ror(val);
            write_target(c, target, wr);
        }
        RTI => {
            let flags = pop_stackb(c);
            c.cpu.rti(flags);
            c.cpu.pc = pop_stackw(c);
        }
        // RTS: Return from Subroutine
        RTS => {
            c.cpu.pc = pop_stackw(c).wrapping_add(1);
        }
        SBC => c.cpu.sbc(val),
        SEC => c.cpu.sec(),
        SED => c.cpu.sed(),
        SEI => c.cpu.sei(),
        STA => write_target(c, target, c.cpu.acc),
        STX => write_target(c, target, c.cpu.x),
        STY => write_target(c, target, c.cpu.y),
        TAX => c.cpu.tax(),
        TAY => c.cpu.tay(),
        TSX => c.cpu.tsx(),
        TXA => c.cpu.txa(),
        TXS => c.cpu.txs(),
        TYA => c.cpu.tya(),
        KIL => panic!("KIL encountered"),
        SAX => {
            let val = read_target(c, target);
            let wr = c.cpu.sax();
            write_target(c, target, wr);
        }
        DCP => {
            let val = read_target(c, target);
            let wr = c.cpu.dcp(val);
            write_target(c, target, wr);
        }
        ISB => {
            let val = read_target(c, target);
            let wr = c.cpu.isb(val);
            write_target(c, target, wr);
        }
        RLA => {
            let val = read_target(c, target);
            let wr = c.cpu.rla(val);
            write_target(c, target, wr);
        }
        RRA => {
            let val = read_target(c, target);
            let wr = c.cpu.rra(val);
            write_target(c, target, wr);
        }
        SLO => {
            let val = read_target(c, target);
            let wr = c.cpu.slo(val);
            write_target(c, target, wr);
        }
        SRE => {
            let val = read_target(c, target);
            let wr = c.cpu.sre(val);
            write_target(c, target, wr);
        }
        _ => panic!("unhandled operation {:?}", op),
    };
}

#[cfg(debug_assertions)]
fn print_instruction(c: &mut Console, op: Operation, opcode: u8, num_args: u16) {
    let word1 = if num_args < 2 {
        "  ".to_string()
    } else {
        format!("{:02X}", readb(c, c.cpu.pc.wrapping_add(1)))
    };
    let word2 = if num_args < 3 {
        "  ".to_string()
    } else {
        format!("{:02X}", readb(c, c.cpu.pc.wrapping_add(2)))
    };
    let asterisk = match op {
        NOP if opcode != 0xEA => "*",
        SBC if opcode == 0xEB => "*",
        DCP | ISB | LAX | RLA | RRA | SAX | SLO | SRE => "*",
        _ => " ",
    };
    let opcode = format!("{:02X}", opcode);
    let flags = c.cpu.flags();
    c.cpu.oplog.push_str(&format!(
        "{:04X}  {} {} {} {}{:29?} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{}\n",
        c.cpu.pc,
        opcode,
        word1,
        word2,
        asterisk,
        op,
        c.cpu.acc,
        c.cpu.x,
        c.cpu.y,
        flags,
        c.cpu.sp,
        c.cpu.cycles,
    ));
}

pub fn readw(c: &mut Console, addr: u16) -> u16 {
    let lo = u16::from(readb(c, addr));
    let hi = u16::from(readb(c, addr.wrapping_add(1)));
    #[cfg(debug_assertions)]
    {
        if c.trace > 1 {
            println!("readw 0x{:04X} from 0x{:02X}", hi << 8 | lo, addr,);
        }
    }
    hi << 8 | lo
}

// readwbug emulates a 6502 bug that caused the low byte to wrap without
// incrementing the high byte
fn readwbug(c: &mut Console, addr: u16) -> u16 {
    let lo = u16::from(readb(c, addr));
    let addr = (addr & 0xFF00) | u16::from(addr.wrapping_add(1) as u8);
    let hi = u16::from(readb(c, addr));
    #[cfg(debug_assertions)]
    {
        if c.trace > 1 {
            println!("readwbug 0x{:04X} from 0x{:02X}", hi << 8 | lo, addr,);
        }
    }
    hi << 8 | lo
}

/// Stack Functions

// Push byte to stack
fn push_stackb(c: &mut Console, val: u8) {
    writeb(c, 0x100 | u16::from(c.cpu.sp), val);
    c.cpu.sp = c.cpu.sp.wrapping_sub(1);
}

// Pull byte from stack
fn pop_stackb(c: &mut Console) -> u8 {
    c.cpu.sp = c.cpu.sp.wrapping_add(1);
    readb(c, 0x100 | u16::from(c.cpu.sp))
}

// Push two bytes to stack
pub fn push_stackw(c: &mut Console, val: u16) {
    let lo = (val & 0xFF) as u8;
    let hi = (val >> 8) as u8;
    push_stackb(c, hi);
    push_stackb(c, lo);
}

// Pull two bytes from stack
fn pop_stackw(c: &mut Console) -> u16 {
    let lo = u16::from(pop_stackb(c));
    let hi = u16::from(pop_stackb(c));
    hi << 8 | lo
}

// Decodes the AddrMode by returning the target value, address, number of bytes after the opcode
// it used, and whether it crossed a page boundary as a tuple
// (val, Option<addr>, bytes, page_crossed)
fn decode_addr_mode(
    c: &mut Console,
    mode: AddrMode,
    addr: u16,
    op: Operation,
) -> (u16, Option<u16>, u16, bool) {
    // Whether to read from memory or not
    // ST* opcodes only require the address not the value
    let read = match op {
        STA | STX | STY => false,
        _ => true,
    };
    match mode {
        IMM => {
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 1, false)
        }
        ZRP => {
            let addr = u16::from(readb(c, addr));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 1, false)
        }
        ZRX => {
            let addr = u16::from(readb(c, addr).wrapping_add(c.cpu.x));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 1, false)
        }
        ZRY => {
            let addr = u16::from(readb(c, addr).wrapping_add(c.cpu.y));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 1, false)
        }
        ABS => {
            let addr = readw(c, addr);
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 2, false)
        }
        ABX => {
            let addr0 = readw(c, addr);
            let addr = addr0.wrapping_add(u16::from(c.cpu.x));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            let page_crossed = pages_differ(addr0, addr);
            (val, Some(addr), 2, page_crossed)
        }
        ABY => {
            let addr0 = readw(c, addr);
            let addr = addr0.wrapping_add(u16::from(c.cpu.y));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            let page_crossed = pages_differ(addr0, addr);
            (val, Some(addr), 2, page_crossed)
        }
        IND => {
            let addr0 = readw(c, addr);
            let addr = readwbug(c, addr0);
            (0, Some(addr), 2, false)
        }
        IDX => {
            let addr0 = readb(c, addr).wrapping_add(c.cpu.x);
            let addr = readwbug(c, u16::from(addr0));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 1, false)
        }
        IDY => {
            let addr0 = readb(c, addr);
            let addr0 = readwbug(c, u16::from(addr0));
            let addr = addr0.wrapping_add(u16::from(c.cpu.y));
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            let page_crossed = pages_differ(addr0, addr);
            (val, Some(addr), 1, page_crossed)
        }
        REL => {
            let val = if read { u16::from(readb(c, addr)) } else { 0 };
            (val, Some(addr), 1, false)
        }
        ACC => (u16::from(c.cpu.acc), None, 0, false),
        IMP => (0, None, 0, false),
    }
}

fn pages_differ(a: u16, b: u16) -> bool {
    a & 0xFF00 != b & 0xFF00
}

fn read_target(c: &mut Console, target: Option<u16>) -> u8 {
    #[cfg(debug_assertions)]
    {
        if c.trace > 1 {
            let (val, t) = if let Some(a) = target {
                (readb(c, a), format!("{:02X}", a))
            } else {
                (c.cpu.acc, "A".to_string())
            };
            println!("Reading {:02X} from {}", val, t);
        }
    }
    match target {
        None => c.cpu.acc,
        Some(addr) => readb(c, addr),
    }
}

fn write_target(c: &mut Console, target: Option<u16>, val: u8) {
    #[cfg(debug_assertions)]
    {
        if c.trace > 1 {
            let t = if let Some(a) = target {
                format!("{:02X}", a)
            } else {
                "A".to_string()
            };
            println!("Writing {:02X} to {}", val, t);
        }
    }
    match target {
        None => {
            c.cpu.acc = val;
        }
        Some(addr) => writeb(c, addr, val),
    }
}

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Operation {
    ADC, AND, ASL, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP, CPX,
    CPY, DEC, DEX, DEY, EOR, INC, INX, INY, JMP, JSR, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA,
    PLP, ROL, ROR, RTI, RTS, SBC, SEC, SED, SEI, STA, STX, STY, TAX, TAY, TSX, TXA, TXS, TYA,
    // "Unofficial" opcodes
    KIL, ISB, DCP, AXS, LAS, LAX, AHX, SAX, XAA, SHX, RRA, TAS, SHY, ARR, SRE, ALR, RLA, ANC, SLO,
}

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum AddrMode {
    Immediate,
    ZeroPage, ZeroPageX, ZeroPageY,
    Absolute, AbsoluteX, AbsoluteY,
    Indirect, IndirectX, IndirectY,
    Relative,
    Accumulator,
    Implied,
}

use AddrMode::*;
use Operation::*;

type CycleCount = u64;

const IMM: AddrMode = Immediate;
const ZRP: AddrMode = ZeroPage;
const ZRX: AddrMode = ZeroPageX;
const ZRY: AddrMode = ZeroPageY;
const ABS: AddrMode = Absolute;
const ABX: AddrMode = AbsoluteX;
const ABY: AddrMode = AbsoluteY;
const IND: AddrMode = Indirect;
const IDX: AddrMode = IndirectX;
const IDY: AddrMode = IndirectY;
const REL: AddrMode = Relative;
const ACC: AddrMode = Accumulator;
const IMP: AddrMode = Implied;

/// CPU Opcodes
///
/// (Operation, Addressing Mode, Clock Cycles, Extra Cycles if page boundary is crossed)
#[rustfmt::skip]
const OPCODES: [(Operation, AddrMode, CycleCount, CycleCount); 256] = [
    (BRK, IMP, 7, 0), (ORA, IDX, 6, 0), (KIL, IMP, 0, 0), (SLO, IDX, 8, 0), // $00 - $03
    (NOP, ZRP, 3, 0), (ORA, ZRP, 3, 0), (ASL, ZRP, 5, 0), (SLO, ZRP, 5, 0), // $04 - $07
    (PHP, IMP, 3, 0), (ORA, IMM, 2, 0), (ASL, ACC, 2, 0), (ANC, IMM, 2, 0), // $08 - $0B
    (NOP, ABS, 4, 0), (ORA, ABS, 4, 0), (ASL, ABS, 6, 0), (SLO, ABS, 6, 0), // $0C - $0F
    (BPL, REL, 2, 1), (ORA, IDY, 5, 1), (KIL, IMP, 0, 0), (SLO, IDY, 8, 0), // $10 - $13
    (NOP, ZRX, 4, 0), (ORA, ZRX, 4, 0), (ASL, ZRX, 6, 0), (SLO, ZRX, 6, 0), // $14 - $17
    (CLC, IMP, 2, 0), (ORA, ABY, 4, 1), (NOP, IMP, 2, 0), (SLO, ABY, 7, 0), // $18 - $1B
    (NOP, ABX, 4, 1), (ORA, ABX, 4, 1), (ASL, ABX, 7, 0), (SLO, ABX, 7, 0), // $1C - $1F
    (JSR, ABS, 6, 0), (AND, IDX, 6, 0), (KIL, IMP, 0, 0), (RLA, IDX, 8, 0), // $20 - $23
    (BIT, ZRP, 3, 0), (AND, ZRP, 3, 0), (ROL, ZRP, 5, 0), (RLA, ZRP, 5, 0), // $24 - $27
    (PLP, IMP, 4, 0), (AND, IMM, 2, 0), (ROL, ACC, 2, 0), (ANC, IMM, 2, 0), // $28 - $2B
    (BIT, ABS, 4, 0), (AND, ABS, 4, 0), (ROL, ABS, 6, 0), (RLA, ABS, 6, 0), // $2C - $2F
    (BMI, REL, 2, 1), (AND, IDY, 5, 1), (KIL, IMP, 0, 0), (RLA, IDY, 8, 0), // $30 - $33
    (NOP, ZRX, 4, 0), (AND, ZRX, 4, 0), (ROL, ZRX, 6, 0), (RLA, ZRX, 6, 0), // $34 - $37
    (SEC, IMP, 2, 0), (AND, ABY, 4, 1), (NOP, IMP, 2, 0), (RLA, ABY, 7, 0), // $38 - $3B
    (NOP, ABX, 4, 1), (AND, ABX, 4, 1), (ROL, ABX, 7, 0), (RLA, ABX, 7, 0), // $3C - $3F
    (RTI, IMP, 6, 0), (EOR, IDX, 6, 0), (KIL, IMP, 0, 0), (SRE, IDX, 8, 0), // $40 - $43
    (NOP, ZRP, 3, 0), (EOR, ZRP, 3, 0), (LSR, ZRP, 5, 0), (SRE, ZRP, 5, 0), // $44 - $47
    (PHA, IMP, 3, 0), (EOR, IMM, 2, 0), (LSR, ACC, 2, 0), (ALR, IMM, 2, 0), // $48 - $4B
    (JMP, ABS, 3, 0), (EOR, ABS, 4, 0), (LSR, ABS, 6, 0), (SRE, ABS, 6, 0), // $4C - $4F
    (BVC, REL, 2, 1), (EOR, IDY, 5, 1), (KIL, IMP, 0, 0), (SRE, IDY, 8, 0), // $50 - $53
    (NOP, ZRX, 4, 0), (EOR, ZRX, 4, 0), (LSR, ZRX, 6, 0), (SRE, ZRX, 6, 0), // $54 - $57
    (CLI, IMP, 2, 0), (EOR, ABY, 4, 1), (NOP, IMP, 2, 0), (SRE, ABY, 7, 0), // $58 - $5B
    (NOP, ABX, 4, 1), (EOR, ABX, 4, 1), (LSR, ABX, 7, 0), (SRE, ABX, 7, 0), // $5C - $5F
    (RTS, IMP, 6, 0), (ADC, IDX, 6, 0), (KIL, IMP, 0, 0), (RRA, IDX, 8, 0), // $60 - $63
    (NOP, ZRP, 3, 0), (ADC, ZRP, 3, 0), (ROR, ZRP, 5, 0), (RRA, ZRP, 5, 0), // $64 - $67
    (PLA, IMP, 4, 0), (ADC, IMM, 2, 0), (ROR, ACC, 2, 0), (ARR, IMM, 2, 0), // $68 - $6B
    (JMP, IND, 5, 0), (ADC, ABS, 4, 0), (ROR, ABS, 6, 0), (RRA, ABS, 6, 0), // $6C - $6F
    (BVS, REL, 2, 1), (ADC, IDY, 5, 1), (KIL, IMP, 0, 0), (RRA, IDY, 8, 0), // $70 - $73
    (NOP, ZRX, 4, 0), (ADC, ZRX, 4, 0), (ROR, ZRX, 6, 0), (RRA, ZRX, 6, 0), // $74 - $77
    (SEI, IMP, 2, 0), (ADC, ABY, 4, 1), (NOP, IMP, 2, 0), (RRA, ABY, 7, 0), // $78 - $7B
    (NOP, ABX, 4, 1), (ADC, ABX, 4, 1), (ROR, ABX, 7, 0), (RRA, ABX, 7, 0), // $7C - $7F
    (NOP, IMM, 2, 0), (STA, IDX, 6, 0), (NOP, IMM, 2, 0), (SAX, IDX, 6, 0), // $80 - $83
    (STY, ZRP, 3, 0), (STA, ZRP, 3, 0), (STX, ZRP, 3, 0), (SAX, ZRP, 3, 0), // $84 - $87
    (DEY, IMP, 2, 0), (NOP, IMM, 2, 0), (TXA, IMP, 2, 0), (XAA, IMM, 2, 1), // $88 - $8B
    (STY, ABS, 4, 0), (STA, ABS, 4, 0), (STX, ABS, 4, 0), (SAX, ABS, 4, 0), // $8C - $8F
    (BCC, REL, 2, 1), (STA, IDY, 6, 0), (KIL, IMP, 0, 0), (AHX, IDY, 6, 0), // $90 - $93
    (STY, ZRX, 4, 0), (STA, ZRX, 4, 0), (STX, ZRY, 4, 0), (SAX, ZRY, 4, 0), // $94 - $97
    (TYA, IMP, 2, 0), (STA, ABY, 5, 0), (TXS, IMP, 2, 0), (TAS, ABY, 5, 0), // $98 - $9B
    (SHY, ABX, 5, 0), (STA, ABX, 5, 0), (SHX, ABY, 5, 0), (AHX, ABY, 5, 0), // $9C - $9F
    (LDY, IMM, 2, 0), (LDA, IDX, 6, 0), (LDX, IMM, 2, 0), (LAX, IDX, 6, 0), // $A0 - $A3
    (LDY, ZRP, 3, 0), (LDA, ZRP, 3, 0), (LDX, ZRP, 3, 0), (LAX, ZRP, 3, 0), // $A4 - $A7
    (TAY, IMP, 2, 0), (LDA, IMM, 2, 0), (TAX, IMP, 2, 0), (LAX, IMM, 2, 0), // $A8 - $AB
    (LDY, ABS, 4, 0), (LDA, ABS, 4, 0), (LDX, ABS, 4, 0), (LAX, ABS, 4, 0), // $AC - $AF
    (BCS, REL, 2, 1), (LDA, IDY, 5, 1), (KIL, IMP, 0, 0), (LAX, IDY, 5, 1), // $B0 - $B3
    (LDY, ZRX, 4, 0), (LDA, ZRX, 4, 0), (LDX, ZRY, 4, 0), (LAX, ZRY, 4, 0), // $B4 - $B7
    (CLV, IMP, 2, 0), (LDA, ABY, 4, 1), (TSX, IMP, 2, 0), (LAS, ABY, 4, 1), // $B8 - $BB
    (LDY, ABX, 4, 1), (LDA, ABX, 4, 1), (LDX, ABY, 4, 1), (LAX, ABY, 4, 1), // $BC - $BF
    (CPY, IMM, 2, 0), (CMP, IDX, 6, 0), (NOP, IMM, 2, 0), (DCP, IDX, 8, 0), // $C0 - $C3
    (CPY, ZRP, 3, 0), (CMP, ZRP, 3, 0), (DEC, ZRP, 5, 0), (DCP, ZRP, 5, 0), // $C4 - $C7
    (INY, IMP, 2, 0), (CMP, IMM, 2, 0), (DEX, IMP, 2, 0), (AXS, IMM, 2, 0), // $C8 - $CB
    (CPY, ABS, 4, 0), (CMP, ABS, 4, 0), (DEC, ABS, 6, 0), (DCP, ABS, 6, 0), // $CC - $CF
    (BNE, REL, 2, 1), (CMP, IDY, 5, 1), (KIL, IMP, 0, 0), (DCP, IDY, 8, 0), // $D0 - $D3
    (NOP, ZRX, 4, 0), (CMP, ZRX, 4, 0), (DEC, ZRX, 6, 0), (DCP, ZRX, 6, 0), // $D4 - $D7
    (CLD, IMP, 2, 0), (CMP, ABY, 4, 1), (NOP, IMP, 2, 0), (DCP, ABY, 7, 0), // $D8 - $DB
    (NOP, ABX, 4, 1), (CMP, ABX, 4, 1), (DEC, ABX, 7, 0), (DCP, ABX, 7, 0), // $DC - $DF
    (CPX, IMM, 2, 0), (SBC, IDX, 6, 0), (NOP, IMM, 2, 0), (ISB, IDX, 8, 0), // $E0 - $E3
    (CPX, ZRP, 3, 0), (SBC, ZRP, 3, 0), (INC, ZRP, 5, 0), (ISB, ZRP, 5, 0), // $E4 - $E7
    (INX, IMP, 2, 0), (SBC, IMM, 2, 0), (NOP, IMP, 2, 0), (SBC, IMM, 2, 0), // $E8 - $EB
    (CPX, ABS, 4, 0), (SBC, ABS, 4, 0), (INC, ABS, 6, 0), (ISB, ABS, 6, 0), // $EC - $EF
    (BEQ, REL, 2, 1), (SBC, IDY, 5, 1), (KIL, IMP, 0, 0), (ISB, IDY, 8, 0), // $F0 - $F3
    (NOP, ZRX, 4, 0), (SBC, ZRX, 4, 0), (INC, ZRX, 6, 0), (ISB, ZRX, 6, 0), // $F4 - $F7
    (SED, IMP, 2, 0), (SBC, ABY, 4, 1), (NOP, IMP, 2, 0), (ISB, ABY, 7, 0), // $F8 - $FB
    (NOP, ABX, 4, 1), (SBC, ABX, 4, 1), (INC, ABX, 7, 0), (ISB, ABX, 7, 0), // $FC - $FF
];

impl CPU {
    /// Flag functions

    fn flags(&self) -> u8 {
        let mut flags: u8 = 0;
        flags |= self.carry as u8;
        flags |= (self.zero as u8) << 1;
        flags |= (self.interrupt_disable as u8) << 2;
        flags |= (self.decimal as u8) << 3;
        flags |= 0 << 4;
        flags |= 1 << 5;
        flags |= (self.overflow as u8) << 6;
        flags |= (self.negative as u8) << 7;
        flags
    }

    fn set_flags(&mut self, flags: u8) {
        self.carry = flags & 1 > 0;
        self.zero = (flags >> 1) & 1 > 0;
        self.interrupt_disable = (flags >> 2) & 1 > 0;
        self.decimal = (flags >> 3) & 1 > 0;
        // Ignore break
        // Bit 5 isn't used
        self.overflow = (flags >> 6) & 1 > 0;
        self.negative = (flags >> 7) & 1 > 0;
    }

    pub fn update_acc(&mut self) {
        self.set_result_flags(self.acc);
    }

    fn set_result_flags(&mut self, val: u8) {
        self.set_z(val);
        self.set_n(val);
    }

    /// Zero Flag - Gets set when val is 0
    fn set_z(&mut self, val: u8) {
        self.zero = match val {
            0 => true,
            _ => false,
        };
    }

    /// Negative Flag - Gets set when val is negative
    fn set_n(&mut self, val: u8) {
        self.negative = match val & 0x80 {
            0 => false,
            _ => true,
        };
    }

    pub fn trigger_irq(&mut self) {
        if let Interrupt::None = self.interrupt {
            self.interrupt = Interrupt::IRQ;
        }
    }

    pub fn trigger_nmi(&mut self) {
        self.interrupt = Interrupt::NMI;
    }

    /// # Storage

    /// LDA: Load A with M
    fn lda(&mut self, val: u8) {
        // println!("Loading {:02X} to A", val);
        self.acc = val;
        self.update_acc();
    }

    /// LDX: Load X with M
    fn ldx(&mut self, val: u8) {
        self.x = val;
        self.set_result_flags(val);
    }

    /// LDY: Load Y with M
    fn ldy(&mut self, val: u8) {
        self.y = val;
        self.set_result_flags(val);
    }

    /// TAX: Transfer A to X
    fn tax(&mut self) {
        self.x = self.acc;
        self.set_result_flags(self.x);
    }

    /// TAY: Transfer A to Y
    fn tay(&mut self) {
        self.y = self.acc;
        self.set_result_flags(self.y);
    }

    /// TSX: Transfer Stack Pointer to X
    fn tsx(&mut self) {
        self.x = self.sp;
        self.set_result_flags(self.x);
    }

    /// TXA: Transfer X to A
    fn txa(&mut self) {
        self.acc = self.x;
        self.update_acc();
    }

    /// TXS: Transfer X to Stack Pointer
    fn txs(&mut self) {
        self.sp = self.x;
    }

    /// TYA: Transfer Y to A
    fn tya(&mut self) {
        self.acc = self.y;
        self.update_acc();
    }

    /// # Arithmetic

    /// ADC: Add M to A with Carry
    fn adc(&mut self, val: u8) {
        let a = self.acc;
        let (x1, o1) = val.overflowing_add(a);
        let (x2, o2) = x1.overflowing_add(u8::from(self.carry));
        self.acc = x2;
        self.carry = o1 | o2;
        self.overflow = (a ^ val) & 0x80 == 0 && (a ^ self.acc) & 0x80 != 0;
        self.update_acc();
    }

    /// SBC: Subtract M from A with Carry
    fn sbc(&mut self, val: u8) {
        let a = self.acc;
        let (x1, o1) = a.overflowing_sub(val);
        let (x2, o2) = x1.overflowing_sub(1 - u8::from(self.carry));
        self.acc = x2;
        self.carry = !(o1 | o2);
        self.overflow = (a ^ val) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0;
        self.update_acc();
    }

    /// DEC: Decrement M by One
    fn dec(&mut self, val: u8) -> u8 {
        let val = val.wrapping_sub(1);
        self.set_result_flags(val);
        val
    }

    /// DEX: Decrement X by One
    fn dex(&mut self) {
        let x = self.x;
        self.x = self.dec(x);
    }

    /// DEY: Decrement Y by One
    fn dey(&mut self) {
        let y = self.y;
        self.y = self.dec(y);
    }

    /// INC: Increment M by One
    fn inc(&mut self, val: u8) -> u8 {
        let val = val.wrapping_add(1);
        self.set_result_flags(val);
        val
    }

    /// INX: Increment X by One
    fn inx(&mut self) {
        let x = self.x;
        self.x = self.inc(x);
    }

    /// INY: Increment Y by One
    fn iny(&mut self) {
        let y = self.y;
        self.y = self.inc(y);
    }

    /// # Bitwise

    /// AND: "And" M with A
    fn and(&mut self, val: u8) {
        self.acc &= val;
        self.update_acc();
    }

    /// ASL: Shift Left One Bit (M or A)
    fn asl(&mut self, val: u8) -> u8 {
        self.carry = (val >> 7) & 1 > 0;
        let val = val.wrapping_shl(1);
        self.set_result_flags(val);
        val
    }

    /// BIT: Test Bits in M with A
    fn bit(&mut self, val: u8) {
        self.overflow = (val >> 6) & 1 > 0;
        self.set_z(val & self.acc);
        self.set_n(val);
    }

    /// EOR: "Exclusive-Or" M with A
    fn eor(&mut self, val: u8) {
        self.acc ^= val;
        self.update_acc();
    }

    /// LSR: Shift Right One Bit (M or A)
    fn lsr(&mut self, val: u8) -> u8 {
        self.carry = val & 1 > 0;
        let val = val.wrapping_shr(1);
        self.set_result_flags(val);
        val
    }

    /// ORA: "OR" M with A
    fn ora(&mut self, val: u8) {
        self.acc |= val;
        self.update_acc();
    }

    /// ROL: Rotate One Bit Left (M or A)
    fn rol(&mut self, val: u8) -> u8 {
        let old_c = self.carry as u8;
        self.carry = (val >> 7) & 1 > 0;
        let val = (val << 1) | old_c;
        self.set_result_flags(val);
        val
    }

    /// ROR: Rotate One Bit Right (M or A)
    fn ror(&mut self, val: u8) -> u8 {
        let mut ret = val.rotate_right(1);
        if self.carry {
            ret |= 1 << 7;
        } else {
            ret &= !(1 << 7);
        }
        self.carry = val & 1 > 0;
        self.set_result_flags(ret);
        ret
    }

    /// # Branch

    fn branch(&mut self, val: u8) {
        let old_pc = self.pc;
        self.pc = self.pc.wrapping_add((val as i8) as u16);
        self.cycles += 1;
        if pages_differ(self.pc, old_pc) {
            self.cycles += 1;
        }
    }

    /// BCC: Branch on Carry Clear
    fn bcc(&mut self, val: u8) {
        if !self.carry {
            self.branch(val);
        }
    }

    /// BCS: Branch on Carry Set
    fn bcs(&mut self, val: u8) {
        if self.carry {
            self.branch(val);
        }
    }

    /// BEQ: Branch on Result Zero
    fn beq(&mut self, val: u8) {
        if self.zero {
            self.branch(val);
        }
    }

    /// BMI: Branch on Result Negative
    fn bmi(&mut self, val: u8) {
        if self.negative {
            self.branch(val);
        }
    }

    /// BNE: Branch on Result Not Zero
    fn bne(&mut self, val: u8) {
        if !self.zero {
            self.branch(val);
        }
    }

    /// BPL: Branch on Result Positive
    fn bpl(&mut self, val: u8) {
        if !self.negative {
            self.branch(val);
        }
    }

    /// BVC: Branch on Overflow Clear
    fn bvc(&mut self, val: u8) {
        if !self.overflow {
            self.branch(val);
        }
    }

    /// BVS: Branch on Overflow Set
    fn bvs(&mut self, val: u8) {
        if self.overflow {
            self.branch(val);
        }
    }

    /// # Jump

    /// JMP: Jump to Location
    fn jmp(&mut self, addr: u16) {
        self.pc = addr;
    }

    /// JSR: Jump to Location Save Return addr
    fn jsr(&mut self, addr: u16) {
        self.pc = addr;
    }

    /// RTI: Return from Interrupt
    fn rti(&mut self, flags: u8) {
        // Unset Decimal Mode/Set Interrupt Disable
        self.set_flags(flags & 0xEF | 0x20);
    }

    /// # Registers

    /// CLC: Clear Carry Flag
    fn clc(&mut self) {
        self.carry = false;
    }

    /// CLD: Clear Decimal Mode
    fn cld(&mut self) {
        self.decimal = false;
    }

    /// CLI: Clear Interrupt Disable Bit
    fn cli(&mut self) {
        self.interrupt_disable = false;
    }

    /// CLV: Clear Overflow Flag
    fn clv(&mut self) {
        self.overflow = false;
    }

    fn compare(&mut self, a: u8, b: u8) {
        let result = a.wrapping_sub(b);
        self.set_result_flags(result);
        self.carry = a >= b;
    }

    /// CMP: Compare M and A
    fn cmp(&mut self, val: u8) {
        let a = self.acc;
        self.compare(a, val);
    }

    /// CPX: Compare M and X
    fn cpx(&mut self, val: u8) {
        let x = self.x;
        self.compare(x, val);
    }

    /// CPY: Compare M and Y
    fn cpy(&mut self, val: u8) {
        let y = self.y;
        self.compare(y, val);
    }

    /// SEC: Set Carry Flag
    fn sec(&mut self) {
        self.carry = true;
    }

    /// SED: Set Decimal Mode
    fn sed(&mut self) {
        self.decimal = true;
    }

    /// SEI: Set Interrupt Disable Status
    fn sei(&mut self) {
        self.interrupt_disable = true;
    }

    /// # Stack

    /// PLP: Pull Processor Status from Stack
    fn plp(&mut self, status: u8) {
        // Unset Decimal Mode/Set Interrupt Disable
        self.set_flags(status & 0xEF | 0x20);
    }

    /// # System

    /// NOP: No Operation
    fn nop(&mut self) {}

    /// # Unofficial

    /// LAX: Shortcut for LDA then TAX
    fn lax(&mut self, val: u8) {
        self.acc = val;
        self.x = val;
        self.update_acc();
    }

    /// SAX: AND A with X
    fn sax(&mut self) -> u8 {
        self.acc & self.x
    }

    /// DCP: Shortcut for DEC then CMP
    fn dcp(&mut self, val: u8) -> u8 {
        let val = val.wrapping_sub(1);
        self.compare(self.acc, val);
        val
    }

    /// ISC/ISB: Shortcut for INC then SBC
    fn isb(&mut self, val: u8) -> u8 {
        let x = self.inc(val);
        self.sbc(x);
        x
        // let val = val.wrapping_add(1);
        // let a = self.acc;
        // let (x1, o1) = a.overflowing_sub(val);
        // let (x2, o2) = x1.overflowing_sub(!u8::from(self.carry));
        // self.acc = x2;
        // self.carry = !(o1 | o2);
        // if (a ^ val) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0 {
        //     self.overflow = 1;
        // } else {
        //     self.overflow = 0;
        // }
        // self.update_acc();
        // val
    }

    /// RLA: Shortcut for ROL then AND
    fn rla(&mut self, val: u8) -> u8 {
        let x = self.rol(val);
        self.and(x);
        x
    }

    /// RRA: Shortcut for ROR then ADC
    fn rra(&mut self, val: u8) -> u8 {
        let x = self.ror(val);
        self.adc(x);
        x
    }

    /// SLO: Shortcut for ASL then ORA
    fn slo(&mut self, val: u8) -> u8 {
        let x = self.asl(val);
        self.ora(x);
        x
    }

    /// SRA: Shortcut for LSR then EOR
    fn sre(&mut self, val: u8) -> u8 {
        let x = self.lsr(val);
        self.eor(x);
        x
    }
}

impl Default for CPU {
    fn default() -> Self {
        Self::new()
    }
}
