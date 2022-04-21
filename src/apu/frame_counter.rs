use crate::common::{Clocked, NesFormat, Powered};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FrameCounter {
    pub(crate) nes_format: NesFormat,
    pub(crate) step_cycles: [[u16; 6]; 2],
    pub(crate) cycles: u16,
    pub(crate) step: usize,
    pub(crate) mode: FcMode,
    pub(crate) write_buffer: Option<u8>,
    pub(crate) write_delay: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FcMode {
    Step4,
    Step5,
}

impl Default for FcMode {
    fn default() -> Self {
        Self::Step4
    }
}

impl FrameCounter {
    const STEP_CYCLES_NTSC: [[u16; 6]; 2] = [
        [7457, 7456, 7458, 7457, 1, 1],
        [7457, 7456, 7458, 7458, 7452, 1],
    ];
    const STEP_CYCLES_PAL: [[u16; 6]; 2] = [
        [8313, 8314, 8312, 8313, 1, 1],
        [8313, 8314, 8312, 8314, 8312, 1],
    ];

    pub(crate) fn new(nes_format: NesFormat) -> Self {
        let step_cycles = Self::step_cycles(nes_format);
        Self {
            nes_format,
            step_cycles,
            cycles: step_cycles[0][0],
            step: 0,
            mode: FcMode::Step4,
            write_buffer: None,
            write_delay: 0,
        }
    }

    #[inline]
    pub(crate) fn set_nes_format(&mut self, nes_format: NesFormat) {
        self.nes_format = nes_format;
        self.step_cycles = Self::step_cycles(nes_format);
    }

    #[inline]
    fn step_cycles(nes_format: NesFormat) -> [[u16; 6]; 2] {
        match nes_format {
            NesFormat::Ntsc | NesFormat::Dendy => Self::STEP_CYCLES_NTSC,
            NesFormat::Pal => Self::STEP_CYCLES_PAL,
        }
    }

    #[inline]
    pub(crate) fn update(&mut self) -> bool {
        if let Some(val) = self.write_buffer {
            self.write_delay -= 1;
            if self.write_delay == 0 {
                self.reload(val);
                self.write_buffer = None;
                return true;
            }
        }
        false
    }

    // On write to $4017
    pub(crate) fn write(&mut self, val: u8, cycle: usize) {
        self.write_buffer = Some(val);
        // Writes occurring on odd clocks are delayed
        self.write_delay = if cycle & 0x01 == 0x01 { 4 } else { 3 };
    }

    pub(crate) fn reload(&mut self, val: u8) {
        self.mode = if val & 0x80 == 0x80 {
            FcMode::Step5
        } else {
            FcMode::Step4
        };
        self.step = 0;
        self.cycles = self.step_cycles[self.mode as usize][self.step];

        // Clock Step5 immediately
        if self.mode == FcMode::Step5 {
            self.clock();
        }
    }
}

impl Clocked for FrameCounter {
    fn clock(&mut self) -> usize {
        if self.cycles > 0 {
            self.cycles -= 1;
        }
        if self.cycles == 0 {
            let clock = self.step;
            self.step += 1;
            if self.step > 5 {
                self.step = 0;
            }
            self.cycles = self.step_cycles[self.mode as usize][self.step];
            clock
        } else {
            0
        }
    }
}

impl Powered for FrameCounter {
    fn reset(&mut self) {
        self.step = 0;
        self.cycles = self.step_cycles[self.mode as usize][self.step];
        // After reset, APU acts as if $4017 was written 9-12 clocks before first instruction,
        // since reset takes 7 cycles, add 3 here
        self.write_buffer = Some(match self.mode {
            FcMode::Step4 => 0x00,
            FcMode::Step5 => 0x80,
        });
        self.write_delay = 3;
    }

    fn power_cycle(&mut self) {
        self.mode = FcMode::Step4;
        self.reset();
    }
}
