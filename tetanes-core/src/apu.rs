//! NES APU (Audio Processing Unit) implementation.
//!
//! See: <https://www.nesdev.org/wiki/APU>
//!
//! # Stability
//!
//! [`Apu`]'s fields, and the per-channel types in this module's submodules, are the emulation's
//! internal wiring. They are public so that embedders and debuggers can read them, but they track
//! the implementation rather than the crate version, and a release may add, rename or retype any
//! of them. The stable entry point is [`ControlDeck`](crate::control_deck::ControlDeck).

use crate::{
    apu::{
        band_limited::BandLimited,
        dmc::Dmc,
        filter::FilterChain,
        frame_counter::{FrameCounter, FrameType},
        noise::Noise,
        pulse::{OutputFreq, Pulse, PulseChannel},
        triangle::Triangle,
    },
    common::{NesRegion, ResetKind},
    cpu::Cpu,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::trace;

pub mod dmc;
pub mod noise;
pub mod pulse;
pub mod triangle;

pub mod band_limited;
pub mod envelope;
pub mod filter;
pub mod frame_counter;
pub mod length_counter;
pub mod timer;

/// Error when parsing `Channel` from a `usize`.
#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `Channel`")]
pub struct ParseChannelError;

/// [`Apu`] Channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Channel {
    /// The first square-wave channel.
    Pulse1,
    /// The second square-wave channel.
    Pulse2,
    /// The triangle-wave channel.
    Triangle,
    /// The noise channel.
    Noise,
    /// The sample channel.
    Dmc,
    /// Expansion audio from the cartridge, if the board has any.
    Mapper,
}

impl TryFrom<usize> for Channel {
    type Error = ParseChannelError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Pulse1),
            1 => Ok(Self::Pulse2),
            2 => Ok(Self::Triangle),
            3 => Ok(Self::Noise),
            4 => Ok(Self::Dmc),
            5 => Ok(Self::Mapper),
            _ => Err(ParseChannelError),
        }
    }
}

/// NES APU (Audio Processing Unit).
///
/// See: <https://wiki.nesdev.org/w/index.php/APU>
#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Apu {
    /// The frame sequencer that clocks envelopes, sweeps and length counters.
    pub frame_counter: FrameCounter,
    /// Cycles into the current [`Apu::CYCLE_SIZE`] block.
    pub master_clock: u32,
    /// Total CPU cycles clocked.
    pub cpu_cycle: u32,
    /// APU cycles clocked.
    pub clock: u32,
    /// CPU clock rate for the current region, in Hz.
    pub clock_rate: f32,
    /// The region the APU is timed for.
    pub region: NesRegion,
    /// The first square-wave channel.
    pub pulse1: Pulse,
    /// The second square-wave channel.
    pub pulse2: Pulse,
    /// The triangle-wave channel.
    pub triangle: Triangle,
    /// The noise channel.
    pub noise: Noise,
    /// The sample channel.
    pub dmc: Dmc,
    /// The high-pass/low-pass chain the mixed output is run through.
    pub filter_chain: FilterChain,
    /// Turns amplitude changes into output-rate samples.
    #[serde(skip, default = "Apu::default_synth")]
    pub synth: BandLimited,
    /// Cycles of this block the mixer has consumed, which trails `master_clock`.
    pub mix_clock: u32,
    /// Mixed level of the five APU channels as of `mix_clock`, so a change is a delta from it.
    pub mixed_level: f32,
    /// Expansion audio level as of the last cycle the board reported one.
    ///
    /// Tracked separately because the board is clocked by the CPU rather than the APU, and
    /// because it is mixed in linearly - so its changes are deltas in their own right rather than
    /// having to go through the channel tables.
    pub mapper_level: f32,
    /// Whether the synthesiser's rate is stale and must be retuned at the next block boundary.
    #[serde(skip)]
    pub rate_dirty: bool,
    /// Mixed samples produced since the last drain.
    #[serde(skip)]
    pub audio_samples: Vec<f32>,
    /// Output sample rate in Hz.
    pub sample_rate: f32,
    /// Emulation speed multiplier, which stretches the sample period.
    pub speed: f32,
    /// Dynamic rate control: a small multiplier on how many samples a frame produces.
    ///
    /// See [`Apu::set_sample_ratio`].
    pub sample_ratio: f32,
    /// Whether cartridge expansion audio is mixed in.
    pub mapper_enabled: bool,
    /// Whether mixing is skipped entirely, as in headless runs.
    pub skip_mixing: bool,
    /// Whether a channel changed state and so the lazy clock must catch up.
    pub should_clock: bool,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(NesRegion::default())
    }
}

impl Apu {
    /// Sample rate used unless the embedder sets another.
    pub const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;
    /// The 5 APU channels plus one for cartridge expansion audio.
    pub const MAX_CHANNEL_COUNT: usize = 6;
    /// How many CPU cycles a mixing block spans before every cycle counter rolls back to zero.
    pub const CYCLE_SIZE: u32 = 10_000;

    /// Create a new APU instance.
    pub fn new(region: NesRegion) -> Self {
        let clock_rate = Cpu::region_clock_rate(region);
        let sample_rate = Self::DEFAULT_SAMPLE_RATE;
        Self {
            frame_counter: FrameCounter::new(region),
            master_clock: 0,
            cpu_cycle: 0,
            clock: 0,
            clock_rate,
            region,
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Default),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Default),
            triangle: Triangle::new(),
            noise: Noise::new(region),
            dmc: Dmc::new(region),
            filter_chain: FilterChain::new(region, sample_rate),
            synth: Self::default_synth(),
            mix_clock: 0,
            mixed_level: 0.0,
            mapper_level: 0.0,
            rate_dirty: false,
            audio_samples: Vec::with_capacity((sample_rate / 60.0) as usize),
            sample_rate,
            speed: 1.0,
            sample_ratio: 1.0,
            mapper_enabled: true,
            skip_mixing: false,
            should_clock: false,
        }
    }

    /// A synthesiser for the default region and rate, which is how a loaded save state gets one.
    pub fn default_synth() -> BandLimited {
        Self::new_synth(
            Cpu::region_clock_rate(NesRegion::default()),
            Self::DEFAULT_SAMPLE_RATE,
        )
    }

    fn new_synth(clock_rate: f32, sample_rate: f32) -> BandLimited {
        // Sized by the same `set_rate` that resizes it whenever the rate moves, so a block fits
        // here for the same reason it fits after a speed change - rather than by a headroom
        // factor guessed once and then outgrown.
        let mut synth = BandLimited::new(clock_rate, sample_rate, 0);
        synth.set_rate(clock_rate, sample_rate, Self::CYCLE_SIZE);
        synth
    }

    /// Records this cycle's expansion-audio sample from the cartridge.
    ///
    /// Only called for a board that has audio; see `Bus::cpu_clock`. Expansion audio is mixed in
    /// linearly, so a change in it is a delta on its own rather than one that has to be taken
    /// through the channel tables.
    #[inline(always)]
    pub fn add_mapper_output(&mut self, output: f32) {
        let level = if self.mapper_enabled { output } else { 0.0 };
        if level != self.mapper_level {
            self.synth
                .add_delta(self.master_clock, level - self.mapper_level);
            self.mapper_level = level;
        }
    }

    /// Advance every channel to `target`, recording each change in the mixed output as it happens.
    ///
    /// Walks from one cycle a channel could change on to the next rather than visiting every
    /// cycle: for a pulse at a typical period that is one stop every few hundred cycles. What is
    /// recorded is the change in the *mixed* level, not any one channel's, because the console
    /// mixes through two non-linear tables - two channels at a given level are quieter than twice
    /// one channel at it - so a channel's change is not a fixed contribution to the output.
    fn channels_clock_to(&mut self, target: u32) {
        if self.skip_mixing {
            self.mix_clock = target;
            self.clock_channels(target);
            return;
        }

        // Anything that moved the mix since the last visit - a register write, a length counter
        // clocked to zero by the frame counter - lands here, at the cycle it happened on.
        self.add_mix_delta();
        while self.mix_clock < target {
            // `max` because a channel can sit ahead of `mix_clock` (see `Timer::run_to`), and a
            // stop that is not in the future would not make progress.
            let next = self.next_change().max(self.mix_clock + 1).min(target);
            self.clock_channels(next);
            self.mix_clock = next;
            self.add_mix_delta();
        }
    }

    /// The next cycle any channel's output could change on.
    fn next_change(&self) -> u32 {
        self.pulse1
            .next_change()
            .min(self.pulse2.next_change())
            .min(self.triangle.next_change())
            .min(self.noise.next_change())
            .min(self.dmc.next_change())
    }

    /// Hand the synthesiser however much the mixed level has moved since it was last told.
    fn add_mix_delta(&mut self) {
        let level = self.mix_level();
        if level != self.mixed_level {
            self.synth
                .add_delta(self.mix_clock, level - self.mixed_level);
            self.mixed_level = level;
        }
    }

    /// The five channels mixed through the console's two non-linear tables.
    fn mix_level(&self) -> f32 {
        let pulse_idx = (self.pulse1.output() + self.pulse2.output()) as usize;
        // Not `mul_add`: it guarantees a single rounding, so without hardware FMA it lowers to a
        // libm `fmaf` call.
        let tnd_idx = ((3.0 * self.triangle.output())
            + (2.0 * self.noise.output())
            + self.dmc.output()) as usize;
        PULSE_TABLE[pulse_idx] + TND_TABLE[tnd_idx]
    }

    /// Run every channel forward to `cycle`.
    fn clock_channels(&mut self, cycle: u32) {
        self.pulse1.clock_to(cycle);
        self.pulse2.clock_to(cycle);
        self.triangle.clock_to(cycle);
        self.noise.clock_to(cycle);
        self.dmc.clock_to(cycle);
    }

    /// Set the audio sample rate.
    #[inline]
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.filter_chain = FilterChain::new(self.region, self.sample_rate / self.speed);
        self.synth = Self::new_synth(self.clock_rate, self.sample_rate / self.speed);
        self.rate_dirty = false;
    }

    /// Set the frame speed of the APU, which affects the sampling rate.
    pub fn set_frame_speed(&mut self, speed: f32) {
        self.speed = speed;
        self.filter_chain = FilterChain::new(self.region, self.sample_rate / self.speed);
        self.rate_dirty = true;
    }

    /// Stretch or squeeze the output rate by a small ratio, for dynamic rate control.
    ///
    /// A frontend syncs video to its own clock and audio to the sound card's, and the two do not
    /// agree - not even on nominally identical rates, because oscillators have tolerances. The
    /// difference has to be absorbed somewhere. Absorbing it in *frame timing*, by waiting on the
    /// audio queue, is what makes an emulator judder; absorbing it here, by handing the frontend
    /// slightly more or fewer samples per frame, costs a pitch shift far below what anyone can
    /// hear. See Arntzen, "Dynamic Rate Control for Retro Game Emulators" (2012).
    ///
    /// A ratio above 1 produces more samples per frame, which fills a draining buffer. Ratios are
    /// clamped to +/-5%, well beyond the +/-0.5% the method calls for, purely so a mistake in a
    /// frontend cannot make the emulator inaudible.
    ///
    /// Deliberately does **not** rebuild [`Apu::filter_chain`]: a fraction of a percent moves its
    /// cutoffs by nothing worth having, and rebuilding recomputes a 161-tap windowed-sinc kernel,
    /// which is not something to do every frame.
    pub fn set_sample_ratio(&mut self, ratio: f32) {
        let ratio = ratio.clamp(0.95, 1.05);
        if ratio != self.sample_ratio {
            self.sample_ratio = ratio;
            self.rate_dirty = true;
        }
    }

    /// Whether a given channel is enabled.
    #[must_use]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        match channel {
            Channel::Pulse1 => !self.pulse1.silent(),
            Channel::Pulse2 => !self.pulse2.silent(),
            Channel::Triangle => !self.triangle.silent(),
            Channel::Noise => !self.noise.silent(),
            Channel::Dmc => !self.dmc.silent(),
            Channel::Mapper => self.mapper_enabled,
        }
    }

    /// Enable or disable a given channel.
    pub const fn set_channel_enabled(&mut self, channel: Channel, enabled: bool) {
        match channel {
            Channel::Pulse1 => self.pulse1.set_silent(!enabled),
            Channel::Pulse2 => self.pulse2.set_silent(!enabled),
            Channel::Triangle => self.triangle.set_silent(!enabled),
            Channel::Noise => self.noise.set_silent(!enabled),
            Channel::Dmc => self.dmc.set_silent(!enabled),
            Channel::Mapper => self.mapper_enabled = enabled,
        }
    }

    /// Toggle a given channel.
    pub const fn toggle_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Pulse1 => self.pulse1.set_silent(!self.pulse1.silent()),
            Channel::Pulse2 => self.pulse2.set_silent(!self.pulse2.silent()),
            Channel::Triangle => self.triangle.set_silent(!self.triangle.silent()),
            Channel::Noise => self.noise.set_silent(!self.noise.silent()),
            Channel::Dmc => self.dmc.set_silent(!self.dmc.silent()),
            Channel::Mapper => self.mapper_enabled = !self.mapper_enabled,
        }
    }

    /// Clocks the APU one CPU cycle, doing the real work only when a channel needs it or the
    /// output block fills.
    pub fn clock_lazy(&mut self) {
        self.cpu_cycle = self.cpu_cycle.wrapping_add(1);
        self.master_clock += 1;
        if self.master_clock == Self::CYCLE_SIZE - 1 {
            self.clock_sync();
        } else if self.should_clock() {
            self.clock_to(self.master_clock);
        }
    }

    /// Runs all componnets up to master clock, synchronizing them.
    #[cold]
    #[inline(never)]
    pub fn clock_sync(&mut self) {
        self.clock_to(self.master_clock);

        debug_assert_eq!(self.master_clock, self.clock);
        debug_assert_eq!(self.master_clock, self.mix_clock);
        if !self.skip_mixing {
            let count = self.synth.end_block(self.master_clock);
            let start = self.audio_samples.len();
            self.synth.read(count, &mut self.audio_samples);
            self.filter_chain.filter(&mut self.audio_samples[start..]);
        }
        // Only between blocks: the deltas already placed were positioned using the old rate.
        if self.rate_dirty {
            self.rate_dirty = false;
            self.synth.set_rate(
                self.clock_rate,
                self.sample_rate / self.speed * self.sample_ratio,
                Self::CYCLE_SIZE,
            );
        }
        self.rewind_block();
    }

    /// Start the next [`Apu::CYCLE_SIZE`] block, putting every cycle counter back to zero.
    ///
    /// Only safe at a block boundary, where the channels really are all level with `master_clock`.
    /// `Apu::reset` deliberately does not come through here: `Dmc::reset` parks its timer a cycle
    /// ahead and `Triangle::reset` leaves its timer where it was, and both are load bearing.
    const fn rewind_block(&mut self) {
        self.master_clock = 0;
        self.clock = 0;
        self.mix_clock = 0;
        self.pulse1.timer.cycle = 0;
        self.pulse2.timer.cycle = 0;
        self.triangle.timer.cycle = 0;
        self.noise.timer.cycle = 0;
        self.dmc.timer.cycle = 0;
    }

    #[inline(always)]
    fn should_clock(&mut self) -> bool {
        // Clock every cycle while DMC is running to get accurate CPU stalling, sprite DMA
        // emulation, etc
        if self.dmc.should_clock() || self.should_clock {
            self.should_clock = false;
            return true;
        }
        let cycles = self.master_clock - self.clock;
        self.frame_counter.should_clock(cycles) || self.dmc.irq_pending_in(cycles)
    }

    fn clock_to(&mut self, cycle: u32) {
        self.master_clock = cycle;

        let cycles = self.master_clock - self.clock;
        trace!(
            "APU cycles to run: {} ({} - {}) - CYC:{}",
            cycles, self.master_clock, self.clock, self.cpu_cycle,
        );
        while self.master_clock - self.clock > 0 {
            self.clock += self
                .frame_counter
                .clock_with(self.master_clock - self.clock, |ty| match ty {
                    FrameType::Quarter => {
                        trace!("APU Quarter Frame clock - CYC:{}", self.cpu_cycle);
                        self.pulse1.clock_quarter_frame();
                        self.pulse2.clock_quarter_frame();
                        self.triangle.clock_quarter_frame();
                        self.noise.clock_quarter_frame();
                    }
                    FrameType::Half => {
                        trace!("APU Half Frame clock - CYC:{}", self.cpu_cycle);
                        self.pulse1.clock_half_frame();
                        self.pulse2.clock_half_frame();
                        self.triangle.clock_half_frame();
                        self.noise.clock_half_frame();
                    }
                    _ => (),
                });

            self.pulse1.length.reload();
            self.pulse2.length.reload();
            self.triangle.length.reload();
            self.noise.length.reload();

            self.channels_clock_to(self.clock);
        }
    }

    /// $4000 Pulse1, $4004 Pulse2, and $400C Noise Control.
    pub fn write_ctrl(&mut self, channel: Channel, val: u8) {
        self.clock_to(self.master_clock);
        match channel {
            Channel::Pulse1 => {
                trace!("APU $4000 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse1.write_ctrl(val);
            }
            Channel::Pulse2 => {
                trace!("APU $4004 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse2.write_ctrl(val);
            }
            Channel::Noise => {
                trace!("APU $400C write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.noise.write_ctrl(val);
            }
            _ => panic!("{channel:?} does not have a control register"),
        }
        self.should_clock = true;
    }

    /// $4001 Pulse1 and $4005 Pulse2 Sweep.
    pub fn write_sweep(&mut self, channel: Channel, val: u8) {
        self.clock_to(self.master_clock);
        match channel {
            Channel::Pulse1 => {
                trace!("APU $4001 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse1.write_sweep(val);
            }
            Channel::Pulse2 => {
                trace!("APU $4005 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse2.write_sweep(val);
            }
            _ => panic!("{channel:?} does not have a sweep register"),
        }
    }

    /// $4002 Pulse1, $4006 Pulse2, $400A Triangle, $400E Noise, and $4010 DMC Timer Low Byte.
    pub fn write_timer_lo(&mut self, channel: Channel, val: u8) {
        self.clock_to(self.master_clock);
        match channel {
            Channel::Pulse1 => {
                trace!("APU $4002 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse1.write_timer_lo(val);
            }
            Channel::Pulse2 => {
                trace!("APU $4006 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse2.write_timer_lo(val);
            }
            Channel::Triangle => {
                trace!("APU $400A write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.triangle.write_timer_lo(val);
            }
            Channel::Noise => {
                trace!("APU $400E write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.noise.write_timer(val);
            }
            Channel::Dmc => {
                trace!("APU $4010 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.dmc.write_timer(val);
            }
            _ => panic!("{channel:?} does not have a timer_lo register"),
        }
    }

    /// $4003 Pulse1, $4007 Pulse2, and $400B Triangle Timer High Byte.
    pub fn write_timer_hi(&mut self, channel: Channel, val: u8) {
        self.clock_to(self.master_clock);
        match channel {
            Channel::Pulse1 => {
                trace!("APU $4003 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse1.write_timer_hi(val);
                self.should_clock = self.pulse1.length.enabled;
            }
            Channel::Pulse2 => {
                trace!("APU $4007 write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.pulse2.write_timer_hi(val);
                self.should_clock = self.pulse2.length.enabled;
            }
            Channel::Triangle => {
                trace!("APU $400B write: ${val:02X} - CYC:{}", self.cpu_cycle);
                self.triangle.write_timer_hi(val);
                self.should_clock = self.triangle.length.enabled;
            }
            _ => panic!("{channel:?} does not have a timer_hi register"),
        }
    }

    /// $4008 Triangle Linear Counter.
    pub fn write_linear_counter(&mut self, val: u8) {
        self.clock_to(self.master_clock);
        trace!("APU $4008 write: ${val:02X} - CYC:{}", self.cpu_cycle);
        self.triangle.write_linear_counter(val);
        self.should_clock = true;
    }

    /// $400F Noise and $4013 DMC Length.
    pub fn write_length(&mut self, channel: Channel, val: u8) {
        self.clock_to(self.master_clock);
        trace!("APU $400F write: ${val:02X} - CYC:{}", self.cpu_cycle);
        match channel {
            Channel::Noise => {
                self.noise.write_length(val);
                self.should_clock = self.noise.length.enabled;
            }
            Channel::Dmc => self.dmc.write_length(val),
            _ => panic!("{channel:?} does not have a length register"),
        }
    }

    /// $4011 DMC Output Level.
    pub fn write_dmc_output(&mut self, val: u8) {
        self.clock_to(self.master_clock);
        trace!("APU $4011 write: ${val:02X} - CYC:{}", self.cpu_cycle);
        // Only 7-bits are used
        self.dmc.write_output(val & 0x7F);
    }

    /// $4012 DMC Sample Addr.
    pub fn write_dmc_addr(&mut self, val: u8) {
        self.clock_to(self.master_clock);
        trace!("APU $4012 write: ${val:02X} - CYC:{}", self.cpu_cycle);
        self.dmc.write_addr(val);
    }

    /// Read APU Status.
    ///
    /// $4015   if-d nt21   DMC IRQ, frame IRQ, length counter statuses
    pub fn read_status(&mut self) -> u8 {
        self.clock_to(self.master_clock);
        let val = self.peek_status();
        trace!("APU $4015 read: ${val:02X} - CYC:{}", self.cpu_cycle);
        if self.frame_counter.irq_pending {
            trace!("APU Frame Counter IRQ - CYC:{}", self.cpu_cycle);
        }
        self.frame_counter.irq_pending = false;
        val
    }

    /// Read APU Status without side-effects.
    ///
    /// Non-mutating version of `read_status`.
    pub fn peek_status(&self) -> u8 {
        let mut status = 0x00;
        if self.pulse1.length.counter > 0 {
            status |= 0x01;
        }
        if self.pulse2.length.counter > 0 {
            status |= 0x02;
        }
        if self.triangle.length.counter > 0 {
            status |= 0x04;
        }
        if self.noise.length.counter > 0 {
            status |= 0x08;
        }
        if self.dmc.bytes_remaining > 0 {
            trace!("dmc bytes remaining: {}", self.dmc.bytes_remaining);
            status |= 0x10;
        }
        if self.frame_counter.irq_pending {
            status |= 0x40;
        }
        if self.dmc.irq_pending {
            status |= 0x80;
        }
        status
    }

    /// Write APU Status.
    ///
    /// $4015   ---d nt21   length ctr enable: DMC, noise, triangle, pulse 2, 1
    pub fn write_status(&mut self, val: u8) {
        self.clock_to(self.master_clock);
        trace!("APU $4015 write: ${val:02X} - CYC:{}", self.cpu_cycle);
        self.pulse1.set_enabled(val & 0x01 == 0x01);
        self.pulse2.set_enabled(val & 0x02 == 0x02);
        self.triangle.set_enabled(val & 0x04 == 0x04);
        self.noise.set_enabled(val & 0x08 == 0x08);
        self.dmc.set_enabled(val & 0x10 == 0x10, self.cpu_cycle);
        self.dmc.irq_pending = false;
    }

    /// $4017 APU Frame Counter.
    pub fn write_frame_counter(&mut self, val: u8) {
        self.clock_to(self.master_clock);
        trace!("APU $4017 write: ${val:02X} - CYC:{}", self.cpu_cycle);
        self.frame_counter.write(val, self.cpu_cycle);
    }

    // Return pending IRQ.
    /// Whether the frame counter or the DMC is asserting IRQ.
    #[inline(always)]
    pub const fn irq_pending(&self) -> bool {
        self.frame_counter.irq_pending | self.dmc.irq_pending
    }

    // Return pending DMA.
    /// Whether the DMC wants a sample fetched.
    #[inline(always)]
    pub const fn dma_pending(&self) -> bool {
        self.dmc.dma_pending
    }

    // Clear pending DMA.
    /// Clears the DMC's sample-fetch request, once the CPU has served it.
    #[inline(always)]
    pub const fn clear_dma_pending(&mut self) {
        self.dmc.dma_pending = false;
    }

    /// Sets the region, which re-times the frame counter, the noise and DMC period tables and the
    /// filter chain.
    pub fn set_region(&mut self, region: NesRegion) {
        if self.region != region {
            self.region = region;
            self.clock_rate = Cpu::region_clock_rate(region);
            self.filter_chain = FilterChain::new(region, self.sample_rate / self.speed);
            self.synth = Self::new_synth(self.clock_rate, self.sample_rate / self.speed);
            self.frame_counter.set_region(region);
            self.noise.set_region(region);
            self.dmc.set_region(region);
        }
    }
    /// Resets the APU.
    pub fn reset(&mut self, kind: ResetKind) {
        self.cpu_cycle = 0;
        self.master_clock = 0;
        self.clock = 0;
        self.mix_clock = 0;
        self.should_clock = false;
        self.frame_counter.reset(kind);
        self.pulse1.reset(kind);
        self.pulse2.reset(kind);
        self.triangle.reset(kind);
        self.noise.reset(kind);
        self.dmc.reset(kind);
    }
}

impl std::fmt::Debug for Apu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Apu")
            .field("cpu_cycle", &self.cpu_cycle)
            .field("master_clock", &self.master_clock)
            .field("cycle", &self.clock)
            .field("frame_counter", &self.frame_counter)
            .field("pulse1", &self.pulse1)
            .field("pulse2", &self.pulse2)
            .field("triangle", &self.triangle)
            .field("noise", &self.noise)
            .field("dmc", &self.dmc)
            .field("filter_chain", &self.filter_chain)
            .field("audio_samples_len", &self.audio_samples.len())
            .finish()
    }
}

/// [`Pulse`] channel lookup table.
///
/// See: <https://www.nesdev.org/wiki/APU_Mixer>
///
/// Original calculation:
///
/// ```rust
/// let mut pulse_table = [0.0; 31];
/// for (i, val) in pulse_table.iter_mut().enumerate().skip(1) {
///     *val = 95.52 / (8_128.0 / (i as f32) + 100.0);
/// }
/// ```
#[rustfmt::skip]
pub static PULSE_TABLE: [f32; 31] = [
    0.0,          0.011_609_139, 0.022_939_48, 0.034_000_948, 0.044_803,    0.055_354_66,
    0.065_664_53, 0.075_740_82,  0.085_591_4,  0.095_223_75,  0.104_645_04, 0.113_862_15,
    0.122_881_64, 0.131_709_8,   0.140_352_64, 0.148_815_96,  0.157_105_25, 0.165_225_88,
    0.173_182_92, 0.180_981_26,  0.188_625_59, 0.196_120_46,  0.203_470_17, 0.210_678_94,
    0.217_750_76, 0.224_689_5,   0.231_498_87, 0.238_182_47,  0.244_743_78, 0.251_186_07,
    0.257_512_57,
];

/// [`Triangle`]/[`Noise`]/[`Dmc`] channels lookup table.
///
/// See: <https://www.nesdev.org/wiki/APU_Mixer>
///
/// Original calculation:
///
/// ```rust
/// let mut tnd_table = [0.0; 203];
/// for (i, val) in tnd_table.iter_mut().enumerate().skip(1) {
///     *val = 163.67 / (24_329.0 / (i as f32) + 100.0);
/// }
/// ```
#[rustfmt::skip]
pub static TND_TABLE: [f32; 203] = [
    0.0,           0.006_699_824, 0.013_345_02,  0.019_936_256, 0.026_474_18,  0.032_959_443,
    0.039_392_676, 0.045_774_5,   0.052_105_535, 0.058_386_38,  0.064_617_634, 0.070_799_87,
    0.076_933_69,  0.083_019_62,  0.089_058_26,  0.095_050_134, 0.100_995_794, 0.106_895_77,
    0.112_750_58,  0.118_560_754, 0.124_326_79,  0.130_049_18,  0.135_728_45,  0.141_365_05,
    0.146_959_5,   0.152_512_22,  0.158_023_7,   0.163_494_4,   0.168_924_76,  0.174_315_24,
    0.179_666_28,  0.184_978_3,   0.190_251_74,  0.195_486_98,  0.200_684_47,  0.205_844_63,
    0.210_967_81,  0.216_054_44,  0.221_104_92,  0.226_119_6,   0.231_098_88,  0.236_043_11,
    0.240_952_72,  0.245_828_,    0.250_669_36,  0.255_477_1,   0.260_251_64,  0.264_993_28,
    0.269_702_37,  0.274_379_22,  0.279_024_18,  0.283_637_58,  0.288_219_72,  0.292_770_95,
    0.297_291_52,  0.301_781_8,   0.306_242_1,   0.310_672_67,  0.315_073_85,  0.319_445_88,
    0.323_789_12,  0.328_103_78,  0.332_390_2,   0.336_648_6,   0.340_879_3,   0.345_082_55,
    0.349_258_63,  0.353_407_77,  0.357_530_27,  0.361_626_36,  0.365_696_34,  0.369_740_37,
    0.373_758_76,  0.377_751_74,  0.381_719_56,  0.385_662_44,  0.389_580_64,  0.393_474_37,
    0.397_343_84,  0.401_189_3,   0.405_011_,    0.408_809_07,  0.412_583_83,  0.416_335_46,
    0.420_064_15,  0.423_770_13,  0.427_453_6,   0.431_114_76,  0.434_753_84,  0.438_370_97,
    0.441_966_44,  0.445_540_4,   0.449_093_,    0.452_624_53,  0.456_135_06,  0.459_624_9,
    0.463_094_12,  0.466_542_93,  0.469_971_57,  0.473_380_15,  0.476_768_94,  0.480_137_94,
    0.483_487_52,  0.486_817_7,   0.490_128_73,  0.493_420_7,   0.496_693_88,  0.499_948_32,
    0.503_184_26,  0.506_401_84,  0.509_601_2,   0.512_782_45,  0.515_945_85,  0.519_091_4,
    0.522_219_5,   0.525_330_07,  0.528_423_25,  0.531_499_3,   0.534_558_36,  0.537_600_5,
    0.540_625_93,  0.543_634_8,   0.546_627_04,  0.549_603_04,  0.552_562_83,  0.555_506_47,
    0.558_434_3,   0.561_346_23,  0.564_242_5,   0.567_123_23,  0.569_988_5,   0.572_838_4,
    0.575_673_2,   0.578_492_94,  0.581_297_7,   0.584_087_6,   0.586_862_8,   0.589_623_45,
    0.592_369_56,  0.595_101_36,  0.597_818_9,   0.600_522_3,   0.603_211_6,   0.605_887_,
    0.608_548_64,  0.611_196_6,   0.613_830_8,   0.616_451_56,  0.619_059_,    0.621_653_14,
    0.624_234_,    0.626_801_85,  0.629_356_7,   0.631_898_64,  0.634_427_7,   0.636_944_2,
    0.639_448_05,  0.641_939_34,  0.644_418_24,  0.646_884_86,  0.649_339_2,   0.651_781_4,
    0.654_211_5,   0.656_629_74,  0.659_036_04,  0.661_430_6,   0.663_813_4,   0.666_184_66,
    0.668_544_35,  0.670_892_6,   0.673_229_46,  0.675_555_05,  0.677_869_44,  0.680_172_74,
    0.682_464_96,  0.684_746_2,   0.687_016_6,   0.689_276_2,   0.691_525_04,  0.693_763_3,
    0.695_990_9,   0.698_208_03,  0.700_414_8,   0.702_611_1,   0.704_797_2,   0.706_973_1,
    0.709_138_8,   0.711_294_5,   0.713_440_1,   0.715_575_9,   0.717_701_8,   0.719_817_9,
    0.721_924_25,  0.724_020_96,  0.726_108_,    0.728_185_65,  0.730_253_8,   0.732_312_56,
    0.734_361_95,  0.736_402_1,   0.738_433_1,   0.740_454_9,   0.742_467_6,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The NES mixes its channels in the analog domain through a resistor ladder, so the mixer is
    /// non-linear: two channels at a given level are quieter than twice one channel at that level,
    /// and each additional step of volume buys less than the last.
    ///
    /// Asserted directly rather than by ROM: judging channel balance by ear needs reference
    /// recordings, and no ROM that demonstrates it reports a pass or fail of its own.
    #[test]
    fn the_mixer_is_non_linear() {
        // Two pulses summing to 20 must be quieter than twice one pulse at 10.
        assert!(
            PULSE_TABLE[20] < 2.0 * PULSE_TABLE[10],
            "pulse mixing must compress: {} vs {}",
            PULSE_TABLE[20],
            2.0 * PULSE_TABLE[10]
        );
        assert!(
            TND_TABLE[100] < 2.0 * TND_TABLE[50],
            "triangle/noise/DMC mixing must compress"
        );

        // Monotonic, and with a diminishing step, all the way up.
        for level in 1..PULSE_TABLE.len() - 1 {
            let step = PULSE_TABLE[level] - PULSE_TABLE[level - 1];
            let next = PULSE_TABLE[level + 1] - PULSE_TABLE[level];
            assert!(step > 0.0, "PULSE_TABLE must rise at {level}");
            assert!(
                next < step,
                "each step must be smaller than the last, at {level}: {next} vs {step}"
            );
        }

        // Silence really is silence, or a muted channel would leak DC into the mix.
        assert_eq!(PULSE_TABLE[0], 0.0, "no pulse output is silent");
        assert_eq!(TND_TABLE[0], 0.0, "no triangle/noise/DMC output is silent");
    }
}
