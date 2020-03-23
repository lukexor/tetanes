use super::{Cpu, StatusRegs::*, IRQ_ADDR, NMI_ADDR, SP_BASE};
use crate::{
    error,
    logging::{LogLevel, Loggable},
    memory::{MemRead, MemWrite},
    serialization::Savable,
    NesResult,
};
use std::{
    fmt,
    io::{Read, Write},
};

// 16x16 grid of 6502 opcodes. Matches datasheet matrix for easy lookup
#[rustfmt::skip]
pub const INSTRUCTIONS: [Instr; 256] = [
    Instr(0x00, IMM, BRK, 7), Instr(0x01, IDX, ORA, 6), Instr(0x02, IMP, XXX, 2), Instr(0x03, IDX, SLO, 8), Instr(0x04, ZP0, NOP, 3), Instr(0x05, ZP0, ORA, 3), Instr(0x06, ZP0, ASL, 5), Instr(0x07, ZP0, SLO, 5), Instr(0x08, IMP, PHP, 3), Instr(0x09, IMM, ORA, 2), Instr(0x0A, ACC, ASL, 2), Instr(0x0B, IMM, ANC, 2), Instr(0x0C, ABS, NOP, 4), Instr(0x0D, ABS, ORA, 4), Instr(0x0E, ABS, ASL, 6), Instr(0x0F, ABS, SLO, 6),
    Instr(0x10, REL, BPL, 2), Instr(0x11, IDY, ORA, 5), Instr(0x12, IMP, XXX, 2), Instr(0x13, IDY, SLO, 8), Instr(0x14, ZPX, NOP, 4), Instr(0x15, ZPX, ORA, 4), Instr(0x16, ZPX, ASL, 6), Instr(0x17, ZPX, SLO, 6), Instr(0x18, IMP, CLC, 2), Instr(0x19, ABY, ORA, 4), Instr(0x1A, IMP, NOP, 2), Instr(0x1B, ABY, SLO, 7), Instr(0x1C, ABX, IGN, 4), Instr(0x1D, ABX, ORA, 4), Instr(0x1E, ABX, ASL, 7), Instr(0x1F, ABX, SLO, 7),
    Instr(0x20, ABS, JSR, 6), Instr(0x21, IDX, AND, 6), Instr(0x22, IMP, XXX, 2), Instr(0x23, IDX, RLA, 8), Instr(0x24, ZP0, BIT, 3), Instr(0x25, ZP0, AND, 3), Instr(0x26, ZP0, ROL, 5), Instr(0x27, ZP0, RLA, 5), Instr(0x28, IMP, PLP, 4), Instr(0x29, IMM, AND, 2), Instr(0x2A, ACC, ROL, 2), Instr(0x2B, IMM, ANC, 2), Instr(0x2C, ABS, BIT, 4), Instr(0x2D, ABS, AND, 4), Instr(0x2E, ABS, ROL, 6), Instr(0x2F, ABS, RLA, 6),
    Instr(0x30, REL, BMI, 2), Instr(0x31, IDY, AND, 5), Instr(0x32, IMP, XXX, 2), Instr(0x33, IDY, RLA, 8), Instr(0x34, ZPX, NOP, 4), Instr(0x35, ZPX, AND, 4), Instr(0x36, ZPX, ROL, 6), Instr(0x37, ZPX, RLA, 6), Instr(0x38, IMP, SEC, 2), Instr(0x39, ABY, AND, 4), Instr(0x3A, IMP, NOP, 2), Instr(0x3B, ABY, RLA, 7), Instr(0x3C, ABX, IGN, 4), Instr(0x3D, ABX, AND, 4), Instr(0x3E, ABX, ROL, 7), Instr(0x3F, ABX, RLA, 7),
    Instr(0x40, IMP, RTI, 6), Instr(0x41, IDX, EOR, 6), Instr(0x42, IMP, XXX, 2), Instr(0x43, IDX, SRE, 8), Instr(0x44, ZP0, NOP, 3), Instr(0x45, ZP0, EOR, 3), Instr(0x46, ZP0, LSR, 5), Instr(0x47, ZP0, SRE, 5), Instr(0x48, IMP, PHA, 3), Instr(0x49, IMM, EOR, 2), Instr(0x4A, ACC, LSR, 2), Instr(0x4B, IMM, ALR, 2), Instr(0x4C, ABS, JMP, 3), Instr(0x4D, ABS, EOR, 4), Instr(0x4E, ABS, LSR, 6), Instr(0x4F, ABS, SRE, 6),
    Instr(0x50, REL, BVC, 2), Instr(0x51, IDY, EOR, 5), Instr(0x52, IMP, XXX, 2), Instr(0x53, IDY, SRE, 8), Instr(0x54, ZPX, NOP, 4), Instr(0x55, ZPX, EOR, 4), Instr(0x56, ZPX, LSR, 6), Instr(0x57, ZPX, SRE, 6), Instr(0x58, IMP, CLI, 2), Instr(0x59, ABY, EOR, 4), Instr(0x5A, IMP, NOP, 2), Instr(0x5B, ABY, SRE, 7), Instr(0x5C, ABX, IGN, 4), Instr(0x5D, ABX, EOR, 4), Instr(0x5E, ABX, LSR, 7), Instr(0x5F, ABX, SRE, 7),
    Instr(0x60, IMP, RTS, 6), Instr(0x61, IDX, ADC, 6), Instr(0x62, IMP, XXX, 2), Instr(0x63, IDX, RRA, 8), Instr(0x64, ZP0, NOP, 3), Instr(0x65, ZP0, ADC, 3), Instr(0x66, ZP0, ROR, 5), Instr(0x67, ZP0, RRA, 5), Instr(0x68, IMP, PLA, 4), Instr(0x69, IMM, ADC, 2), Instr(0x6A, ACC, ROR, 2), Instr(0x6B, IMM, ARR, 2), Instr(0x6C, IND, JMP, 5), Instr(0x6D, ABS, ADC, 4), Instr(0x6E, ABS, ROR, 6), Instr(0x6F, ABS, RRA, 6),
    Instr(0x70, REL, BVS, 2), Instr(0x71, IDY, ADC, 5), Instr(0x72, IMP, XXX, 2), Instr(0x73, IDY, RRA, 8), Instr(0x74, ZPX, NOP, 4), Instr(0x75, ZPX, ADC, 4), Instr(0x76, ZPX, ROR, 6), Instr(0x77, ZPX, RRA, 6), Instr(0x78, IMP, SEI, 2), Instr(0x79, ABY, ADC, 4), Instr(0x7A, IMP, NOP, 2), Instr(0x7B, ABY, RRA, 7), Instr(0x7C, ABX, IGN, 4), Instr(0x7D, ABX, ADC, 4), Instr(0x7E, ABX, ROR, 7), Instr(0x7F, ABX, RRA, 7),
    Instr(0x80, IMM, SKB, 2), Instr(0x81, IDX, STA, 6), Instr(0x82, IMM, SKB, 2), Instr(0x83, IDX, SAX, 6), Instr(0x84, ZP0, STY, 3), Instr(0x85, ZP0, STA, 3), Instr(0x86, ZP0, STX, 3), Instr(0x87, ZP0, SAX, 3), Instr(0x88, IMP, DEY, 2), Instr(0x89, IMM, SKB, 2), Instr(0x8A, IMP, TXA, 2), Instr(0x8B, IMM, XAA, 2), Instr(0x8C, ABS, STY, 4), Instr(0x8D, ABS, STA, 4), Instr(0x8E, ABS, STX, 4), Instr(0x8F, ABS, SAX, 4),
    Instr(0x90, REL, BCC, 2), Instr(0x91, IDY, STA, 6), Instr(0x92, IMP, XXX, 2), Instr(0x93, IDY, AHX, 6), Instr(0x94, ZPX, STY, 4), Instr(0x95, ZPX, STA, 4), Instr(0x96, ZPY, STX, 4), Instr(0x97, ZPY, SAX, 4), Instr(0x98, IMP, TYA, 2), Instr(0x99, ABY, STA, 5), Instr(0x9A, IMP, TXS, 2), Instr(0x9B, ABY, TAS, 5), Instr(0x9C, ABX, SYA, 5), Instr(0x9D, ABX, STA, 5), Instr(0x9E, ABY, SXA, 5), Instr(0x9F, ABY, AHX, 5),
    Instr(0xA0, IMM, LDY, 2), Instr(0xA1, IDX, LDA, 6), Instr(0xA2, IMM, LDX, 2), Instr(0xA3, IDX, LAX, 6), Instr(0xA4, ZP0, LDY, 3), Instr(0xA5, ZP0, LDA, 3), Instr(0xA6, ZP0, LDX, 3), Instr(0xA7, ZP0, LAX, 3), Instr(0xA8, IMP, TAY, 2), Instr(0xA9, IMM, LDA, 2), Instr(0xAA, IMP, TAX, 2), Instr(0xAB, IMM, LAX, 2), Instr(0xAC, ABS, LDY, 4), Instr(0xAD, ABS, LDA, 4), Instr(0xAE, ABS, LDX, 4), Instr(0xAF, ABS, LAX, 4),
    Instr(0xB0, REL, BCS, 2), Instr(0xB1, IDY, LDA, 5), Instr(0xB2, IMP, XXX, 2), Instr(0xB3, IDY, LAX, 5), Instr(0xB4, ZPX, LDY, 4), Instr(0xB5, ZPX, LDA, 4), Instr(0xB6, ZPY, LDX, 4), Instr(0xB7, ZPY, LAX, 4), Instr(0xB8, IMP, CLV, 2), Instr(0xB9, ABY, LDA, 4), Instr(0xBA, IMP, TSX, 2), Instr(0xBB, ABY, LAS, 4), Instr(0xBC, ABX, LDY, 4), Instr(0xBD, ABX, LDA, 4), Instr(0xBE, ABY, LDX, 4), Instr(0xBF, ABY, LAX, 4),
    Instr(0xC0, IMM, CPY, 2), Instr(0xC1, IDX, CMP, 6), Instr(0xC2, IMM, SKB, 2), Instr(0xC3, IDX, DCP, 8), Instr(0xC4, ZP0, CPY, 3), Instr(0xC5, ZP0, CMP, 3), Instr(0xC6, ZP0, DEC, 5), Instr(0xC7, ZP0, DCP, 5), Instr(0xC8, IMP, INY, 2), Instr(0xC9, IMM, CMP, 2), Instr(0xCA, IMP, DEX, 2), Instr(0xCB, IMM, AXS, 2), Instr(0xCC, ABS, CPY, 4), Instr(0xCD, ABS, CMP, 4), Instr(0xCE, ABS, DEC, 6), Instr(0xCF, ABS, DCP, 6),
    Instr(0xD0, REL, BNE, 2), Instr(0xD1, IDY, CMP, 5), Instr(0xD2, IMP, XXX, 2), Instr(0xD3, IDY, DCP, 8), Instr(0xD4, ZPX, NOP, 4), Instr(0xD5, ZPX, CMP, 4), Instr(0xD6, ZPX, DEC, 6), Instr(0xD7, ZPX, DCP, 6), Instr(0xD8, IMP, CLD, 2), Instr(0xD9, ABY, CMP, 4), Instr(0xDA, IMP, NOP, 2), Instr(0xDB, ABY, DCP, 7), Instr(0xDC, ABX, IGN, 4), Instr(0xDD, ABX, CMP, 4), Instr(0xDE, ABX, DEC, 7), Instr(0xDF, ABX, DCP, 7),
    Instr(0xE0, IMM, CPX, 2), Instr(0xE1, IDX, SBC, 6), Instr(0xE2, IMM, SKB, 2), Instr(0xE3, IDX, ISB, 8), Instr(0xE4, ZP0, CPX, 3), Instr(0xE5, ZP0, SBC, 3), Instr(0xE6, ZP0, INC, 5), Instr(0xE7, ZP0, ISB, 5), Instr(0xE8, IMP, INX, 2), Instr(0xE9, IMM, SBC, 2), Instr(0xEA, IMP, NOP, 2), Instr(0xEB, IMM, SBC, 2), Instr(0xEC, ABS, CPX, 4), Instr(0xED, ABS, SBC, 4), Instr(0xEE, ABS, INC, 6), Instr(0xEF, ABS, ISB, 6),
    Instr(0xF0, REL, BEQ, 2), Instr(0xF1, IDY, SBC, 5), Instr(0xF2, IMP, XXX, 2), Instr(0xF3, IDY, ISB, 8), Instr(0xF4, ZPX, NOP, 4), Instr(0xF5, ZPX, SBC, 4), Instr(0xF6, ZPX, INC, 6), Instr(0xF7, ZPX, ISB, 6), Instr(0xF8, IMP, SED, 2), Instr(0xF9, ABY, SBC, 4), Instr(0xFA, IMP, NOP, 2), Instr(0xFB, ABY, ISB, 7), Instr(0xFC, ABX, IGN, 4), Instr(0xFD, ABX, SBC, 4), Instr(0xFE, ABX, INC, 7), Instr(0xFF, ABX, ISB, 7),
];

#[rustfmt::skip]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
// List of all CPU official and unofficial operations
// http://wiki.nesdev.com/w/index.php/6502_instructions
// http://archive.6502.org/datasheets/rockwell_r650x_r651x.pdf
pub enum Operation {
    ADC, AND, ASL, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP, CPX,
    CPY, DEC, DEX, DEY, EOR, INC, INX, INY, JMP, JSR, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA,
    PLP, ROL, ROR, RTI, RTS, SBC, SEC, SED, SEI, STA, STX, STY, TAX, TAY, TSX, TXA, TXS, TYA,
    // "Unofficial" opcodes
    SKB, IGN, ISB, DCP, AXS, LAS, LAX, AHX, SAX, XAA, SXA, RRA, TAS, SYA, ARR, SRE, ALR, RLA, ANC,
    SLO, XXX
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[rustfmt::skip]
pub enum AddrMode {
    IMM,
    ZP0, ZPX, ZPY,
    ABS, ABX, ABY,
    IND, IDX, IDY,
    REL, ACC, IMP,
}

use AddrMode::*;
use Operation::*;

// (opcode, Addressing Mode, Operation, cycles taken)
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Instr(u8, AddrMode, Operation, usize);

impl Instr {
    pub fn opcode(&self) -> u8 {
        self.0
    }
    pub fn addr_mode(&self) -> AddrMode {
        self.1
    }
    pub fn op(&self) -> Operation {
        self.2
    }
    pub fn cycles(&self) -> usize {
        self.3
    }
}

/// CPU instructions
impl Cpu {
    /// Storage opcodes

    /// LDA: Load A with M
    pub(super) fn lda(&mut self) {
        self.fetch_data();
        self.acc = self.fetched_data;
        self.set_flags_zn(self.acc);
    }
    /// LDX: Load X with M
    pub(super) fn ldx(&mut self) {
        self.fetch_data();
        self.x = self.fetched_data;
        self.set_flags_zn(self.x);
    }
    /// LDY: Load Y with M
    pub(super) fn ldy(&mut self) {
        self.fetch_data();
        self.y = self.fetched_data;
        self.set_flags_zn(self.y);
    }
    /// STA: Store A into M
    pub(super) fn sta(&mut self) {
        self.write(self.abs_addr, self.acc);
    }
    /// STX: Store X into M
    pub(super) fn stx(&mut self) {
        self.write(self.abs_addr, self.x);
    }
    /// STY: Store Y into M
    pub(super) fn sty(&mut self) {
        self.write(self.abs_addr, self.y);
    }
    /// TAX: Transfer A to X
    pub(super) fn tax(&mut self) {
        self.x = self.acc;
        self.set_flags_zn(self.x);
    }
    /// TAY: Transfer A to Y
    pub(super) fn tay(&mut self) {
        self.y = self.acc;
        self.set_flags_zn(self.y);
    }
    /// TSX: Transfer Stack Pointer to X
    pub(super) fn tsx(&mut self) {
        self.x = self.sp as u8;
        self.set_flags_zn(self.x);
    }
    /// TXA: Transfer X to A
    pub(super) fn txa(&mut self) {
        self.acc = self.x;
        self.set_flags_zn(self.acc);
    }
    /// TXS: Transfer X to Stack Pointer
    pub(super) fn txs(&mut self) {
        self.sp = u16::from(self.x);
    }
    /// TYA: Transfer Y to A
    pub(super) fn tya(&mut self) {
        self.acc = self.y;
        self.set_flags_zn(self.acc);
    }

    /// Arithmetic opcodes

    /// ADC: Add M to A with Carry
    pub(super) fn adc(&mut self) {
        self.fetch_data();
        let a = self.acc;
        let (x1, o1) = self.fetched_data.overflowing_add(a);
        let (x2, o2) = x1.overflowing_add(self.get_flag(C));
        self.acc = x2;
        self.set_flag(C, o1 | o2);
        self.set_flag(
            V,
            (a ^ self.fetched_data) & 0x80 == 0 && (a ^ self.acc) & 0x80 != 0,
        );
        self.set_flags_zn(self.acc);
    }
    /// SBC: Subtract M from A with Carry
    pub(super) fn sbc(&mut self) {
        self.fetch_data();
        let a = self.acc;
        let (x1, o1) = a.overflowing_sub(self.fetched_data);
        let (x2, o2) = x1.overflowing_sub(1 - self.get_flag(C));
        self.acc = x2;
        self.set_flag(C, !(o1 | o2));
        self.set_flag(
            V,
            (a ^ self.fetched_data) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0,
        );
        self.set_flags_zn(self.acc);
    }
    /// DEC: Decrement M by One
    pub(super) fn dec(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_sub(1);
        self.write_fetched(val);
        self.set_flags_zn(val);
    }
    /// DEX: Decrement X by One
    pub(super) fn dex(&mut self) {
        self.x = self.x.wrapping_sub(1);
        self.set_flags_zn(self.x);
    }
    /// DEY: Decrement Y by One
    pub(super) fn dey(&mut self) {
        self.y = self.y.wrapping_sub(1);
        self.set_flags_zn(self.y);
    }
    /// INC: Increment M by One
    pub(super) fn inc(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_add(1);
        self.set_flags_zn(val);
        self.write_fetched(val);
    }
    /// INX: Increment X by One
    pub(super) fn inx(&mut self) {
        self.x = self.x.wrapping_add(1);
        self.set_flags_zn(self.x);
    }
    /// INY: Increment Y by One
    pub(super) fn iny(&mut self) {
        self.y = self.y.wrapping_add(1);
        self.set_flags_zn(self.y);
    }

    /// Bitwise opcodes

    /// AND: "And" M with A
    pub(super) fn and(&mut self) {
        self.fetch_data();
        self.acc &= self.fetched_data;
        self.set_flags_zn(self.acc);
    }
    /// ASL: Shift Left One Bit (M or A)
    pub(super) fn asl(&mut self) {
        self.fetch_data(); // Cycle 4 & 5
        self.write_fetched(self.fetched_data); // Cycle 6
        self.set_flag(C, (self.fetched_data >> 7) & 1 > 0);
        let val = self.fetched_data.wrapping_shl(1);
        self.set_flags_zn(val);
        self.write_fetched(val); // Cycle 7
    }
    /// BIT: Test Bits in M with A (Affects N, V, and Z)
    pub(super) fn bit(&mut self) {
        self.fetch_data();
        let val = self.acc & self.fetched_data;
        self.set_flag(Z, val == 0);
        self.set_flag(N, self.fetched_data & (1 << 7) > 0);
        self.set_flag(V, self.fetched_data & (1 << 6) > 0);
    }
    /// EOR: "Exclusive-Or" M with A
    pub(super) fn eor(&mut self) {
        self.fetch_data();
        self.acc ^= self.fetched_data;
        self.set_flags_zn(self.acc);
    }
    /// LSR: Shift Right One Bit (M or A)
    pub(super) fn lsr(&mut self) {
        self.fetch_data(); // Cycle 4 & 5
        self.write_fetched(self.fetched_data); // Cycle 6
        self.set_flag(C, self.fetched_data & 1 > 0);
        let val = self.fetched_data.wrapping_shr(1);
        self.set_flags_zn(val);
        self.write_fetched(val); // Cycle 7
    }
    /// ORA: "OR" M with A
    pub(super) fn ora(&mut self) {
        self.fetch_data();
        self.acc |= self.fetched_data;
        self.set_flags_zn(self.acc);
    }
    /// ROL: Rotate One Bit Left (M or A)
    pub(super) fn rol(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let old_c = self.get_flag(C);
        self.set_flag(C, (self.fetched_data >> 7) & 1 > 0);
        let val = (self.fetched_data << 1) | old_c;
        self.set_flags_zn(val);
        self.write_fetched(val);
    }
    /// ROR: Rotate One Bit Right (M or A)
    pub(super) fn ror(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let mut ret = self.fetched_data.rotate_right(1);
        if self.get_flag(C) == 1 {
            ret |= 1 << 7;
        } else {
            ret &= !(1 << 7);
        }
        self.set_flag(C, self.fetched_data & 1 > 0);
        self.set_flags_zn(ret);
        self.write_fetched(ret);
    }

    /// Branch opcodes

    /// Utility function used by all branch instructions
    pub(super) fn branch(&mut self) {
        // If an interrupt occurs during the final cycle of a non-pagecrossing branch
        // then it will be ignored until the next instruction completes
        let skip_nmi = self.nmi_pending && !self.last_nmi;
        let skip_irq = self.irq_pending > 0 && !self.last_irq;

        self.run_cycle();

        self.abs_addr = if self.rel_addr >= 128 {
            self.pc.wrapping_add(self.rel_addr | 0xFF00)
        } else {
            self.pc.wrapping_add(self.rel_addr)
        };
        if self.pages_differ(self.abs_addr, self.pc) {
            self.run_cycle();
        } else {
            if skip_nmi {
                self.last_nmi = false;
            }
            if skip_irq {
                self.last_irq = false;
            }
        }
        self.pc = self.abs_addr;
    }
    /// BCC: Branch on Carry Clear
    pub(super) fn bcc(&mut self) {
        if self.get_flag(C) == 0 {
            self.branch();
        }
    }
    /// BCS: Branch on Carry Set
    pub(super) fn bcs(&mut self) {
        if self.get_flag(C) == 1 {
            self.branch();
        }
    }
    /// BEQ: Branch on Result Zero
    pub(super) fn beq(&mut self) {
        if self.get_flag(Z) == 1 {
            self.branch();
        }
    }
    /// BMI: Branch on Result Negative
    pub(super) fn bmi(&mut self) {
        if self.get_flag(N) == 1 {
            self.branch();
        }
    }
    /// BNE: Branch on Result Not Zero
    pub(super) fn bne(&mut self) {
        if self.get_flag(Z) == 0 {
            self.branch();
        }
    }
    /// BPL: Branch on Result Positive
    pub(super) fn bpl(&mut self) {
        if self.get_flag(N) == 0 {
            self.branch();
        }
    }
    /// BVC: Branch on Overflow Clear
    pub(super) fn bvc(&mut self) {
        if self.get_flag(V) == 0 {
            self.branch();
        }
    }
    /// BVS: Branch on Overflow Set
    pub(super) fn bvs(&mut self) {
        if self.get_flag(V) == 1 {
            self.branch();
        }
    }

    /// Jump opcodes

    /// JMP: Jump to Location
    // #  address R/W description
    //   --- ------- --- -------------------------------------------------
    //    1    PC     R  fetch opcode, increment PC
    //    2    PC     R  fetch low address byte, increment PC
    //    3    PC     R  copy low address byte to PCL, fetch high address
    //                   byte to PCH
    pub(super) fn jmp(&mut self) {
        self.pc = self.abs_addr;
    }
    /// JSR: Jump to Location Save Return addr
    //  #  address R/W description
    // --- ------- --- -------------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  fetch low address byte, increment PC
    //  3  $0100,S  R  internal operation (predecrement S?)
    //  4  $0100,S  W  push PCH on stack, decrement S
    //  5  $0100,S  W  push PCL on stack, decrement S
    //  6    PC     R  copy low address byte to PCL, fetch high address
    //                 byte to PCH
    pub(super) fn jsr(&mut self) {
        let _ = self.read(SP_BASE | self.sp); // Cycle 3
        self.push_stackw(self.pc.wrapping_sub(1));
        self.pc = self.abs_addr;
    }
    /// RTI: Return from Interrupt
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  R  increment S
    //  4  $0100,S  R  pull P from stack, increment S
    //  5  $0100,S  R  pull PCL from stack, increment S
    //  6  $0100,S  R  pull PCH from stack
    pub(super) fn rti(&mut self) {
        let _ = self.read(SP_BASE | self.sp); // Cycle 3
        self.status = self.pop_stackb(); // Cycle 4
        self.status &= !(U as u8);
        self.status &= !(B as u8);
        self.pc = self.pop_stackw(); // Cycles 5 & 6
    }
    /// RTS: Return from Subroutine
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  R  increment S
    //  4  $0100,S  R  pull PCL from stack, increment S
    //  5  $0100,S  R  pull PCH from stack
    //  6    PC     R  increment PC
    pub(super) fn rts(&mut self) {
        let _ = self.read(SP_BASE | self.sp); // Cycle 3
        self.pc = self.pop_stackw().wrapping_add(1); // Cycles 4 & 5
        let _ = self.read(self.pc); // Cycle 6
    }

    ///  Register opcodes

    /// CLC: Clear Carry Flag
    pub(super) fn clc(&mut self) {
        self.set_flag(C, false);
    }
    /// SEC: Set Carry Flag
    pub(super) fn sec(&mut self) {
        self.set_flag(C, true);
    }
    /// CLD: Clear Decimal Mode
    pub(super) fn cld(&mut self) {
        self.set_flag(D, false);
    }
    /// SED: Set Decimal Mode
    pub(super) fn sed(&mut self) {
        self.set_flag(D, true);
    }
    /// CLI: Clear Interrupt Disable Bit
    pub(super) fn cli(&mut self) {
        self.set_flag(I, false);
    }
    /// SEI: Set Interrupt Disable Status
    pub(super) fn sei(&mut self) {
        self.set_flag(I, true);
    }
    /// CLV: Clear Overflow Flag
    pub(super) fn clv(&mut self) {
        self.set_flag(V, false);
    }

    /// Compare opcodes

    /// Utility function used by all compare instructions
    pub(super) fn compare(&mut self, a: u8, b: u8) {
        let result = a.wrapping_sub(b);
        self.set_flags_zn(result);
        self.set_flag(C, a >= b);
    }
    /// CMP: Compare M and A
    pub(super) fn cmp(&mut self) {
        self.fetch_data();
        self.compare(self.acc, self.fetched_data);
    }
    /// CPX: Compare M and X
    pub(super) fn cpx(&mut self) {
        self.fetch_data();
        self.compare(self.x, self.fetched_data);
    }
    /// CPY: Compare M and Y
    pub(super) fn cpy(&mut self) {
        self.fetch_data();
        self.compare(self.y, self.fetched_data);
    }

    /// Stack opcodes

    /// PHP: Push Processor Status on Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  W  push register on stack, decrement S
    pub(super) fn php(&mut self) {
        // Set U and B when pushing during PHP and BRK
        self.push_stackb(self.status | U as u8 | B as u8);
    }
    /// PLP: Pull Processor Status from Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  R  increment S
    //  4  $0100,S  R  pull register from stack
    pub(super) fn plp(&mut self) {
        let _ = self.read(SP_BASE | self.sp); // Cycle 3
        self.status = self.pop_stackb();
    }
    /// PHA: Push A on Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  W  push register on stack, decrement S
    pub(super) fn pha(&mut self) {
        self.push_stackb(self.acc);
    }
    /// PLA: Pull A from Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  R  increment S
    //  4  $0100,S  R  pull register from stack
    pub(super) fn pla(&mut self) {
        let _ = self.read(SP_BASE | self.sp); // Cycle 3
        self.acc = self.pop_stackb();
        self.set_flags_zn(self.acc);
    }

    /// System opcodes

    /// BRK: Force Break Interrupt
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away),
    //                 increment PC
    //  3  $0100,S  W  push PCH on stack (with B flag set), decrement S
    //  4  $0100,S  W  push PCL on stack, decrement S
    //  5  $0100,S  W  push P on stack, decrement S
    //  6   $FFFE   R  fetch PCL
    //  7   $FFFF   R  fetch PCH
    pub(super) fn brk(&mut self) {
        self.fetch_data(); // throw away
        self.push_stackw(self.pc);
        // Set U and B when pushing during PHP and BRK
        self.push_stackb(self.status | U as u8 | B as u8);
        self.set_flag(I, true);
        if self.last_nmi {
            self.nmi_pending = false;
            self.bus.ppu.nmi_pending = false;
            self.pc = self.readw(NMI_ADDR);
        } else {
            self.pc = self.readw(IRQ_ADDR);
        }
        // Prevent NMI from triggering immediately after BRK
        if self.last_nmi {
            self.last_nmi = false;
        }
    }
    /// NOP: No Operation
    pub(super) fn nop(&mut self) {
        self.fetch_data(); // throw away
    }

    /// Unofficial opcodes

    /// SKB: Like NOP
    pub(super) fn skb(&mut self) {
        self.fetch_data();
    }

    /// IGN: Like NOP, but can cross page boundary
    pub(super) fn ign(&mut self) {
        self.fetch_data();
    }

    /// XXX: Captures all unimplemented opcodes
    pub(super) fn xxx(&mut self) {
        error!(
            self,
            "Invalid opcode ${:02X} {:?} #{:?} encountered!",
            self.instr.opcode(),
            self.instr.op(),
            self.instr.addr_mode(),
        );
    }
    /// ISC/ISB: Shortcut for INC then SBC
    pub(super) fn isb(&mut self) {
        // INC
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_add(1);
        // SBC
        let a = self.acc;
        let (x1, o1) = a.overflowing_sub(val);
        let (x2, o2) = x1.overflowing_sub(1 - self.get_flag(C));
        self.acc = x2;
        self.set_flag(C, !(o1 | o2));
        self.set_flag(V, (a ^ val) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0);
        self.set_flags_zn(self.acc);
        self.write_fetched(val);
    }
    /// DCP: Shortcut for DEC then CMP
    pub(super) fn dcp(&mut self) {
        // DEC
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_sub(1);
        // CMP
        self.compare(self.acc, val);
        self.write_fetched(val);
    }
    /// AXS: A & X into X
    pub(super) fn axs(&mut self) {
        self.fetch_data();
        let t = u32::from(self.acc & self.x).wrapping_sub(u32::from(self.fetched_data));
        self.set_flags_zn((t & 0xFF) as u8);
        self.set_flag(C, (((t >> 8) & 0x01) ^ 0x01) == 0x01);
        self.x = (t & 0xFF) as u8;
    }
    /// LAS: Shortcut for LDA then TSX
    pub(super) fn las(&mut self) {
        self.lda();
        self.tsx();
    }
    /// LAX: Shortcut for LDA then TAX
    pub(super) fn lax(&mut self) {
        self.lda();
        self.tax();
    }
    /// AHX/SHA/AXA: AND X with A then AND with 7, then store in memory
    pub(super) fn ahx(&mut self) {
        let val = self.acc
            & self.x
            & self
                .fetched_data
                .wrapping_sub(self.y)
                .wrapping_shr(8)
                .wrapping_add(1);
        self.write_fetched(val);
    }
    /// SAX: AND A with X
    pub(super) fn sax(&mut self) {
        if self.instr.addr_mode() == IDY {
            self.fetch_data();
        }
        let val = self.acc & self.x;
        self.write_fetched(val);
    }
    /// XAA: Unknown
    pub(super) fn xaa(&mut self) {
        self.fetch_data();
        self.acc |= 0xEE;
        self.acc &= self.x;
        // AND
        self.acc &= self.fetched_data;
        self.set_flags_zn(self.acc);
    }
    /// SXA/SHX/XAS: AND X with the high byte of the target address + 1
    pub(super) fn sxa(&mut self) {
        let hi = (self.abs_addr >> 8) as u8;
        let lo = (self.abs_addr & 0xFF) as u8;
        let val = self.x & hi.wrapping_add(1);
        self.abs_addr = ((u16::from(self.x) & u16::from(hi.wrapping_add(1))) << 8) | u16::from(lo);
        self.write_fetched(val);
    }
    /// SYA/SHY/SAY: AND Y with the high byte of the target address + 1
    pub(super) fn sya(&mut self) {
        let hi = (self.abs_addr >> 8) as u8;
        let lo = (self.abs_addr & 0xFF) as u8;
        let val = self.y & hi.wrapping_add(1);
        self.abs_addr = ((u16::from(self.y) & u16::from(hi.wrapping_add(1))) << 8) | u16::from(lo);
        self.write_fetched(val);
    }
    /// RRA: Shortcut for ROR then ADC
    pub(super) fn rra(&mut self) {
        self.fetch_data();
        // ROR
        self.write_fetched(self.fetched_data); // dummy write
        let mut ret = self.fetched_data.rotate_right(1);
        if self.get_flag(C) == 1 {
            ret |= 1 << 7;
        } else {
            ret &= !(1 << 7);
        }
        self.set_flag(C, self.fetched_data & 1 > 0);
        // ADC
        let a = self.acc;
        let (x1, o1) = ret.overflowing_add(a);
        let (x2, o2) = x1.overflowing_add(self.get_flag(C));
        self.acc = x2;
        self.set_flag(C, o1 | o2);
        self.set_flag(V, (a ^ ret) & 0x80 == 0 && (a ^ self.acc) & 0x80 != 0);
        self.set_flags_zn(self.acc);
        self.write_fetched(ret);
    }
    /// TAS: Shortcut for STA then TXS
    pub(super) fn tas(&mut self) {
        // STA
        self.write(self.abs_addr, self.acc);
        // TXS
        self.sp = u16::from(self.x);
    }
    /// ARR: Shortcut for AND #imm then ROR, but sets flags differently
    /// C is bit 6 and V is bit 6 xor bit 5
    pub(super) fn arr(&mut self) {
        // AND
        self.fetch_data();
        self.acc &= self.fetched_data;
        // ROR
        self.set_flag(V, (self.acc ^ (self.acc >> 1)) & 0x40 == 0x40);
        let t = self.acc >> 7;
        self.acc >>= 1;
        self.acc |= self.get_flag(C) << 7;
        self.set_flag(C, t & 0x01 == 0x01);
        self.set_flags_zn(self.acc);
    }
    /// SRA: Shortcut for LSR then EOR
    pub(super) fn sre(&mut self) {
        self.fetch_data();
        // LSR
        self.write_fetched(self.fetched_data); // dummy write
        self.set_flag(C, self.fetched_data & 1 > 0);
        let val = self.fetched_data.wrapping_shr(1);
        // EOR
        self.acc ^= val;
        self.set_flags_zn(self.acc);
        self.write_fetched(val);
    }
    /// ALR/ASR: Shortcut for AND #imm then LSR
    pub(super) fn alr(&mut self) {
        // AND
        self.fetch_data();
        self.acc &= self.fetched_data;
        // LSR
        self.set_flag(C, self.acc & 0x01 == 0x01);
        self.acc >>= 1;
        self.set_flags_zn(self.acc);
    }
    /// RLA: Shortcut for ROL then AND
    pub(super) fn rla(&mut self) {
        self.fetch_data();
        // ROL
        self.write_fetched(self.fetched_data); // dummy write
        let old_c = self.get_flag(C);
        self.set_flag(C, (self.fetched_data >> 7) & 1 > 0);
        let val = (self.fetched_data << 1) | old_c;
        // AND
        self.acc &= val;
        self.set_flags_zn(self.acc);
        self.write_fetched(val);
    }
    /// ANC/AAC: AND #imm but puts bit 7 into carry as if ASL was executed
    pub(super) fn anc(&mut self) {
        // AND
        self.fetch_data();
        self.acc &= self.fetched_data;
        self.set_flags_zn(self.acc);
        // Put bit 7 into carry
        self.set_flag(C, (self.acc >> 7) & 1 > 0);
    }
    /// SLO: Shortcut for ASL then ORA
    pub(super) fn slo(&mut self) {
        self.fetch_data();
        // ASL
        self.write_fetched(self.fetched_data); // dummy write
        self.set_flag(C, (self.fetched_data >> 7) & 1 > 0);
        let val = self.fetched_data.wrapping_shl(1);
        self.write_fetched(val);
        // ORA
        self.acc |= val;
        self.set_flags_zn(self.acc);
    }
}

impl Savable for Operation {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => Operation::ADC,
            1 => Operation::AND,
            2 => Operation::ASL,
            3 => Operation::BCC,
            4 => Operation::BCS,
            5 => Operation::BEQ,
            6 => Operation::BIT,
            7 => Operation::BMI,
            8 => Operation::BNE,
            9 => Operation::BPL,
            10 => Operation::BRK,
            11 => Operation::BVC,
            12 => Operation::BVS,
            13 => Operation::CLC,
            14 => Operation::CLD,
            15 => Operation::CLI,
            16 => Operation::CLV,
            17 => Operation::CMP,
            18 => Operation::CPX,
            19 => Operation::CPY,
            20 => Operation::DEC,
            21 => Operation::DEX,
            22 => Operation::DEY,
            23 => Operation::EOR,
            24 => Operation::INC,
            25 => Operation::INX,
            26 => Operation::INY,
            27 => Operation::JMP,
            28 => Operation::JSR,
            29 => Operation::LDA,
            30 => Operation::LDX,
            31 => Operation::LDY,
            32 => Operation::LSR,
            33 => Operation::NOP,
            34 => Operation::ORA,
            35 => Operation::PHA,
            36 => Operation::PHP,
            37 => Operation::PLA,
            38 => Operation::PLP,
            39 => Operation::ROL,
            40 => Operation::ROR,
            41 => Operation::RTI,
            42 => Operation::RTS,
            43 => Operation::SBC,
            44 => Operation::SEC,
            45 => Operation::SED,
            46 => Operation::SEI,
            47 => Operation::STA,
            48 => Operation::STX,
            49 => Operation::STY,
            50 => Operation::TAX,
            51 => Operation::TAY,
            52 => Operation::TSX,
            53 => Operation::TXA,
            54 => Operation::TXS,
            55 => Operation::TYA,
            56 => Operation::SKB,
            57 => Operation::IGN,
            58 => Operation::ISB,
            59 => Operation::DCP,
            60 => Operation::AXS,
            61 => Operation::LAS,
            62 => Operation::LAX,
            63 => Operation::AHX,
            64 => Operation::SAX,
            65 => Operation::XAA,
            66 => Operation::SXA,
            67 => Operation::RRA,
            68 => Operation::TAS,
            69 => Operation::SYA,
            70 => Operation::ARR,
            71 => Operation::SRE,
            72 => Operation::ALR,
            73 => Operation::RLA,
            74 => Operation::ANC,
            75 => Operation::SLO,
            76 => Operation::XXX,
            _ => panic!("invalid Operation value"),
        };
        Ok(())
    }
}

impl Savable for AddrMode {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => AddrMode::IMM,
            1 => AddrMode::ZP0,
            2 => AddrMode::ZPX,
            3 => AddrMode::ZPY,
            4 => AddrMode::ABS,
            5 => AddrMode::ABX,
            6 => AddrMode::ABY,
            7 => AddrMode::IND,
            8 => AddrMode::IDX,
            9 => AddrMode::IDY,
            10 => AddrMode::REL,
            11 => AddrMode::ACC,
            12 => AddrMode::IMP,
            _ => panic!("invalid AddrMode value"),
        };
        Ok(())
    }
}

impl Savable for Instr {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.0.save(fh)?;
        self.1.save(fh)?;
        self.2.save(fh)?;
        self.3.save(fh)
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.0.load(fh)?;
        self.1.load(fh)?;
        self.2.load(fh)?;
        self.3.load(fh)
    }
}

impl fmt::Debug for Instr {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        let mut op = self.op();
        let unofficial = match self.op() {
            XXX | ISB | DCP | AXS | LAS | LAX | AHX | SAX | XAA | SXA | RRA | TAS | SYA | ARR
            | SRE | ALR | RLA | ANC | SLO => "*",
            NOP if self.opcode() != 0xEA => "*", // 0xEA is the only official NOP
            SKB | IGN => {
                op = NOP;
                "*"
            }
            SBC if self.opcode() == 0xEB => "*",
            _ => "",
        };
        write!(f, "{:1}{:?}", unofficial, op)
    }
}
