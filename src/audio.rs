use crate::{audio::filter::Filter, profile, NesResult};
use anyhow::Context;
use chrono::Local;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, SampleRate, Stream, StreamConfig,
};
use ringbuf::{HeapRb, Producer, SharedRb};
use std::{
    collections::VecDeque,
    fmt,
    fs::File,
    io::{BufWriter, Write},
    mem::MaybeUninit,
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};

pub mod filter;
pub mod window_sinc;

pub trait Audio {
    fn output(&self) -> f32;
}

type AudioBuf = Arc<SharedRb<f32, Vec<MaybeUninit<f32>>>>;

/// A moving average buffer.
#[must_use]
struct MovingAverage {
    sum: usize,
    values: VecDeque<usize>,
}

impl MovingAverage {
    fn new(window_size: usize) -> Self {
        MovingAverage {
            sum: 0,
            values: VecDeque::with_capacity(window_size),
        }
    }

    fn push(&mut self, sample: usize) {
        if self.values.len() == self.values.capacity() {
            if let Some(old_sample) = self.values.pop_front() {
                self.sum -= old_sample;
            }
        }
        self.values.push_back(sample);
        self.sum += sample;
    }

    fn average(&self) -> f32 {
        let len = self.values.len();
        if len == 0 {
            0.0
        } else {
            self.sum as f32 / len as f32
        }
    }
}

#[test]
fn moving_average() {
    let mut average = MovingAverage::new(5);
    average.push(1);
    assert!(average.average() == 1.0);
    average.push(7);
    assert!(average.average() == 4.0);
    average.push(7);
    assert!(average.average() == 5.0);
    average.push(4);
    assert!(average.average() == 4.75);
    average.push(10);
    assert!(average.average() == 5.8);
    average.push(10);
    assert!(average.average() == 7.6);
    average.push(10);
    assert!(average.average() == 8.2);
}

#[must_use]
pub struct Mixer {
    stream: Option<Stream>,
    producer: Producer<f32, AudioBuf>,
    // resampler: SincFixedIn<f32>,
    // output_buffer: Vec<Vec<f32>>,
    buffer_len_average: MovingAverage,
    input_frequency: f32,
    output_frequency: f32,
    resample_ratio: f32,
    dynamic_rate_control_delta: Option<f32>,
    pitch_modulation: f32,
    fraction: f32,
    avg: f32,
    count: f32,
    filters: [Filter; 3],
    recording_file: Option<BufWriter<File>>,
}

impl Mixer {
    /// Creates a new audio mixer.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device fails to be opened.
    pub fn new(
        input_frequency: f32,
        output_frequency: f32,
        buffer_size: usize,
        dynamic_rate_control_delta: Option<f32>,
    ) -> Self {
        let buffer = HeapRb::<f32>::new(buffer_size);
        let (producer, mut consumer) = buffer.split();

        let stream = cpal::default_host()
            .default_output_device()
            .expect("audio device")
            .build_output_stream(
                &StreamConfig {
                    channels: 1,
                    sample_rate: SampleRate(output_frequency as u32),
                    buffer_size: BufferSize::Fixed((buffer_size / 2) as u32),
                },
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    consumer.pop_slice(output);
                },
                |err| eprintln!("an error occurred on stream: {err}"),
                None,
            );
        if let Ok(ref stream) = stream {
            let _ = stream.play();
        }

        let resample_ratio = input_frequency / output_frequency;

        Self {
            stream: stream.ok(),
            producer,
            // resampler,
            // output_buffer,
            buffer_len_average: MovingAverage::new(32),
            input_frequency,
            output_frequency,
            resample_ratio,
            dynamic_rate_control_delta,
            pitch_modulation: 1.0,
            fraction: resample_ratio,
            avg: 0.0,
            count: 0.0,
            filters: [
                Filter::high_pass(output_frequency, 90.0, 1500.0),
                Filter::high_pass(output_frequency, 440.0, 1500.0),
                // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
                Filter::low_pass(output_frequency, 12_000.0, 1500.0),
            ],
            recording_file: None,
        }
    }

    /// Pause the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device does not support pausing.
    #[inline]
    pub fn pause(&mut self) {
        self.stream.as_ref().map(StreamTrait::pause);
    }

    /// Resume the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if audio can not be resumed.
    #[inline]
    pub fn play(&mut self) -> Option<NesResult<()>> {
        self.stream.as_ref().map(|stream| Ok(stream.play()?))
    }

    /// Change the audio output frequency. This changes the sampling ratio used during
    /// [`Audio::process`].
    pub fn set_output_frequency(&mut self, output_frequency: f32) {
        self.output_frequency = output_frequency;
    }

    /// Start recording audio to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file can not be created.
    pub fn start_recording(&mut self) -> NesResult<()> {
        // TODO: Add wav format
        let filename = PathBuf::from(
            Local::now()
                .format("Screen_Shot_%Y-%m-%d_at_%H_%M_%S.png")
                .to_string(),
        )
        .with_extension("raw");
        self.recording_file = Some(BufWriter::new(
            File::create(filename).with_context(|| "failed to create audio recording")?,
        ));
        Ok(())
    }

    /// Stop recording audio to a file.
    pub fn stop_recording(&mut self) {
        self.recording_file = None;
    }

    /// Outputs audio using multi-rate-control re-sampling.
    ///
    /// Sources:
    /// - <https://near.sh/articles/audio/dynamic-rate-control>
    /// - <https://github.com/libretro/docs/blob/master/archive/ratecontrol.pdf>
    pub fn process(&mut self, samples: &[f32]) {
        profile!("audio::process");

        if self.stream.is_none() {
            return;
        }

        self.pitch_modulation = if let Some(delta) = self.dynamic_rate_control_delta {
            self.buffer_len_average.push(self.producer.len());
            let capacity = self.producer.capacity() as f32;
            let average = self.buffer_len_average.average();
            // AB / ((1 + d)AB - 2dAbc)
            // AB: buffer capacity
            // d: delta
            // Abc: average buffer size which should converge to 1/2 AB
            capacity / ((1.0 + delta) * capacity - 2.0 * delta * average)
        } else {
            1.0
        };
        self.resample_ratio = self.input_frequency / self.output_frequency * self.pitch_modulation;

        for sample in samples {
            self.avg += *sample;
            self.count += 1.0;
            while self.fraction <= 0.0 {
                let sample = self
                    .filters
                    .iter_mut()
                    .fold(self.avg / self.count, |sample, filter| filter.apply(sample));
                loop {
                    profile!("audio sync");

                    let queued = self.producer.push(sample);
                    match queued {
                        Ok(()) => {
                            if let Some(recording_file) = &mut self.recording_file {
                                let _ = recording_file.write_all(&sample.to_le_bytes());
                            }
                            break;
                        }
                        Err(_) => {
                            // wait for ~2 samples to be consumed
                            thread::sleep(Duration::from_micros(50));
                        }
                    }
                }
                self.avg = 0.0;
                self.count = 0.0;
                self.fraction += self.resample_ratio;
            }
            self.fraction -= 1.0;
        }
    }
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioMixer")
            .field("producer_len", &self.producer.len())
            .field("producer_capacity", &self.producer.capacity())
            .field("buffer_len_average", &self.buffer_len_average.average())
            .field("input_frequency", &self.input_frequency)
            .field("output_frequency", &self.output_frequency)
            .field("resample_ratio", &self.resample_ratio)
            .field(
                "dynamic_rate_control_delta",
                &self.dynamic_rate_control_delta,
            )
            .field("pitch_modulation", &self.pitch_modulation)
            .field("fraction", &self.fraction)
            // .field("filters", &self.filters)
            .field("recording_file", &self.recording_file)
            .finish()
    }
}
