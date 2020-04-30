use super::{envelope::Envelope, length_counter::LengthCounter, sweep::Sweep};
use crate::{
    common::{Clocked, Powered},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum PulseChannel {
    One,
    Two,
}

#[derive(Clone)]
pub struct Pulse {
    pub enabled: bool,
    duty_cycle: u8,        // Select row in DUTY_TABLE
    duty_counter: u8,      // Select column in DUTY_TABLE
    freq_timer: u16,       // timer freq_counter reload value
    freq_counter: u16,     // Current frequency timer value
    channel: PulseChannel, // One or Two
    pub length: LengthCounter,
    envelope: Envelope,
    sweep: Sweep,
}

impl Pulse {
    const DUTY_TABLE: [[u8; 8]; 4] = [
        [0, 1, 0, 0, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 0, 0, 0],
        [0, 1, 1, 1, 1, 0, 0, 0],
        [1, 0, 0, 1, 1, 1, 1, 1],
    ];

    pub fn new(channel: PulseChannel) -> Self {
        Self {
            enabled: false,
            duty_cycle: 0u8,
            duty_counter: 0u8,
            freq_timer: 0u16,
            freq_counter: 0u16,
            channel,
            length: LengthCounter::new(),
            envelope: Envelope::new(),
            sweep: Sweep {
                enabled: false,
                reload: false,
                negate: false,
                timer: 0u8,
                counter: 0u8,
                shift: 0u8,
            },
        }
    }

    pub fn clock_quarter_frame(&mut self) {
        self.envelope.clock();
    }
    pub fn clock_half_frame(&mut self) {
        let sweep_forcing_silence = self.sweep_forcing_silence();
        let mut swp = &mut self.sweep;
        if swp.reload {
            swp.counter = swp.timer;
            swp.reload = false;
        } else if swp.counter > 0 {
            swp.counter -= 1;
        } else {
            swp.counter = swp.timer;
            if swp.enabled && !sweep_forcing_silence {
                let delta = self.freq_timer >> swp.shift;
                if swp.negate {
                    self.freq_timer -= delta + 1;
                    if self.channel == PulseChannel::One {
                        self.freq_timer += 1;
                    }
                } else {
                    self.freq_timer += delta
                }
            }
        }

        self.length.clock();
    }

    pub fn sweep_forcing_silence(&self) -> bool {
        let next_freq = self.freq_timer + (self.freq_timer >> self.sweep.shift);
        self.freq_timer < 8 || (!self.sweep.negate && next_freq >= 0x800)
    }

    pub fn output(&self) -> f32 {
        if Self::DUTY_TABLE[self.duty_cycle as usize][self.duty_counter as usize] != 0
            && self.length.counter != 0
            && !self.sweep_forcing_silence()
        {
            if self.envelope.enabled {
                f32::from(self.envelope.volume)
            } else {
                f32::from(self.envelope.constant_volume)
            }
        } else {
            0f32
        }
    }

    // $4000 Pulse control
    pub fn write_control(&mut self, val: u8) {
        self.duty_cycle = (val >> 6) & 0x03; // D7..D6
        self.length.write_control(val);
        self.envelope.write_control(val);
    }
    // $4001 Pulse sweep
    pub fn write_sweep(&mut self, val: u8) {
        self.sweep.timer = (val >> 4) & 0x07; // D6..D4
        self.sweep.negate = (val >> 3) & 1 == 1; // D3
        self.sweep.shift = val & 0x07; // D2..D0
        self.sweep.enabled = ((val >> 7) & 1 == 1) && (self.sweep.shift != 0); // D7
        self.sweep.reload = true;
    }
    // $4002 Pulse timer lo
    pub fn write_timer_lo(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0xFF00) | u16::from(val); // D7..D0
    }
    // $4003 Pulse timer hi
    pub fn write_timer_hi(&mut self, val: u8) {
        self.freq_timer = (self.freq_timer & 0x00FF) | u16::from(val & 0x07) << 8; // D2..D0
        self.freq_counter = self.freq_timer;
        self.duty_counter = 0;
        self.envelope.reset = true;
        if self.enabled {
            self.length.load_value(val);
        }
    }
}

impl Clocked for Pulse {
    fn clock(&mut self) -> usize {
        if self.freq_counter > 0 {
            self.freq_counter -= 1;
        } else {
            self.freq_counter = self.freq_timer;
            self.duty_counter = (self.duty_counter + 1) % 8;
        }
        1
    }
}

impl Powered for Pulse {
    fn reset(&mut self) {
        *self = Self::new(self.channel);
    }
}

impl Savable for Pulse {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.enabled.save(fh)?;
        self.duty_cycle.save(fh)?;
        self.duty_counter.save(fh)?;
        self.freq_timer.save(fh)?;
        self.freq_counter.save(fh)?;
        self.channel.save(fh)?;
        self.length.save(fh)?;
        self.envelope.save(fh)?;
        self.sweep.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.enabled.load(fh)?;
        self.duty_cycle.load(fh)?;
        self.duty_counter.load(fh)?;
        self.freq_timer.load(fh)?;
        self.freq_counter.load(fh)?;
        self.channel.load(fh)?;
        self.length.load(fh)?;
        self.envelope.load(fh)?;
        self.sweep.load(fh)?;
        Ok(())
    }
}

impl Savable for PulseChannel {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        (*self as u8).save(fh)
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        let mut val = 0u8;
        val.load(fh)?;
        *self = match val {
            0 => PulseChannel::One,
            1 => PulseChannel::Two,
            _ => panic!("invalid PulseChannel value"),
        };
        Ok(())
    }
}

impl Default for Pulse {
    fn default() -> Self {
        Self::new(PulseChannel::One)
    }
}
