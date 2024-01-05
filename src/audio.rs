use crate::{audio::filter::Filter, profile, NesResult};
use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Device, SampleRate, Stream, StreamConfig,
};
use crossbeam::channel::{self, Receiver, Sender};
use std::{
    fmt,
    fs::File,
    io::BufWriter,
    iter,
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

#[derive(Debug)]
#[must_use]
pub enum CallbackMsg {
    FrameSamples(Vec<f32>),
    UpdateSampleRate((f32, Duration)),
    UpdateResampleRatio(f32),
    Record(bool),
}

#[derive(Debug)]
#[must_use]
struct Callback {
    sample_avg: f32,
    sample_count: f32,
    decim_fraction: f32,
    resample_ratio: f32,
    filters: [Filter; 3],
    processed_samples: Vec<f32>,
    callback_rx: Receiver<CallbackMsg>,
    buffer_len: Arc<AtomicUsize>,
    recording_file: Option<BufWriter<File>>,
}

impl Callback {
    const BYTES_PER_SAMPLE: f32 = 4.0;

    fn new(
        sample_rate: f32,
        resample_ratio: f32,
        audio_latency: Duration,
        callback_rx: Receiver<CallbackMsg>,
        buffer_len: Arc<AtomicUsize>,
    ) -> Self {
        let buffer_size = ((sample_rate * audio_latency.as_secs_f32() * Self::BYTES_PER_SAMPLE)
            as usize)
            .next_power_of_two();
        Self {
            sample_avg: 0.0,
            sample_count: 0.0,
            decim_fraction: 0.0,
            resample_ratio,
            filters: [
                Filter::high_pass(sample_rate, 90.0, 1500.0),
                Filter::high_pass(sample_rate, 440.0, 1500.0),
                // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
                Filter::low_pass(sample_rate, 11_000.0, 1500.0),
            ],
            processed_samples: Vec::with_capacity(buffer_size),
            callback_rx,
            buffer_len,
            recording_file: None,
        }
    }

    fn execute(&mut self, out: &mut [f32], _: &cpal::OutputCallbackInfo) {
        profile!();

        while let Ok(msg) = self.callback_rx.try_recv() {
            match msg {
                CallbackMsg::UpdateSampleRate((sample_rate, audio_latency)) => {
                    let buffer_size = ((sample_rate
                        * audio_latency.as_secs_f32()
                        * Self::BYTES_PER_SAMPLE) as usize)
                        .next_power_of_two();
                    self.processed_samples
                        .reserve(buffer_size.saturating_sub(self.processed_samples.len()));
                }
                CallbackMsg::UpdateResampleRatio(resample_ratio) => {
                    self.resample_ratio = resample_ratio;
                }
                // TODO: Maybe create another thread to send processed samples to?
                CallbackMsg::Record(_recording) => todo!(),
                CallbackMsg::FrameSamples(samples) => {
                    // let mut sample_count = 0;
                    // log::info!(
                    //     "DEBUG {:.0?}, generated samples: {}",
                    //     web_time::Instant::now(),
                    //     samples.len()
                    // );
                    for sample in &samples {
                        self.sample_avg += sample;
                        self.sample_count += 1.0;
                        self.decim_fraction -= 1.0;
                        while self.decim_fraction < 1.0 {
                            let sample = self
                                .filters
                                .iter_mut()
                                .fold(self.sample_avg / self.sample_count, |sample, filter| {
                                    filter.apply(sample)
                                });

                            self.processed_samples.push(sample);
                            // sample_count += 1;
                            self.sample_avg = 0.0;
                            self.sample_count = 0.0;
                            self.decim_fraction += self.resample_ratio;
                        }
                    }
                }
            }
            // log::info!(
            //     "DEBUG {:.0?}, pushed samples: {sample_count}",
            //     web_time::Instant::now()
            // );
        }

        // TODO: Pass off to thread worker to write to file
        // if let Some(recording_file) = &mut self.recording_file {
        //     for sample in &self.sample_buffer {
        //         let _ = recording_file.write_all(&sample.to_le_bytes());
        //     }
        // }

        // log::info!(
        //     "DEBUG {:.0?}, requested samples: {}, available: {}",
        //     web_time::Instant::now(),
        //     out.len(),
        //     self.processed_samples.len(),
        // );
        let channels = 2;
        let num_samples = out.len() / channels;
        let len = num_samples.min(self.processed_samples.len());
        // if len < num_samples {
        //     log::warn!("underun: {len} < {num_samples}");
        // }
        for (frame, sample) in out
            .chunks_mut(channels)
            .zip(self.processed_samples.drain(..len).chain(iter::repeat(0.0)))
        {
            for out in frame.iter_mut() {
                *out = sample;
            }
        }
        self.buffer_len
            .store(self.processed_samples.len(), Ordering::Relaxed);
    }
}

#[must_use]
pub struct Mixer {
    stream: Option<Stream>,
    input_frequency: f32,
    output_frequency: f32,
    audio_latency: Duration,
    buffer_len: Arc<AtomicUsize>,
    enabled: bool,
    callback_tx: Option<Sender<CallbackMsg>>,
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
        audio_latency: Duration,
        enabled: bool,
    ) -> Self {
        Self {
            stream: None,
            input_frequency,
            output_frequency,
            audio_latency,
            buffer_len: Arc::new(AtomicUsize::new(0)),
            enabled,
            callback_tx: None,
        }
    }

    /// Returns the number of samples queued for playback.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer_len.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn queued_time(&self) -> Duration {
        Duration::from_secs_f64(self.buffer_len() as f64 / self.output_frequency as f64)
    }

    fn choose_audio_config(device: &Device, sample_rate: f32) -> NesResult<StreamConfig> {
        let mut supported_configs = device.supported_output_configs()?;
        let desired_sample_rate = SampleRate(sample_rate as u32);
        let chosen_config = supported_configs
            .find(|config| config.max_sample_rate() >= desired_sample_rate)
            .or_else(|| {
                device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut c| c.next())
            })
            .map(|config| {
                log::debug!("desired_sample_rate: {desired_sample_rate:?}, config: {config:?}");
                let min_sample_rate = config.min_sample_rate();
                let max_sample_rate = config.max_sample_rate();
                config.with_sample_rate(desired_sample_rate.clamp(min_sample_rate, max_sample_rate))
            })
            .ok_or_else(|| anyhow!("no supported audio configurations found"))?;
        log::info!("chosen audio config: {chosen_config:?}");

        Ok(StreamConfig {
            channels: chosen_config.channels(),
            sample_rate: chosen_config.sample_rate(),
            buffer_size: BufferSize::Fixed(256),
        })
    }

    /// Pause the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device does not support pausing.
    #[inline]
    pub fn pause(&mut self) {
        self.stream.as_ref().map(Stream::pause);
    }

    /// Resume the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if audio can not be resumed.
    #[inline]
    pub fn play(&mut self) -> NesResult<()> {
        match self.stream {
            Some(ref stream) => stream.play()?,
            None => {
                let host = cpal::default_host();
                let device = host
                    .default_output_device()
                    .ok_or_else(|| anyhow!("no available audio devices found"))?;
                log::debug!(
                    "device name: {}",
                    device
                        .name()
                        .as_ref()
                        .map(String::as_str)
                        .unwrap_or("unknown")
                );
                // TODO: update output_frequency if choose_audio_config doesnt support it
                let config = Self::choose_audio_config(&device, self.output_frequency)?;

                let (callback_tx, callback_rx) = channel::bounded::<CallbackMsg>(32);
                let mut callback = Callback::new(
                    self.output_frequency,
                    self.input_frequency / self.output_frequency,
                    self.audio_latency,
                    callback_rx,
                    Arc::clone(&self.buffer_len),
                );
                let stream = device.build_output_stream(
                    &config,
                    move |out, info| {
                        callback.execute(out, info);
                    },
                    |err| eprintln!("an error occurred on stream: {err}"),
                    None,
                )?;
                stream.play()?;
                self.stream = Some(stream);
                self.callback_tx = Some(callback_tx);
            }
        }
        Ok(())
    }

    /// Change the audio output frequency. This changes the sampling ratio.
    pub fn set_output_frequency(&mut self, output_frequency: f32) -> NesResult<()> {
        self.output_frequency = output_frequency;
        if let Some(ref mut callback_tx) = self.callback_tx {
            callback_tx.try_send(CallbackMsg::UpdateSampleRate((
                self.output_frequency,
                self.audio_latency,
            )))?;
        }
        Ok(())
    }

    /// Change the audio latency. This changes the buffer size.
    pub fn set_audio_latency(&mut self, audio_latency: Duration) -> NesResult<()> {
        self.audio_latency = audio_latency;
        if let Some(ref mut callback_tx) = self.callback_tx {
            callback_tx.try_send(CallbackMsg::UpdateSampleRate((
                self.output_frequency,
                self.audio_latency,
            )))?;
        }
        Ok(())
    }

    /// Set whether audio is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Start recording audio to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file can not be created.
    pub fn start_recording(&mut self) -> NesResult<()> {
        if let Some(ref mut callback_tx) = self.callback_tx {
            callback_tx.try_send(CallbackMsg::Record(true))?;
        }
        Ok(())
    }

    /// Stop recording audio to a file.
    pub fn stop_recording(&mut self) {
        if let Some(ref mut callback_tx) = self.callback_tx {
            let _ = callback_tx.try_send(CallbackMsg::Record(false));
        }
    }

    /// Processes and filters generated audio samples.
    pub fn process(&mut self, samples: &[f32]) -> NesResult<()> {
        if let Some(ref mut callback_tx) = self.callback_tx {
            // TODO: create a shared circular buffer of Vecs to avoid allocations
            callback_tx.try_send(CallbackMsg::FrameSamples(if self.enabled {
                samples.to_vec()
            } else {
                vec![0.0; samples.len()]
            }))?;
        }
        Ok(())
    }
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mixer")
            .field("playing", &self.stream.is_some())
            .field("input_frequency", &self.input_frequency)
            .field("output_frequency", &self.output_frequency)
            .field("audio_latency", &self.audio_latency)
            .finish()
    }
}
