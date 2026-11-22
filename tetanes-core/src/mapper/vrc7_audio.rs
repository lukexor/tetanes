//! The `VRC7`'s FM synthesizer, a six-channel cut-down Yamaha YM2413 (OPLL).
//!
//! <https://www.nesdev.org/wiki/VRC7_audio>

// Synth register state, whose meaning is the chip's rather than this crate's.
#![allow(missing_docs)]

use crate::common::ResetKind;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// Attenuation inside the synth is in units of 1/256 of an octave (about 0.0235 dB), the unit of
// the log-sine and exponent tables. The chip's own steps convert to it as below.

/// One envelope step, 0.375 dB.
const ENV_UNIT: u32 = 16;
/// One step of the modulator's total level, 0.75 dB.
const TL_UNIT: u32 = 32;
/// One step of a channel's volume, 3 dB.
const VOLUME_UNIT: u32 = 128;
/// One step of key scaling, 0.1875 dB.
const KSL_UNIT: u32 = 8;

/// The envelope's quietest level. Seven bits, 48 dB below full.
const ENV_SILENT: u8 = 127;
/// The level at which a damped operator restarts its attack.
const DAMP_END: u8 = 124;

/// CPU cycles per output sample. The chip runs from its own 3.58 MHz oscillator, twice the NTSC
/// CPU clock, and takes 72 of its clocks to update all six channels.
const SAMPLE_PERIOD: u8 = 36;

/// Samples per tremolo cycle, about 3.7 Hz.
const AM_PERIOD: u16 = 13_440;
/// Tremolo steps per cycle. The level climbs 0 to 13 envelope steps (4.875 dB) and falls back.
const AM_STEPS: u32 = 28;
/// Each vibrato step lasts `1 << PM_SHIFT` samples, and there are eight per cycle, about 6 Hz.
const PM_SHIFT: u16 = 10;

/// Output scale. One channel at full volume comes out near one APU pulse channel at full volume.
const GAIN: f32 = 0.20 / 4096.0;

/// The phase step for each multiplier value, in half-steps so that `$0` can be 1/2.
const MULTIPLIER: [u32; 16] = [1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30];

/// Key-scale attenuation at block 7 by the top four bits of the frequency, in 0.75 dB steps.
const KSL_ROM: [u32; 16] = [
    0, 32, 40, 45, 48, 51, 53, 55, 56, 58, 59, 60, 61, 62, 63, 64,
];

/// The 15 built-in instruments, as dumped from the chip's ROM. Instrument 0 is the custom patch
/// at registers `$00-$07`.
const PATCHES: [[u8; 8]; 15] = [
    [0x03, 0x21, 0x05, 0x06, 0xE8, 0x81, 0x42, 0x27], // Buzzy bell
    [0x13, 0x41, 0x14, 0x0D, 0xD8, 0xF6, 0x23, 0x12], // Guitar
    [0x11, 0x11, 0x08, 0x08, 0xFA, 0xB2, 0x20, 0x12], // Wurly
    [0x31, 0x61, 0x0C, 0x07, 0xA8, 0x64, 0x61, 0x27], // Flute
    [0x32, 0x21, 0x1E, 0x06, 0xE1, 0x76, 0x01, 0x28], // Clarinet
    [0x02, 0x01, 0x06, 0x00, 0xA3, 0xE2, 0xF4, 0xF4], // Synth
    [0x21, 0x61, 0x1D, 0x07, 0x82, 0x81, 0x11, 0x07], // Trumpet
    [0x23, 0x21, 0x22, 0x17, 0xA2, 0x72, 0x01, 0x17], // Organ
    [0x35, 0x11, 0x25, 0x00, 0x40, 0x73, 0x72, 0x01], // Bells
    [0xB5, 0x01, 0x0F, 0x0F, 0xA8, 0xA5, 0x51, 0x02], // Vibes
    [0x17, 0xC1, 0x24, 0x07, 0xF8, 0xF8, 0x22, 0x12], // Vibraphone
    [0x71, 0x23, 0x11, 0x06, 0x65, 0x74, 0x18, 0x16], // Tutti
    [0x01, 0x02, 0xD3, 0x05, 0xC9, 0x95, 0x03, 0x02], // Fretless
    [0x61, 0x63, 0x0C, 0x00, 0x94, 0xC0, 0x33, 0xF6], // Synth bass
    [0x21, 0x72, 0x0D, 0x00, 0xC1, 0xD5, 0x56, 0x06], // Sweep
];

/// The quarter sine as attenuation, and attenuation back to a linear level.
struct Tables {
    /// `-log2(sin(x))` over the first quarter wave, in attenuation units.
    log_sin: [u16; 256],
    /// `4096 * 2^(-i / 256)`, the linear level of the fractional part of an attenuation.
    exp: [u16; 256],
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut log_sin = [0; 256];
    let mut exp = [0; 256];
    for i in 0..256 {
        let x = (i as f64 + 0.5) * std::f64::consts::PI / 512.0;
        log_sin[i] = (-x.sin().log2() * 256.0).round() as u16;
        exp[i] = (4096.0 * (-(i as f64) / 256.0).exp2()).round() as u16;
    }
    Tables { log_sin, exp }
});

/// One operator's output for a 10-bit wave position at `att` attenuation. A half-wave operator
/// is silent for the negative half of the sine.
fn wave(index: i32, att: u32, half_wave: bool) -> i32 {
    let index = (index & 0x3FF) as usize;
    let negative = index & 0x200 != 0;
    if negative && half_wave {
        return 0;
    }
    let quarter = if index & 0x100 == 0 {
        index & 0xFF
    } else {
        0xFF - (index & 0xFF)
    };
    let tables = &*TABLES;
    let total = u32::from(tables.log_sin[quarter]) + att;
    // Past 13 octaves the level is under one, and the shift below would only find zero.
    let level = if total >= 13 * 256 {
        0
    } else {
        i32::from(tables.exp[(total & 0xFF) as usize] >> (total >> 8))
    };
    if negative { -level } else { level }
}

/// The patch fields one operator reads.
#[derive(Debug, Copy, Clone)]
struct OperatorPatch {
    tremolo: bool,
    vibrato: bool,
    /// Holds at the sustain level while the key is down. Clear, the envelope keeps falling at
    /// the release rate.
    sustained: bool,
    /// Scales envelope rates by the full key code instead of its top two bits.
    key_scale_rate: bool,
    multiplier: usize,
    key_scale_level: u8,
    half_wave: bool,
    attack: u8,
    decay: u8,
    sustain_level: u8,
    release: u8,
}

impl OperatorPatch {
    /// Reads operator `op` (0 for the modulator, 1 for the carrier) out of an 8-byte patch.
    const fn new(patch: &[u8; 8], op: usize) -> Self {
        let flags = patch[op];
        Self {
            tremolo: flags & 0x80 != 0,
            vibrato: flags & 0x40 != 0,
            sustained: flags & 0x20 != 0,
            key_scale_rate: flags & 0x10 != 0,
            multiplier: (flags & 0x0F) as usize,
            key_scale_level: patch[2 + op] >> 6,
            // Byte 3 bit 3 rectifies the modulator, bit 4 the carrier.
            half_wave: patch[3] & (0x08 << op) != 0,
            attack: patch[4 + op] >> 4,
            decay: patch[4 + op] & 0x0F,
            sustain_level: patch[6 + op] >> 4,
            release: patch[6 + op] & 0x0F,
        }
    }
}

/// Where an operator's envelope is.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// Falling quickly to silence after a key-on, so the new note starts from zero.
    Damp,
    Attack,
    Decay,
    Sustain,
    #[default]
    Release,
}

/// One of a channel's two operators.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Operator {
    /// The 19-bit phase accumulator. Its top 10 bits are the wave position.
    pub phase: u32,
    /// Envelope attenuation in 0.375 dB steps, 0 loudest.
    pub env: u8,
    /// The envelope's current stage.
    pub stage: Stage,
    /// Fractional envelope steps, 14 bits.
    pub env_frac: u32,
    /// The last two outputs, which feed back into the modulator.
    pub out: [i16; 2],
}

impl Default for Operator {
    fn default() -> Self {
        Self {
            phase: 0,
            env: ENV_SILENT,
            stage: Stage::Release,
            env_frac: 0,
            out: [0; 2],
        }
    }
}

impl Operator {
    const fn step_phase(&mut self, patch: &OperatorPatch, fnum: u32, block: u32, pm_step: usize) {
        let pm = if patch.vibrato {
            // The vibrato depth scales with the frequency's top three bits.
            let depth = (fnum >> 6) as i32;
            let half = depth >> 1;
            [0, half, depth, half, 0, -half, -depth, -half][pm_step]
        } else {
            0
        };
        let fnum2 = (fnum * 2).saturating_add_signed(pm);
        let step = ((fnum2 * MULTIPLIER[patch.multiplier]) << block) >> 2;
        self.phase = (self.phase + step) & 0x7_FFFF;
    }

    fn step_envelope(&mut self, patch: &OperatorPatch, key_code: u32, channel_sustain: bool) {
        if self.stage == Stage::Damp && self.env >= DAMP_END {
            self.phase = 0;
            self.stage = Stage::Attack;
        }
        let rate = match self.stage {
            Stage::Damp => 12,
            Stage::Attack => patch.attack,
            Stage::Decay => patch.decay,
            Stage::Sustain if patch.sustained => 0,
            Stage::Sustain => patch.release,
            Stage::Release if channel_sustain => 5,
            Stage::Release if patch.sustained => patch.release,
            Stage::Release => 7,
        };
        if rate == 0 {
            return;
        }
        let scale = if patch.key_scale_rate {
            key_code
        } else {
            key_code >> 2
        };
        let rate = (u32::from(rate) * 4 + scale).min(63);
        if self.stage == Stage::Attack && rate >= 60 {
            self.env = 0;
            self.stage = Stage::Decay;
            return;
        }

        // Each step of the top four rate bits doubles the speed, and the low two add quarters.
        // Rate 48 moves one step per sample.
        self.env_frac += (4 + (rate & 0x03)) << (rate >> 2);
        let steps = self.env_frac >> 14;
        self.env_frac &= 0x3FFF;
        if steps == 0 {
            return;
        }
        if self.stage == Stage::Attack {
            // The attack is exponential, closing a quarter of the remaining distance per step.
            for _ in 0..steps {
                self.env = self.env.saturating_sub((self.env >> 2) + 1);
            }
            if self.env == 0 {
                self.stage = Stage::Decay;
            }
            return;
        }
        self.env = (u32::from(self.env) + steps).min(u32::from(ENV_SILENT)) as u8;
        if self.stage == Stage::Decay && self.env >= patch.sustain_level << 3 {
            self.stage = Stage::Sustain;
        }
    }

    /// Total attenuation for this sample: envelope, level, key scaling and tremolo.
    fn attenuation(&self, patch: &OperatorPatch, level: u32, ksl: u32, am: u32) -> u32 {
        let ksl = match patch.key_scale_level {
            0 => 0,
            // 1.5, 3 and 6 dB per octave.
            n => (ksl >> [0, 2, 1, 0][usize::from(n)]) * KSL_UNIT,
        };
        let am = if patch.tremolo { am * ENV_UNIT } else { 0 };
        u32::from(self.env) * ENV_UNIT + level + ksl + am
    }
}

/// One FM channel, a modulator feeding a carrier.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Channel {
    /// `$1x` and `$2x` bit 0: the 9-bit frequency.
    pub fnum: u16,
    /// `$2x` bits 1-3: the octave.
    pub block: u8,
    /// `$2x` bit 4: the key.
    pub key: bool,
    /// `$2x` bit 5: releases at rate 5 instead of the patch's rate.
    pub sustain: bool,
    /// `$3x` bits 4-7.
    pub instrument: u8,
    /// `$3x` bits 0-3: attenuation in 3 dB steps, 0 loudest.
    pub volume: u8,
    /// The operator whose output bends the carrier's phase.
    pub modulator: Operator,
    /// The operator the channel outputs.
    pub carrier: Operator,
}

impl Channel {
    const fn key_on(&mut self) {
        self.modulator.stage = Stage::Damp;
        self.carrier.stage = Stage::Damp;
    }

    const fn key_off(&mut self, patch: &[u8; 8]) {
        self.carrier.stage = Stage::Release;
        // A sustained modulator keeps sustaining while the carrier releases.
        if !OperatorPatch::new(patch, 0).sustained {
            self.modulator.stage = Stage::Release;
        }
    }

    fn sample(&mut self, patch: &[u8; 8], am: u32, pm_step: usize) -> i32 {
        let modulator = OperatorPatch::new(patch, 0);
        let carrier = OperatorPatch::new(patch, 1);
        let fnum = u32::from(self.fnum);
        let block = u32::from(self.block);
        let key_code = (block << 1) | (fnum >> 8);
        let ksl = (KSL_ROM[(fnum >> 5) as usize] * 4).saturating_sub((8 - block) << 5);

        self.modulator.step_phase(&modulator, fnum, block, pm_step);
        self.carrier.step_phase(&carrier, fnum, block, pm_step);
        self.modulator
            .step_envelope(&modulator, key_code, self.sustain);
        self.carrier.step_envelope(&carrier, key_code, self.sustain);

        let feedback = match patch[3] & 0x07 {
            0 => 0,
            // 7 is a modulation index of 4π, and each step down halves it.
            fb => (i32::from(self.modulator.out[0]) + i32::from(self.modulator.out[1])) >> (9 - fb),
        };
        let level = u32::from(patch[2] & 0x3F) * TL_UNIT;
        let att = self.modulator.attenuation(&modulator, level, ksl, am);
        let index = (self.modulator.phase >> 9) as i32 + feedback;
        let modulation = wave(index, att, modulator.half_wave);
        self.modulator.out = [modulation as i16, self.modulator.out[0]];

        let level = u32::from(self.volume) * VOLUME_UNIT;
        let att = self.carrier.attenuation(&carrier, level, ksl, am);
        let index = (self.carrier.phase >> 9) as i32 + modulation;
        wave(index, att, carrier.half_wave)
    }
}

/// The synth behind `$9010` (register select) and `$9030` (register data).
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Audio {
    /// `$00-$07`: the custom instrument.
    pub custom: [u8; 8],
    /// `$10-$35`: the six FM channels.
    pub channels: [Channel; 6],
    /// The register `$9030` writes to.
    pub address: u8,
    /// CPU cycles since the last sample.
    pub cycle: u8,
    /// Samples into the tremolo cycle.
    pub am_counter: u16,
    /// Samples into the vibrato cycle.
    pub pm_counter: u16,
    /// `$E000` bit 6: the chip is held in reset, silent and ignoring writes.
    pub silenced: bool,
    /// The last sample's mix, scaled for the APU mixer.
    pub out: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            custom: [0; 8],
            channels: [Channel::default(); 6],
            address: 0,
            cycle: 0,
            am_counter: 0,
            pm_counter: 0,
            silenced: false,
            out: 0.0,
        }
    }
}

impl Audio {
    /// The current output level, as mixed into the APU.
    #[must_use]
    pub const fn output(&self) -> f32 {
        self.out
    }

    /// `$9010`: selects the register the next `$9030` write goes to.
    pub const fn write_address(&mut self, val: u8) {
        if !self.silenced {
            self.address = val;
        }
    }

    /// `$9030`: writes the selected register. A key bit going 0 to 1 starts a note, and 1 to 0
    /// releases it.
    pub fn write_data(&mut self, val: u8) {
        if self.silenced {
            return;
        }
        let reg = self.address;
        let ch = usize::from(reg & 0x0F);
        match reg {
            0x00..=0x07 => self.custom[usize::from(reg)] = val,
            0x10..=0x15 => {
                let channel = &mut self.channels[ch];
                channel.fnum = (channel.fnum & 0x100) | u16::from(val);
            }
            0x20..=0x25 => {
                let patch = self.patch(self.channels[ch].instrument);
                let channel = &mut self.channels[ch];
                channel.fnum = (channel.fnum & 0xFF) | (u16::from(val & 0x01) << 8);
                channel.block = (val >> 1) & 0x07;
                channel.sustain = val & 0x20 != 0;
                let key = val & 0x10 != 0;
                match (channel.key, key) {
                    (false, true) => channel.key_on(),
                    (true, false) => channel.key_off(&patch),
                    _ => (),
                }
                channel.key = key;
            }
            0x30..=0x35 => {
                let channel = &mut self.channels[ch];
                channel.instrument = val >> 4;
                channel.volume = val & 0x0F;
            }
            // $0E (rhythm) drives channels the VRC7 doesn't have, and $0F is a test register.
            _ => (),
        }
    }

    /// `$E000` bit 6. Setting it clears every register and silences the chip until it is cleared.
    pub fn set_silenced(&mut self, silenced: bool) {
        if silenced {
            *self = Self {
                silenced,
                ..Self::default()
            };
        } else {
            self.silenced = false;
        }
    }

    /// Advances one CPU cycle, producing a sample every 36 cycles.
    pub fn clock(&mut self) {
        if self.silenced {
            return;
        }
        self.cycle += 1;
        if self.cycle == SAMPLE_PERIOD {
            self.cycle = 0;
            self.sample();
        }
    }

    /// A hard reset clears the chip. A soft reset leaves it, since the console's reset line does
    /// not reach it.
    pub fn reset(&mut self, kind: ResetKind) {
        if kind == ResetKind::Hard {
            *self = Self::default();
        }
    }

    const fn patch(&self, instrument: u8) -> [u8; 8] {
        match instrument {
            0 => self.custom,
            n => PATCHES[n as usize - 1],
        }
    }

    /// Advances every channel one sample and mixes them.
    fn sample(&mut self) {
        self.am_counter = (self.am_counter + 1) % AM_PERIOD;
        self.pm_counter = self.pm_counter.wrapping_add(1);
        let step = u32::from(self.am_counter) * AM_STEPS / u32::from(AM_PERIOD);
        let am = if step < AM_STEPS / 2 {
            step
        } else {
            AM_STEPS - 1 - step
        };
        let pm_step = usize::from((self.pm_counter >> PM_SHIFT) & 0x07);

        let mut mix = 0;
        for ch in 0..self.channels.len() {
            let patch = self.patch(self.channels[ch].instrument);
            mix += self.channels[ch].sample(&patch, am, pm_step);
        }
        self.out = mix as f32 * GAIN;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain sine on the carrier: the modulator at its lowest level, instant attack, no decay,
    /// and the fastest release.
    const SINE: [u8; 8] = [0x20, 0x21, 0x3F, 0x00, 0xF0, 0xF0, 0x0F, 0x0F];

    fn write(audio: &mut Audio, reg: u8, val: u8) {
        audio.write_address(reg);
        audio.write_data(val);
    }

    /// Runs `samples` samples and returns each one's output.
    fn run(audio: &mut Audio, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|_| {
                for _ in 0..SAMPLE_PERIOD {
                    audio.clock();
                }
                audio.output()
            })
            .collect()
    }

    fn play(audio: &mut Audio, fnum: u16, block: u8) {
        for (reg, val) in SINE.into_iter().enumerate() {
            write(audio, reg as u8, val);
        }
        write(audio, 0x30, 0x00);
        write(audio, 0x10, fnum as u8);
        write(audio, 0x20, 0x10 | (block << 1) | (fnum >> 8) as u8);
    }

    #[test]
    fn wave_table() {
        assert_eq!(wave(0x100, 0, false), 4096, "peak of the sine");
        assert!(wave(0x300, 0, false) < -4000, "trough");
        assert_eq!(wave(0x300, 0, true), 0, "half-wave drops the negative half");
        assert_eq!(wave(0x100, 256, false), 2048, "256 units is half the level");
    }

    /// The pitch is `49716 * fnum / 2^(19 - block)` Hz.
    #[test]
    fn frequency() {
        let mut audio = Audio::default();
        // 256 at block 4 is about 388.4 Hz.
        play(&mut audio, 256, 4);
        let out = run(&mut audio, 49_716);
        let rising = out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!(
            (387..=390).contains(&rising),
            "{rising} cycles in one second"
        );
    }

    #[test]
    fn key_off_releases_to_silence() {
        let mut audio = Audio::default();
        play(&mut audio, 256, 4);
        let peak = run(&mut audio, 1000).into_iter().fold(0.0, f32::max);
        assert!(peak > 0.1, "sounding at full volume: {peak}");
        write(&mut audio, 0x20, 0x08);
        run(&mut audio, 1000);
        let tail = run(&mut audio, 200)
            .into_iter()
            .fold(0.0, |m, s| f32::max(m, s.abs()));
        assert!(tail < 0.001, "silent after release: {tail}");
    }

    /// Volume steps are 3 dB, so four steps is 12 dB, a quarter of the amplitude.
    #[test]
    fn volume() {
        let mut audio = Audio::default();
        play(&mut audio, 256, 4);
        let loud = run(&mut audio, 2000).into_iter().fold(0.0, f32::max);
        write(&mut audio, 0x30, 0x04);
        let quiet = run(&mut audio, 2000).into_iter().fold(0.0, f32::max);
        let ratio = loud / quiet;
        assert!((3.8..=4.2).contains(&ratio), "ratio {ratio}");
    }

    #[test]
    fn silenced_ignores_writes() {
        let mut audio = Audio::default();
        play(&mut audio, 256, 4);
        run(&mut audio, 100);
        audio.set_silenced(true);
        assert_eq!(audio.output(), 0.0);
        play(&mut audio, 256, 4);
        assert!(!audio.channels[0].key, "write ignored while silenced");
        audio.set_silenced(false);
        play(&mut audio, 256, 4);
        assert!(run(&mut audio, 1000).into_iter().fold(0.0, f32::max) > 0.1);
    }
}
