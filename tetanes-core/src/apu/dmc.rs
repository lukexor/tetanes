//! APU DMC (Delta Modulation Channel) implementation.
//!
//! See: <https://www.nesdev.org/wiki/APU_DMC>

use crate::{
    apu::timer::{Timer, TimerCycle},
    common::{Clock, NesRegion, Regional, Reset, ResetKind, Sample},
    cpu::{Cpu, Irq},
};
use serde::{Deserialize, Serialize};
use tracing::trace;

/// APU DMC (Delta Modulation Channel) provides sample playback.
///
/// See: <https://www.nesdev.org/wiki/APU_DMC>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Dmc {
    pub region: NesRegion,
    pub timer: Timer,
    pub force_silent: bool,
    pub irq_enabled: bool,
    pub loops: bool,
    pub addr: u16,
    pub sample_addr: u16,
    pub bytes_remaining: u16,
    pub sample_length: u16,
    pub sample_buffer: u8,
    pub buffer_empty: bool,
    pub init: u8,
    pub output_level: u8,
    pub bits_remaining: u8,
    pub shift: u8,
    pub silence: bool,
    pub should_clock: bool,
}

impl Default for Dmc {
    fn default() -> Self {
        Self::new(NesRegion::Ntsc)
    }
}

impl Dmc {
    const PERIOD_TABLE_NTSC: [usize; 16] = [
        428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
    ];
    const PERIOD_TABLE_PAL: [usize; 16] = [
        398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
    ];

    pub const fn new(region: NesRegion) -> Self {
        Self {
            region,
            timer: Timer::preload(Self::period(region, 0)),
            force_silent: false,
            irq_enabled: false,
            loops: false,
            addr: 0xC000,
            sample_addr: 0x0000,
            bytes_remaining: 0x0000,
            sample_length: 0x0001,
            sample_buffer: 0x00,
            buffer_empty: true,
            init: 0,
            output_level: 0x00,
            bits_remaining: 0x08,
            shift: 0x00,
            silence: true,
            should_clock: false,
        }
    }

    #[must_use]
    pub const fn silent(&self) -> bool {
        self.force_silent
    }

    pub const fn set_silent(&mut self, silent: bool) {
        self.force_silent = silent;
    }

    #[must_use]
    pub fn irq_pending_in(&self, cycles_to_run: usize) -> bool {
        if self.irq_enabled && self.bytes_remaining > 0 {
            let cycles_to_empty = (usize::from(self.bits_remaining)
                + usize::from(self.bytes_remaining - 1) * 8)
                * self.timer.period;
            cycles_to_run >= cycles_to_empty
        } else {
            false
        }
    }

    #[must_use]
    pub const fn dma_addr(&self) -> u16 {
        self.addr
    }

    fn init_sample(&mut self) {
        self.addr = self.sample_addr;
        self.bytes_remaining = self.sample_length;
        trace!(
            "APU DMC sample started. bytes remaining: {}",
            self.bytes_remaining
        );
        self.should_clock = self.bytes_remaining > 0;
    }

    pub fn load_buffer(&mut self, val: u8) {
        if self.bytes_remaining > 0 {
            self.sample_buffer = val;
            self.buffer_empty = false;
            if self.addr == 0xFFFF {
                self.addr = 0x8000;
            } else {
                self.addr += 1;
            }
            self.bytes_remaining -= 1;
            trace!("APU DMC bytes remaining: {}", self.bytes_remaining);
            if self.bytes_remaining == 0 {
                self.should_clock = false;
                if self.loops {
                    self.init_sample();
                } else if self.irq_enabled {
                    Cpu::set_irq(Irq::DMC);
                }
            }
        }
    }

    const fn period(region: NesRegion, val: u8) -> usize {
        let index = (val & 0x0F) as usize;
        match region {
            NesRegion::Auto | NesRegion::Ntsc | NesRegion::Dendy => {
                Self::PERIOD_TABLE_NTSC[index] - 1
            }
            NesRegion::Pal => Self::PERIOD_TABLE_PAL[index] - 1,
        }
    }

    /// $4010 DMC timer
    pub fn write_timer(&mut self, val: u8) {
        self.irq_enabled = val & 0x80 == 0x80;
        self.loops = val & 0x40 == 0x40;
        self.timer.period = Self::period(self.region, val);
        if !self.irq_enabled {
            Cpu::clear_irq(Irq::DMC);
        }
    }

    /// $4011 DMC output
    pub const fn write_output(&mut self, val: u8) {
        self.output_level = val & 0x7F;
    }

    /// $4012 DMC addr load
    pub fn write_addr(&mut self, val: u8) {
        self.sample_addr = 0xC000 | (u16::from(val) << 6);
    }

    /// $4013 DMC length
    pub fn write_length(&mut self, val: u8) {
        self.sample_length = (u16::from(val) << 4) | 1;
    }

    /// $4015 WRITE
    pub fn set_enabled(&mut self, enabled: bool, cycle: usize) {
        if !enabled {
            self.bytes_remaining = 0;
            self.should_clock = false;
        } else if self.bytes_remaining == 0 {
            self.init_sample();
            // Delay a number of cycles based on even/odd cycle
            self.init = if cycle & 0x01 == 0x00 { 2 } else { 3 };
        }
    }

    pub fn should_clock(&mut self) -> bool {
        if self.init > 0 {
            self.init -= 1;
            if self.init == 0 && self.buffer_empty && self.bytes_remaining > 0 {
                trace!("APU DMC DMA pending");
                Cpu::start_dmc_dma();
            }
        }
        self.should_clock
    }
}

impl Sample for Dmc {
    fn output(&self) -> f32 {
        if self.silent() {
            0.0
        } else {
            f32::from(self.output_level)
        }
    }
}

impl TimerCycle for Dmc {
    fn cycle(&self) -> usize {
        self.timer.cycle
    }
}

impl Clock for Dmc {
    //                          Timer
    //                            |
    //                            v
    // Reader ---> Buffer ---> Shifter ---> Output level ---> (to the mixer)
    fn clock(&mut self) -> usize {
        if self.timer.clock() > 0 {
            if !self.silence {
                // Update output level but clamp to 0..=127 range
                if self.shift & 0x01 == 0x01 {
                    if self.output_level <= 125 {
                        self.output_level += 2;
                    }
                } else if self.output_level >= 2 {
                    self.output_level -= 2;
                }
                self.shift >>= 1;
            }

            if self.bits_remaining > 0 {
                self.bits_remaining -= 1;
            }
            trace!("APU DMC bits remaining: {}", self.bits_remaining);

            if self.bits_remaining == 0 {
                self.bits_remaining = 8;
                self.silence = self.buffer_empty;
                if !self.buffer_empty {
                    self.shift = self.sample_buffer;
                    self.buffer_empty = true;
                    if self.bytes_remaining > 0 {
                        trace!("APU DMC DMA pending");
                        Cpu::start_dmc_dma();
                    }
                }
            }
            1
        } else {
            0
        }
    }
}

impl Regional for Dmc {
    fn region(&self) -> NesRegion {
        self.region
    }

    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.timer.period = Self::period(region, 0);
    }
}

impl Reset for Dmc {
    fn reset(&mut self, kind: ResetKind) {
        self.timer.reset(kind);
        self.timer.period = Self::period(self.region, 0);
        self.timer.reload();
        self.timer.cycle += 1; // FIXME: Startup timing is slightly wrong, DMA tests fail with the
        // default
        if let ResetKind::Hard = kind {
            self.sample_addr = 0xC000;
            self.sample_length = 1;
        }
        self.irq_enabled = false;
        self.loops = false;
        self.addr = 0x0000;
        self.bytes_remaining = 0;
        self.sample_buffer = 0x00;
        self.buffer_empty = true;
        self.output_level = 0x00;
        self.bits_remaining = 0x08;
        self.shift = 0x00;
        self.silence = true;
        self.should_clock = false;
    }
}
