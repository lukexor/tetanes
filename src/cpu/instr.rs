use crate::{
    cpu::{Cpu, Status},
    mem::{Access, Mem},
};
use serde::{Deserialize, Serialize};

#[rustfmt::skip]
#[allow(clippy::upper_case_acronyms)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
// List of all CPU official and unofficial operations
// http://wiki.nesdev.com/w/index.php/6502_instructions
// http://archive.6502.org/datasheets/rockwell_r650x_r651x.pdf
#[must_use]
pub enum Operation {
    ADC, AND, ASL, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC, CLD, CLI, CLV, CMP, CPX,
    CPY, DEC, DEX, DEY, EOR, INC, INX, INY, JMP, JSR, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA,
    PLP, ROL, ROR, RTI, RTS, SBC, SEC, SED, SEI, STA, STX, STY, TAX, TAY, TSX, TXA, TXS, TYA,
    // "Unofficial" opcodes
    SKB, IGN, ISB, DCP, AXS, LAS, LAX, AHX, SAX, XAA, SXA, RRA, TAS, SYA, ARR, SRE, ALR, RLA, ANC,
    SLO, XXX
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
#[rustfmt::skip]
#[must_use]
pub enum AddrMode {
    IMM,
    ZP0, ZPX, ZPY,
    ABS, ABX, ABY,
    IND, IDX, IDY,
    REL, ACC, IMP,
}

use AddrMode::{ABS, ABX, ABY, ACC, IDX, IDY, IMM, IMP, IND, REL, ZP0, ZPX, ZPY};
use Operation::{
    ADC, AHX, ALR, ANC, AND, ARR, ASL, AXS, BCC, BCS, BEQ, BIT, BMI, BNE, BPL, BRK, BVC, BVS, CLC,
    CLD, CLI, CLV, CMP, CPX, CPY, DCP, DEC, DEX, DEY, EOR, IGN, INC, INX, INY, ISB, JMP, JSR, LAS,
    LAX, LDA, LDX, LDY, LSR, NOP, ORA, PHA, PHP, PLA, PLP, RLA, ROL, ROR, RRA, RTI, RTS, SAX, SBC,
    SEC, SED, SEI, SKB, SLO, SRE, STA, STX, STY, SXA, SYA, TAS, TAX, TAY, TSX, TXA, TXS, TYA, XAA,
    XXX,
};

// (opcode, Addressing Mode, Operation, cycles taken)
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct Instr(u8, AddrMode, Operation, usize);

impl Instr {
    #[inline]
    #[must_use]
    pub const fn opcode(&self) -> u8 {
        self.0
    }
    #[inline]
    pub const fn addr_mode(&self) -> AddrMode {
        self.1
    }
    #[inline]
    pub const fn op(&self) -> Operation {
        self.2
    }
    #[inline]
    #[must_use]
    pub const fn cycles(&self) -> usize {
        self.3
    }
}

/// CPU Addressing Modes
///
/// The 6502 can address 64KB from 0x0000 - 0xFFFF. The high byte is usually the page and the
/// low byte the offset into the page. There are 256 total pages of 256 bytes.
impl Cpu {
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

    /// Accumulator
    /// No additional data is required, but the default target will be the accumulator.
    //  ASL, ROL, LSR, ROR
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    #[inline]
    pub(super) fn acc(&mut self) {
        let _ = self.read(self.pc, Access::Read); // Cycle 2, Read and throw away
    }

    /// Implied
    /// No additional data is required, but the default target will be the accumulator.
    // #  address R/W description
    //   --- ------- --- -----------------------------------------------
    //    1    PC     R  fetch opcode, increment PC
    //    2    PC     R  read next instruction byte (and throw it away)
    #[inline]
    pub(super) fn imp(&mut self) {
        let _ = self.read(self.pc, Access::Read); // Cycle 2, Read and throw away
    }

    /// Immediate
    /// Uses the next byte as the value, so we'll update the `abs_addr` to the next byte.
    // #  address R/W description
    //   --- ------- --- ------------------------------------------
    //    1    PC     R  fetch opcode, increment PC
    //    2    PC     R  fetch value, increment PC
    #[inline]
    pub(super) fn imm(&mut self) {
        self.abs_addr = self.pc;
        self.pc = self.pc.wrapping_add(1);
    }

    /// Zero Page
    /// Accesses the first 0xFF bytes of the address range, so this only requires one extra byte
    /// instead of the usual two.
    //  Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                    LAX, NOP)

    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch address, increment PC
    //     3  address  R  read from effective address

    //  Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                  SLO, SRE, RLA, RRA, ISB, DCP)

    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch address, increment PC
    //     3  address  R  read from effective address
    //     4  address  W  write the value back to effective address,
    //                    and do the operation on it
    //     5  address  W  write the new value to effective address

    //  Write instructions (STA, STX, STY, SAX)

    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch address, increment PC
    //     3  address  W  write register to effective address
    #[inline]
    pub(super) fn zp0(&mut self) {
        self.abs_addr = u16::from(self.read_instr()); // Cycle 2
    }

    /// Zero Page w/ X offset
    /// Same as Zero Page, but is offset by adding the x register.
    //  Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                     LAX, NOP)

    //     #   address  R/W description
    //    --- --------- --- ------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+X* R  read from effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.

    //  Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                  SLO, SRE, RLA, RRA, ISB, DCP)

    //     #   address  R/W description
    //    --- --------- --- ---------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register X to it
    //     4  address+X* R  read from effective address
    //     5  address+X* W  write the value back to effective address,
    //                      and do the operation on it
    //     6  address+X* W  write the new value to effective address

    //    Note: * The high byte of the effective address is always zero,
    //            i.e. page boundary crossings are not handled.

    //  Write instructions (STA, STX, STY, SAX)

    //     #   address  R/W description
    //    --- --------- --- -------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+X* W  write to effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.
    #[inline]
    pub(super) fn zpx(&mut self) {
        let addr = u16::from(self.read_instr()); // Cycle 2
        let _ = self.read(addr, Access::Read); // Cycle 3
        self.abs_addr = addr.wrapping_add(self.x.into()) & 0x00FF;
    }

    /// Zero Page w/ Y offset
    /// Same as Zero Page, but is offset by adding the y register.
    //  Read instructions (LDX, LAX)

    //     #   address  R/W description
    //    --- --------- --- ------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+Y* R  read from effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.

    //  Write instructions (STX, SAX)

    //     #   address  R/W description
    //    --- --------- --- -------------------------------------------
    //     1     PC      R  fetch opcode, increment PC
    //     2     PC      R  fetch address, increment PC
    //     3   address   R  read from address, add index register to it
    //     4  address+Y* W  write to effective address

    //           * The high byte of the effective address is always zero,
    //             i.e. page boundary crossings are not handled.
    #[inline]
    pub(super) fn zpy(&mut self) {
        let addr = u16::from(self.read_instr()); // Cycle 2
        let _ = self.read(addr, Access::Read); // Cycle 3
        self.abs_addr = addr.wrapping_add(self.y.into()) & 0x00FF;
    }

    /// Relative
    /// This mode is only used by branching instructions. The address must be between -128 and +127,
    /// allowing the branching instruction to move backward or forward relative to the current
    /// program counter.
    //    #   address  R/W description
    //   --- --------- --- ---------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch fetched_data, increment PC
    //    3     PC      R  Fetch opcode of next instruction,
    //                     If branch is taken, add fetched_data to PCL.
    //                     Otherwise increment PC.
    //    4+    PC*     R  Fetch opcode of next instruction.
    //                     Fix PCH. If it did not change, increment PC.
    //    5!    PC      R  Fetch opcode of next instruction,
    //                     increment PC.

    //   Notes: The opcode fetch of the next instruction is included to
    //          this diagram for illustration purposes. When determining
    //          real execution times, remember to subtract the last
    //          cycle.

    //          * The high byte of Program Counter (PCH) may be invalid
    //            at this time, i.e. it may be smaller or bigger by $100.

    //          + If branch is taken, this cycle will be executed.

    //          ! If branch occurs to different page, this cycle will be
    //            executed.
    #[inline]
    pub(super) fn rel(&mut self) {
        self.rel_addr = u16::from(self.read_instr()); // Cycle 2
    }

    /// Absolute
    /// Uses a full 16-bit address as the next value.
    //  Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                     LAX, NOP)
    //
    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch low byte of address, increment PC
    //     3    PC     R  fetch high byte of address, increment PC
    //     4  address  R  read from effective address

    //  Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                  SLO, SRE, RLA, RRA, ISB, DCP)
    //
    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch low byte of address, increment PC
    //     3    PC     R  fetch high byte of address, increment PC
    //     4  address  R  read from effective address
    //     5  address  W  write the value back to effective address,
    //                    and do the operation on it
    //     6  address  W  write the new value to effective address

    //  Write instructions (STA, STX, STY, SAX)
    //
    //     #  address R/W description
    //    --- ------- --- ------------------------------------------
    //     1    PC     R  fetch opcode, increment PC
    //     2    PC     R  fetch low byte of address, increment PC
    //     3    PC     R  fetch high byte of address, increment PC
    //     4  address  W  write register to effective address
    #[inline]
    pub(super) fn abs(&mut self) {
        self.abs_addr = self.read_instr_u16(); // Cycle 2 & 3
    }

    /// Absolute w/ X offset
    /// Same as Absolute, but is offset by adding the x register. If a page boundary is crossed, an
    /// additional clock is required.
    // Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                    LAX, LAE, SHS, NOP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+X* R  read from effective address,
    //                     fix the high byte of effective address
    //    5+ address+X  R  re-read from effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    //          + This cycle will be executed only if the effective address
    //            was invalid during cycle #4, i.e. page boundary was crossed.

    // Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                 SLO, SRE, RLA, RRA, ISB, DCP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1    PC       R  fetch opcode, increment PC
    //    2    PC       R  fetch low byte of address, increment PC
    //    3    PC       R  fetch high byte of address,
    //                     add index register X to low address byte,
    //                     increment PC
    //    4  address+X* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+X  R  re-read from effective address
    //    6  address+X  W  write the value back to effective address,
    //                     and do the operation on it
    //    7  address+X  W  write the new value to effective address

    //   Notes: * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    // Write instructions (STA, STX, STY, SHA, SHX, SHY)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+X* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+X  W  write to effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100. Because
    //            the processor cannot undo a write to an invalid
    //            address, it always reads from the address first.
    #[inline]
    pub(super) fn abx(&mut self) {
        let addr = self.read_instr_u16(); // Cycle 2 & 3
        self.abs_addr = addr.wrapping_add(self.x.into());
        // Cycle 4 Read with fixed high byte
        self.fetched_data = self.read((addr & 0xFF00) | (self.abs_addr & 0x00FF), Access::Read);
    }

    /// Absolute w/ Y offset
    /// Same as Absolute, but is offset by adding the y register. If a page boundary is crossed, an
    /// additional clock is required.
    // Read instructions (LDA, LDX, LDY, EOR, AND, ORA, ADC, SBC, CMP, BIT,
    //                    LAX, LAE, SHS, NOP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+Y* R  read from effective address,
    //                     fix the high byte of effective address
    //    5+ address+Y  R  re-read from effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    //          + This cycle will be executed only if the effective address
    //            was invalid during cycle #4, i.e. page boundary was crossed.

    // Read-Modify-Write instructions (ASL, LSR, ROL, ROR, INC, DEC,
    //                                 SLO, SRE, RLA, RRA, ISB, DCP)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1    PC       R  fetch opcode, increment PC
    //    2    PC       R  fetch low byte of address, increment PC
    //    3    PC       R  fetch high byte of address,
    //                     add index register Y to low address byte,
    //                     increment PC
    //    4  address+Y* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+Y  R  re-read from effective address
    //    6  address+Y  W  write the value back to effective address,
    //                     and do the operation on it
    //    7  address+Y  W  write the new value to effective address

    //   Notes: * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    // Write instructions (STA, STX, STY, SHA, SHX, SHY)

    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch low byte of address, increment PC
    //    3     PC      R  fetch high byte of address,
    //                     add index register to low address byte,
    //                     increment PC
    //    4  address+Y* R  read from effective address,
    //                     fix the high byte of effective address
    //    5  address+Y  W  write to effective address

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100. Because
    //            the processor cannot undo a write to an invalid
    //            address, it always reads from the address first.
    #[inline]
    pub(super) fn aby(&mut self) {
        let addr = self.read_instr_u16(); // Cycles 2 & 3
        self.abs_addr = addr.wrapping_add(self.y.into());
        // Cycle 4 Read with fixed high byte
        self.fetched_data = self.read((addr & 0xFF00) | (self.abs_addr & 0x00FF), Access::Read);
    }

    /// Indirect (JMP)
    /// The next 16-bit address is used to get the actual 16-bit address. This instruction has
    /// a bug in the original hardware. If the lo byte is 0xFF, the hi byte would cross a page
    /// boundary. However, this doesn't work correctly on the original hardware and instead
    /// wraps back around to 0.
    //    #   address  R/W description
    //   --- --------- --- ------------------------------------------
    //    1     PC      R  fetch opcode, increment PC
    //    2     PC      R  fetch pointer address low, increment PC
    //    3     PC      R  fetch pointer address high, increment PC
    //    4   pointer   R  fetch low address to latch
    //    5  pointer+1* R  fetch PCH, copy latch to PCL

    //   Note: * The PCH will always be fetched from the same page
    //           than PCL, i.e. page boundary crossing is not handled.

    //            How Real Programmers Acknowledge Interrupts
    #[inline]
    pub(super) fn ind(&mut self) {
        let addr = self.read_instr_u16();
        if addr & 0xFF == 0xFF {
            // Simulate bug
            let lo = self.read(addr, Access::Read);
            let hi = self.read(addr & 0xFF00, Access::Read);
            self.abs_addr = u16::from_le_bytes([lo, hi]);
        } else {
            // Normal behavior
            self.abs_addr = self.read_u16(addr);
        }
    }

    /// Indirect X
    /// The next 8-bit address is offset by the X register to get the actual 16-bit address from
    /// page 0x00.
    // Read instructions (LDA, ORA, EOR, AND, ADC, CMP, SBC, LAX)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  read from the address, add X to it
    //    4   pointer+X   R  fetch effective address low
    //    5  pointer+X+1  R  fetch effective address high
    //    6    address    R  read from effective address

    //   Note: The effective address is always fetched from zero page,
    //         i.e. the zero page boundary crossing is not handled.

    // Read-Modify-Write instructions (SLO, SRE, RLA, RRA, ISB, DCP)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  read from the address, add X to it
    //    4   pointer+X   R  fetch effective address low
    //    5  pointer+X+1  R  fetch effective address high
    //    6    address    R  read from effective address
    //    7    address    W  write the value back to effective address,
    //                       and do the operation on it
    //    8    address    W  write the new value to effective address

    //   Note: The effective address is always fetched from zero page,
    //         i.e. the zero page boundary crossing is not handled.

    // Write instructions (STA, SAX)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  read from the address, add X to it
    //    4   pointer+X   R  fetch effective address low
    //    5  pointer+X+1  R  fetch effective address high
    //    6    address    W  write to effective address

    //   Note: The effective address is always fetched from zero page,
    //         i.e. the zero page boundary crossing is not handled.
    #[inline]
    pub(super) fn idx(&mut self) {
        let addr = self.read_instr(); // Cycle 2
        let _ = self.read(u16::from(addr), Access::Read); // Cycle 3
        let addr = addr.wrapping_add(self.x);
        self.abs_addr = self.read_zp_u16(addr); // Cycles 4 & 5
    }

    /// Indirect Y
    /// The next 8-bit address is read to get a 16-bit address from page 0x00, which is then offset
    /// by the Y register. If a page boundary is crossed, add a clock cycle.
    // Read instructions (LDA, EOR, AND, ORA, ADC, SBC, CMP)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  fetch effective address low
    //    4   pointer+1   R  fetch effective address high,
    //                       add Y to low byte of effective address
    //    5   address+Y*  R  read from effective address,
    //                       fix high byte of effective address
    //    6+  address+Y   R  read from effective address

    //   Notes: The effective address is always fetched from zero page,
    //          i.e. the zero page boundary crossing is not handled.

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    //          + This cycle will be executed only if the effective address
    //            was invalid during cycle #5, i.e. page boundary was crossed.

    // Read-Modify-Write instructions (SLO, SRE, RLA, RRA, ISB, DCP)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  fetch effective address low
    //    4   pointer+1   R  fetch effective address high,
    //                       add Y to low byte of effective address
    //    5   address+Y*  R  read from effective address,
    //                       fix high byte of effective address
    //    6   address+Y   R  re-read from effective address
    //    7   address+Y   W  write the value back to effective address,
    //                       and do the operation on it
    //    8   address+Y   W  write the new value to effective address

    //   Notes: The effective address is always fetched from zero page,
    //          i.e. the zero page boundary crossing is not handled.

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.

    // Write instructions (STA, SHA)

    //    #    address   R/W description
    //   --- ----------- --- ------------------------------------------
    //    1      PC       R  fetch opcode, increment PC
    //    2      PC       R  fetch pointer address, increment PC
    //    3    pointer    R  fetch effective address low
    //    4   pointer+1   R  fetch effective address high,
    //                       add Y to low byte of effective address
    //    5   address+Y*  R  read from effective address,
    //                       fix high byte of effective address
    //    6   address+Y   W  write to effective address

    //   Notes: The effective address is always fetched from zero page,
    //          i.e. the zero page boundary crossing is not handled.

    //          * The high byte of the effective address may be invalid
    //            at this time, i.e. it may be smaller by $100.
    #[inline]
    pub(super) fn idy(&mut self) {
        let addr = self.read_instr(); // Cycle 2
        let addr = self.read_zp_u16(addr); // Cycles 3 & 4
        self.abs_addr = addr.wrapping_add(self.y.into());
        // Cycle 4 Read with fixed high byte
        self.fetched_data = self.read((addr & 0xFF00) | (self.abs_addr & 0x00FF), Access::Read);
    }
}

/// CPU instructions
impl Cpu {
    /// Storage opcodes

    /// LDA: Load A with M
    #[inline]
    pub(super) fn lda(&mut self) {
        self.fetch_data();
        self.acc = self.fetched_data;
        self.set_zn_status(self.acc);
    }
    /// LDX: Load X with M
    #[inline]
    pub(super) fn ldx(&mut self) {
        self.fetch_data();
        self.x = self.fetched_data;
        self.set_zn_status(self.x);
    }
    /// LDY: Load Y with M
    #[inline]
    pub(super) fn ldy(&mut self) {
        self.fetch_data();
        self.y = self.fetched_data;
        self.set_zn_status(self.y);
    }
    /// STA: Store A into M
    #[inline]
    pub(super) fn sta(&mut self) {
        self.write(self.abs_addr, self.acc, Access::Write);
    }
    /// STX: Store X into M
    #[inline]
    pub(super) fn stx(&mut self) {
        self.write(self.abs_addr, self.x, Access::Write);
    }
    /// STY: Store Y into M
    #[inline]
    pub(super) fn sty(&mut self) {
        self.write(self.abs_addr, self.y, Access::Write);
    }
    /// TAX: Transfer A to X
    #[inline]
    pub(super) fn tax(&mut self) {
        self.x = self.acc;
        self.set_zn_status(self.x);
    }
    /// TAY: Transfer A to Y
    #[inline]
    pub(super) fn tay(&mut self) {
        self.y = self.acc;
        self.set_zn_status(self.y);
    }
    /// TSX: Transfer Stack Pointer to X
    #[inline]
    pub(super) fn tsx(&mut self) {
        self.x = self.sp;
        self.set_zn_status(self.x);
    }
    /// TXA: Transfer X to A
    #[inline]
    pub(super) fn txa(&mut self) {
        self.acc = self.x;
        self.set_zn_status(self.acc);
    }
    /// TXS: Transfer X to Stack Pointer
    #[inline]
    pub(super) fn txs(&mut self) {
        self.sp = self.x;
    }
    /// TYA: Transfer Y to A
    #[inline]
    pub(super) fn tya(&mut self) {
        self.acc = self.y;
        self.set_zn_status(self.acc);
    }

    /// Arithmetic opcodes

    /// ADC: Add M to A with Carry
    #[inline]
    pub(super) fn adc(&mut self) {
        self.fetch_data();
        let a = self.acc;
        let (x1, o1) = self.fetched_data.overflowing_add(a);
        let (x2, o2) = x1.overflowing_add(self.status_bit(Status::C));
        self.acc = x2;
        self.status.set(Status::C, o1 | o2);
        self.status.set(
            Status::V,
            (a ^ self.fetched_data) & 0x80 == 0 && (a ^ self.acc) & 0x80 != 0,
        );
        self.set_zn_status(self.acc);
    }
    /// SBC: Subtract M from A with Carry
    #[inline]
    pub(super) fn sbc(&mut self) {
        self.fetch_data();
        let a = self.acc;
        let (x1, o1) = a.overflowing_sub(self.fetched_data);
        let (x2, o2) = x1.overflowing_sub(1 - self.status_bit(Status::C));
        self.acc = x2;
        self.status.set(Status::C, !(o1 | o2));
        self.status.set(
            Status::V,
            (a ^ self.fetched_data) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0,
        );
        self.set_zn_status(self.acc);
    }
    /// DEC: Decrement M by One
    #[inline]
    pub(super) fn dec(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_sub(1);
        self.write_fetched(val);
        self.set_zn_status(val);
    }
    /// DEX: Decrement X by One
    #[inline]
    pub(super) fn dex(&mut self) {
        self.x = self.x.wrapping_sub(1);
        self.set_zn_status(self.x);
    }
    /// DEY: Decrement Y by One
    #[inline]
    pub(super) fn dey(&mut self) {
        self.y = self.y.wrapping_sub(1);
        self.set_zn_status(self.y);
    }
    /// INC: Increment M by One
    #[inline]
    pub(super) fn inc(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_add(1);
        self.set_zn_status(val);
        self.write_fetched(val);
    }
    /// INX: Increment X by One
    #[inline]
    pub(super) fn inx(&mut self) {
        self.x = self.x.wrapping_add(1);
        self.set_zn_status(self.x);
    }
    /// INY: Increment Y by One
    #[inline]
    pub(super) fn iny(&mut self) {
        self.y = self.y.wrapping_add(1);
        self.set_zn_status(self.y);
    }

    /// Bitwise opcodes

    /// AND: "And" M with A
    #[inline]
    pub(super) fn and(&mut self) {
        self.fetch_data();
        self.acc &= self.fetched_data;
        self.set_zn_status(self.acc);
    }
    /// ASL: Shift Left One Bit (M or A)
    #[inline]
    pub(super) fn asl(&mut self) {
        self.fetch_data(); // Cycle 4 & 5
        self.write_fetched(self.fetched_data); // Cycle 6
        self.status.set(Status::C, (self.fetched_data >> 7) & 1 > 0);
        let val = self.fetched_data.wrapping_shl(1);
        self.set_zn_status(val);
        self.write_fetched(val); // Cycle 7
    }
    /// BIT: Test Bits in M with A (Affects N, V, and Z)
    #[inline]
    pub(super) fn bit(&mut self) {
        self.fetch_data();
        let val = self.acc & self.fetched_data;
        self.status.set(Status::Z, val == 0);
        self.status.set(Status::N, self.fetched_data & (1 << 7) > 0);
        self.status.set(Status::V, self.fetched_data & (1 << 6) > 0);
    }
    /// EOR: "Exclusive-Or" M with A
    #[inline]
    pub(super) fn eor(&mut self) {
        self.fetch_data();
        self.acc ^= self.fetched_data;
        self.set_zn_status(self.acc);
    }
    /// LSR: Shift Right One Bit (M or A)
    #[inline]
    pub(super) fn lsr(&mut self) {
        self.fetch_data(); // Cycle 4 & 5
        self.write_fetched(self.fetched_data); // Cycle 6
        self.status.set(Status::C, self.fetched_data & 1 > 0);
        let val = self.fetched_data.wrapping_shr(1);
        self.set_zn_status(val);
        self.write_fetched(val); // Cycle 7
    }
    /// ORA: "OR" M with A
    #[inline]
    pub(super) fn ora(&mut self) {
        self.fetch_data();
        self.acc |= self.fetched_data;
        self.set_zn_status(self.acc);
    }
    /// ROL: Rotate One Bit Left (M or A)
    #[inline]
    pub(super) fn rol(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let old_c = self.status_bit(Status::C);
        self.status.set(Status::C, (self.fetched_data >> 7) & 1 > 0);
        let val = (self.fetched_data << 1) | old_c;
        self.set_zn_status(val);
        self.write_fetched(val);
    }
    /// ROR: Rotate One Bit Right (M or A)
    #[inline]
    pub(super) fn ror(&mut self) {
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let mut ret = self.fetched_data.rotate_right(1);
        if self.status.intersects(Status::C) {
            ret |= 1 << 7;
        } else {
            ret &= !(1 << 7);
        }
        self.status.set(Status::C, self.fetched_data & 1 > 0);
        self.set_zn_status(ret);
        self.write_fetched(ret);
    }

    /// Branch opcodes

    /// Utility function used by all branch instructions
    #[inline]
    pub(super) fn branch(&mut self) {
        // If an interrupt occurs during the final cycle of a non-pagecrossing branch
        // then it will be ignored until the next instruction completes
        if self.run_irq && !self.prev_run_irq {
            self.run_irq = false;
        }

        self.read(self.pc, Access::Read); // Dummy read

        self.abs_addr = if self.rel_addr & 0x80 == 0x80 {
            self.pc.wrapping_add(self.rel_addr | 0xFF00)
        } else {
            self.pc.wrapping_add(self.rel_addr)
        };
        if Self::pages_differ(self.abs_addr, self.pc) {
            self.read(self.pc, Access::Read); // Dummy read
        }
        self.pc = self.abs_addr;
    }
    /// BCC: Branch on Carry Clear
    #[inline]
    pub(super) fn bcc(&mut self) {
        if !self.status.intersects(Status::C) {
            self.branch();
        }
    }
    /// BCS: Branch on Carry Set
    #[inline]
    pub(super) fn bcs(&mut self) {
        if self.status.intersects(Status::C) {
            self.branch();
        }
    }
    /// BEQ: Branch on Result Zero
    #[inline]
    pub(super) fn beq(&mut self) {
        if self.status.intersects(Status::Z) {
            self.branch();
        }
    }
    /// BMI: Branch on Result Negative
    #[inline]
    pub(super) fn bmi(&mut self) {
        if self.status.intersects(Status::N) {
            self.branch();
        }
    }
    /// BNE: Branch on Result Not Zero
    #[inline]
    pub(super) fn bne(&mut self) {
        if !self.status.intersects(Status::Z) {
            self.branch();
        }
    }
    /// BPL: Branch on Result Positive
    #[inline]
    pub(super) fn bpl(&mut self) {
        if !self.status.intersects(Status::N) {
            self.branch();
        }
    }
    /// BVC: Branch on Overflow Clear
    #[inline]
    pub(super) fn bvc(&mut self) {
        if !self.status.intersects(Status::V) {
            self.branch();
        }
    }
    /// BVS: Branch on Overflow Set
    #[inline]
    pub(super) fn bvs(&mut self) {
        if self.status.intersects(Status::V) {
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
    #[inline]
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
    #[inline]
    pub(super) fn jsr(&mut self) {
        let _ = self.read(Self::SP_BASE | u16::from(self.sp), Access::Read); // Cycle 3
        self.push_u16(self.pc.wrapping_sub(1));
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
    #[inline]
    pub(super) fn rti(&mut self) {
        let _ = self.read(Self::SP_BASE | u16::from(self.sp), Access::Read); // Cycle 3
        self.status = Status::from_bits_truncate(self.pop()); // Cycle 4
        self.status &= !Status::U;
        self.status &= !Status::B;
        self.pc = self.pop_u16(); // Cycles 5 & 6
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
    #[inline]
    pub(super) fn rts(&mut self) {
        let _ = self.read(Self::SP_BASE | u16::from(self.sp), Access::Read); // Cycle 3
        self.pc = self.pop_u16().wrapping_add(1); // Cycles 4 & 5
        let _ = self.read(self.pc, Access::Read); // Cycle 6
    }

    ///  Register opcodes

    /// CLC: Clear Carry Flag
    #[inline]
    pub(super) fn clc(&mut self) {
        self.status.set(Status::C, false);
    }
    /// SEC: Set Carry Flag
    #[inline]
    pub(super) fn sec(&mut self) {
        self.status.set(Status::C, true);
    }
    /// CLD: Clear Decimal Mode
    #[inline]
    pub(super) fn cld(&mut self) {
        self.status.set(Status::D, false);
    }
    /// SED: Set Decimal Mode
    #[inline]
    pub(super) fn sed(&mut self) {
        self.status.set(Status::D, true);
    }
    /// CLI: Clear Interrupt Disable Bit
    #[inline]
    pub(super) fn cli(&mut self) {
        self.status.set(Status::I, false);
    }
    /// SEI: Set Interrupt Disable Status
    #[inline]
    pub(super) fn sei(&mut self) {
        self.status.set(Status::I, true);
    }
    /// CLV: Clear Overflow Flag
    #[inline]
    pub(super) fn clv(&mut self) {
        self.status.set(Status::V, false);
    }

    /// Compare opcodes

    /// Utility function used by all compare instructions
    #[inline]
    pub(super) fn compare(&mut self, a: u8, b: u8) {
        let result = a.wrapping_sub(b);
        self.set_zn_status(result);
        self.status.set(Status::C, a >= b);
    }
    /// CMP: Compare M and A
    #[inline]
    pub(super) fn cmp(&mut self) {
        self.fetch_data();
        self.compare(self.acc, self.fetched_data);
    }
    /// CPX: Compare M and X
    #[inline]
    pub(super) fn cpx(&mut self) {
        self.fetch_data();
        self.compare(self.x, self.fetched_data);
    }
    /// CPY: Compare M and Y
    #[inline]
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
    #[inline]
    pub(super) fn php(&mut self) {
        // Set U and B when pushing during PHP and BRK
        self.push((self.status | Status::U | Status::B).bits());
    }
    /// PLP: Pull Processor Status from Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  R  increment S
    //  4  $0100,S  R  pull register from stack
    #[inline]
    pub(super) fn plp(&mut self) {
        let _ = self.read(Self::SP_BASE | u16::from(self.sp), Access::Read); // Cycle 3
        self.status = Status::from_bits_truncate(self.pop());
    }
    /// PHA: Push A on Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  W  push register on stack, decrement S
    #[inline]
    pub(super) fn pha(&mut self) {
        self.push(self.acc);
    }
    /// PLA: Pull A from Stack
    //  #  address R/W description
    // --- ------- --- -----------------------------------------------
    //  1    PC     R  fetch opcode, increment PC
    //  2    PC     R  read next instruction byte (and throw it away)
    //  3  $0100,S  R  increment S
    //  4  $0100,S  R  pull register from stack
    #[inline]
    pub(super) fn pla(&mut self) {
        let _ = self.read(Self::SP_BASE | u16::from(self.sp), Access::Read); // Cycle 3
        self.acc = self.pop();
        self.set_zn_status(self.acc);
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
    #[inline]
    pub(super) fn brk(&mut self) {
        self.fetch_data(); // throw away
        self.push_u16(self.pc);

        // Pushing status to the stack has to happen after checking NMI since it can hijack the BRK
        // IRQ when it occurs between cycles 4 and 5.
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // Set U and B when pushing during PHP and BRK
        let status = (self.status | Status::U | Status::B).bits();

        if self.nmi {
            self.nmi = false;
            self.push(status);
            self.status.set(Status::I, true);

            self.pc = self.read_u16(Self::NMI_VECTOR);
            log::trace!("NMI: {}", self.cycle);
        } else {
            self.push(status);
            self.status.set(Status::I, true);

            self.pc = self.read_u16(Self::IRQ_VECTOR);
            log::trace!("IRQ: {}", self.cycle);
        }
        // Prevent NMI from triggering immediately after BRK
        log::trace!(
            "Suppress NMI after BRK: {}, {} -> false",
            self.cycle,
            self.prev_nmi,
        );
        self.prev_nmi = false;
    }
    /// NOP: No Operation
    #[inline]
    pub(super) fn nop(&mut self) {
        self.fetch_data(); // throw away
    }

    /// Unofficial opcodes

    /// SKB: Like NOP
    #[inline]
    pub(super) fn skb(&mut self) {
        self.fetch_data();
    }

    /// IGN: Like NOP, but can cross page boundary
    #[inline]
    pub(super) fn ign(&mut self) {
        self.fetch_data();
    }

    /// XXX: Captures all unimplemented opcodes
    #[inline]
    pub(super) fn xxx(&mut self) {
        self.corrupted = true;
        log::error!(
            "Invalid opcode ${:02X} {:?} #{:?} encountered!",
            self.instr.opcode(),
            self.instr.op(),
            self.instr.addr_mode(),
        );
    }
    /// ISC/ISB: Shortcut for INC then SBC
    #[inline]
    pub(super) fn isb(&mut self) {
        // INC
        self.fetch_data();
        self.write_fetched(self.fetched_data); // dummy write
        let val = self.fetched_data.wrapping_add(1);
        // SBC
        let a = self.acc;
        let (x1, o1) = a.overflowing_sub(val);
        let (x2, o2) = x1.overflowing_sub(1 - self.status_bit(Status::C));
        self.acc = x2;
        self.status.set(Status::C, !(o1 | o2));
        self.status.set(
            Status::V,
            (a ^ val) & 0x80 != 0 && (a ^ self.acc) & 0x80 != 0,
        );
        self.set_zn_status(self.acc);
        self.write_fetched(val);
    }
    /// DCP: Shortcut for DEC then CMP
    #[inline]
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
    #[inline]
    pub(super) fn axs(&mut self) {
        self.fetch_data();
        let t = u32::from(self.acc & self.x).wrapping_sub(u32::from(self.fetched_data));
        self.set_zn_status((t & 0xFF) as u8);
        self.status
            .set(Status::C, (((t >> 8) & 0x01) ^ 0x01) == 0x01);
        self.x = (t & 0xFF) as u8;
    }
    /// LAS: Shortcut for LDA then TSX
    #[inline]
    pub(super) fn las(&mut self) {
        self.lda();
        self.tsx();
    }
    /// LAX: Shortcut for LDA then TAX
    #[inline]
    pub(super) fn lax(&mut self) {
        self.lda();
        self.tax();
    }
    /// AHX/SHA/AXA: AND X with A then AND with 7, then store in memory
    #[inline]
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
    #[inline]
    pub(super) fn sax(&mut self) {
        if self.instr.addr_mode() == IDY {
            self.fetch_data();
        }
        let val = self.acc & self.x;
        self.write_fetched(val);
    }
    /// XAA: Unknown
    #[inline]
    pub(super) fn xaa(&mut self) {
        self.fetch_data();
        self.acc |= 0xEE;
        self.acc &= self.x;
        // AND
        self.acc &= self.fetched_data;
        self.set_zn_status(self.acc);
    }
    /// SXA/SHX/XAS: AND X with the high byte of the target address + 1
    #[inline]
    pub(super) fn sxa(&mut self) {
        let hi = (self.abs_addr >> 8) as u8;
        let lo = (self.abs_addr & 0xFF) as u8;
        let val = self.x & hi.wrapping_add(1);
        self.abs_addr = u16::from_le_bytes([lo, self.x & hi.wrapping_add(1)]);
        self.write_fetched(val);
    }
    /// SYA/SHY/SAY: AND Y with the high byte of the target address + 1
    #[inline]
    pub(super) fn sya(&mut self) {
        let hi = (self.abs_addr >> 8) as u8;
        let lo = (self.abs_addr & 0xFF) as u8;
        let val = self.y & hi.wrapping_add(1);
        self.abs_addr = u16::from_le_bytes([lo, self.y & hi.wrapping_add(1)]);
        self.write_fetched(val);
    }
    /// RRA: Shortcut for ROR then ADC
    #[inline]
    pub(super) fn rra(&mut self) {
        self.fetch_data();
        // ROR
        self.write_fetched(self.fetched_data); // dummy write
        let mut ret = self.fetched_data.rotate_right(1);
        if self.status.intersects(Status::C) {
            ret |= 1 << 7;
        } else {
            ret &= !(1 << 7);
        }
        self.status.set(Status::C, self.fetched_data & 1 > 0);
        // ADC
        let a = self.acc;
        let (x1, o1) = ret.overflowing_add(a);
        let (x2, o2) = x1.overflowing_add(self.status_bit(Status::C));
        self.acc = x2;
        self.status.set(Status::C, o1 | o2);
        self.status.set(
            Status::V,
            (a ^ ret) & 0x80 == 0 && (a ^ self.acc) & 0x80 != 0,
        );
        self.set_zn_status(self.acc);
        self.write_fetched(ret);
    }
    /// TAS: Shortcut for STA then TXS
    #[inline]
    pub(super) fn tas(&mut self) {
        // STA
        self.write(self.abs_addr, self.acc, Access::Write);
        // TXS
        self.sp = self.x;
    }
    /// ARR: Shortcut for AND #imm then ROR, but sets flags differently
    /// C is bit 6 and V is bit 6 xor bit 5
    #[inline]
    pub(super) fn arr(&mut self) {
        // AND
        self.fetch_data();
        self.acc &= self.fetched_data;
        // ROR
        self.status
            .set(Status::V, (self.acc ^ (self.acc >> 1)) & 0x40 == 0x40);
        let t = self.acc >> 7;
        self.acc >>= 1;
        self.acc |= self.status_bit(Status::C) << 7;
        self.status.set(Status::C, t & 0x01 == 0x01);
        self.set_zn_status(self.acc);
    }
    /// SRA: Shortcut for LSR then EOR
    #[inline]
    pub(super) fn sre(&mut self) {
        self.fetch_data();
        // LSR
        self.write_fetched(self.fetched_data); // dummy write
        self.status.set(Status::C, self.fetched_data & 1 > 0);
        let val = self.fetched_data.wrapping_shr(1);
        // EOR
        self.acc ^= val;
        self.set_zn_status(self.acc);
        self.write_fetched(val);
    }
    /// ALR/ASR: Shortcut for AND #imm then LSR
    #[inline]
    pub(super) fn alr(&mut self) {
        // AND
        self.fetch_data();
        self.acc &= self.fetched_data;
        // LSR
        self.status.set(Status::C, self.acc & 0x01 == 0x01);
        self.acc >>= 1;
        self.set_zn_status(self.acc);
    }
    /// RLA: Shortcut for ROL then AND
    #[inline]
    pub(super) fn rla(&mut self) {
        self.fetch_data();
        // ROL
        self.write_fetched(self.fetched_data); // dummy write
        let old_c = self.status_bit(Status::C);
        self.status.set(Status::C, (self.fetched_data >> 7) & 1 > 0);
        let val = (self.fetched_data << 1) | old_c;
        // AND
        self.acc &= val;
        self.set_zn_status(self.acc);
        self.write_fetched(val);
    }
    /// ANC/AAC: AND #imm but puts bit 7 into carry as if ASL was executed
    #[inline]
    pub(super) fn anc(&mut self) {
        // AND
        self.fetch_data();
        self.acc &= self.fetched_data;
        self.set_zn_status(self.acc);
        // Put bit 7 into carry
        self.status.set(Status::C, (self.acc >> 7) & 1 > 0);
    }
    /// SLO: Shortcut for ASL then ORA
    #[inline]
    pub(super) fn slo(&mut self) {
        self.fetch_data();
        // ASL
        self.write_fetched(self.fetched_data); // dummy write
        self.status.set(Status::C, (self.fetched_data >> 7) & 1 > 0);
        let val = self.fetched_data.wrapping_shl(1);
        self.write_fetched(val);
        // ORA
        self.acc |= val;
        self.set_zn_status(self.acc);
    }
}

impl std::fmt::Debug for Instr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
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
        write!(f, "{unofficial:1}{op:?}")
    }
}
