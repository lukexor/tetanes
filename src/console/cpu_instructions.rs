use super::{
    memory::{read_byte, write_byte},
    Console,
};

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    ADC, AND, ASL, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP, CPX,
    CPY, DEC, DEX, DEY, EOR, INC, INX, INY, JMP, JSR, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA,
    PLP, ROL, ROR, RTI, RTS, SBC, SEC, SED, SEI, STA, STX, STY, TAX, TAY, TSX, TXA, TXS, TYA,
    // "Unofficial" opcodes
    KIL, ISC, DCP, AXS, LAS, LAX, AHX, SAX, XAA, SHX, RRA, TAS, SHY, ARR, SRE, ALR, RLA, ANC, SLO,
    ISB,
}

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AddrMode {
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
pub const OPCODES: [(Operation, AddrMode, CycleCount, CycleCount); 256] = [
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
    (CPX, IMM, 2, 0), (SBC, IDX, 6, 0), (NOP, IMM, 2, 0), (ISC, IDX, 8, 0), // $E0 - $E3
    (CPX, ZRP, 3, 0), (SBC, ZRP, 3, 0), (INC, ZRP, 5, 0), (ISC, ZRP, 5, 0), // $E4 - $E7
    (INX, IMP, 2, 0), (SBC, IMM, 2, 0), (NOP, IMP, 2, 0), (SBC, IMM, 2, 0), // $E8 - $EB
    (CPX, ABS, 4, 0), (SBC, ABS, 4, 0), (INC, ABS, 6, 0), (ISC, ABS, 6, 0), // $EC - $EF
    (BEQ, REL, 2, 1), (SBC, IDY, 5, 1), (KIL, IMP, 0, 0), (ISC, IDY, 8, 0), // $F0 - $F3
    (NOP, ZRX, 4, 0), (SBC, ZRX, 4, 0), (INC, ZRX, 6, 0), (ISC, ZRX, 6, 0), // $F4 - $F7
    (SED, IMP, 2, 0), (SBC, ABY, 4, 1), (NOP, IMP, 2, 0), (ISC, ABY, 7, 0), // $F8 - $FB
    (NOP, ABX, 4, 1), (SBC, ABX, 4, 1), (INC, ABX, 7, 0), (ISC, ABX, 7, 0), // $FC - $FF
];

pub fn print_instruction(c: &mut Console, op: Operation, opcode: u8, num_args: u8, addr: u16) {
    let mut word1 = if num_args < 2 {
        "  ".to_string()
    } else {
        format!("{:02X}", read_byte(c, c.cpu.pc.wrapping_add(1)))
    };
    let mut word2 = if num_args < 3 {
        "  ".to_string()
    } else {
        format!("{:02X}", read_byte(c, c.cpu.pc.wrapping_add(2)))
    };
    let word = if num_args == 3 {
        format!("${}{}", word2, word1)
    } else {
        format!("${}", word1)
    };
    let mut asterisk = match op {
        NOP | DCP | ISB | LAX | RLA | RRA | SAX | SLO | SRE => "*",
        _ => " ",
    };
    if op == NOP && opcode == 0xEA {
        asterisk = " ";
    }
    let opcode = format!("{:02X}", opcode);
    let operand = ""; // TODO add operand words
    let flags = c.cpu.flags();
    println!(
        "{:04X}  {} {} {} {}{:?} {:27} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{}",
        c.cpu.pc,
        opcode,
        word1,
        word2,
        asterisk,
        op,
        operand,
        c.cpu.a,
        c.cpu.x,
        c.cpu.y,
        flags,
        c.cpu.sp,
        c.cpu.cycles,
    );
}

pub fn execute(c: &mut Console, opcode: u8) {
    let (op, addr_mode, cycles, page_cycles) = OPCODES[opcode as usize];
    let (addr, num_args, page_crossed) = decode_addr_mode(c, addr_mode);
    print_instruction(c, op, opcode, 1 + num_args, addr);
    c.cpu.pc = c.cpu.pc.wrapping_add(u16::from(1 + num_args));
    c.cpu.cycles += cycles;
    if page_crossed {
        c.cpu.cycles += page_cycles;
    }
    match op {
        BRK => brk(c),
        ORA => ora(c, addr),
        ASL => asl(c, addr, addr_mode),
        PHP => php(c),
        BPL => bpl(c, addr),
        CLC => clc(c),
        JSR => jsr(c, addr),
        AND => and(c, addr),
        BIT => bit(c, addr),
        ROL => rol(c, addr, addr_mode),
        PLP => plp(c),
        BMI => bmi(c, addr),
        SEC => sec(c),
        RTI => rti(c),
        EOR => eor(c, addr),
        LSR => lsr(c, addr, addr_mode),
        PHA => pha(c),
        JMP => jmp(c, addr),
        BVC => bvc(c, addr),
        CLI => cli(c),
        RTS => rts(c),
        ADC => adc(c, addr),
        ROR => ror(c, addr, addr_mode),
        PLA => pla(c),
        BVS => bvs(c, addr),
        SEI => sei(c),
        STA => sta(c, addr),
        STY => sty(c, addr),
        STX => stx(c, addr),
        DEY => dey(c),
        TXA => txa(c),
        BCC => bcc(c, addr),
        TYA => tya(c),
        TXS => txs(c),
        LDY => ldy(c, addr),
        LDA => lda(c, addr),
        LDX => ldx(c, addr),
        LAX => lax(c, addr),
        TAY => tay(c),
        TAX => tax(c),
        BCS => bcs(c, addr),
        CLV => clv(c),
        TSX => tsx(c),
        CPY => cpy(c, addr),
        CMP => cmp(c, addr),
        DEC => dec(c, addr),
        INY => iny(c),
        DEX => dex(c),
        BNE => bne(c, addr),
        CLD => cld(c),
        CPX => cpx(c, addr),
        SBC => sbc(c, addr),
        INC => inc(c, addr),
        INX => inx(c),
        BEQ => beq(c, addr),
        SED => sed(c),
        _ => (),
    };
}

pub fn read16(c: &mut Console, addr: u16) -> u16 {
    let lo = u16::from(read_byte(c, addr));
    let hi = u16::from(read_byte(c, addr.wrapping_add(1)));
    hi << 8 | lo
}

// read16bug emulates a 6502 bug that caused the low byte to wrap without
// incrementing the high byte
pub fn read16bug(c: &mut Console, addr: u16) -> u16 {
    let lo = u16::from(read_byte(c, addr));
    let addr = (addr & 0xFF00) | u16::from(addr.wrapping_add(1) as u8);
    let hi = u16::from(read_byte(c, addr));
    hi << 8 | lo
}

/// Stack Functions

// Push byte to stack
pub fn push(c: &mut Console, val: u8) {
    write_byte(c, 0x100 | u16::from(c.cpu.sp), val);
    c.cpu.sp = c.cpu.sp.wrapping_sub(1);
}

// Pull byte from stack
pub fn pull(c: &mut Console) -> u8 {
    c.cpu.sp = c.cpu.sp.wrapping_add(1);
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

pub fn decode_addr_mode(c: &mut Console, mode: AddrMode) -> (u16, u8, bool) {
    match mode {
        IMM => (c.cpu.pc.wrapping_add(1), 1, false),
        ZRP => (u16::from(read_byte(c, c.cpu.pc.wrapping_add(1))), 1, false),
        ZRX => (
            u16::from(read_byte(c, c.cpu.pc.wrapping_add(1)).wrapping_add(c.cpu.x)) & 0xFF,
            1,
            false,
        ),
        ZRY => (
            u16::from(read_byte(c, c.cpu.pc.wrapping_add(1)).wrapping_add(c.cpu.y)) & 0xFF,
            1,
            false,
        ),
        ABS => (read16(c, c.cpu.pc.wrapping_add(1)), 2, false),
        ABX => {
            let addr = read16(c, c.cpu.pc.wrapping_add(1));
            let xaddr = addr.wrapping_add(u16::from(c.cpu.x));
            let page_crossed = pages_differ(addr, xaddr);
            (xaddr, 2, page_crossed)
        }
        ABY => {
            let addr = read16(c, c.cpu.pc.wrapping_add(1));
            let yaddr = addr.wrapping_add(u16::from(c.cpu.y));
            let page_crossed = pages_differ(addr, yaddr);
            (yaddr, 2, page_crossed)
        }
        IND => {
            let addr = read16(c, c.cpu.pc.wrapping_add(1));
            (read16bug(c, addr), 2, false)
        }
        IDX => {
            let addr = u16::from(read_byte(c, c.cpu.pc.wrapping_add(1)).wrapping_add(c.cpu.x));
            (read16bug(c, addr), 1, false)
        }
        IDY => {
            let addr = u16::from(read_byte(c, c.cpu.pc.wrapping_add(1)));
            let addr = read16bug(c, addr);
            let yaddr = addr.wrapping_add(u16::from(c.cpu.y));
            let page_crossed = pages_differ(addr, yaddr);
            (yaddr, 1, page_crossed)
        }
        REL => {
            let offset = u16::from(read_byte(c, c.cpu.pc.wrapping_add(1)));
            let mut addr = c.cpu.pc.wrapping_add(2 + offset);
            if offset >= 0x80 {
                addr -= 0x100;
            }
            (addr, 1, false)
        }
        ACC => (0, 0, false),
        IMP => (0, 0, false),
        _ => panic!("invalid addressing mode"),
    }
}

fn pages_differ(a: u16, b: u16) -> bool {
    a & 0xFF00 != b & 0xFF00
}

/// # Storage

/// LDA: Load A with M
pub fn lda(c: &mut Console, addr: u16) {
    println!("lda addr:0x8904 ({:2X})", read_byte(c, 0x8904));
    c.cpu.a = read_byte(c, addr);
    c.cpu.set_zn(c.cpu.a);
}

/// LDX: Load X with M
pub fn ldx(c: &mut Console, addr: u16) {
    c.cpu.x = read_byte(c, addr);
    c.cpu.set_zn(c.cpu.x);
}

/// LDY: Load Y with M
pub fn ldy(c: &mut Console, addr: u16) {
    c.cpu.y = read_byte(c, addr);
    c.cpu.set_zn(c.cpu.y);
}

/// STA: Store A in M
pub fn sta(c: &mut Console, addr: u16) {
    write_byte(c, addr, c.cpu.a);
}

/// STX: Store X in M
pub fn stx(c: &mut Console, addr: u16) {
    write_byte(c, addr, c.cpu.x);
}

/// STY: Store Y in M
pub fn sty(c: &mut Console, addr: u16) {
    write_byte(c, addr, c.cpu.y);
}

/// TAX: Transfer A to X
pub fn tax(c: &mut Console) {
    c.cpu.x = c.cpu.a;
    c.cpu.set_zn(c.cpu.x);
}

/// TAY: Transfer A to Y
pub fn tay(c: &mut Console) {
    c.cpu.y = c.cpu.a;
    c.cpu.set_zn(c.cpu.y);
}

/// TSX: Transfer Stack Pointer to X
pub fn tsx(c: &mut Console) {
    c.cpu.x = c.cpu.sp;
    c.cpu.set_zn(c.cpu.x);
}

/// TXA: Transfer X to A
pub fn txa(c: &mut Console) {
    c.cpu.a = c.cpu.x;
    c.cpu.set_zn(c.cpu.a);
}

/// TXS: Transfer X to Stack Pointer
pub fn txs(c: &mut Console) {
    c.cpu.sp = c.cpu.x;
}

/// TYA: Transfer Y to A
pub fn tya(c: &mut Console) {
    c.cpu.a = c.cpu.y;
    c.cpu.set_zn(c.cpu.a);
}

/// # Arithmetic

/// ADC: Add M to A with Carry
pub fn adc(c: &mut Console, addr: u16) {
    let a = c.cpu.a;
    let val = read_byte(c, addr);
    let (x1, o1) = val.overflowing_add(a);
    let (x2, o2) = x1.overflowing_add(c.cpu.c);
    c.cpu.a = x2;
    c.cpu.c = (o1 | o2) as u8;
    if (a ^ val) & 0x80 == 0 && (a ^ c.cpu.a) & 0x80 != 0 {
        c.cpu.v = 1;
    } else {
        c.cpu.v = 0;
    }
    c.cpu.set_zn(c.cpu.a);
}

/// SBC: Subtract M from A with Carry
pub fn sbc(c: &mut Console, addr: u16) {
    let a = c.cpu.a;
    let val = read_byte(c, addr);
    let (x1, o1) = a.overflowing_sub(val);
    let (x2, o2) = x1.overflowing_sub(1 - c.cpu.c);
    c.cpu.a = x2;
    c.cpu.c = !(o1 | o2) as u8;
    if (a ^ val) & 0x80 != 0 && (a ^ c.cpu.a) & 0x80 != 0 {
        c.cpu.v = 1;
    } else {
        c.cpu.v = 0;
    }
    c.cpu.set_zn(c.cpu.a);
}

/// DEC: Decrement M by One
pub fn dec(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr).wrapping_sub(1);
    write_byte(c, addr, val);
    c.cpu.set_zn(val);
}

/// DEX: Decrement X by One
pub fn dex(c: &mut Console) {
    c.cpu.x = c.cpu.x.wrapping_sub(1);
    c.cpu.set_zn(c.cpu.x);
}

/// DEY: Decrement Y by One
pub fn dey(c: &mut Console) {
    c.cpu.y = c.cpu.y.wrapping_sub(1);
    c.cpu.set_zn(c.cpu.y);
}

/// INC: Increment M by One
pub fn inc(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr).wrapping_add(1);
    write_byte(c, addr, val);
    c.cpu.set_zn(val);
}

/// INX: Increment X by One
pub fn inx(c: &mut Console) {
    c.cpu.x = c.cpu.x.wrapping_add(1);
    c.cpu.set_zn(c.cpu.x);
}

/// INY: Increment Y by One
pub fn iny(c: &mut Console) {
    c.cpu.y = c.cpu.y.wrapping_add(1);
    c.cpu.set_zn(c.cpu.y);
}

/// SBC: Subtract M from A with Borrow
pub fn scb() {
    unimplemented!();
}

/// # Bitwise

/// AND: "And" M with A
pub fn and(c: &mut Console, addr: u16) {
    c.cpu.a &= read_byte(c, addr);
    c.cpu.set_zn(c.cpu.a);
}

/// ASL: Shift Left One Bit (M or A)
pub fn asl(c: &mut Console, addr: u16, mode: AddrMode) {
    match mode {
        ACC => {
            c.cpu.c = (c.cpu.a >> 7) & 1;
            c.cpu.a = c.cpu.a.wrapping_shl(1);
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = (val >> 7) & 1;
            val = val.wrapping_shl(1);
            write_byte(c, addr, val);
            c.cpu.set_zn(val);
        }
    }
}

/// BIT: Test Bits in M with A
pub fn bit(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr);
    c.cpu.v = (val >> 6) & 1;
    c.cpu.set_z(val & c.cpu.a);
    c.cpu.set_n(val);
}

/// EOR: "Exclusive-Or" M with A
pub fn eor(c: &mut Console, addr: u16) {
    c.cpu.a ^= read_byte(c, addr);
    c.cpu.set_zn(c.cpu.a);
}

/// LSR: Shift Right One Bit (M or A)
pub fn lsr(c: &mut Console, addr: u16, mode: AddrMode) {
    match mode {
        ACC => {
            c.cpu.c = c.cpu.a & 1;
            c.cpu.a = c.cpu.a.wrapping_shr(1);
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = val & 1;
            val = val.wrapping_shr(1);
            write_byte(c, addr, val);
            c.cpu.set_zn(val);
        }
    }
}

/// ORA: "OR" M with A
pub fn ora(c: &mut Console, addr: u16) {
    c.cpu.a |= read_byte(c, addr);
    c.cpu.set_zn(c.cpu.a);
}

/// ROL: Rotate One Bit Left (M or A)
pub fn rol(c: &mut Console, addr: u16, mode: AddrMode) {
    let tmp_c = c.cpu.c;
    match mode {
        ACC => {
            c.cpu.c = (c.cpu.a >> 7) & 1;
            c.cpu.a = (c.cpu.a << 1) | tmp_c;
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = (val >> 7) & 1;
            val = (val << 1) | tmp_c;
            write_byte(c, addr, val);
            c.cpu.set_zn(val);
        }
    }
}

/// ROR: Rotate One Bit Right (M or A)
pub fn ror(c: &mut Console, addr: u16, mode: AddrMode) {
    let val = match mode {
        ACC => c.cpu.a,
        _ => read_byte(c, addr),
    };
    let mut res = val.rotate_right(1);
    if c.cpu.c == 1 {
        res |= 1 << 7;
    } else {
        res &= !(1 << 7);
    }
    c.cpu.c = val & 1;
    c.cpu.set_zn(res);
    match mode {
        ACC => c.cpu.a = res,
        _ => write_byte(c, addr, res),
    }
}

/// # Branch

fn add_branch_cycles(c: &mut Console, pc: u16, addr: u16) {
    c.cpu.cycles += if pages_differ(pc, addr) { 2 } else { 1 };
}

/// BCC: Branch on Carry Clear
pub fn bcc(c: &mut Console, addr: u16) {
    if c.cpu.c == 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BCS: Branch on Carry Set
pub fn bcs(c: &mut Console, addr: u16) {
    if c.cpu.c != 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BEQ: Branch on Result Zero
pub fn beq(c: &mut Console, addr: u16) {
    if c.cpu.z != 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BMI: Branch on Result Minus
pub fn bmi(c: &mut Console, addr: u16) {
    if c.cpu.n != 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BNE: Branch on Result Not Zero
pub fn bne(c: &mut Console, addr: u16) {
    if c.cpu.z == 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BPL: Branch on Result Plus
pub fn bpl(c: &mut Console, addr: u16) {
    if c.cpu.n == 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BVC: Branch on Overflow Clear
pub fn bvc(c: &mut Console, addr: u16) {
    if c.cpu.v == 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// BVS: Branch on Overflow Set
pub fn bvs(c: &mut Console, addr: u16) {
    if c.cpu.v != 0 {
        add_branch_cycles(c, c.cpu.pc, addr);
        c.cpu.pc = addr;
    }
}

/// # Jump

/// JMP: Jump to Location
pub fn jmp(c: &mut Console, addr: u16) {
    c.cpu.pc = addr;
}

/// JSR: Jump to Location Save Return addr
pub fn jsr(c: &mut Console, addr: u16) {
    push16(c, c.cpu.pc.wrapping_sub(1));
    c.cpu.pc = addr;
}

/// RTI: Return from Interrupt
pub fn rti(c: &mut Console) {
    let flags = pull(c);
    // Unset Decimal Mode/Set Interrupt Disable
    c.cpu.set_flags(flags & 0xEF | 0x20);
    c.cpu.pc = pull16(c);
}

/// RTS: Return from Subroutine
pub fn rts(c: &mut Console) {
    c.cpu.pc = pull16(c).wrapping_add(1);
}

/// # Registers

/// CLC: Clear Carry Flag
pub fn clc(c: &mut Console) {
    c.cpu.c = 0;
}

/// CLD: Clear Decimal Mode
pub fn cld(c: &mut Console) {
    c.cpu.d = 0;
}

/// CLI: Clear Interrupt Disable Bit
pub fn cli(c: &mut Console) {
    c.cpu.i = 0;
}

/// CLV: Clear Overflow Flag
pub fn clv(c: &mut Console) {
    c.cpu.v = 0;
}

fn compare(c: &mut Console, a: u8, b: u8) {
    let result = a.wrapping_sub(b);
    c.cpu.set_zn(result);
    c.cpu.c = (a >= b) as u8;
}

/// CMP: Compare M and A
pub fn cmp(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr);
    compare(c, c.cpu.a, val);
}

/// CPX: Compare M and X
pub fn cpx(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr);
    compare(c, c.cpu.x, val);
}

/// CPY: Compare M and Y
pub fn cpy(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr);
    compare(c, c.cpu.y, val);
}

/// SEC: Set Carry Flag
pub fn sec(c: &mut Console) {
    c.cpu.c = 1;
}

/// SED: Set Decimal Mode
pub fn sed(c: &mut Console) {
    c.cpu.d = 1;
}

/// SEI: Set Interrupt Disable Status
pub fn sei(c: &mut Console) {
    c.cpu.i = 1;
}

/// # Stack

/// PHA: Push A on Stack
pub fn pha(c: &mut Console) {
    push(c, c.cpu.a);
}

/// PHP: Push Processor Status on Stack
pub fn php(c: &mut Console) {
    let flags = c.cpu.flags();
    push(c, flags | 0x10); // Set Decimal Mode Flag
}

/// PLA: Pull A from Stack
pub fn pla(c: &mut Console) {
    c.cpu.a = pull(c);
    c.cpu.set_zn(c.cpu.a);
}

/// PLP: Pull Processor Status from Stack
pub fn plp(c: &mut Console) {
    let status = pull(c);
    // Unset Decimal Mode/Set Interrupt Disable
    c.cpu.set_flags(status & 0xEF | 0x20);
}

/// # System

/// BRK: Force Interrupt
pub fn brk(c: &mut Console) {
    push16(c, c.cpu.pc);
    php(c);
    sei(c);
    c.cpu.pc = read16(c, 0xFFFE);
}

/// NOP: No Operation
pub fn nop() {}

/// # Unofficial

// Shortcut for LDA then TAX
pub fn lax(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr);
    c.cpu.a = val;
    c.cpu.x = val;
    c.cpu.set_zn(c.cpu.a);
}

/// KIL: Stop program counter
pub fn kil() {
    unimplemented!();
}

/// ANC: AND byte with accumulator
pub fn anc() {
    unimplemented!();
}

pub fn ahx() {
    unimplemented!();
}

pub fn arr() {
    unimplemented!();
}

pub fn axs() {
    unimplemented!();
}

pub fn dcp() {
    unimplemented!();
}

pub fn isc() {
    unimplemented!();
}

pub fn las() {
    unimplemented!();
}

pub fn rla() {
    unimplemented!();
}

pub fn rra() {
    unimplemented!();
}

pub fn sax() {
    unimplemented!();
}

pub fn shx() {
    unimplemented!();
}

pub fn shy() {
    unimplemented!();
}

pub fn slo() {
    unimplemented!();
}

pub fn sre() {
    unimplemented!();
}

pub fn tas() {
    unimplemented!();
}

pub fn xaa() {
    unimplemented!();
}
