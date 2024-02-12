use crate::{
    apu::{
        dmc::Dmc,
        frame_counter::{FcMode, FrameCounter},
        noise::Noise,
        pulse::{OutputFreq, Pulse, PulseChannel},
        triangle::Triangle,
    },
    audio::Audio,
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    cpu::Irq,
};
use serde::{Deserialize, Serialize};

pub mod dmc;
pub mod noise;
pub mod pulse;
pub mod triangle;

pub mod envelope;
pub mod frame_counter;
pub mod length_counter;
pub mod linear_counter;
pub mod sweep;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Channel {
    Pulse1,
    Pulse2,
    Triangle,
    Noise,
    Dmc,
}

pub trait ApuRegisters {
    fn write_ctrl(&mut self, channel: Channel, val: u8);
    fn write_sweep(&mut self, channel: Channel, val: u8);
    fn write_timer_lo(&mut self, channel: Channel, val: u8);
    fn write_timer_hi(&mut self, channel: Channel, val: u8);
    fn write_linear_counter(&mut self, channel: Channel, val: u8);
    fn write_length(&mut self, channel: Channel, val: u8);
    fn write_output(&mut self, channel: Channel, val: u8);
    fn write_addr_load(&mut self, channel: Channel, val: u8);
    fn read_status(&mut self) -> u8;
    fn peek_status(&self) -> u8;
    fn write_status(&mut self, val: u8);
    fn write_frame_counter(&mut self, val: u8);
}

#[derive(Clone, Serialize, Deserialize)]
#[must_use]
pub struct Apu {
    cycle: usize,
    region: NesRegion,
    irq_pending: bool, // Set by $4017 if irq_enabled is clear or set during step 4 of Step4 mode
    irq_disabled: bool, // Set by $4017 D6
    frame_counter: FrameCounter,
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    dmc: Dmc,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            cycle: 0,
            region: NesRegion::default(),
            irq_pending: false,
            irq_disabled: false,
            frame_counter: FrameCounter::new(),
            pulse1: Pulse::new(PulseChannel::One, OutputFreq::Default),
            pulse2: Pulse::new(PulseChannel::Two, OutputFreq::Default),
            triangle: Triangle::new(),
            noise: Noise::new(),
            dmc: Dmc::new(),
        }
    }

    #[inline]
    #[must_use]
    pub const fn channel_enabled(&self, channel: Channel) -> bool {
        match channel {
            Channel::Pulse1 => !self.pulse1.silent(),
            Channel::Pulse2 => !self.pulse2.silent(),
            Channel::Triangle => !self.triangle.silent(),
            Channel::Noise => !self.noise.silent(),
            Channel::Dmc => !self.dmc.silent(),
        }
    }

    pub fn toggle_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Pulse1 => self.pulse1.toggle_silent(),
            Channel::Pulse2 => self.pulse2.toggle_silent(),
            Channel::Triangle => self.triangle.toggle_silent(),
            Channel::Noise => self.noise.toggle_silent(),
            Channel::Dmc => self.dmc.toggle_silent(),
        }
    }

    #[inline]
    pub fn irqs_pending(&self) -> Irq {
        let mut irq = Irq::empty();
        irq.set(Irq::FRAME_COUNTER, self.irq_pending);
        irq.set(Irq::DMC, self.dmc.irq_pending());
        irq
    }

    #[inline]
    #[must_use]
    pub fn dmc_dma(&mut self) -> bool {
        self.dmc.dma()
    }

    #[inline]
    #[must_use]
    pub const fn dmc_dma_addr(&self) -> u16 {
        self.dmc.dma_addr()
    }

    #[inline]
    pub fn load_dmc_buffer(&mut self, val: u8) {
        self.dmc.load_buffer(val);
    }

    // Counts CPU clocks and determines when to clock quarter/half frames
    // counter is in CPU clocks to avoid APU half-frames
    fn clock_frame_counter(&mut self) {
        let clock = self.frame_counter.clock();

        if self.frame_counter.mode == FcMode::Step4
            && !self.irq_disabled
            && self.frame_counter.step >= 4
        {
            self.irq_pending = true;
        }

        // mode 0: 4-step  effective rate (approx)
        // ---------------------------------------
        // - - - f f f      60 Hz
        // - l - - l -     120 Hz
        // e e e - e -     240 Hz
        //
        // mode 1: 5-step  effective rate (approx)
        // ---------------------------------------
        // - - - - - -     (interrupt flag never set)
        // - l - - l -     96 Hz
        // e e e - e -     192 Hz
        match clock {
            1 | 3 => {
                self.clock_quarter_frame();
            }
            2 | 5 => {
                self.clock_quarter_frame();
                self.clock_half_frame();
            }
            _ => (),
        }

        // Clock Step5 immediately
        if self.frame_counter.update() && self.frame_counter.mode == FcMode::Step5 {
            self.clock_quarter_frame();
            self.clock_half_frame();
        }
    }

    #[inline]
    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_quarter_frame();
        self.pulse2.clock_quarter_frame();
        self.triangle.clock_quarter_frame();
        self.noise.clock_quarter_frame();
    }

    #[inline]
    fn clock_half_frame(&mut self) {
        self.pulse1.clock_half_frame();
        self.pulse2.clock_half_frame();
        self.triangle.clock_half_frame();
        self.noise.clock_half_frame();
    }
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl ApuRegisters for Apu {
    // $4000 Pulse1, $4004 Pulse2, and $400C Noise Control
    fn write_ctrl(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_ctrl(val),
            Channel::Pulse2 => self.pulse2.write_ctrl(val),
            Channel::Noise => self.noise.write_ctrl(val),
            _ => panic!("{channel:?} does not have a control register"),
        }
    }

    // $4001 Pulse1 and $4005 Pulse2 Sweep
    fn write_sweep(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_sweep(val),
            Channel::Pulse2 => self.pulse2.write_sweep(val),
            _ => panic!("{channel:?} does not have a sweep register"),
        }
    }

    // $4002 Pulse1, $4006 Pulse2, $400A Triangle, $400E Noise, and $4010 DMC Timer Low Byte
    fn write_timer_lo(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_timer_lo(val),
            Channel::Pulse2 => self.pulse2.write_timer_lo(val),
            Channel::Triangle => self.triangle.write_timer_lo(val),
            Channel::Noise => self.noise.write_timer(val),
            Channel::Dmc => self.dmc.write_timer(val),
        }
    }

    // $4003 Pulse1, $4007 Pulse2, and $400B Triangle Timer High Byte
    fn write_timer_hi(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Pulse1 => self.pulse1.write_timer_hi(val),
            Channel::Pulse2 => self.pulse2.write_timer_hi(val),
            Channel::Triangle => self.triangle.write_timer_hi(val),
            _ => panic!("{channel:?} does not have a timer_hi register"),
        }
    }

    // $4008 Triangle Linear Counter
    fn write_linear_counter(&mut self, channel: Channel, val: u8) {
        if channel == Channel::Triangle {
            self.triangle.write_linear_counter(val);
        } else {
            panic!("{channel:?} does not have a linear_counter register");
        }
    }

    // $400F Noise and $4013 DMC Length
    fn write_length(&mut self, channel: Channel, val: u8) {
        match channel {
            Channel::Noise => self.noise.write_length(val),
            Channel::Dmc => self.dmc.write_length(val),
            _ => panic!("{channel:?} does not have a length register"),
        }
    }

    // $4011 DMC Output
    fn write_output(&mut self, channel: Channel, val: u8) {
        if channel == Channel::Dmc {
            // Only 7-bits are used
            self.dmc.write_output(val & 0x7F);
        } else {
            panic!("{channel:?} does not have output register");
        }
    }

    // $4012 DMC Addr Load
    fn write_addr_load(&mut self, channel: Channel, val: u8) {
        if channel == Channel::Dmc {
            self.dmc.write_addr_load(val);
        } else {
            panic!("{channel:?} does not have addr_load register");
        }
    }

    // $4015 | RW  | APU Status
    //       |   0 | Channel 1, 1 = enable sound
    //       |   1 | Channel 2, 1 = enable sound
    //       |   2 | Channel 3, 1 = enable sound
    //       |   3 | Channel 4, 1 = enable sound
    //       |   4 | Channel 5, 1 = enable sound
    //       | 5-7 | Unused (???)
    fn read_status(&mut self) -> u8 {
        let val = self.peek_status();
        self.irq_pending = false;
        val
    }

    // $4015 | RW  | APU Status
    //       |   0 | Channel 1, 1 = enable sound
    //       |   1 | Channel 2, 1 = enable sound
    //       |   2 | Channel 3, 1 = enable sound
    //       |   3 | Channel 4, 1 = enable sound
    //       |   4 | Channel 5, 1 = enable sound
    //       | 5-7 | Unused (???)
    //
    // Non-mutating version of `read_status`.
    fn peek_status(&self) -> u8 {
        let mut status = 0x00;
        if self.pulse1.length_counter() > 0 {
            status |= 0x01;
        }
        if self.pulse2.length_counter() > 0 {
            status |= 0x02;
        }
        if self.triangle.length_counter() > 0 {
            status |= 0x04;
        }
        if self.noise.length_counter() > 0 {
            status |= 0x08;
        }
        if self.dmc.length() > 0 {
            status |= 0x10;
        }
        if self.irq_pending {
            status |= 0x40;
        }
        if self.dmc.irq_pending() {
            status |= 0x80;
        }
        status
    }

    // $4015 | RW  | APU Status
    //       |   0 | Channel 1, 1 = enable sound
    //       |   1 | Channel 2, 1 = enable sound
    //       |   2 | Channel 3, 1 = enable sound
    //       |   3 | Channel 4, 1 = enable sound
    //       |   4 | Channel 5, 1 = enable sound
    //       | 5-7 | Unused (???)
    fn write_status(&mut self, val: u8) {
        self.pulse1.set_enabled(val & 0x01 == 0x01);
        self.pulse2.set_enabled(val & 0x02 == 0x02);
        self.triangle.set_enabled(val & 0x04 == 0x04);
        self.noise.set_enabled(val & 0x08 == 0x08);
        self.dmc.set_enabled(val & 0x10 == 0x10, self.cycle);
    }

    // $4017 APU Frame Counter
    fn write_frame_counter(&mut self, val: u8) {
        self.frame_counter.write(val, self.cycle);
        self.irq_disabled = val & 0x40 == 0x40; // D6
        if self.irq_disabled {
            self.irq_pending = false;
        }
    }
}

impl Audio for Apu {
    #[inline]
    #[must_use]
    fn output(&self) -> f32 {
        let pulse1 = self.pulse1.output();
        let pulse2 = self.pulse2.output();
        let triangle = self.triangle.output();
        let noise = self.noise.output();
        let dmc = self.dmc.output();
        let pulse_idx = (pulse1 + pulse2) as usize;
        let tnd_idx = (3.0f32.mul_add(triangle, 2.0 * noise) + dmc) as usize;
        PULSE_TABLE[pulse_idx] + TND_TABLE[tnd_idx]
    }
}

impl Clock for Apu {
    #[inline]
    fn clock(&mut self) -> usize {
        self.dmc.check_pending_dma();
        if self.cycle & 0x01 == 0x00 {
            self.pulse1.clock();
            self.pulse2.clock();
            self.noise.clock();
            self.dmc.clock();
        }
        self.triangle.clock();
        // Technically only clocks every 2 CPU cycles, but due
        // to half-cycle timings, we clock every cycle
        self.clock_frame_counter();
        self.cycle = self.cycle.wrapping_add(1);
        1
    }
}

impl Regional for Apu {
    #[inline]
    fn region(&self) -> NesRegion {
        self.region
    }

    #[inline]
    fn set_region(&mut self, region: NesRegion) {
        self.region = region;
        self.frame_counter.set_region(region);
        self.noise.set_region(region);
        self.dmc.set_region(region);
    }
}

impl Reset for Apu {
    fn reset(&mut self, kind: ResetKind) {
        self.cycle = 0;
        self.irq_pending = false;
        self.irq_disabled = false;
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
            .field("cycle", &self.cycle)
            .field("irq_pending", &self.irq_pending)
            .field("irq_disabled", &self.irq_disabled)
            .field("frame_counter", &self.frame_counter)
            .field("pulse1", &self.pulse1)
            .field("pulse2", &self.pulse2)
            .field("triangle", &self.triangle)
            .field("noise", &self.noise)
            .field("dmc", &self.dmc)
            .finish()
    }
}

// Generated values to avoid constant Lazy deref cost during runtime.
//
// Original calculation:
// let mut pulse_table = [0.0; 31];
// for (i, val) in pulse_table.iter_mut().enumerate().skip(1) {
//     *val = 95.52 / (8_128.0 / (i as f32) + 100.0);
// }
pub(crate) static PULSE_TABLE: [f32; 31] = [
    0.0,
    0.011_609_139,
    0.022_939_48,
    0.034_000_948,
    0.044_803,
    0.055_354_66,
    0.065_664_53,
    0.075_740_82,
    0.085_591_4,
    0.095_223_75,
    0.104_645_04,
    0.113_862_15,
    0.122_881_64,
    0.131_709_8,
    0.140_352_64,
    0.148_815_96,
    0.157_105_25,
    0.165_225_88,
    0.173_182_92,
    0.180_981_26,
    0.188_625_59,
    0.196_120_46,
    0.203_470_17,
    0.210_678_94,
    0.217_750_76,
    0.224_689_5,
    0.231_498_87,
    0.238_182_47,
    0.244_743_78,
    0.251_186_07,
    0.257_512_57,
];
// Generated values to avoid constant Lazy deref cost during runtime.
//
// Original calculation:
// let mut tnd_table = [0.0; 203];
// for (i, val) in tnd_table.iter_mut().enumerate().skip(1) {
//     *val = 163.67 / (24_329.0 / (i as f32) + 100.0);
// }
static TND_TABLE: [f32; 203] = [
    0.0,
    0.006_699_824,
    0.013_345_02,
    0.019_936_256,
    0.026_474_18,
    0.032_959_443,
    0.039_392_676,
    0.045_774_5,
    0.052_105_535,
    0.058_386_38,
    0.064_617_634,
    0.070_799_87,
    0.076_933_69,
    0.083_019_62,
    0.089_058_26,
    0.095_050_134,
    0.100_995_794,
    0.106_895_77,
    0.112_750_58,
    0.118_560_754,
    0.124_326_79,
    0.130_049_18,
    0.135_728_45,
    0.141_365_05,
    0.146_959_5,
    0.152_512_22,
    0.158_023_7,
    0.163_494_4,
    0.168_924_76,
    0.174_315_24,
    0.179_666_28,
    0.184_978_3,
    0.190_251_74,
    0.195_486_98,
    0.200_684_47,
    0.205_844_63,
    0.210_967_81,
    0.216_054_44,
    0.221_104_92,
    0.226_119_6,
    0.231_098_88,
    0.236_043_11,
    0.240_952_72,
    0.245_828_,
    0.250_669_36,
    0.255_477_1,
    0.260_251_64,
    0.264_993_28,
    0.269_702_37,
    0.274_379_22,
    0.279_024_18,
    0.283_637_58,
    0.288_219_72,
    0.292_770_95,
    0.297_291_52,
    0.301_781_8,
    0.306_242_1,
    0.310_672_67,
    0.315_073_85,
    0.319_445_88,
    0.323_789_12,
    0.328_103_78,
    0.332_390_2,
    0.336_648_6,
    0.340_879_3,
    0.345_082_55,
    0.349_258_63,
    0.353_407_77,
    0.357_530_27,
    0.361_626_36,
    0.365_696_34,
    0.369_740_37,
    0.373_758_76,
    0.377_751_74,
    0.381_719_56,
    0.385_662_44,
    0.389_580_64,
    0.393_474_37,
    0.397_343_84,
    0.401_189_3,
    0.405_011_,
    0.408_809_07,
    0.412_583_83,
    0.416_335_46,
    0.420_064_15,
    0.423_770_13,
    0.427_453_6,
    0.431_114_76,
    0.434_753_84,
    0.438_370_97,
    0.441_966_44,
    0.445_540_4,
    0.449_093_,
    0.452_624_53,
    0.456_135_06,
    0.459_624_9,
    0.463_094_12,
    0.466_542_93,
    0.469_971_57,
    0.473_380_15,
    0.476_768_94,
    0.480_137_94,
    0.483_487_52,
    0.486_817_7,
    0.490_128_73,
    0.493_420_7,
    0.496_693_88,
    0.499_948_32,
    0.503_184_26,
    0.506_401_84,
    0.509_601_2,
    0.512_782_45,
    0.515_945_85,
    0.519_091_4,
    0.522_219_5,
    0.525_330_07,
    0.528_423_25,
    0.531_499_3,
    0.534_558_36,
    0.537_600_5,
    0.540_625_93,
    0.543_634_8,
    0.546_627_04,
    0.549_603_04,
    0.552_562_83,
    0.555_506_47,
    0.558_434_3,
    0.561_346_23,
    0.564_242_5,
    0.567_123_23,
    0.569_988_5,
    0.572_838_4,
    0.575_673_2,
    0.578_492_94,
    0.581_297_7,
    0.584_087_6,
    0.586_862_8,
    0.589_623_45,
    0.592_369_56,
    0.595_101_36,
    0.597_818_9,
    0.600_522_3,
    0.603_211_6,
    0.605_887_,
    0.608_548_64,
    0.611_196_6,
    0.613_830_8,
    0.616_451_56,
    0.619_059_,
    0.621_653_14,
    0.624_234_,
    0.626_801_85,
    0.629_356_7,
    0.631_898_64,
    0.634_427_7,
    0.636_944_2,
    0.639_448_05,
    0.641_939_34,
    0.644_418_24,
    0.646_884_86,
    0.649_339_2,
    0.651_781_4,
    0.654_211_5,
    0.656_629_74,
    0.659_036_04,
    0.661_430_6,
    0.663_813_4,
    0.666_184_66,
    0.668_544_35,
    0.670_892_6,
    0.673_229_46,
    0.675_555_05,
    0.677_869_44,
    0.680_172_74,
    0.682_464_96,
    0.684_746_2,
    0.687_016_6,
    0.689_276_2,
    0.691_525_04,
    0.693_763_3,
    0.695_990_9,
    0.698_208_03,
    0.700_414_8,
    0.702_611_1,
    0.704_797_2,
    0.706_973_1,
    0.709_138_8,
    0.711_294_5,
    0.713_440_1,
    0.715_575_9,
    0.717_701_8,
    0.719_817_9,
    0.721_924_25,
    0.724_020_96,
    0.726_108_,
    0.728_185_65,
    0.730_253_8,
    0.732_312_56,
    0.734_361_95,
    0.736_402_1,
    0.738_433_1,
    0.740_454_9,
    0.742_467_6,
];

#[cfg(test)]
impl Apu {
    pub(crate) const fn cycle(&self) -> usize {
        self.cycle
    }
}
