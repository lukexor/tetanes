use crate::filter::Filter;
use std::collections::VecDeque;

#[derive(Default, Debug)]
#[must_use]
pub struct Audio {
    input_rate: f32,
    output_rate: f32,
    downsampler: Downsampler,
    resampler: Resampler,
    filters: [Filter; 3],
}

impl Audio {
    pub fn new(input_rate: f32, output_rate: f32) -> Self {
        Self {
            input_rate,
            output_rate,
            downsampler: Downsampler::new(input_rate, output_rate),
            resampler: Resampler::new(output_rate),
            filters: [
                Filter::high_pass(90.0, output_rate),
                Filter::high_pass(440.0, output_rate),
                Filter::low_pass(14_000.0, output_rate),
            ],
        }
    }

    #[inline]
    pub fn set_input_rate(&mut self, input_rate: f32) {
        self.input_rate = input_rate;
        self.reset(self.input_rate, self.output_rate);
    }

    #[inline]
    pub fn set_output_rate(&mut self, output_rate: f32) {
        self.output_rate = output_rate;
        self.reset(self.input_rate, self.output_rate);
    }

    #[inline]
    pub fn reset(&mut self, input_rate: f32, output_rate: f32) {
        self.downsampler.reset(input_rate, output_rate);
        self.resampler.reset(output_rate);
        self.filters = [
            Filter::high_pass(90.0, output_rate),
            Filter::high_pass(440.0, output_rate),
            Filter::low_pass(14_000.0, output_rate),
        ];
    }

    /// Outputs audio using multi-rate-control re-sampling.
    ///
    /// Sources:
    /// - <https://near.sh/articles/audio/dynamic-rate-control>
    /// - <https://github.com/libretro/docs/blob/master/archive/ratecontrol.pdf>
    ///
    /// # Errors
    ///
    /// This function will return an error if it fails to enqueue audio.
    #[inline]
    pub fn output(&mut self, samples: &mut [f32], sample_ratio: f32) -> &[f32] {
        let samples = self.downsampler.apply(samples);
        self.resampler.set_ratio(sample_ratio);
        let samples = self.resampler.read(samples);
        for filter in &mut self.filters {
            filter.apply(samples);
        }
        samples
    }

    #[inline]
    #[must_use]
    pub fn output_len(&self) -> usize {
        self.resampler.read_len()
    }
}

#[derive(Default, Debug)]
#[must_use]
pub struct Downsampler {
    decimation: f32,
    filter: Filter,
    fraction: f32,
    avg: f32,
    count: f32,
    samples: Vec<f32>,
}

impl Downsampler {
    fn new(input_rate: f32, output_rate: f32) -> Self {
        let decimation = input_rate / output_rate;
        Self {
            decimation,
            filter: Filter::low_pass(output_rate / 2.0, output_rate),
            fraction: decimation,
            avg: 0.0,
            count: 0.0,
            samples: Vec::with_capacity((output_rate * 0.02) as usize),
        }
    }

    #[inline]
    fn reset(&mut self, input_rate: f32, output_rate: f32) {
        self.decimation = input_rate / output_rate;
        self.fraction = self.decimation;
    }

    #[inline]
    #[must_use]
    fn apply(&mut self, samples: &mut [f32]) -> &mut [f32] {
        let mu = &mut self.fraction;
        self.samples.clear();
        self.filter.apply(samples);
        for sample in samples {
            self.avg += *sample;
            self.count += 1.0;
            if *mu <= 1.0 {
                self.samples.push(self.avg / self.count);
                self.avg = 0.0;
                self.count = 0.0;
                *mu += self.decimation;
            }
            *mu -= 1.0;
        }
        &mut self.samples
    }
}

#[derive(Default, Debug)]
#[must_use]
pub struct Resampler {
    output_rate: f32,
    ratio: f32,
    fraction: f32,
    history: [f32; 4],
    samples: VecDeque<f32>,
}

impl Resampler {
    fn new(output_rate: f32) -> Self {
        Self {
            output_rate,
            ratio: 1.0,
            fraction: 0.0,
            history: [0.0; 4],
            // Start with ~20ms of audio capacity
            samples: VecDeque::with_capacity((output_rate * 0.02) as usize),
        }
    }

    #[inline]
    fn reset(&mut self, output_rate: f32) {
        self.output_rate = output_rate;
        self.ratio = 1.0;
        self.fraction = 0.0;
        self.history.fill(0.0);
    }

    #[inline]
    fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio;
    }

    #[inline]
    #[must_use]
    fn read_len(&self) -> usize {
        self.samples.as_slices().0.len()
    }

    #[inline]
    fn read(&mut self, samples: &mut [f32]) -> &mut [f32] {
        self.samples.clear();

        let mu = &mut self.fraction;
        let s = &mut self.history;

        for sample in samples {
            s[0] = s[1];
            s[1] = s[2];
            s[2] = s[3];
            s[3] = *sample;

            while *mu <= 1.0 {
                let a = s[3] - s[2] - s[0] + s[1];
                let b = s[0] - s[1] - a;
                let c = s[2] - s[0];
                let d = s[1];

                self.samples
                    .push_back(a * mu.powi(3) + b * mu.powi(2) + c * *mu + d);
                *mu += self.ratio;
            }
            *mu -= 1.0;
        }
        self.samples.make_contiguous();
        self.samples.as_mut_slices().0
    }
}
