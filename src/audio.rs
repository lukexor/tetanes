use crate::{audio::filter::Filter, profile, NesResult};
use anyhow::{anyhow, Context};
use chrono::Local;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, SampleRate, Stream, StreamConfig,
};
use crossbeam::channel::{self, Sender, TrySendError};
use std::{
    collections::VecDeque,
    fmt,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};
use web_time::Duration;

pub mod filter;
pub mod window_sinc;

pub trait Audio {
    fn output(&self) -> f32;
}

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
pub struct Playback {
    stream: Stream,
    sample_tx: Sender<f32>,
}

impl Playback {
    pub fn new(output_frequency: f32, buffer_size: usize) -> NesResult<Self> {
        let (sample_tx, sample_rx) = channel::bounded(buffer_size);
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no available audio devices found"))?;
        let mut supported_configs = device.supported_output_configs()?;
        let desired_sample_rate = SampleRate(output_frequency as u32);
        let closest_config = supported_configs
            .find(|config| config.max_sample_rate() >= desired_sample_rate)
            .or(supported_configs.next())
            .map(|config| {
                let sample_rate = config.max_sample_rate();
                config.with_sample_rate(desired_sample_rate.min(sample_rate))
            })
            .ok_or_else(|| anyhow!("no supported audio configurations found"))?;
        let config = StreamConfig {
            channels: 1,
            sample_rate: closest_config.sample_rate(),
            buffer_size: BufferSize::Fixed(buffer_size as u32 / 4),
        };
        log::info!("audio config: {config:?}");
        let stream = device.build_output_stream(
            &config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // log::info!(
                //     "DEBUG {:.0}, requested samples: {}, available: {}",
                //     platform::ms_since_epoch(),
                //     output.len(),
                //     sample_rx.len(),
                // );
                // if let Ok(chunk) = sample_rx.try_recv() {
                for (out, sample) in output.iter_mut().zip(sample_rx.try_iter()) {
                    *out = sample;
                }
                // }
            },
            |err| eprintln!("an error occurred on stream: {err}"),
            None,
        )?;
        Ok(Self { stream, sample_tx })
    }

    #[inline]
    pub fn pause(&self) {
        let _ = self.stream.pause();
    }

    #[inline]
    pub fn play(&self) -> NesResult<()> {
        Ok(self.stream.play()?)
    }

    #[inline(always)]
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.sample_tx.len()
    }

    #[inline(always)]
    // pub fn push(&mut self, samples: Vec<f32>) -> Result<(), TrySendError<Vec<f32>>> {
    pub fn push(&mut self, sample: f32) -> Result<(), TrySendError<f32>> {
        // self.sample_tx.try_send(samples)
        self.sample_tx.try_send(sample)
    }
}

impl fmt::Debug for Playback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Playback")
            .field("buffer_len", &self.sample_tx.len())
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Mixer {
    playback: Option<Playback>,
    buffer_size: usize,
    buffer_len_average: MovingAverage,
    max_queued_time: Duration,
    input_frequency: f32,
    output_frequency: f32,
    resample_ratio: f32,
    fraction: f32,
    avg: f32,
    count: f32,
    filters: [Filter; 3],
    sample_buffer: Vec<f32>,
    recording_file: Option<BufWriter<File>>,
}

impl Mixer {
    /// Creates a new audio mixer.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device fails to be opened.
    pub fn new(input_frequency: f32, output_frequency: f32, buffer_size: usize) -> Self {
        let resample_ratio = input_frequency / output_frequency;
        Self {
            playback: None,
            buffer_size,
            buffer_len_average: MovingAverage::new(32),
            max_queued_time: Duration::from_secs_f64(buffer_size as f64 / output_frequency as f64),
            input_frequency,
            output_frequency,
            resample_ratio,
            fraction: resample_ratio,
            avg: 0.0,
            count: 0.0,
            filters: [
                Filter::high_pass(output_frequency, 90.0, 1500.0),
                Filter::high_pass(output_frequency, 440.0, 1500.0),
                // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
                Filter::low_pass(output_frequency, 10_000.0, 1500.0),
            ],
            sample_buffer: Vec::with_capacity(output_frequency as usize / 60),
            recording_file: None,
        }
    }

    /// Return the time, in milliseconds, of audio queued for playback.
    #[inline]
    #[must_use]
    pub fn queued_time(&self) -> Duration {
        let buffer_len = self.playback.as_ref().map_or(0, Playback::buffer_len);
        Duration::from_secs_f64(buffer_len as f64 / self.output_frequency as f64)
    }

    /// Return the max time, in milliseconds, of audio that can be queued for playback based on the
    /// currently set buffer size.
    #[inline]
    #[must_use]
    pub const fn max_queued_time(&self) -> Duration {
        self.max_queued_time
    }

    /// Pause the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device does not support pausing.
    #[inline]
    pub fn pause(&mut self) {
        self.playback.as_ref().map(Playback::pause);
    }

    /// Resume the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if audio can not be resumed.
    #[inline]
    pub fn play(&mut self) -> NesResult<()> {
        match self.playback {
            Some(ref playback) => playback.play(),
            None => {
                let playback = Playback::new(self.output_frequency, self.buffer_size)?;
                let play_result = playback.play();
                self.playback = Some(playback);
                play_result
            }
        }
    }

    /// Change the audio output frequency. This changes the sampling ratio used during
    /// [`Audio::process`].
    pub fn set_output_frequency(&mut self, output_frequency: f32) -> NesResult<()> {
        self.output_frequency = output_frequency;
        self.resample_ratio = self.input_frequency / output_frequency;
        // TODO: update audio filters
        // self.filters = [
        //     Filter::high_pass(output_frequency, 90.0, 1500.0),
        //     Filter::high_pass(output_frequency, 440.0, 1500.0),
        //     Filter::low_pass(output_frequency, 10_000.0, 1500.0),
        // ];
        // TODO: Handle changing output frequency while playing
        if self.playback.is_some() {
            let playback = Playback::new(self.output_frequency, self.buffer_size)?;
            playback.play()?;
            self.playback = Some(playback);
        }
        Ok(())
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

    /// Processes and filters generated audio samples.
    pub fn process(&mut self, samples: &[f32]) {
        profile!("audio::process");

        let Some(ref mut playback) = self.playback else {
            return;
        };

        let mut sample_count = 0;
        // log::info!(
        //     "DEBUG {:.0}, generated samples: {}",
        //     Instant::now(),
        //     samples.len()
        // );
        for sample in samples {
            self.avg += *sample;
            self.count += 1.0;
            self.fraction -= 1.0;
            // log::info!(
            //     "DEBUG avg: {}, count: {}, fraction: {}",
            //     self.avg,
            //     self.count,
            //     self.fraction
            // );
            while self.fraction < 1.0 {
                let sample = self
                    .filters
                    .iter_mut()
                    .fold(self.avg / self.count, |sample, filter| filter.apply(sample));
                // log::info!("DEBUG filtered sample: {sample}, sample_count: {sample_count}");

                // self.sample_buffer.push(sample);
                let queued = playback.push(sample);
                match queued {
                    Ok(()) => {
                        sample_count += 1;
                        // log::info!(
                        //     "DEBUG audio queued: {}, sample_count: {sample_count}",
                        //     playback.buffer_len()
                        // );
                        if let Some(recording_file) = &mut self.recording_file {
                            let _ = recording_file.write_all(&sample.to_le_bytes());
                        }
                    }
                    Err(err) => {
                        log::error!("failed to send audio sample: {err:?}");
                    }
                }
                self.avg = 0.0;
                self.count = 0.0;
                self.fraction += self.resample_ratio;
            }
        }
        // log::info!( "DEBUG {:.0}, pushed samples: {sample_count}", Instant::now());
        // let queued = playback.push(self.sample_buffer.clone());
        // match queued {
        //     Ok(()) => {
        //         if let Some(recording_file) = &mut self.recording_file {
        //             for sample in &self.sample_buffer {
        //                 let _ = recording_file.write_all(&sample.to_le_bytes());
        //             }
        //         }
        //         self.sample_buffer.clear();
        //     }
        //     Err(err) => {
        //         log::error!("failed to send audio sample: {err:?}");
        //     }
        // }
    }
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mixer")
            .field("playback", &self.playback)
            .field("buffer_size", &self.buffer_size)
            .field("buffer_len_average", &self.buffer_len_average.average())
            .field("input_frequency", &self.input_frequency)
            .field("output_frequency", &self.output_frequency)
            .field("resample_ratio", &self.resample_ratio)
            .field("fraction", &self.fraction)
            // .field("filters", &self.filters)
            .field("recording_file", &self.recording_file)
            .finish()
    }
}
