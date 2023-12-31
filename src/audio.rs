use crate::{audio::filter::Filter, profile, NesResult};
use anyhow::{anyhow, Context};
use chrono::Local;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, SampleRate, Stream, StreamConfig,
};
use crossbeam::channel::{self, Sender, TrySendError};
use std::{
    fmt,
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use web_time::Duration;

pub mod filter;
pub mod window_sinc;

pub trait Audio {
    fn output(&self) -> f32;
}

#[must_use]
pub struct Playback {
    stream: Stream,
    samples_tx: Sender<Vec<f32>>,
    buffer_len: Arc<AtomicUsize>,
}

impl Playback {
    pub fn new(input_frequency: f32, output_frequency: f32, buffer_size: usize) -> NesResult<Self> {
        let (samples_tx, samples_rx) = channel::bounded::<Vec<f32>>(100);
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

        let resample_ratio = input_frequency / output_frequency;
        let mut filters = [
            Filter::high_pass(output_frequency, 90.0, 1500.0),
            Filter::high_pass(output_frequency, 440.0, 1500.0),
            // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
            Filter::low_pass(output_frequency, 11_000.0, 1500.0),
        ];
        let mut avg = 0.0;
        let mut count = 0.0;
        let mut fraction = 0.0;
        let buffer_len = Arc::new(AtomicUsize::new(0));
        let mut processed_samples = Vec::with_capacity(buffer_size);
        let stream_buffer_len = Arc::clone(&buffer_len);
        let stream = device.build_output_stream(
            &config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(samples) = samples_rx.try_recv() {
                    // let mut sample_count = 0;
                    // log::info!(
                    //     "DEBUG {:.0?}, generated samples: {}",
                    //     web_time::Instant::now(),
                    //     samples.len()
                    // );
                    for sample in &samples {
                        avg += sample;
                        count += 1.0;
                        fraction -= 1.0;
                        while fraction < 1.0 {
                            let sample = filters
                                .iter_mut()
                                .fold(avg / count, |sample, filter| filter.apply(sample));

                            processed_samples.push(sample);
                            // sample_count += 1;
                            avg = 0.0;
                            count = 0.0;
                            fraction += resample_ratio;
                        }
                    }
                    // log::info!(
                    //     "DEBUG {:.0?}, pushed samples: {sample_count}",
                    //     web_time::Instant::now()
                    // );
                }

                // log::info!(
                //     "DEBUG {:.0?}, requested samples: {}, available: {}",
                //     web_time::Instant::now(),
                //     output.len(),
                //     processed_samples.len(),
                // );
                let len = output.len().min(processed_samples.len());
                for (out, sample) in output.iter_mut().zip(processed_samples.drain(..len)) {
                    *out = sample;
                }
                stream_buffer_len.store(processed_samples.len(), Ordering::Release);
            },
            |err| eprintln!("an error occurred on stream: {err}"),
            None,
        )?;
        Ok(Self {
            stream,
            samples_tx,
            buffer_len,
        })
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
        self.buffer_len.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn push(&mut self, samples: Vec<f32>) -> Result<(), TrySendError<Vec<f32>>> {
        self.samples_tx.try_send(samples)
    }
}

impl fmt::Debug for Playback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Playback")
            .field("buffer_len", &self.samples_tx.len())
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Mixer {
    playback: Option<Playback>,
    buffer_size: usize,
    max_queued_time: Duration,
    input_frequency: f32,
    output_frequency: f32,
    resample_ratio: f32,
    fraction: f32,
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
            max_queued_time: Duration::from_secs_f64(buffer_size as f64 / output_frequency as f64),
            input_frequency,
            output_frequency,
            resample_ratio,
            fraction: resample_ratio,
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
                let playback = Playback::new(
                    self.input_frequency,
                    self.output_frequency,
                    self.buffer_size,
                )?;
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
        // TODO: Handle changing output frequency while playing
        if self.playback.is_some() {
            let playback = Playback::new(
                self.input_frequency,
                self.output_frequency,
                self.buffer_size,
            )?;
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

        if let Some(ref mut playback) = self.playback {
            // TODO: create a shared circular buffer of Vecs to avoid allocations
            let queued = playback.push(samples.to_vec());
            match queued {
                Ok(()) => {
                    // if let Some(recording_file) = &mut self.recording_file {
                    //     for sample in &self.sample_buffer {
                    //         let _ = recording_file.write_all(&sample.to_le_bytes());
                    //     }
                    // }
                }
                Err(err) => {
                    log::error!("failed to send audio samples: {err:?}");
                }
            }
        };
    }
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mixer")
            .field("playback", &self.playback)
            .field("buffer_size", &self.buffer_size)
            .field("input_frequency", &self.input_frequency)
            .field("output_frequency", &self.output_frequency)
            .field("resample_ratio", &self.resample_ratio)
            .field("fraction", &self.fraction)
            .field("recording_file", &self.recording_file)
            .finish()
    }
}
