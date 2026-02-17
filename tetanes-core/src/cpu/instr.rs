//! CPU Asddressing cmps and Operations

use crate::{
    cpu::{Cpu, IrqFlags, Status},
    mem::{Read, Write},
};
use serde::{Deserialize, Serialize};

/// List of all CPU official and unofficial operations.
///
/// # References
///
/// - <https://wiki.nesdev.org/w/index.php/6502_instructions>
/// - <http://archive.6502.org/datasheets/rockwell_r650x_r651x.pdf>
#[rustfmt::skip]
#[allow(clippy::upper_case_acronyms, reason = "more idiomatic for cpu instructions")]
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Instr {
    ADC, AND, ASL, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP, CPX,
    CPY, DEC, DEX, DEY, EOR, INC, INX, INY, JMP, JSR, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA,
    PLP, ROL, ROR, RTI, RTS, SBC, SEC, SED, SEI, STA, STX, STY, TAX, TAY, TSX, TXA, TXS, TYA,
    // "Unofficial" opcodes
    ISB, DCP, AXS, LAS, LAX, AHX, SAX, XAA, SXA, RRA, TAS, SYA, ARR, SRE, ALR, RLA, ANC, SHAZ, ATX,
    SHAA, SLO, #[default] HLT
}

/// CPU Addressing mode.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms, reason = "more idiomatic for cpu addressing modes")]
#[rustfmt::skip]
#[must_use]
pub enum AddrMode {
    // Accumulator and Implied
    ACC, IMP,
    // Immediate and relative
    #[default] IMM, REL,
    // Zero Page
    ZP0, ABS, ZPX, ZPY,
    // Indirect, with read/write variants
    IND, IDX, IDY, IDYW,
    // Absolute, with read/write variants
    ABX, ABXW, ABY, ABYW,
    // Special address mode, handled separately
    OTH
}

/// CPU Opcode.
#[derive(Debug, Copy, Clone)]
#[must_use]
pub struct Op {
    f: fn(&mut Cpu),
    addr_mode: AddrMode,
}

impl Op {
    #[inline(always)]
    pub fn run(&self, cpu: &mut Cpu) {
        (self.f)(cpu)
    }

    #[inline(always)]
    pub const fn addr_mode(&self) -> AddrMode {
        self.addr_mode
    }
}

macro_rules! op {
    ($f:ident, $addr_mode:ident) => {
        Op {
            f: Cpu::$f,
            addr_mode: AddrMode::$addr_mode,
        }
    };
}

/// CPU Instruction Reference.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct InstrRef {
    pub opcode: u8,
    pub instr: Instr,
    pub addr_mode: AddrMode,
    pub cycles: u8,
}

impl std::fmt::Display for InstrRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let instr = self.instr;
        #[allow(
            clippy::wildcard_enum_match_arm,
            reason = "only unofficial instructions are marked with a *"
        )]
        let unofficial = match instr {
            Instr::HLT
            | Instr::ISB
            | Instr::DCP
            | Instr::AXS
            | Instr::LAS
            | Instr::LAX
            | Instr::AHX
            | Instr::SAX
            | Instr::XAA
            | Instr::SXA
            | Instr::RRA
            | Instr::TAS
            | Instr::SYA
            | Instr::ARR
            | Instr::SRE
            | Instr::ALR
            | Instr::RLA
            | Instr::ANC
            | Instr::SLO => "*",
            Instr::NOP if self.opcode != 0xEA => "*", // 0xEA is the only official NOP
            Instr::SBC if self.opcode == 0xEB => "*",
            _ => "",
        };
        write!(f, "{unofficial:1}{instr:?}")
    }
}

macro_rules! instr {
    ($opcode:expr, $instr:ident, $addr_mode:ident, $cycles:expr) => {
        InstrRef {
            opcode: $opcode,
            instr: Instr::$instr,
            addr_mode: AddrMode::$addr_mode,
            cycles: $cycles,
        }
    };
}

/// CPU Addressing Modes
///
/// The 6502 can address 64KB from 0x0000 - 0xFFFF. The high byte is usually the page and the
/// low byte the offset into the page. There are 256 total pages of 256 bytes.
impl Cpu {
    /// 16x16 grid of 6502 opcode operations. Matches datasheet matrix for easy lookup
    #[rustfmt::skip]
    pub const OPS: [Op; 256] = [
        //      0              1               2              3                4              5              6               7              8              9               A               B               C               D               E                F
        /* 0 */ op!(brk, IMM), op!(ora, IDX),  op!(hlt, IMP), op!(slo,  IDX),  op!(nop, ZP0), op!(ora, ZP0), op!(aslm, ZP0), op!(slo, ZP0), op!(php, IMP), op!(ora, IMM),  op!(asla, ACC), op!(anc, IMM),  op!(nop,  ABS), op!(ora, ABS),  op!(aslm, ABS),  op!(slo,  ABS),
        /* 1 */ op!(bpl, REL), op!(ora, IDY),  op!(hlt, IMP), op!(slo,  IDYW), op!(nop, ZPX), op!(ora, ZPX), op!(aslm, ZPX), op!(slo, ZPX), op!(clc, IMP), op!(ora, ABY),  op!(nop,  IMP), op!(slo, ABYW), op!(nop,  ABX), op!(ora, ABX),  op!(aslm, ABXW), op!(slo,  ABXW),
        /* 2 */ op!(jsr, OTH), op!(and, IDX),  op!(hlt, IMP), op!(rla,  IDX),  op!(bit, ZP0), op!(and, ZP0), op!(rolm, ZP0), op!(rla, ZP0), op!(plp, IMP), op!(and, IMM),  op!(rola, ACC), op!(anc, IMM),  op!(bit,  ABS), op!(and, ABS),  op!(rolm, ABS),  op!(rla,  ABS),
        /* 3 */ op!(bmi, REL), op!(and, IDY),  op!(hlt, IMP), op!(rla,  IDYW), op!(nop, ZPX), op!(and, ZPX), op!(rolm, ZPX), op!(rla, ZPX), op!(sec, IMP), op!(and, ABY),  op!(nop,  IMP), op!(rla, ABYW), op!(nop,  ABX), op!(and, ABX),  op!(rolm, ABXW), op!(rla,  ABXW),
        /* 4 */ op!(rti, IMP), op!(eor, IDX),  op!(hlt, IMP), op!(sre,  IDX),  op!(nop, ZP0), op!(eor, ZP0), op!(lsrm, ZP0), op!(sre, ZP0), op!(pha, IMP), op!(eor, IMM),  op!(lsra, ACC), op!(alr, IMM),  op!(jmpa, ABS), op!(eor, ABS),  op!(lsrm, ABS),  op!(sre,  ABS),
        /* 5 */ op!(bvc, REL), op!(eor, IDY),  op!(hlt, IMP), op!(sre,  IDYW), op!(nop, ZPX), op!(eor, ZPX), op!(lsrm, ZPX), op!(sre, ZPX), op!(cli, IMP), op!(eor, ABY),  op!(nop,  IMP), op!(sre, ABYW), op!(nop,  ABX), op!(eor, ABX),  op!(lsrm, ABXW), op!(sre,  ABXW),
        /* 6 */ op!(rts, IMP), op!(adc, IDX),  op!(hlt, IMP), op!(rra,  IDX),  op!(nop, ZP0), op!(adc, ZP0), op!(rorm, ZP0), op!(rra, ZP0), op!(pla, IMP), op!(adc, IMM),  op!(rora, ACC), op!(arr, IMM),  op!(jmpi, IND), op!(adc, ABS),  op!(rorm, ABS),  op!(rra,  ABS),
        /* 7 */ op!(bvs, REL), op!(adc, IDY),  op!(hlt, IMP), op!(rra,  IDYW), op!(nop, ZPX), op!(adc, ZPX), op!(rorm, ZPX), op!(rra, ZPX), op!(sei, IMP), op!(adc, ABY),  op!(nop,  IMP), op!(rra, ABYW), op!(nop,  ABX), op!(adc, ABX),  op!(rorm, ABXW), op!(rra,  ABXW),
        /* 8 */ op!(nop, IMM), op!(sta, IDX),  op!(nop, IMM), op!(sax,  IDX),  op!(sty, ZP0), op!(sta, ZP0), op!(stx,  ZP0), op!(sax, ZP0), op!(dey, IMP), op!(nop, IMM),  op!(txa,  IMP), op!(xaa, IMM),  op!(sty,  ABS), op!(sta, ABS),  op!(stx,  ABS),  op!(sax,  ABS),
        /* 9 */ op!(bcc, REL), op!(sta, IDYW), op!(hlt, IMP), op!(shaz, OTH),  op!(sty, ZPX), op!(sta, ZPX), op!(stx,  ZPY), op!(sax, ZPY), op!(tya, IMP), op!(sta, ABYW), op!(txs,  IMP), op!(tas, OTH),  op!(sya,  OTH), op!(sta, ABXW), op!(sxa,  OTH),  op!(shaa, OTH),
        /* A */ op!(ldy, IMM), op!(lda, IDX),  op!(ldx, IMM), op!(lax,  IDX),  op!(ldy, ZP0), op!(lda, ZP0), op!(ldx,  ZP0), op!(lax, ZP0), op!(tay, IMP), op!(lda, IMM),  op!(tax,  IMP), op!(atx, IMM),  op!(ldy,  ABS), op!(lda, ABS),  op!(ldx,  ABS),  op!(lax,  ABS),
        /* B */ op!(bcs, REL), op!(lda, IDY),  op!(hlt, IMP), op!(lax,  IDY),  op!(ldy, ZPX), op!(lda, ZPX), op!(ldx,  ZPY), op!(lax, ZPY), op!(clv, IMP), op!(lda, ABY),  op!(tsx,  IMP), op!(las, ABY),  op!(ldy,  ABX), op!(lda, ABX),  op!(ldx,  ABY),  op!(lax,  ABY),
        /* C */ op!(cpy, IMM), op!(cpa, IDX),  op!(nop, IMM), op!(dcp,  IDX),  op!(cpy, ZP0), op!(cpa, ZP0), op!(dec,  ZP0), op!(dcp, ZP0), op!(iny, IMP), op!(cpa, IMM),  op!(dex,  IMP), op!(axs, IMM),  op!(cpy,  ABS), op!(cpa, ABS),  op!(dec,  ABS),  op!(dcp,  ABS),
        /* D */ op!(bne, REL), op!(cpa, IDY),  op!(hlt, IMP), op!(dcp,  IDYW), op!(nop, ZPX), op!(cpa, ZPX), op!(dec,  ZPX), op!(dcp, ZPX), op!(cld, IMP), op!(cpa, ABY),  op!(nop,  IMP), op!(dcp, ABYW), op!(nop,  ABX), op!(cpa, ABX),  op!(dec,  ABXW), op!(dcp,  ABXW),
        /* E */ op!(cpx, IMM), op!(sbc, IDX),  op!(nop, IMM), op!(isb,  IDX),  op!(cpx, ZP0), op!(sbc, ZP0), op!(inc,  ZP0), op!(isb, ZP0), op!(inx, IMP), op!(sbc, IMM),  op!(nop,  IMP), op!(sbc, IMM),  op!(cpx,  ABS), op!(sbc, ABS),  op!(inc,  ABS),  op!(isb,  ABS),
        /* F */ op!(beq, REL), op!(sbc, IDY),  op!(hlt, IMP), op!(isb,  IDYW), op!(nop, ZPX), op!(sbc, ZPX), op!(inc,  ZPX), op!(isb, ZPX), op!(sed, IMP), op!(sbc, ABY),  op!(nop,  IMP), op!(isb, ABYW), op!(nop,  ABX), op!(sbc, ABX),  op!(inc,  ABXW), op!(isb,  ABXW),
    ];

    /// 16x16 grid of 6502 opcode instructions. Matches datasheet matrix for easy lookup
    #[rustfmt::skip]
    pub const INSTR_REF: [InstrRef; 256] = [
        instr!(0x00, BRK, IMM, 7), instr!(0x01, ORA, IDX,  6), instr!(0x02, HLT, IMP, 2), instr!(0x03, SLO, IDX,  8), instr!(0x04, NOP, ZP0, 3), instr!(0x05, ORA, ZP0, 3), instr!(0x06, ASL, ZP0, 5), instr!(0x07, SLO, ZP0, 5), instr!(0x08, PHP, IMP, 3), instr!(0x09, ORA, IMM,  2), instr!(0x0A, ASL, ACC, 2), instr!(0x0B, ANC, IMM,  2), instr!(0x0C, NOP, ABS, 4), instr!(0x0D, ORA, ABS,  4), instr!(0x0E, ASL, ABS,  6), instr!(0x0F, SLO,  ABS,  6),
        instr!(0x10, BPL, REL, 2), instr!(0x11, ORA, IDY,  5), instr!(0x12, HLT, IMP, 2), instr!(0x13, SLO, IDYW, 8), instr!(0x14, NOP, ZPX, 4), instr!(0x15, ORA, ZPX, 4), instr!(0x16, ASL, ZPX, 6), instr!(0x17, SLO, ZPX, 6), instr!(0x18, CLC, IMP, 2), instr!(0x19, ORA, ABY,  4), instr!(0x1A, NOP, IMP, 2), instr!(0x1B, SLO, ABYW, 7), instr!(0x1C, NOP, ABX, 4), instr!(0x1D, ORA, ABX,  4), instr!(0x1E, ASL, ABXW, 7), instr!(0x1F, SLO,  ABXW, 7),
        instr!(0x20, JSR, OTH, 6), instr!(0x21, AND, IDX,  6), instr!(0x22, HLT, IMP, 2), instr!(0x23, RLA, IDX,  8), instr!(0x24, BIT, ZP0, 3), instr!(0x25, AND, ZP0, 3), instr!(0x26, ROL, ZP0, 5), instr!(0x27, RLA, ZP0, 5), instr!(0x28, PLP, IMP, 4), instr!(0x29, AND, IMM,  2), instr!(0x2A, ROL, ACC, 2), instr!(0x2B, ANC, IMM,  2), instr!(0x2C, BIT, ABS, 4), instr!(0x2D, AND, ABS,  4), instr!(0x2E, ROL, ABS,  6), instr!(0x2F, RLA,  ABS,  6),
        instr!(0x30, BMI, REL, 2), instr!(0x31, AND, IDY,  5), instr!(0x32, HLT, IMP, 2), instr!(0x33, RLA, IDYW, 8), instr!(0x34, NOP, ZPX, 4), instr!(0x35, AND, ZPX, 4), instr!(0x36, ROL, ZPX, 6), instr!(0x37, RLA, ZPX, 6), instr!(0x38, SEC, IMP, 2), instr!(0x39, AND, ABY,  4), instr!(0x3A, NOP, IMP, 2), instr!(0x3B, RLA, ABYW, 7), instr!(0x3C, NOP, ABX, 4), instr!(0x3D, AND, ABX,  4), instr!(0x3E, ROL, ABXW, 7), instr!(0x3F, RLA,  ABXW, 7),
        instr!(0x40, RTI, IMP, 6), instr!(0x41, EOR, IDX,  6), instr!(0x42, HLT, IMP, 2), instr!(0x43, SRE, IDX,  8), instr!(0x44, NOP, ZP0, 3), instr!(0x45, EOR, ZP0, 3), instr!(0x46, LSR, ZP0, 5), instr!(0x47, SRE, ZP0, 5), instr!(0x48, PHA, IMP, 3), instr!(0x49, EOR, IMM,  2), instr!(0x4A, LSR, ACC, 2), instr!(0x4B, ALR, IMM,  2), instr!(0x4C, JMP, ABS, 3), instr!(0x4D, EOR, ABS,  4), instr!(0x4E, LSR, ABS,  6), instr!(0x4F, SRE,  ABS,  6),
        instr!(0x50, BVC, REL, 2), instr!(0x51, EOR, IDY,  5), instr!(0x52, HLT, IMP, 2), instr!(0x53, SRE, IDYW, 8), instr!(0x54, NOP, ZPX, 4), instr!(0x55, EOR, ZPX, 4), instr!(0x56, LSR, ZPX, 6), instr!(0x57, SRE, ZPX, 6), instr!(0x58, CLI, IMP, 2), instr!(0x59, EOR, ABY,  4), instr!(0x5A, NOP, IMP, 2), instr!(0x5B, SRE, ABYW, 7), instr!(0x5C, NOP, ABX, 4), instr!(0x5D, EOR, ABX,  4), instr!(0x5E, LSR, ABXW, 7), instr!(0x5F, SRE,  ABXW, 7),
        instr!(0x60, RTS, IMP, 6), instr!(0x61, ADC, IDX,  6), instr!(0x62, HLT, IMP, 2), instr!(0x63, RRA, IDX,  8), instr!(0x64, NOP, ZP0, 3), instr!(0x65, ADC, ZP0, 3), instr!(0x66, ROR, ZP0, 5), instr!(0x67, RRA, ZP0, 5), instr!(0x68, PLA, IMP, 4), instr!(0x69, ADC, IMM,  2), instr!(0x6A, ROR, ACC, 2), instr!(0x6B, ARR, IMM,  2), instr!(0x6C, JMP, IND, 5), instr!(0x6D, ADC, ABS,  4), instr!(0x6E, ROR, ABS,  6), instr!(0x6F, RRA,  ABS,  6),
        instr!(0x70, BVS, REL, 2), instr!(0x71, ADC, IDY,  5), instr!(0x72, HLT, IMP, 2), instr!(0x73, RRA, IDYW, 8), instr!(0x74, NOP, ZPX, 4), instr!(0x75, ADC, ZPX, 4), instr!(0x76, ROR, ZPX, 6), instr!(0x77, RRA, ZPX, 6), instr!(0x78, SEI, IMP, 2), instr!(0x79, ADC, ABY,  4), instr!(0x7A, NOP, IMP, 2), instr!(0x7B, RRA, ABYW, 7), instr!(0x7C, NOP, ABX, 4), instr!(0x7D, ADC, ABX,  4), instr!(0x7E, ROR, ABXW, 7), instr!(0x7F, RRA,  ABXW, 7),
        instr!(0x80, NOP, IMM, 2), instr!(0x81, STA, IDX,  6), instr!(0x82, NOP, IMM, 2), instr!(0x83, SAX, IDX,  6), instr!(0x84, STY, ZP0, 3), instr!(0x85, STA, ZP0, 3), instr!(0x86, STX, ZP0, 3), instr!(0x87, SAX, ZP0, 3), instr!(0x88, DEY, IMP, 2), instr!(0x89, NOP, IMM,  2), instr!(0x8A, TXA, IMP, 2), instr!(0x8B, XAA, IMM,  2), instr!(0x8C, STY, ABS, 4), instr!(0x8D, STA, ABS,  4), instr!(0x8E, STX, ABS,  4), instr!(0x8F, SAX,  ABS , 4),
        instr!(0x90, BCC, REL, 2), instr!(0x91, STA, IDYW, 6), instr!(0x92, HLT, IMP, 2), instr!(0x93, AHX, OTH,  6), instr!(0x94, STY, ZPX, 4), instr!(0x95, STA, ZPX, 4), instr!(0x96, STX, ZPY, 4), instr!(0x97, SAX, ZPY, 4), instr!(0x98, TYA, IMP, 2), instr!(0x99, STA, ABYW, 5), instr!(0x9A, TXS, IMP, 2), instr!(0x9B, TAS, OTH,  5), instr!(0x9C, SYA, OTH, 5), instr!(0x9D, STA, ABXW, 5), instr!(0x9E, SXA, OTH,  5), instr!(0x9F, SHAA, OTH,  5),
        instr!(0xA0, LDY, IMM, 2), instr!(0xA1, LDA, IDX,  6), instr!(0xA2, LDX, IMM, 2), instr!(0xA3, LAX, IDX,  6), instr!(0xA4, LDY, ZP0, 3), instr!(0xA5, LDA, ZP0, 3), instr!(0xA6, LDX, ZP0, 3), instr!(0xA7, LAX, ZP0, 3), instr!(0xA8, TAY, IMP, 2), instr!(0xA9, LDA, IMM,  2), instr!(0xAA, TAX, IMP, 2), instr!(0xAB, ATX, IMM,  2), instr!(0xAC, LDY, ABS, 4), instr!(0xAD, LDA, ABS,  4), instr!(0xAE, LDX, ABS,  4), instr!(0xAF, LAX,  ABS,  4),
        instr!(0xB0, BCS, REL, 2), instr!(0xB1, LDA, IDY,  5), instr!(0xB2, HLT, IMP, 2), instr!(0xB3, LAX, IDY,  5), instr!(0xB4, LDY, ZPX, 4), instr!(0xB5, LDA, ZPX, 4), instr!(0xB6, LDX, ZPY, 4), instr!(0xB7, LAX, ZPY, 4), instr!(0xB8, CLV, IMP, 2), instr!(0xB9, LDA, ABY,  4), instr!(0xBA, TSX, IMP, 2), instr!(0xBB, LAS, ABY,  4), instr!(0xBC, LDY, ABX, 4), instr!(0xBD, LDA, ABX,  4), instr!(0xBE, LDX, ABY,  4), instr!(0xBF, LAX,  ABY,  4),
        instr!(0xC0, CPY, IMM, 2), instr!(0xC1, CMP, IDX,  6), instr!(0xC2, NOP, IMM, 2), instr!(0xC3, DCP, IDX,  8), instr!(0xC4, CPY, ZP0, 3), instr!(0xC5, CMP, ZP0, 3), instr!(0xC6, DEC, ZP0, 5), instr!(0xC7, DCP, ZP0, 5), instr!(0xC8, INY, IMP, 2), instr!(0xC9, CMP, IMM,  2), instr!(0xCA, DEX, IMP, 2), instr!(0xCB, AXS, IMM,  2), instr!(0xCC, CPY, ABS, 4), instr!(0xCD, CMP, ABS,  4), instr!(0xCE, DEC, ABS,  6), instr!(0xCF, DCP,  ABS,  6),
        instr!(0xD0, BNE, REL, 2), instr!(0xD1, CMP, IDY,  5), instr!(0xD2, HLT, IMP, 2), instr!(0xD3, DCP, IDYW, 8), instr!(0xD4, NOP, ZPX, 4), instr!(0xD5, CMP, ZPX, 4), instr!(0xD6, DEC, ZPX, 6), instr!(0xD7, DCP, ZPX, 6), instr!(0xD8, CLD, IMP, 2), instr!(0xD9, CMP, ABY,  4), instr!(0xDA, NOP, IMP, 2), instr!(0xDB, DCP, ABYW, 7), instr!(0xDC, NOP, ABX, 4), instr!(0xDD, CMP, ABX,  4), instr!(0xDE, DEC, ABXW, 7), instr!(0xDF, DCP,  ABXW, 7),
        instr!(0xE0, CPX, IMM, 2), instr!(0xE1, SBC, IDX,  6), instr!(0xE2, NOP, IMM, 2), instr!(0xE3, ISB, IDX,  8), instr!(0xE4, CPX, ZP0, 3), instr!(0xE5, SBC, ZP0, 3), instr!(0xE6, INC, ZP0, 5), instr!(0xE7, ISB, ZP0, 5), instr!(0xE8, INX, IMP, 2), instr!(0xE9, SBC, IMM,  2), instr!(0xEA, NOP, IMP, 2), instr!(0xEB, SBC, IMM,  2), instr!(0xEC, CPX, ABS, 4), instr!(0xED, SBC, ABS,  4), instr!(0xEE, INC, ABS,  6), instr!(0xEF, ISB,  ABS,  6),
        instr!(0xF0, BEQ, REL, 2), instr!(0xF1, SBC, IDY,  5), instr!(0xF2, HLT, IMP, 2), instr!(0xF3, ISB, IDYW, 8), instr!(0xF4, NOP, ZPX, 4), instr!(0xF5, SBC, ZPX, 4), instr!(0xF6, INC, ZPX, 6), instr!(0xF7, ISB, ZPX, 6), instr!(0xF8, SED, IMP, 2), instr!(0xF9, SBC, ABY,  4), instr!(0xFA, NOP, IMP, 2), instr!(0xFB, ISB, ABYW, 7), instr!(0xFC, NOP, ABX, 4), instr!(0xFD, SBC, ABX,  4), instr!(0xFE, INC, ABXW, 7), instr!(0xFF, ISB,  ABXW, 7),
    ];

    /// Accumulator Addressing.
    ///
    /// No additional data is required, but the default target will be the accumulator.
    ///
    /// # Instructions
    ///
    /// ASL, ROL, LSR, ROR
    ///
    /// ```text
    ///    #  address R/W description
    ///   --- ------- --- -----------------------------------------------
    ///    1    PC     R  fetch opcode, increment PC
    ///    2    PC     R  read next instruction byte (and throw it away)
    /// ```
    ///
    /// Implied Addressing.
    ///
    /// No additional data is required, but the default target will be the accumulator.
    ///
    /// ```text
    ///    #  address R/W description
    ///   --- ------- --- -----------------------------------------------
    ///    1    PC     R  fetch opcode, increment PC
    ///    2    PC     R  read next instruction byte (and throw it away)
    /// ```
    #[inline(always)]
    pub fn acc_imp(&mut self) -> u16 {
        self.read(self.pc); // Cycle 2, dummy read
        0
    }

    /// Immediate Addressing.
    ///
    /// Uses the next byte as the value.
    ///
    /// ```text
    ///    #  address R/W description
    ///   --- ------- --- ------------------------------------------
    ///    1    PC     R  fetch opcode, increment PC
    ///    2    PC     R  fetch value, increment PC
    /// ```
    ///
    /// Relative Addressing.
    ///
    /// This mode is only used by branching instructions. The address must be between -128 and +127,
    /// allowing the branching instruction to move backward or forward relative to the current
    /// program counter.
    ///
    /// # Notes
    ///
    /// The opcode fetch of the next instruction is included to this diagram for illustration
    /// purposes. When determining real execution times, remember to subtract the last cycle.
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ---------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch fetched_data, increment PC
    ///  3     PC      R  Fetch opcode of next instruction,
    ///                   If branch is taken, add fetched_data to PCL.
    ///                   Otherwise increment PC.
    ///  4+    PC*     R  Fetch opcode of next instruction.
    ///                   Fix PCH. If it did not change, increment PC.
    ///  5!    PC      R  Fetch opcode of next instruction,
    ///                   increment PC.
    ///
    ///     * The high byte of Program Counter (PCH) may be invalid
    ///       at this time, i.e. it may be smaller or bigger by $100.
    ///     + If branch is taken, this cycle will be executed.
    ///     ! If branch occurs to different page, this cycle will be
    ///       executed.
    /// ```
    ///
    /// Zero Page Addressing.
    ///
    /// Accesses the first 0xFF bytes of the address range, so this only requires one extra byte
    /// instead of the usual two.
    ///
    /// # Read instructions
    ///
    /// LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT, LAX, NOP
    ///
    /// ```text
    ///    #  address R/W description
    ///   --- ------- --- ------------------------------------------
    ///    1    PC     R  fetch opcode, increment PC
    ///    2    PC     R  fetch address, increment PC
    ///    3  address  R  read from effective address
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// ASL, LSR, ROL, ROR, INC, DEC, SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    ///    #  address R/W description
    ///   --- ------- --- ------------------------------------------
    ///    1    PC     R  fetch opcode, increment PC
    ///    2    PC     R  fetch address, increment PC
    ///    3  address  R  read from effective address
    ///    4  address  W  write the value back to effective address,
    ///                   and do the operation on it
    ///    5  address  W  write the new value to effective address
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, STX, STY, SAX
    ///
    /// ```text
    ///    #  address R/W description
    ///   --- ------- --- ------------------------------------------
    ///    1    PC     R  fetch opcode, increment PC
    ///    2    PC     R  fetch address, increment PC
    ///    3  address  W  write register to effective address
    /// ```
    #[inline(always)]
    pub fn imm_rel_zp(&mut self) -> u16 {
        u16::from(self.fetch_byte()) // Cycle 2
    }

    /// Zero Page Addressing w/ X offset.
    ///
    /// Same as Zero Page, but is offset by adding the x register.
    ///
    /// # Read instructions
    ///
    /// LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT, LAX, NOP
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch address, increment PC
    ///  3   address   R  read from address, add index register to it
    ///  4  address+X* R  read from effective address
    ///
    ///     * The high byte of the effective address is always zero,
    ///       i.e. page boundary crossings are not handled.
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// ASL, LSR, ROL, ROR, INC, DEC, SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ---------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch address, increment PC
    ///  3   address   R  read from address, add index register X to it
    ///  4  address+X* R  read from effective address
    ///  5  address+X* W  write the value back to effective address,
    ///                   and do the operation on it
    ///  6  address+X* W  write the new value to effective address
    ///
    ///     * The high byte of the effective address is always zero,
    ///       i.e. page boundary crossings are not handled.
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, STX, STY, SAX
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- -------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch address, increment PC
    ///  3   address   R  read from address, add index register to it
    ///  4  address+X* W  write to effective address
    ///
    ///     * The high byte of the effective address is always zero,
    ///       i.e. page boundary crossings are not handled.
    /// ```
    #[inline(always)]
    pub fn zpx(&mut self) -> u16 {
        let addr = u16::from(self.fetch_byte()); // Cycle 2
        self.read(addr); // Cycle 3, dummy read
        // High byte is always zero
        addr.wrapping_add(u16::from(self.x)) & 0x00FF
    }

    /// Zero Page Addressing w/ Y offset.
    ///
    /// Same as Zero Page, but is offset by adding the y register.
    ///
    /// # Read instructions
    ///
    /// LDX, LAX
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch address, increment PC
    ///  3   address   R  read from address, add index register to it
    ///  4  address+Y* R  read from effective address
    ///
    ///     * The high byte of the effective address is always zero,
    ///       i.e. page boundary crossings are not handled.
    /// ```
    ///
    /// # Write instructions
    ///
    /// STX, SAX
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- -------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch address, increment PC
    ///  3   address   R  read from address, add index register to it
    ///  4  address+Y* W  write to effective address
    ///
    ///     * The high byte of the effective address is always zero,
    ///       i.e. page boundary crossings are not handled.
    /// ```
    #[inline(always)]
    pub fn zpy(&mut self) -> u16 {
        let addr = u16::from(self.fetch_byte()); // Cycle 2
        self.read(addr); // Cycle 3, dummy read
        // High byte is always zero
        addr.wrapping_add(u16::from(self.y)) & 0x00FF
    }

    /// Absolute Addressing.
    ///
    /// Uses a full 16-bit address as the next value.
    ///
    /// # Read instructions
    ///
    /// LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT, LAX, NOP
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- ------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  fetch low byte of address, increment PC
    ///  3    PC     R  fetch high byte of address, increment PC
    ///  4  address  R  read from effective address
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// ASL, LSR, ROL, ROR, INC, DEC, SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- ------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  fetch low byte of address, increment PC
    ///  3    PC     R  fetch high byte of address, increment PC
    ///  4  address  R  read from effective address
    ///  5  address  W  write the value back to effective address,
    ///                 and do the operation on it
    ///  6  address  W  write the new value to effective address
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, STX, STY, SAX
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- ------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  fetch low byte of address, increment PC
    ///  3    PC     R  fetch high byte of address, increment PC
    ///  4  address  W  write register to effective address
    /// ```
    #[inline(always)]
    pub fn abs(&mut self) -> u16 {
        self.fetch_word() // Cycles 2-3
    }

    /// Absolute Address w/ X offset.
    ///
    /// Same as Absolute, but is offset by adding the x register. If a page boundary is crossed, an
    /// additional clock is required.
    ///
    /// # Read instructions
    ///
    /// LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT, LAX, LAE, SHS, NOP
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch low byte of address, increment PC
    ///  3     PC      R  fetch high byte of address,
    ///                   add index register to low address byte,
    ///                   increment PC
    ///  4  address+X* R  read from effective address,
    ///                   fix the high byte of effective address
    ///  5+ address+X  R  re-read from effective address
    ///
    ///     * The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    ///     + This cycle will be executed only if the effective address
    ///       was invalid during cycle #4, i.e. page boundary was crossed.
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// ASL, LSR, ROL, ROR, INC, DEC, SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    /// #   address  R/W description
    /// -- --------- --- ------------------------------------------
    /// 1    PC       R  fetch opcode, increment PC
    /// 2    PC       R  fetch low byte of address, increment PC
    /// 3    PC       R  fetch high byte of address,
    ///                  add index register X to low address byte,
    ///                  increment PC
    /// 4  address+X* R  read from effective address,
    ///                  fix the high byte of effective address
    /// 5  address+X  R  re-read from effective address
    /// 6  address+X  W  write the value back to effective address,
    ///                  and do the operation on it
    /// 7  address+X  W  write the new value to effective address
    ///
    ///     * The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, STX, STY, SHA, SHX, SHY
    ///
    /// ```text
    /// #   address  R/W description
    /// -- --------- --- ------------------------------------------
    /// 1     PC      R  fetch opcode, increment PC
    /// 2     PC      R  fetch low byte of address, increment PC
    /// 3     PC      R  fetch high byte of address,
    ///                  add index register to low address byte,
    ///                  increment PC
    /// 4  address+X* R  read from effective address,
    ///                  fix the high byte of effective address
    /// 5  address+X  W  write to effective address
    ///
    ///     * The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100. Because
    ///       the processor cannot undo a write to an invalid
    ///       address, it always reads from the address first.
    /// ```
    #[inline(always)]
    pub fn abx(&mut self, dummy_read: bool) -> u16 {
        let base_addr = self.fetch_word(); // Cycles 2-3
        let addr = base_addr.wrapping_add(u16::from(self.x));
        if Cpu::pages_differ(base_addr, addr) || dummy_read {
            // Cycle 4 dummy read with fixed high byte
            self.read((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        addr
    }

    /// Absolute Address w/ Y offset.
    ///
    /// Same as Absolute, but is offset by adding the y register. If a page boundary is crossed, an
    /// additional clock is required.
    ///
    /// # Read instructions
    ///
    /// LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT, LAX, LAE, SHS, NOP
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch low byte of address, increment PC
    ///  3     PC      R  fetch high byte of address,
    ///                   add index register to low address byte,
    ///                   increment PC
    ///  4  address+Y* R  read from effective address,
    ///                   fix the high byte of effective address
    ///  5+ address+Y  R  re-read from effective address
    ///
    ///     * The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    ///     + This cycle will be executed only if the effective address
    ///       was invalid during cycle #4, i.e. page boundary was crossed.
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// ASL, LSR, ROL, ROR, INC, DEC, SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1    PC       R  fetch opcode, increment PC
    ///  2    PC       R  fetch low byte of address, increment PC
    ///  3    PC       R  fetch high byte of address,
    ///                   add index register Y to low address byte,
    ///                   increment PC
    ///  4  address+Y* R  read from effective address,
    ///                   fix the high byte of effective address
    ///  5  address+Y  R  re-read from effective address
    ///  6  address+Y  W  write the value back to effective address,
    ///                   and do the operation on it
    ///  7  address+Y  W  write the new value to effective address
    ///
    ///     * The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, STX, STY, SHA, SHX, SHY
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch low byte of address, increment PC
    ///  3     PC      R  fetch high byte of address,
    ///                   add index register to low address byte,
    ///                   increment PC
    ///  4  address+Y* R  read from effective address,
    ///                   fix the high byte of effective address
    ///  5  address+Y  W  write to effective address
    ///
    ///     * The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100. Because
    ///       the processor cannot undo a write to an invalid
    ///       address, it always reads from the address first.
    /// ```
    #[inline(always)]
    pub fn aby(&mut self, dummy_read: bool) -> u16 {
        let base_addr = self.fetch_word(); // Cycles 2 & 3
        let addr = base_addr.wrapping_add(u16::from(self.y));
        if Cpu::pages_differ(base_addr, addr) || dummy_read {
            // Cycle 4 dummy read with fixed high byte
            self.read((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        addr
    }

    /// Indirect Addressing.
    ///
    /// The next 16-bit address is used to get the actual 16-bit address. This instruction has
    /// a bug in the original hardware. If the lo byte is 0xFF, the hi byte would cross a page
    /// boundary. However, this doesn't work correctly on the original hardware and instead
    /// wraps back around to 0.
    ///
    /// # Instructions
    ///
    /// JMP
    ///
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch pointer address low, increment PC
    ///  3     PC      R  fetch pointer address high, increment PC
    ///  4   pointer   R  fetch low address to latch
    ///  5  pointer+1* R  fetch PCH, copy latch to PCL
    ///
    ///     * The PCH will always be fetched from the same page
    ///       than PCL, i.e. page boundary crossing is not handled.
    /// ```
    #[inline(always)]
    pub fn ind(&mut self) -> u16 {
        self.fetch_word()
    }

    /// Indirect X Addressing.
    ///
    /// The next 8-bit address is offset by the X register to get the actual 16-bit address from
    /// page 0x00.
    ///
    /// # Read instructions
    ///
    /// LDA, ORA, EOR, AND, ADC, CMP, SBC, LAX
    ///
    /// ```text
    ///  #    address   R/W description
    /// --- ----------- --- ------------------------------------------
    ///  1      PC       R  fetch opcode, increment PC
    ///  2      PC       R  fetch pointer address, increment PC
    ///  3    pointer    R  read from the address, add X to it
    ///  4   pointer+X*  R  fetch effective address low
    ///  5  pointer+X+1* R  fetch effective address high
    ///  6    address    R  read from effective address
    ///
    ///     * The effective address is always fetched from zero page,
    ///       i.e. the zero page boundary crossing is not handled.
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    ///  #    address   R/W description
    /// --- ----------- --- ------------------------------------------
    ///  1      PC       R  fetch opcode, increment PC
    ///  2      PC       R  fetch pointer address, increment PC
    ///  3    pointer    R  read from the address, add X to it
    ///  4   pointer+X*  R  fetch effective address low
    ///  5  pointer+X+1* R  fetch effective address high
    ///  6    address    R  read from effective address
    ///  7    address    W  write the value back to effective address,
    ///                     and do the operation on it
    ///  8    address    W  write the new value to effective address
    ///
    ///     * The effective address is always fetched from zero page,
    ///       i.e. the zero page boundary crossing is not handled.
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, SAX
    ///
    /// ```text
    ///  #    address   R/W description
    /// --- ----------- --- ------------------------------------------
    ///  1      PC       R  fetch opcode, increment PC
    ///  2      PC       R  fetch pointer address, increment PC
    ///  3    pointer    R  read from the address, add X to it
    ///  4   pointer+X*  R  fetch effective address low
    ///  5  pointer+X+1* R  fetch effective address high
    ///  6    address    W  write to effective address
    ///
    ///     * The effective address is always fetched from zero page,
    ///       i.e. the zero page boundary crossing is not handled.
    /// ```
    #[inline(always)]
    pub fn idx(&mut self) -> u16 {
        let mut zero_addr = self.fetch_byte(); // Cycle 2
        self.read(u16::from(zero_addr)); // Cycle 3 dummy read
        zero_addr = zero_addr.wrapping_add(self.x);
        let lo = self.read(u16::from(zero_addr)); // Cycle 4
        let hi = self.read(u16::from(zero_addr.wrapping_add(1))); // Cycle 5
        u16::from_le_bytes([lo, hi])
    }

    /// Indirect Y Addressing.
    ///
    /// The next 8-bit address is read to get a 16-bit address from page 0x00, which is then offset
    /// by the Y register. If a page boundary is crossed, add a clock cycle.
    ///
    /// # Read instructions
    ///
    /// LDA, EOR, AND, ORA, ADC, SBC, CMP
    ///
    /// ```text
    ///  #    address   R/W description
    /// --- ----------- --- ------------------------------------------
    ///  1      PC       R  fetch opcode, increment PC
    ///  2      PC       R  fetch pointer address, increment PC
    ///  3    pointer    R  fetch effective address low
    ///  4   pointer+1*  R  fetch effective address high,
    ///                     add Y to low byte of effective address
    ///  5   address+Y+  R  read from effective address,
    ///                     fix high byte of effective address
    ///  6!  address+Y   R  read from effective address
    ///
    ///     * The effective address is always fetched from zero page,
    ///       i.e. the zero page boundary crossing is not handled.
    ///     + The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    ///     ! This cycle will be executed only if the effective address
    ///       was invalid during cycle #5, i.e. page boundary was crossed.
    /// ```
    ///
    /// # Read-Modify-Write instructions
    ///
    /// SLO, SRE, RLA, RRA, ISB, DCP
    ///
    /// ```text
    ///  #    address   R/W description
    /// --- ----------- --- ------------------------------------------
    ///  1      PC       R  fetch opcode, increment PC
    ///  2      PC       R  fetch pointer address, increment PC
    ///  3    pointer    R  fetch effective address low
    ///  4   pointer+1*  R  fetch effective address high,
    ///                     add Y to low byte of effective address
    ///  5   address+Y+  R  read from effective address,
    ///                     fix high byte of effective address
    ///  6   address+Y   R  re-read from effective address
    ///  7   address+Y   W  write the value back to effective address,
    ///                     and do the operation on it
    ///  8   address+Y   W  write the new value to effective address
    ///
    ///     * The effective address is always fetched from zero page,
    ///       i.e. the zero page boundary crossing is not handled.
    ///     + The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    /// ```
    ///
    /// # Write instructions
    ///
    /// STA, SHA
    ///
    /// ```text
    ///  #    address   R/W description
    /// --- ----------- --- ------------------------------------------
    ///  1      PC       R  fetch opcode, increment PC
    ///  2      PC       R  fetch pointer address, increment PC
    ///  3    pointer    R  fetch effective address low
    ///  4   pointer+1*  R  fetch effective address high,
    ///                     add Y to low byte of effective address
    ///  5   address+Y+  R  read from effective address,
    ///                     fix high byte of effective address
    ///  6   address+Y   W  write to effective address
    ///
    ///     * The effective address is always fetched from zero page,
    ///       i.e. the zero page boundary crossing is not handled.
    ///     + The high byte of the effective address may be invalid
    ///       at this time, i.e. it may be smaller by $100.
    /// ```
    #[inline(always)]
    pub fn idy(&mut self, dummy_read: bool) -> u16 {
        let zero_addr = self.fetch_byte(); // Cycle 2
        let base_addr = {
            let lo = self.read(u16::from(zero_addr)); // Cycle 3
            let hi = self.read(u16::from(zero_addr.wrapping_add(1))); // Cycle 4
            u16::from_le_bytes([lo, hi])
        };

        let addr = base_addr.wrapping_add(u16::from(self.y));
        if Cpu::pages_differ(base_addr, addr) || dummy_read {
            // Cycle 5 dummy read with fixed high byte
            self.read((base_addr & 0xFF00) | (addr & 0x00FF));
        }
        addr
    }
}

/// CPU instructions
impl Cpu {
    // Storage opcodes

    /// LDA: Load A with M
    #[inline(always)]
    pub fn lda(&mut self) {
        let val = self.read_operand();
        self.set_acc(val);
    }
    /// LDX: Load X with M
    #[inline(always)]
    pub fn ldx(&mut self) {
        let val = self.read_operand();
        self.set_x(val);
    }
    /// LDY: Load Y with M
    #[inline(always)]
    pub fn ldy(&mut self) {
        let val = self.read_operand();
        self.set_y(val);
    }

    /// STA: Store A into M
    #[inline(always)]
    pub fn sta(&mut self) {
        self.write(self.operand, self.acc);
    }
    /// STX: Store X into M
    #[inline(always)]
    pub fn stx(&mut self) {
        self.write(self.operand, self.x);
    }
    /// STY: Store Y into M
    #[inline(always)]
    pub fn sty(&mut self) {
        self.write(self.operand, self.y);
    }

    /// TAX: Transfer A to X
    #[inline(always)]
    pub fn tax(&mut self) {
        self.set_x(self.acc);
    }
    /// TAY: Transfer A to Y
    #[inline(always)]
    pub fn tay(&mut self) {
        self.set_y(self.acc);
    }
    /// TSX: Transfer Stack Pointer to X
    #[inline(always)]
    pub fn tsx(&mut self) {
        self.set_x(self.sp);
    }
    /// TXA: Transfer X to A
    #[inline(always)]
    pub fn txa(&mut self) {
        self.set_acc(self.x);
    }
    /// TXS: Transfer X to Stack Pointer
    #[inline(always)]
    pub const fn txs(&mut self) {
        self.set_sp(self.x);
    }
    /// TYA: Transfer Y to A
    #[inline(always)]
    pub fn tya(&mut self) {
        self.set_acc(self.y);
    }

    // Arithmetic opcodes

    /// ADC: Add M to A with Carry
    #[inline(always)]
    pub fn adc(&mut self) {
        let val = self.read_operand();
        self.add(val);
    }
    /// SBC: Subtract M from A with Carry
    #[inline(always)]
    pub fn sbc(&mut self) {
        let val = self.read_operand();
        self.add(val ^ 0xFF);
    }
    /// Utility function used by all add instructions
    #[inline(always)]
    fn add(&mut self, val: u8) {
        let a = u16::from(self.acc);
        let val = u16::from(val);
        let carry = u16::from(self.status_bits(Status::C));
        let res = a + val + carry;
        self.status
            .set(Status::V, (a ^ val) & 0x80 == 0 && (a ^ res) & 0x80 != 0);
        self.status.set(Status::C, res > 0xFF);
        self.set_acc(res as u8);
    }

    /// INC: Increment M by One
    #[inline(always)]
    pub fn inc(&mut self) {
        let addr = self.operand;
        let val = self.read(addr);
        self.write(addr, val); // Dummy write
        let res = val.wrapping_add(1);
        self.write(addr, res);
        self.set_zn_status(res);
    }
    /// DEC: Decrement M by One
    #[inline(always)]
    pub fn dec(&mut self) {
        let addr = self.operand;
        let val = self.read(addr);
        self.write(addr, val); // Dummy write
        let res = val.wrapping_sub(1);
        self.write(addr, res);
        self.set_zn_status(res);
    }

    /// INX: Increment X by One
    #[inline(always)]
    pub fn inx(&mut self) {
        self.set_x(self.x.wrapping_add(1));
    }
    /// INY: Increment Y by One
    #[inline(always)]
    pub fn iny(&mut self) {
        self.set_y(self.y.wrapping_add(1));
    }

    /// DEX: Decrement X by One
    #[inline(always)]
    pub fn dex(&mut self) {
        self.set_x(self.x.wrapping_sub(1));
    }

    /// DEY: Decrement Y by One
    #[inline(always)]
    pub fn dey(&mut self) {
        self.set_y(self.y.wrapping_sub(1));
    }

    // Bitwise opcodes

    /// AND: "And" M with A
    #[inline(always)]
    pub fn and(&mut self) {
        let val = self.read_operand();
        self.set_acc(self.acc & val);
    }
    /// EOR: "Exclusive-Or" M with A
    #[inline(always)]
    pub fn eor(&mut self) {
        let val = self.read_operand();
        self.set_acc(self.acc ^ val);
    }
    /// ORA: "OR" M with A
    #[inline(always)]
    pub fn ora(&mut self) {
        let val = self.read_operand();
        self.set_acc(self.acc | val);
    }

    /// ASL: Shift Left One Bit (A)
    #[inline(always)]
    fn asla(&mut self) {
        let val = self.asl(self.acc);
        self.set_acc(val);
    }
    /// ASL: Shift Left One Bit (M)
    #[inline(always)]
    fn aslm(&mut self) {
        let addr = self.operand;
        let val = self.read(addr);
        self.write(addr, val); // Dummy write
        let res = self.asl(val);
        self.write(addr, res);
    }
    /// Utility function used by all ASL instructions
    #[inline(always)]
    fn asl(&mut self, val: u8) -> u8 {
        self.status.set(Status::C, (val & 0x80) > 0);
        let res = val.wrapping_shl(1);
        self.set_zn_status(res);
        res
    }

    /// LSR: Shift Right One Bit (A)
    #[inline(always)]
    pub fn lsra(&mut self) {
        let res = self.lsr(self.acc);
        self.set_acc(res);
    }
    /// LSR: Shift Right One Bit (M)
    #[inline(always)]
    pub fn lsrm(&mut self) {
        let addr = self.operand;
        let val = self.read(addr);
        self.write(addr, val); // Dummy write
        let res = self.lsr(val);
        self.write(addr, res);
    }
    /// Utility function used by all LSR instructions
    #[inline(always)]
    fn lsr(&mut self, val: u8) -> u8 {
        self.status.set(Status::C, (val & 1) > 0);
        let res = val.wrapping_shr(1);
        self.set_zn_status(res);
        res
    }

    /// ROL: Rotate One Bit Left (A)
    #[inline(always)]
    pub fn rola(&mut self) {
        let val = self.rol(self.acc);
        self.set_acc(val);
    }
    /// ROL: Rotate One Bit Left (M)
    #[inline(always)]
    pub fn rolm(&mut self) {
        let addr = self.operand;
        let val = self.read(addr);
        self.write(addr, val); // Dummy write
        let val = self.rol(val);
        self.write(addr, val);
    }
    /// Utility function used by all ROL instructions
    #[inline(always)]
    pub fn rol(&mut self, val: u8) -> u8 {
        let carry = self.status_bits(Status::C);
        self.status.set(Status::C, (val & 0x80) > 0);
        let res = (val << 1) | carry;
        self.set_zn_status(res);
        res
    }

    /// ROR: Rotate One Bit Right (A)
    #[inline(always)]
    pub fn rora(&mut self) {
        let val = self.ror(self.acc);
        self.set_acc(val);
    }
    /// ROR: Rotate One Bit Right (M)
    #[inline(always)]
    pub fn rorm(&mut self) {
        let addr = self.operand;
        let val = self.read(addr);
        self.write(addr, val); // Dummy write
        let val = self.ror(val);
        self.write(addr, val);
    }
    /// Utility function used by all ROR instructions
    #[inline(always)]
    fn ror(&mut self, val: u8) -> u8 {
        let carry = self.status_bits(Status::C);
        self.status.set(Status::C, (val & 1) > 0);
        let res = (val >> 1) | (carry << 7);
        self.set_zn_status(res);
        res
    }

    /// BIT: Test Bits in M with A
    #[inline(always)]
    pub fn bit(&mut self) {
        let val = self.read_operand();
        self.status.set(Status::Z, (self.acc & val) == 0);
        self.status.set(Status::N, (val & 0x80) > 0);
        self.status.set(Status::V, (val & 0x40) > 0);
    }

    // Branch opcodes

    /// BCC: Branch on Carry Clear
    #[inline(always)]
    pub fn bcc(&mut self) {
        self.branch(!self.status.contains(Status::C));
    }
    /// BCS: Branch on Carry Set
    #[inline(always)]
    pub fn bcs(&mut self) {
        self.branch(self.status.contains(Status::C));
    }
    /// BEQ: Branch on Result Zero
    #[inline(always)]
    pub fn beq(&mut self) {
        self.branch(self.status.contains(Status::Z));
    }
    /// BMI: Branch on Result Negative
    #[inline(always)]
    pub fn bmi(&mut self) {
        self.branch(self.status.contains(Status::N));
    }
    /// BNE: Branch on Result Not Zero
    #[inline(always)]
    pub fn bne(&mut self) {
        self.branch(!self.status.contains(Status::Z));
    }
    /// BPL: Branch on Result Positive
    #[inline(always)]
    pub fn bpl(&mut self) {
        self.branch(!self.status.contains(Status::N));
    }
    /// BVC: Branch on Overflow Clear
    #[inline(always)]
    pub fn bvc(&mut self) {
        self.branch(!self.status.contains(Status::V));
    }
    /// BVS: Branch on Overflow Set
    #[inline(always)]
    pub fn bvs(&mut self) {
        self.branch(self.status.contains(Status::V));
    }
    /// Utility function used by all branch instructions.
    #[inline(always)]
    fn branch(&mut self, branch: bool) {
        if !branch {
            return;
        }
        // If an interrupt occurs during the final cycle of a non-pagecrossing branch
        // then it will be ignored until the next instruction completes
        let run_irq = self.irq_flags.contains(IrqFlags::RUN_IRQ);
        let prev_run_irq = self.irq_flags.contains(IrqFlags::PREV_RUN_IRQ);
        if run_irq && !prev_run_irq {
            self.irq_flags.remove(IrqFlags::RUN_IRQ);
        }
        self.read(self.pc); // Dummy read

        let offset = i16::from(self.operand as i8);
        if Self::page_crossed(self.pc, offset) {
            self.read(self.pc); // Dummy read
        }
        self.pc = (self.pc as i16).wrapping_add(offset) as u16;
    }

    // Jump opcodes

    /// JMP: Jump to Location (absolute)
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -------------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  fetch low address byte, increment PC
    ///  3    PC     R  copy low address byte to PCL, copy high address
    ///                   byte to PCH
    /// ```
    #[inline(always)]
    pub const fn jmpa(&mut self) {
        self.pc = self.operand;
    }
    /// JMP: Jump to Location (indirect)
    /// ```text
    ///  #   address  R/W description
    /// --- --------- --- ------------------------------------------
    ///  1     PC      R  fetch opcode, increment PC
    ///  2     PC      R  fetch pointer address low, increment PC
    ///  3     PC      R  fetch pointer address high, increment PC
    ///  4   pointer   R  fetch low address to latch
    ///  5  pointer+1* R  fetch PCH, copy latch to PCL
    ///
    ///     * The PCH will always be fetched from the same page
    ///       than PCL, i.e. page boundary crossing is not handled.
    /// ```
    #[inline(always)]
    pub fn jmpi(&mut self) {
        let addr = self.operand;
        self.pc = if (addr & 0xFF) == 0xFF {
            let lo = self.read(addr);
            let hi = self.read(addr - 0xFF);
            u16::from_le_bytes([lo, hi])
        } else {
            self.read_word(addr)
        };
    }
    /// JSR: Jump to Location Save Return addr
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -------------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  fetch low address byte, increment PC
    ///  3  $0100,S  R  internal operation (predecrement S?)
    ///  4  $0100,S  W  push PCH on stack, decrement S
    ///  5  $0100,S  W  push PCL on stack, decrement S
    ///  6    PC     R  copy low address byte to PCL, copy high address
    ///                 byte to PCH
    /// ```
    #[inline(always)]
    pub fn jsr(&mut self) {
        let lo = self.fetch_byte();
        self.read(self.pc); // Dummy read
        self.push_word(self.pc);
        let hi = self.fetch_byte();
        let addr = u16::from_le_bytes([lo, hi]);
        self.pc = addr;
    }

    /// RTI: Return from Interrupt
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away)
    ///  3  $0100,S  R  increment S
    ///  4  $0100,S  R  pull P from stack, increment S
    ///  5  $0100,S  R  pull PCL from stack, increment S
    ///  6  $0100,S  R  pull PCH from stack
    /// ```
    #[inline(always)]
    pub fn rti(&mut self) {
        self.read(self.pc); // Dummy read
        let status = Status::from_bits_truncate(self.pop_byte());
        self.set_status(status);
        self.pc = self.pop_word();
    }

    /// RTS: Return from Subroutine
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away)
    ///  3  $0100,S  R  increment S
    ///  4  $0100,S  R  pull PCL from stack, increment S
    ///  5  $0100,S  R  pull PCH from stack
    ///  6    PC     R  increment PC
    /// ```
    #[inline(always)]
    pub fn rts(&mut self) {
        self.read(self.pc); // Dummy read
        let addr = self.pop_word();
        self.read(self.pc); // Dummy read
        self.pc = addr.wrapping_add(1);
    }

    //  Register opcodes

    /// CLC: Clear Carry Flag
    #[inline(always)]
    pub fn clc(&mut self) {
        self.status.set(Status::C, false);
    }
    /// SEC: Set Carry Flag
    #[inline(always)]
    pub fn sec(&mut self) {
        self.status.set(Status::C, true);
    }
    /// CLD: Clear Decimal Mode
    #[inline(always)]
    pub fn cld(&mut self) {
        self.status.set(Status::D, false);
    }
    /// SED: Set Decimal Mode
    #[inline(always)]
    pub fn sed(&mut self) {
        self.status.set(Status::D, true);
    }
    /// CLI: Clear Interrupt Disable Bit
    #[inline(always)]
    pub fn cli(&mut self) {
        self.status.set(Status::I, false);
    }
    /// SEI: Set Interrupt Disable Status
    #[inline(always)]
    pub fn sei(&mut self) {
        self.status.set(Status::I, true);
    }
    /// CLV: Clear Overflow Flag
    #[inline(always)]
    pub fn clv(&mut self) {
        self.status.set(Status::V, false);
    }

    // Compare opcodes

    /// CMP: Compare M and A
    #[inline(always)]
    pub fn cpa(&mut self) {
        let val = self.read_operand();
        self.cmp(self.acc, val);
    }
    /// CPX: Compare M and X
    #[inline(always)]
    pub fn cpx(&mut self) {
        let val = self.read_operand();
        self.cmp(self.x, val);
    }
    /// CPY: Compare M and Y
    #[inline(always)]
    pub fn cpy(&mut self) {
        let val = self.read_operand();
        self.cmp(self.y, val);
    }
    /// Utility function used by all compare instructions
    #[inline(always)]
    fn cmp(&mut self, reg: u8, val: u8) {
        let result = reg.wrapping_sub(val);
        self.status.set(Status::C, reg >= val);
        self.set_zn_status(result);
    }

    // Stack opcodes

    /// PHP: Push Processor Status on Stack
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away)
    ///  3  $0100,S  W  push register on stack, decrement S
    /// ```
    #[inline(always)]
    pub fn php(&mut self) {
        // Set U and B when pushing during PHP and BRK
        self.push_byte((self.status | Status::U | Status::B).bits());
    }

    /// PLP: Pull Processor Status from Stack
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away)
    ///  3  $0100,S  R  increment S
    ///  4  $0100,S  R  pull register from stack
    ///  ```
    #[inline(always)]
    pub fn plp(&mut self) {
        self.read(self.pc); // Dummy read
        let status = Status::from_bits_truncate(self.pop_byte());
        self.set_status(status);
    }

    /// PHA: Push A on Stack
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away)
    ///  3  $0100,S  W  push register on stack, decrement S
    /// ```
    #[inline(always)]
    pub fn pha(&mut self) {
        self.push_byte(self.acc); // Cycle 3
    }

    /// PLA: Pull A from Stack
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away)
    ///  3  $0100,S  R  increment S
    ///  4  $0100,S  R  pull register from stack
    /// ```
    #[inline(always)]
    pub fn pla(&mut self) {
        self.read(Self::SP_BASE | u16::from(self.sp)); // Dummy read
        self.acc = self.pop_byte(); // Cycle 4
        self.set_zn_status(self.acc);
    }

    // System opcodes

    /// BRK: Force Break Interrupt
    ///
    /// ```text
    ///  #  address R/W description
    /// --- ------- --- -----------------------------------------------
    ///  1    PC     R  fetch opcode, increment PC
    ///  2    PC     R  read next instruction byte (and throw it away),
    ///                 increment PC
    ///  3  $0100,S  W  push PCH on stack (with B flag set), decrement S
    ///  4  $0100,S  W  push PCL on stack, decrement S
    ///  5  $0100,S  W  push P on stack, decrement S
    ///  6   $FFFE   R  fetch PCL
    ///  7   $FFFF   R  fetch PCH
    /// ```
    #[inline(always)]
    pub fn brk(&mut self) {
        self.push_word(self.pc);

        // Pushing status to the stack has to happen after checking NMI since it can hijack the BRK
        // IRQ when it occurs between cycles 4 and 5.
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // Set U and B when pushing during PHP and BRK
        let status = (self.status | Status::U | Status::B).bits();
        let nmi = self.irq_flags.contains(IrqFlags::NMI);
        self.push_byte(status); // Cycle 5
        self.status.set(Status::I, true);

        if nmi {
            self.irq_flags.remove(IrqFlags::NMI);
            self.pc = self.read_word(Self::NMI_VECTOR); // Cycles 6-7
            tracing::trace!(
                "NMI - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle,
                self.bus.ppu.scanline,
                self.cycle
            );
        } else {
            self.pc = self.read_word(Self::IRQ_VECTOR); // Cycles 6-7
            tracing::trace!(
                "IRQ - PPU:{:3},{:3} CYC:{}",
                self.bus.ppu.cycle,
                self.bus.ppu.scanline,
                self.cycle
            );
        }

        // Prevent NMI from triggering immediately after BRK
        tracing::trace!(
            "Suppress NMI after BRK - PPU:{:3},{:3} CYC:{}, prev_nmi:{}",
            self.bus.ppu.cycle,
            self.bus.ppu.scanline,
            self.cycle,
            self.irq_flags.contains(IrqFlags::PREV_NMI)
        );
        self.irq_flags.remove(IrqFlags::PREV_NMI);
    }

    /// NOP: No Operation
    #[inline(always)]
    pub fn nop(&mut self) {
        let _ = self.read_operand();
    }

    // Unofficial opcodes

    /// HLT: Captures all unimplemented opcodes and halts CPU
    #[inline(always)]
    pub fn hlt(&mut self) {
        // Freezes CPU by rewiding and re-executing the bad opcode.
        self.pc = self.pc.wrapping_sub(1);
        // Prevent IRQ/NMI
        self.irq_flags
            .remove(IrqFlags::PREV_RUN_IRQ | IrqFlags::PREV_NMI);

        self.corrupted = true;
        let opcode = usize::from(self.peek(self.pc.wrapping_sub(1)));
        let instr = Cpu::INSTR_REF[opcode];
        tracing::error!(
            "Invalid opcode ${opcode:02X} {:?} #{:?} encountered!",
            instr.instr,
            instr.addr_mode,
        );
    }

    /// ISC/ISB: Shortcut for INC then SBC
    #[inline(always)]
    pub fn isb(&mut self) {
        let val = self.read_operand();
        let addr = self.operand;
        // INC
        self.write(addr, val); // Dummy write
        let val = val.wrapping_add(1);
        // SBC
        self.add(val ^ 0xFF);
        self.write(addr, val);
    }

    /// DCP: Shortcut for DEC then CMP
    #[inline(always)]
    pub fn dcp(&mut self) {
        let val = self.read_operand();
        let addr = self.operand;
        // DEC
        self.write(addr, val); // Dummy write
        let val = val.wrapping_sub(1);
        // CMP
        self.cmp(self.acc, val);
        self.write(addr, val);
    }

    /// ATX: Shortcut for LDA & TAX
    #[inline(always)]
    pub fn atx(&mut self) {
        let val = self.read_operand();
        self.set_acc(val); // LDA
        self.set_x(self.acc); // TAX
    }

    /// AXS: A & X into X
    #[inline(always)]
    pub fn axs(&mut self) {
        let val = self.read_operand();
        // CMP & DEX
        let res = (self.acc & self.x).wrapping_sub(val);
        self.status.set(Status::C, (self.acc & self.x) >= val);
        self.set_x(res);
    }

    /// LAS: Shortcut for LDA then TSX, but ANDs memory stack pointer
    #[inline(always)]
    pub fn las(&mut self) {
        let val = self.read_operand();
        self.set_acc(val & self.sp);
        self.set_x(self.acc);
        self.set_sp(self.acc);
    }

    /// LAX: Shortcut for LDA then TAX
    #[inline(always)]
    pub fn lax(&mut self) {
        let val = self.read_operand();
        self.set_x(val);
        self.set_acc(val);
    }

    /// SYA/A11/SHY/SAY/TEY: Combinations of STA/STX/STY
    /// AND Y register with the high byte of the target address of the argument + 1. Store the
    /// result in memory.
    #[inline(always)]
    pub fn sya(&mut self) {
        let base_addr = self.fetch_word();
        self.sya_sxa_axa(base_addr, self.x, self.y);
    }

    /// SXA/SHX/XAS: AND X with the high byte of the target address + 1
    #[inline(always)]
    pub fn sxa(&mut self) {
        let base_addr = self.fetch_word();
        self.sya_sxa_axa(base_addr, self.y, self.x);
    }

    /// SHA/AXA: AND X with A then AND with 7, then store in memory
    #[inline(always)]
    pub fn shaa(&mut self) {
        let base_addr = self.fetch_word();
        self.sya_sxa_axa(base_addr, self.y, self.x & self.acc);
    }

    /// AHX: And X with A stores A&X&H into {adr}
    #[inline(always)]
    pub fn shaz(&mut self) {
        let zero_addr = self.fetch_byte();
        let base_addr = {
            let lo = self.read(u16::from(zero_addr));
            let hi = self.read(u16::from(zero_addr.wrapping_add(1)));
            u16::from_le_bytes([lo, hi])
        };
        self.sya_sxa_axa(base_addr, self.y, self.x & self.acc);
    }

    fn sya_sxa_axa(&mut self, base_addr: u16, index_reg: u8, val_reg: u8) {
        let addr = base_addr.wrapping_add(u16::from(index_reg));
        let page_crossed = Cpu::pages_differ(base_addr, addr);

        let start_cycles = self.cycle;
        // Dummy read with fixed high byte
        self.read((base_addr & 0xFF00) | (addr & 0x00FF));

        // Dummy read took more than 1 cycle, so it was interrupted by a DMA
        let had_dma = (self.cycle - start_cycles) > 1;

        let mut hi = (addr >> 8) as u8;
        let lo = (addr & 0xFF) as u8;
        if page_crossed {
            hi &= val_reg;
        }

        let val = if had_dma {
            val_reg
        } else {
            val_reg & ((base_addr >> 8) + 1) as u8
        };
        self.write(u16::from_le_bytes([lo, hi]), val);
    }

    /// SAX: AND A with X
    #[inline(always)]
    pub fn sax(&mut self) {
        self.write(self.operand, self.acc & self.x);
    }

    /// XXA: Shortcutr for TXA with AND
    #[inline(always)]
    pub fn xaa(&mut self) {
        let val = self.read_operand();
        self.set_acc((self.acc | 0xEE) & self.x & val);
    }

    /// RRA: Shortcut for ROR then ADC
    #[inline(always)]
    pub fn rra(&mut self) {
        let val = self.read_operand();
        let addr = self.operand;
        // ROR
        self.write(addr, val); // Dummy write
        let shifted_val = self.ror(val);
        // ADC
        self.add(shifted_val);
        self.write(addr, shifted_val);
    }

    /// TAS: Shortcut for STA then TXS, Same as SHA but sets SP = A & X
    #[inline(always)]
    pub fn tas(&mut self) {
        self.shaa();
        // TXS
        self.set_sp(self.x & self.acc);
    }

    /// ARR: Shortcut for AND #imm then ROR, but sets flags differently
    /// C is bit 6 and V is bit 6 xor bit 5
    #[inline(always)]
    pub fn arr(&mut self) {
        let val = self.read_operand();
        let carry = self.status_bits(Status::C);
        self.set_acc(((self.acc & val) >> 1) | (carry << 7));
        self.status.set(Status::C, (self.acc & 0x40) > 0);
        self.status.set(
            Status::V,
            (self.status_bits(Status::C) ^ (self.acc >> 5) & 0x01) > 0,
        );
    }

    /// SRA: Shortcut for LSR then EOR
    #[inline(always)]
    pub fn sre(&mut self) {
        let val = self.read_operand();
        let addr = self.operand;
        // LSR
        self.write(addr, val); // Dummy write
        let shifted_val = self.lsr(val);
        // EOR
        self.set_acc(self.acc ^ shifted_val);
        self.write(addr, shifted_val);
    }

    /// ALR/ASR: Shortcut for AND #imm then LSR
    #[inline(always)]
    pub fn alr(&mut self) {
        let val = self.read_operand();
        self.set_acc(self.acc & val);
        self.status.set(Status::C, (self.acc & 0x01) > 0);
        self.set_acc(self.acc >> 1);
    }

    /// RLA: Shortcut for ROL then AND
    #[inline(always)]
    pub fn rla(&mut self) {
        let val = self.read_operand();
        let addr = self.operand;
        // ROL
        self.write(addr, val); // Dummy write
        let shifted_val = self.rol(val);
        // AND
        self.set_acc(self.acc & shifted_val);
        self.write(addr, shifted_val);
    }

    /// ANC/AAC: AND #imm but puts bit 7 into carry as if ASL was executed
    #[inline(always)]
    pub fn anc(&mut self) {
        let val = self.read_operand();
        self.set_acc(self.acc & val);
        self.status.set(Status::C, self.status.contains(Status::N));
    }

    /// SLO: Shortcut for ASL then ORA
    #[inline(always)]
    pub fn slo(&mut self) {
        let val = self.read_operand();
        let addr = self.operand;
        // ASL
        self.write(addr, val); // Dummy write
        let shifted_val = self.asl(val);
        // ORA
        self.set_acc(self.acc | shifted_val);
        self.write(addr, shifted_val);
    }
}
