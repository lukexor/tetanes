/// Interrupt Types
pub enum Interrupt {
    None,
    NMI,
    IRQ,
}

/// The Central Processing Unit
pub struct CPU {
    pub cycles: u64,          // number of cycles
    pub pc: u16,              // program counter
    pub sp: u8,               // stack pointer - stack is at $0100-$01FF
    pub a: u8,                // accumulator
    pub x: u8,                // x register
    pub y: u8,                // y register
    pub c: u8,                // carry flag
    pub z: u8,                // zero flag
    pub i: u8,                // interrupt disable flag
    pub d: u8,                // decimal mode flag
    pub b: u8,                // break command flag
    pub u: u8,                // unused flag
    pub v: u8,                // overflow flag
    pub n: u8,                // negative flag
    pub interrupt: Interrupt, // interrupt type to perform
    pub stall: isize,         // number of cycles to stall
}

impl CPU {
    pub fn new() -> Self {
        CPU {
            cycles: 0,
            pc: 0,
            sp: 0,
            a: 0,
            x: 0,
            y: 0,
            c: 0,
            z: 0,
            i: 0,
            d: 0,
            b: 0,
            u: 0,
            v: 0,
            n: 0,
            interrupt: Interrupt::None,
            stall: 0,
        }
    }

    /// Flag functions

    pub fn flags(&self) -> u8 {
        let mut flags: u8 = 0;
        flags |= self.c;
        flags |= self.z << 1;
        flags |= self.i << 2;
        flags |= self.d << 3;
        flags |= self.b << 4;
        flags |= self.u << 5;
        flags |= self.v << 6;
        flags |= self.n << 7;
        flags
    }

    pub fn set_flags(&mut self, flags: u8) {
        self.c = flags & 1;
        self.z = (flags >> 1) & 1;
        self.i = (flags >> 2) & 1;
        self.d = (flags >> 3) & 1;
        self.b = (flags >> 4) & 1;
        self.u = (flags >> 5) & 1;
        self.v = (flags >> 6) & 1;
        self.n = (flags >> 7) & 1;
    }

    pub fn set_zn(&mut self, val: u8) {
        self.set_z(val);
        self.set_n(val);
    }

    /// Zero Flag - Gets set when val is 0
    pub fn set_z(&mut self, val: u8) {
        self.z = match val {
            0 => 1,
            _ => 0,
        };
    }

    /// Negative Flag - Gets set when val is negative
    pub fn set_n(&mut self, val: u8) {
        self.n = match val & 0x80 {
            0 => 0,
            _ => 1,
        };
    }
}

impl Default for CPU {
    fn default() -> Self {
        Self::new()
    }
}
