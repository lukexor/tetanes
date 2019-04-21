use super::{
    console::Console,
    memory::{pull, pull16, push, push16, read16, read16bug, read_byte, write},
};

// The addressing mode for each instruction
#[rustfmt::skip]
pub const INSTRUCTION_MODES: [u8; 256] = [
    6, 7, 6, 7, 11, 11, 11, 11, 6, 5, 4, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 12, 12, 6,  3,  6,  3, 2, 2, 2, 2,
    1, 7, 6, 7, 11, 11, 11, 11, 6, 5, 4, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 12, 12, 6,  3,  6,  3, 2, 2, 2, 2,
    6, 7, 6, 7, 11, 11, 11, 11, 6, 5, 4, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 12, 12, 6,  3,  6,  3, 2, 2, 2, 2,
    6, 7, 6, 7, 11, 11, 11, 11, 6, 5, 4, 5, 8, 1, 1, 1, 10, 9, 6, 9, 12, 12, 12, 12, 6,  3,  6,  3, 2, 2, 2, 2,
    5, 7, 5, 7, 11, 11, 11, 11, 6, 5, 6, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 13, 13, 6,  3,  6,  3, 2, 2, 3, 3,
    5, 7, 5, 7, 11, 11, 11, 11, 6, 5, 6, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 13, 13, 6,  3,  6,  3, 2, 2, 3, 3,
    5, 7, 5, 7, 11, 11, 11, 11, 6, 5, 6, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 12, 12, 6,  3,  6,  3, 2, 2, 2, 2,
    5, 7, 5, 7, 11, 11, 11, 11, 6, 5, 6, 5, 1, 1, 1, 1, 10, 9, 6, 9, 12, 12, 12, 12, 6,  3,  6,  3, 2, 2, 2, 2,
];

// The size of each instruction in bytes
pub const INSTRUCTION_SIZES: [u8; 256] = [
    2, 2, 0, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
    3, 2, 0, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
    1, 2, 0, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
    1, 2, 0, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
    2, 2, 0, 0, 2, 2, 2, 0, 1, 0, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 0, 3, 0, 0,
    2, 2, 2, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
    2, 2, 0, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
    2, 2, 0, 0, 2, 2, 2, 0, 1, 2, 1, 0, 3, 3, 3, 0, 2, 2, 0, 0, 2, 2, 2, 0, 1, 3, 1, 0, 3, 3, 3, 0,
];

// The number of cycles used by each instruction, not including conditional cycles
const INSTRUCTION_CYCLES: [u8; 256] = [
    7, 6, 2, 8, 3, 3, 5, 5, 3, 2, 2, 2, 4, 4, 6, 6, 2, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    6, 6, 2, 8, 3, 3, 5, 5, 4, 2, 2, 2, 4, 4, 6, 6, 2, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    6, 6, 2, 8, 3, 3, 5, 5, 3, 2, 2, 2, 3, 4, 6, 6, 2, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    6, 6, 2, 8, 3, 3, 5, 5, 4, 2, 2, 2, 5, 4, 6, 6, 2, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, 2, 6, 2, 6, 4, 4, 4, 4, 2, 5, 2, 5, 5, 5, 5, 5,
    2, 6, 2, 6, 3, 3, 3, 3, 2, 2, 2, 2, 4, 4, 4, 4, 2, 5, 2, 5, 4, 4, 4, 4, 2, 4, 2, 4, 4, 4, 4, 4,
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, 2, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
    2, 6, 2, 8, 3, 3, 5, 5, 2, 2, 2, 2, 4, 4, 6, 6, 2, 5, 2, 8, 4, 4, 6, 6, 2, 4, 2, 7, 4, 4, 7, 7,
];

// The number of cycles used by each instruction when a page is crossed
const INSTRUCTION_PAGE_CYCLES: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0,
];

// The name of each instruction
const INSTRUCTION_NAMES: [&str; 256] = [
    "BRK", "ORA", "KIL", "SLO", "NOP", "ORA", "ASL", "SLO", "PHP", "ORA", "ASL", "ANC", "NOP",
    "ORA", "ASL", "SLO", "BPL", "ORA", "KIL", "SLO", "NOP", "ORA", "ASL", "SLO", "CLC", "ORA",
    "NOP", "SLO", "NOP", "ORA", "ASL", "SLO", "JSR", "AND", "KIL", "RLA", "BIT", "AND", "ROL",
    "RLA", "PLP", "AND", "ROL", "ANC", "BIT", "AND", "ROL", "RLA", "BMI", "AND", "KIL", "RLA",
    "NOP", "AND", "ROL", "RLA", "SEC", "AND", "NOP", "RLA", "NOP", "AND", "ROL", "RLA", "RTI",
    "EOR", "KIL", "SRE", "NOP", "EOR", "LSR", "SRE", "PHA", "EOR", "LSR", "ALR", "JMP", "EOR",
    "LSR", "SRE", "BVC", "EOR", "KIL", "SRE", "NOP", "EOR", "LSR", "SRE", "CLI", "EOR", "NOP",
    "SRE", "NOP", "EOR", "LSR", "SRE", "RTS", "ADC", "KIL", "RRA", "NOP", "ADC", "ROR", "RRA",
    "PLA", "ADC", "ROR", "ARR", "JMP", "ADC", "ROR", "RRA", "BVS", "ADC", "KIL", "RRA", "NOP",
    "ADC", "ROR", "RRA", "SEI", "ADC", "NOP", "RRA", "NOP", "ADC", "ROR", "RRA", "NOP", "STA",
    "NOP", "SAX", "STY", "STA", "STX", "SAX", "DEY", "NOP", "TXA", "XAA", "STY", "STA", "STX",
    "SAX", "BCC", "STA", "KIL", "AHX", "STY", "STA", "STX", "SAX", "TYA", "STA", "TXS", "TAS",
    "SHY", "STA", "SHX", "AHX", "LDY", "LDA", "LDX", "LAX", "LDY", "LDA", "LDX", "LAX", "TAY",
    "LDA", "TAX", "LAX", "LDY", "LDA", "LDX", "LAX", "BCS", "LDA", "KIL", "LAX", "LDY", "LDA",
    "LDX", "LAX", "CLV", "LDA", "TSX", "LAS", "LDY", "LDA", "LDX", "LAX", "CPY", "CMP", "NOP",
    "DCP", "CPY", "CMP", "DEC", "DCP", "INY", "CMP", "DEX", "AXS", "CPY", "CMP", "DEC", "DCP",
    "BNE", "CMP", "KIL", "DCP", "NOP", "CMP", "DEC", "DCP", "CLD", "CMP", "NOP", "DCP", "NOP",
    "CMP", "DEC", "DCP", "CPX", "SBC", "NOP", "ISC", "CPX", "SBC", "INC", "ISC", "INX", "SBC",
    "NOP", "SBC", "CPX", "SBC", "INC", "ISC", "BEQ", "SBC", "KIL", "ISC", "NOP", "SBC", "INC",
    "ISC", "SED", "SBC", "NOP", "ISC", "NOP", "SBC", "INC", "ISC",
];

pub fn print_mode_map() {
    for (i, ins) in INSTRUCTION_NAMES.iter().enumerate() {
        let mode_name = match INSTRUCTION_MODES[i] {
            1 => "Absolute",
            2 => "AbsoluteX",
            3 => "AbsoluteY",
            4 => "Accumulator",
            5 => "Immediate",
            6 => "Implied",
            7 => "IndexedIndirect",
            8 => "Indirect",
            9 => "IndirectIndexed",
            10 => "Relative",
            11 => "ZeroPaged",
            12 => "ZeroPagedX",
            13 => "ZeroPagedY",
            _ => "",
        };
        println!("{}: {} - {} ({})", i, ins, mode_name, INSTRUCTION_MODES[i]);
    }
}

pub fn print_instruction(c: &mut Console) {
    let opcode = read_byte(c, c.cpu.pc);
    let bytes = INSTRUCTION_SIZES[opcode as usize];
    let name = INSTRUCTION_NAMES[opcode as usize];
    let w0 = format!("{:2X}", read_byte(c, c.cpu.pc));
    let mut w1 = format!("{:2X}", read_byte(c, c.cpu.pc + 1));
    let mut w2 = format!("{:2X}", read_byte(c, c.cpu.pc + 2));
    if bytes < 2 {
        w1 = "  ".to_string();
    }
    if bytes < 3 {
        w2 = "  ".to_string();
    }
    let flags = c.cpu.flags();
    println!(
        "{:04X}  {} {} {}  {}           A:{:02X} X:{:02X} Y:{:02X} P:{:02b} SP:{:02X} CYC:{:3}",
        c.cpu.pc,
        w0,
        w1,
        w2,
        name,
        c.cpu.a,
        c.cpu.x,
        c.cpu.y,
        flags,
        c.cpu.sp,
        (c.cpu.cycles * 3) % 341,
    );
}

pub fn execute(c: &mut Console, opcode: u8) {
    let mode = INSTRUCTION_MODES[opcode as usize];
    let (addr, page_crossed) = addr_mode(c, mode);
    c.cpu.pc += u16::from(INSTRUCTION_SIZES[opcode as usize]);
    c.cpu.cycles += u64::from(INSTRUCTION_CYCLES[opcode as usize]);
    if page_crossed {
        c.cpu.cycles += u64::from(INSTRUCTION_PAGE_CYCLES[opcode as usize]);
    }
    match opcode {
        0 => brk(c),
        1 | 5 | 9 | 13 | 17 | 21 | 25 | 29 => ora(c, addr),
        6 | 10 | 14 | 22 | 30 => asl(c, addr, mode),
        8 => php(c),
        16 => bpl(c, addr),
        24 => clc(c),
        32 => jsr(c, addr),
        33 | 37 | 41 | 45 | 49 | 53 | 57 | 61 => and(c, addr),
        36 | 44 => bit(c, addr),
        38 | 42 | 46 | 54 | 62 => rol(c, addr, mode),
        40 => plp(c),
        48 => bmi(c, addr),
        56 => sec(c),
        64 => rti(c),
        65 | 69 | 73 | 77 | 81 | 85 | 89 | 93 => eor(c, addr),
        70 | 74 | 78 | 86 | 94 => lsr(c, addr, mode),
        72 => pha(c),
        76 | 108 => jmp(c, addr),
        80 => bvc(c, addr),
        88 => cli(c),
        96 => rts(c),
        97 | 101 | 105 | 109 | 113 | 117 | 121 | 125 => adc(c, addr),
        102 | 106 | 110 | 118 | 126 => ror(c, addr, mode),
        104 => pla(c),
        112 => bvs(c, addr),
        120 => sei(c),
        129 | 133 | 141 | 145 | 149 | 153 | 157 => sta(c, addr),
        132 | 140 | 148 => sty(c, addr),
        134 | 142 | 150 => stx(c, addr),
        136 => dey(c),
        138 => txa(c),
        144 => bcc(c, addr),
        152 => tya(c),
        154 => txs(c),
        160 | 164 | 172 | 180 | 188 => ldy(c, addr),
        161 | 165 | 169 | 173 | 177 | 181 | 185 | 189 => lda(c, addr),
        162 | 166 | 174 | 182 | 190 => ldx(c, addr),
        168 => tay(c),
        170 => tax(c),
        176 => bcs(c, addr),
        184 => clv(c),
        186 => tsx(c),
        192 | 196 | 204 => cpy(c, addr),
        193 | 197 | 201 | 205 | 209 | 213 | 217 | 221 => cmp(c, addr),
        198 | 206 | 214 | 222 => dec(c, addr),
        200 => iny(c),
        202 => dex(c),
        208 => bne(c, addr),
        216 => cld(c),
        224 | 228 | 236 => cpx(c, addr),
        225 | 229 | 233 | 235 | 237 | 241 | 245 | 249 | 253 => sbc(c, addr),
        230 | 238 | 246 | 254 => inc(c, addr),
        232 => inx(c),
        240 => beq(c, addr),
        248 => sed(c),
        _ => (),
    };
}

pub fn addr_mode(c: &Console, mode: u8) -> (u16, bool) {
    match mode {
        1 => abs(c),
        2 => absx(c),
        3 => absy(c),
        4 => acc(),
        5 => imm(c),
        6 => imp(),
        7 => idxind(c),
        8 => ind(c),
        9 => indidx(c),
        10 => rel(c),
        11 => zpg(c),
        12 => zpgx(c),
        13 => zpgy(c),
        _ => panic!("invalid addressing mode"),
    }
}

fn pages_differ(a: u16, b: u16) -> bool {
    a & 0xFF00 != b & 0xFF00
}

fn add_branch_cycles(c: &mut Console, pc: u16, addr: u16) {
    c.cpu.cycles += if pages_differ(pc, addr) { 2 } else { 1 };
}

fn compare(c: &mut Console, a: u8, b: u8) {
    c.cpu.set_zn(a - b);
    if a >= b {
        c.cpu.c = 1;
    } else {
        c.cpu.c = 0;
    }
}

/// # Addressing modes

/// Absolute
pub fn abs(c: &Console) -> (u16, bool) {
    (read16(c, c.cpu.pc + 1), false)
}

/// AbsoluteX
pub fn absx(c: &Console) -> (u16, bool) {
    let addr = read16(c, c.cpu.pc + 1);
    let xaddr = addr + u16::from(c.cpu.x);
    let page_crossed = pages_differ(addr, xaddr);
    (xaddr, page_crossed)
}

/// AbsoluteY
pub fn absy(c: &Console) -> (u16, bool) {
    let addr = read16(c, c.cpu.pc + 1);
    let yaddr = addr + u16::from(c.cpu.y);
    let page_crossed = pages_differ(addr, yaddr);
    (yaddr, page_crossed)
}

/// Accumulator
pub fn acc() -> (u16, bool) {
    (0, false)
}

/// Immediate
pub fn imm(c: &Console) -> (u16, bool) {
    (c.cpu.pc + 1, false)
}

/// Implied
pub fn imp() -> (u16, bool) {
    (0, false)
}

/// IndexedIndirect
pub fn idxind(c: &Console) -> (u16, bool) {
    println!(
        "read addr: 0x{:04X} -> 0x{:04X} + 0x{:04X}\nfinal addr: 0x{:04X}",
        c.cpu.pc + 1,
        read_byte(c, c.cpu.pc + 1),
        c.cpu.x,
        read16bug(c, u16::from(read_byte(c, c.cpu.pc + 1) + c.cpu.x)),
    );
    (
        read16bug(c, u16::from(read_byte(c, c.cpu.pc + 1) + c.cpu.x)),
        false,
    )
}

/// Indirect
pub fn ind(c: &Console) -> (u16, bool) {
    (read16bug(c, read16(c, c.cpu.pc + 1)), false)
}

/// IndirectIndexed
pub fn indidx(c: &Console) -> (u16, bool) {
    let addr = read16bug(c, u16::from(read_byte(c, c.cpu.pc + 1)));
    let yaddr = addr + u16::from(c.cpu.y);
    let page_crossed = pages_differ(addr, yaddr);
    (yaddr, page_crossed)
}

/// Relative
pub fn rel(c: &Console) -> (u16, bool) {
    let mut offset = u16::from(read_byte(c, c.cpu.pc + 1));
    if offset >= 0x80 {
        offset -= 0x100;
    }
    let addr = c.cpu.pc + 2 + offset;
    (addr, false)
}

/// ZeroPage
pub fn zpg(c: &Console) -> (u16, bool) {
    (u16::from(read_byte(c, c.cpu.pc + 1)), false)
}

/// ZeroPageX
pub fn zpgx(c: &Console) -> (u16, bool) {
    (
        u16::from(read_byte(c, c.cpu.pc + 1) + c.cpu.x) & 0xFF,
        false,
    )
}

/// ZeroPageY
pub fn zpgy(c: &Console) -> (u16, bool) {
    (
        u16::from(read_byte(c, c.cpu.pc + 1) + c.cpu.y) & 0xFF,
        false,
    )
}

/// # Storage

/// LDA: Load A with M
pub fn lda(c: &mut Console, addr: u16) {
    c.cpu.a = read_byte(c, addr);
    println!("lda: 0x{:04X} ({})", addr, c.cpu.a);
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
    println!("sta: {} -> 0x{:04X}", c.cpu.a, addr);
    write(c, addr, c.cpu.a);
}

/// STX: Store X in M
pub fn stx(c: &mut Console, addr: u16) {
    write(c, addr, c.cpu.x);
}

/// STY: Store Y in M
pub fn sty(c: &mut Console, addr: u16) {
    write(c, addr, c.cpu.y);
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
    let carry = c.cpu.c;
    c.cpu.a += val + carry;
    c.cpu.set_zn(c.cpu.a);
    if i32::from(a) + i32::from(val) + i32::from(carry) > 0xFF {
        c.cpu.c = 1;
    } else {
        c.cpu.c = 0;
    }
    if (a ^ val) & 0x80 == 0 && (a ^ c.cpu.a) & 0x80 != 0 {
        c.cpu.v = 1;
    } else {
        c.cpu.v = 0;
    }
}

/// SBC: Subtract M from A with Carry
pub fn sbc(c: &mut Console, addr: u16) {
    let a = c.cpu.a;
    let val = read_byte(c, addr);
    let carry = c.cpu.c;
    c.cpu.a -= val - (1 - carry);
    c.cpu.set_zn(c.cpu.a);
    if i32::from(a) + i32::from(val) + i32::from(1 - carry) >= 0 {
        c.cpu.c = 1;
    } else {
        c.cpu.c = 0;
    }
    if (a ^ val) & 0x80 != 0 && (a ^ c.cpu.a) & 0x80 != 0 {
        c.cpu.v = 1;
    } else {
        c.cpu.v = 0;
    }
}

/// DEC: Decrement M by One
pub fn dec(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr) - 1;
    write(c, addr, val);
    c.cpu.set_zn(val);
}

/// DEX: Decrement X by One
pub fn dex(c: &mut Console) {
    c.cpu.x -= 1;
    c.cpu.set_zn(c.cpu.x);
}

/// DEY: Decrement Y by One
pub fn dey(c: &mut Console) {
    c.cpu.y -= 1;
    c.cpu.set_zn(c.cpu.y);
}

/// INC: Increment M by One
pub fn inc(c: &mut Console, addr: u16) {
    let val = read_byte(c, addr) + 1;
    write(c, addr, val);
    c.cpu.set_zn(val);
}

/// INX: Increment X by One
pub fn inx(c: &mut Console) {
    c.cpu.x += 1;
    c.cpu.set_zn(c.cpu.y);
}

/// INY: Increment Y by One
pub fn iny(c: &mut Console) {
    c.cpu.y += 1;
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
pub fn asl(c: &mut Console, addr: u16, mode: u8) {
    match mode {
        // Accumulator
        4 => {
            c.cpu.c = (c.cpu.a >> 7) & 1;
            c.cpu.a <<= 1;
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = (val >> 7) & 1;
            val <<= 1;
            write(c, addr, val);
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
pub fn lsr(c: &mut Console, addr: u16, mode: u8) {
    match mode {
        // Accumulator
        4 => {
            c.cpu.c = c.cpu.a & 1;
            c.cpu.a >>= 1;
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = val & 1;
            val >>= 1;
            write(c, addr, val);
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
pub fn rol(c: &mut Console, addr: u16, mode: u8) {
    let tmp_c = c.cpu.c;
    match mode {
        // Accumulator
        4 => {
            c.cpu.c = (c.cpu.a >> 7) & 1;
            c.cpu.a = (c.cpu.a << 1) | tmp_c;
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = (val >> 7) & 1;
            val = (val << 1) | tmp_c;
            write(c, addr, val);
            c.cpu.set_zn(val);
        }
    }
}

/// ROR: Rotate One Bit Right (M or A)
pub fn ror(c: &mut Console, addr: u16, mode: u8) {
    let tmp_c = c.cpu.c;
    match mode {
        // Accumulator
        4 => {
            c.cpu.c = c.cpu.a & 1;
            c.cpu.a = (c.cpu.a >> 1) | (tmp_c << 7);
            c.cpu.set_zn(c.cpu.a);
        }
        _ => {
            let mut val = read_byte(c, addr);
            c.cpu.c = val & 1;
            val = (val >> 1) | (tmp_c << 7);
            write(c, addr, val);
            c.cpu.set_zn(val);
        }
    }
}

/// # Branch

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
    println!("jmp 0x{:04X}", addr);
    c.cpu.pc = addr;
}

/// JSR: Jump to Location Save Return addr
pub fn jsr(c: &mut Console, addr: u16) {
    push16(c, c.cpu.pc - 1);
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
    c.cpu.pc = pull16(c) + 1;
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
pub fn nop() {
    unimplemented!();
}

/// # Unofficial

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

pub fn lax() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::console::Console;
    use crate::core::memory::{pull, pull16, read_byte, write};
    use std::path::PathBuf;

    fn new_console() -> Console {
        let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
        let rom_path = PathBuf::from(rom);
        Console::new(&rom_path).expect("valid console")
    }

    #[test]
    fn test_opcodes() {
        for i in 0u8..=255 {
            let mut c = new_console();
            test_opstate(&mut c, i);
        }
    }

    fn test_opstate(c: &mut Console, opcode: u8) {
        let addr = 0x0100;
        match opcode {
            // BRK - Force Interrupt
            0 => {
                let flags = c.cpu.flags();
                let pc = c.cpu.pc;
                brk(c);
                // Interrupt disable bit set
                assert_eq!(c.cpu.i, 1);
                // Startup processor status is on the stack
                assert_eq!(pull(c), flags | 0x10);
                // pc stored on stack
                assert_eq!(pull16(c), pc);
            }
            // ORA - "OR" M with A
            1 | 5 | 9 | 13 | 17 | 21 | 25 | 29 => {
                // Test cases
                // M | A | M OR A | z | n
                // 0 | 0 | 0      | 1 | 0
                // 1 | 0 | 1      | 0 | 0
                // 0 | 1 | 1      | 0 | 0
                // 1 | 1 | 1      | 0 | 0

                write(c, addr, 0);
                c.cpu.a = 0;
                ora(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.pc = u16::from(opcode);
                c.cpu.a = 0;
                ora(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 0);
                c.cpu.pc = u16::from(opcode);
                c.cpu.a = 1;
                ora(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.pc = u16::from(opcode);
                c.cpu.a = 1;
                ora(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();
            }
            // ASL Shift Left M
            6 | 14 | 22 | 30 => {
                // Test cases
                //            | C | M   | z | n
                // val == 0   | 0 | 0   | 1 | 0
                // val <= 127 | 0 | 2*M | 0 | 0
                // val > 127  | 1 | 2*M | 0 | 0
                write(c, addr, 0);
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(read_byte(c, addr), 0);
                c.reset();

                write(c, addr, 50);
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(read_byte(c, addr), 100);
                c.reset();

                write(c, addr, 130);
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 1);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(read_byte(c, addr), 4);
                c.reset();
            }
            // PHP Push Processor Status
            8 => {
                let flags = c.cpu.flags();
                php(c);
                // Startup processor status is on the stack
                assert_eq!(pull(c), flags | 0x10);
            }
            // ASL Shift Left A
            10 => {
                // Test cases
                //            | C | A   | z | n
                // val == 0   | 0 | 0   | 1 | 0
                // val <= 127 | 0 | 2*M | 0 | 0
                // val > 127  | 1 | 2*M | 0 | 0
                c.cpu.a = 0;
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(c.cpu.a, 0);
                c.reset();

                c.cpu.a = 50;
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 0);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(c.cpu.a, 100);
                c.reset();

                c.cpu.a = 130;
                asl(c, addr, INSTRUCTION_MODES[opcode as usize]);
                assert_eq!(c.cpu.c, 1);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                assert_eq!(c.cpu.a, 4);
                c.reset();
            }
            // BPL Branch on Result Plus
            16 => {
                // Test cases
                // pages_differ
                // 0
                // 1
                let cycles = c.cpu.cycles;
                let addr = 0x0080;
                bpl(c, addr);
                assert_eq!(c.cpu.pc, addr);
                assert_eq!(c.cpu.cycles, cycles + 1);

                let cycles = c.cpu.cycles;
                let addr = 0xFF00;
                bpl(c, addr);
                assert_eq!(c.cpu.pc, addr);
                assert_eq!(c.cpu.cycles, cycles + 2);
            }
            // CLC Clear Carry Flag
            24 => {
                // Test cases
                // cpu.c = 0
                // cpu.c = 1
                c.cpu.c = 0;
                clc(c);
                assert_eq!(c.cpu.c, 0);

                c.cpu.c = 1;
                clc(c);
                assert_eq!(c.cpu.c, 0);
            }
            // Jump and Save return addr
            32 => {
                let pc = c.cpu.pc;
                jsr(c, addr);
                assert_eq!(u16::from(pull(c)), pc - 1);
                assert_eq!(c.cpu.pc, addr);
            }
            // "And" M with A
            33 | 37 | 41 | 45 | 49 | 53 | 57 | 61 => {
                // Test cases
                // M | A | M & A | z | n
                // 0 | 0 | 0     | 1 | 0
                // 1 | 0 | 0     | 1 | 0
                // 0 | 1 | 0     | 1 | 0
                // 1 | 1 | 1     | 0 | 0

                write(c, addr, 0);
                c.cpu.a = 0;
                and(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.a = 0;
                and(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 0);
                c.cpu.a = 1;
                and(c, addr);
                assert_eq!(c.cpu.z, 1);
                assert_eq!(c.cpu.n, 0);
                c.reset();

                write(c, addr, 1);
                c.cpu.a = 1;
                and(c, addr);
                assert_eq!(c.cpu.z, 0);
                assert_eq!(c.cpu.n, 0);
                c.reset();
            }
            // BIT Test bits in M with A
            36 | 44 => {
                // Test cases
                // V | Z | N
                // 0 | 0 | 0
                // 1 | 0 | 0
                // 0 | 1 | 0
                // 1 | 1 | 0
                // 0 | 0 | 1
                // 1 | 0 | 1
                // 0 | 1 | 1
                // 1 | 1 | 1
                // bit(c, addr);
            }
            // 38 | 42 | 46 | 54 | 62 => rol(),
            // 40 => plp(c),
            // 48 => bmi(c, addr),
            // 56 => sec(c),
            // 64 => rti(),
            // 65 | 69 | 73 | 77 | 81 | 85 | 89 | 93 => eor(c, addr),
            // 70 | 74 | 78 | 86 | 94 => lsr(),
            // 72 => pha(c),
            // 76 | 108 => jmp(c, addr),
            // 80 => bvc(c, addr),
            // 88 => cli(c),
            // 96 => rts(c),
            // 97 | 101 | 105 | 109 | 113 | 117 | 121 | 125 => adc(c, addr),
            // 102 | 106 | 110 | 118 | 126 => ror(),
            // 104 => pla(c),
            // 112 => bvs(c, addr),
            // 120 => sei(c),
            // 129 | 133 | 141 | 145 | 149 | 153 | 157 => sta(c, addr),
            // 132 | 140 | 148 => sty(),
            // 134 | 142 | 150 => stx(c, addr),
            // 136 => dey(),
            // 138 => txa(),
            // 144 => bcc(c, addr),
            // 152 => tya(),
            // 154 => txs(c),
            // 160 | 164 | 172 | 180 | 188 => ldy(c, addr),
            // 161 | 165 | 169 | 173 | 177 | 181 | 185 | 189 => lda(c, addr),
            // 162 | 166 | 174 | 182 | 190 => ldx(c, addr),
            // 168 => tay(),
            // 170 => tax(),
            // 176 => bcs(c, addr),
            // 184 => clv(c),
            // 186 => tsx(),
            // 192 | 196 | 204 => cpy(),
            // 193 | 197 | 201 | 205 | 209 | 213 | 217 | 221 => cmp(c, addr),
            // 198 | 206 | 214 | 222 => dec(),
            // 200 => iny(),
            // 202 => dex(),
            // 208 => bne(c, addr),
            // 216 => cld(c),
            // 224 | 228 | 236 => cpx(),
            // 225 | 229 | 233 | 235 | 237 | 241 | 245 | 249 | 253 => sbc(),
            // 230 | 238 | 246 | 254 => inc(),
            // 232 => inx(),
            // 240 => beq(c, addr),
            // 248 => sed(c),
            _ => eprintln!("Warning: opcode {} not covered", opcode),
        }
    }
}
