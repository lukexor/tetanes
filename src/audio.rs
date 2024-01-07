use crate::{audio::filter::Filter, profile, NesResult};
use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Device, FromSample, SampleFormat, SampleRate, SizedSample, Stream, StreamConfig,
    SupportedStreamConfig,
};
use crossbeam::channel::{self, Sender};
use std::{
    fmt, iter,
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
    Enable(bool),
    Record(bool),
}

#[must_use]
pub struct Mixer {
    stream: Option<Stream>,
    clock_rate: f32,
    sample_rate: f32,
    audio_latency: Duration,
    buffer_len: Arc<AtomicUsize>,
    callback_tx: Option<Sender<CallbackMsg>>,
    enabled: bool,
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mixer")
            .field("playing", &self.stream.is_some())
            .field("input_frequency", &self.clock_rate)
            .field("sample_rate", &self.sample_rate)
            .field("audio_latency", &self.audio_latency)
            .field("buffer_len", &self.buffer_len)
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl Mixer {
    /// Creates a new audio mixer.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device fails to be opened.
    pub fn new(clock_rate: f32, sample_rate: f32, audio_latency: Duration, enabled: bool) -> Self {
        Self {
            stream: None,
            clock_rate,
            sample_rate,
            audio_latency,
            buffer_len: Arc::new(AtomicUsize::new(0)),
            callback_tx: None,
            enabled,
        }
    }

    /// Returns the number of samples queued for playback.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer_len.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn queued_time(&self) -> Duration {
        Duration::from_secs_f64(self.buffer_len() as f64 / self.sample_rate as f64)
    }

    fn choose_audio_config(device: &Device, sample_rate: f32) -> NesResult<SupportedStreamConfig> {
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
        Ok(chosen_config)
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
                let config = Self::choose_audio_config(&device, self.sample_rate)?;

                self.stream = Some(match config.sample_format() {
                    SampleFormat::I8 => self.make_stream::<i8>(&device, config.into()),
                    SampleFormat::I16 => self.make_stream::<i16>(&device, config.into()),
                    SampleFormat::I32 => self.make_stream::<i32>(&device, config.into()),
                    SampleFormat::I64 => self.make_stream::<i64>(&device, config.into()),
                    SampleFormat::U8 => self.make_stream::<u8>(&device, config.into()),
                    SampleFormat::U16 => self.make_stream::<u16>(&device, config.into()),
                    SampleFormat::U32 => self.make_stream::<u32>(&device, config.into()),
                    SampleFormat::U64 => self.make_stream::<u64>(&device, config.into()),
                    SampleFormat::F32 => self.make_stream::<f32>(&device, config.into()),
                    SampleFormat::F64 => self.make_stream::<f64>(&device, config.into()),
                    sample_format => Err(anyhow!("Unsupported sample format {sample_format}")),
                }?);
            }
        }
        Ok(())
    }

    #[inline]
    fn allocate_buffer(sample_rate: f32, latency: Duration) -> Vec<f32> {
        const BYTES_PER_SAMPLE: f32 = 4.0;

        let buffer_size =
            ((sample_rate * latency.as_secs_f32() * BYTES_PER_SAMPLE) as usize).next_power_of_two();
        Vec::with_capacity(buffer_size)
    }

    #[inline]
    fn process_samples(
        samples: &[f32],
        buffer: &mut Vec<f32>,
        filters: &mut [Filter],
        resample_ratio: f32,
        avg: &mut f32,
        count: &mut f32,
        fraction: &mut f32,
    ) {
        log::trace!("frame samples: {}", samples.len());
        let prev_samples_len = buffer.len();
        for sample in samples {
            *avg += sample;
            *count += 1.0;
            *fraction -= 1.0;
            while *fraction < 1.0 {
                let sample = filters
                    .iter_mut()
                    .fold(*avg / *count, |sample, filter| filter.apply(sample));
                buffer.push(sample);
                *avg = 0.0;
                *count = 0.0;
                *fraction += resample_ratio;
            }
        }
        log::trace!("pushed samples: {}", buffer.len() - prev_samples_len);
    }

    fn make_stream<T>(&mut self, device: &Device, mut config: StreamConfig) -> NesResult<Stream>
    where
        T: SizedSample + FromSample<f32>,
    {
        config.buffer_size = BufferSize::Fixed(256);
        log::info!("creating audio stream with config: {config:?}");

        self.sample_rate = config.sample_rate.0 as f32;
        let (callback_tx, callback_rx) = channel::bounded::<CallbackMsg>(32);
        self.callback_tx = Some(callback_tx);
        let num_channels = config.channels as usize;
        let mut sample_avg = 0.0;
        let mut sample_count = 0.0;
        let mut decim_fraction = 0.0;
        let clock_rate = self.clock_rate;
        let mut resample_ratio = self.clock_rate / self.sample_rate;
        let mut filters = [
            Filter::high_pass(self.sample_rate, 90.0, 1500.0),
            Filter::high_pass(self.sample_rate, 440.0, 1500.0),
            // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
            Filter::low_pass(self.sample_rate, 11_000.0, 1500.0),
        ];
        let buffer_len = Arc::clone(&self.buffer_len);
        let mut processed_samples = Self::allocate_buffer(self.sample_rate, self.audio_latency);
        let mut enabled = self.enabled;
        let stream = device.build_output_stream(
            &config,
            move |out: &mut [T], _info| {
                profile!("audio callback");

                while let Ok(msg) = callback_rx.try_recv() {
                    match msg {
                        CallbackMsg::UpdateSampleRate((sample_rate, audio_latency)) => {
                            processed_samples = Mixer::allocate_buffer(sample_rate, audio_latency);
                            resample_ratio = clock_rate / sample_rate;
                        }
                        // TODO: Pass off to thread worker to write to file
                        // if let Some(recording_file) = &mut recording_file {
                        //     for sample in &sample_buffer {
                        //         let _ = recording_file.write_all(&sample.to_le_bytes());
                        //     }
                        // }
                        CallbackMsg::Record(_recording) => todo!(),
                        CallbackMsg::Enable(e) => enabled = e,
                        CallbackMsg::FrameSamples(samples) => Self::process_samples(
                            &samples,
                            &mut processed_samples,
                            &mut filters,
                            resample_ratio,
                            &mut sample_avg,
                            &mut sample_count,
                            &mut decim_fraction,
                        ),
                    }
                }

                log::trace!(
                    "requested samples: {}, available: {}",
                    out.len(),
                    processed_samples.len(),
                );
                let num_samples = out.len() / num_channels;
                let len = num_samples.min(processed_samples.len());
                for (frame, sample) in out
                    .chunks_mut(num_channels)
                    .zip(processed_samples.drain(..len).chain(iter::repeat(0.0)))
                {
                    let sample = if enabled { sample } else { 0.0 };
                    for out in frame.iter_mut() {
                        *out = T::from_sample(sample);
                    }
                }

                buffer_len.store(processed_samples.len(), Ordering::Relaxed);
            },
            |err| eprintln!("an error occurred on stream: {err}"),
            None,
        )?;
        stream.play()?;

        Ok(stream)
    }

    /// Change the audio sample rate. This alsp changes the sampling ratio.
    pub fn set_sample_rate(&mut self, sample_rate: f32) -> NesResult<()> {
        self.sample_rate = sample_rate;
        if let Some(ref mut callback_tx) = self.callback_tx {
            callback_tx.try_send(CallbackMsg::UpdateSampleRate((
                self.sample_rate,
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
                self.sample_rate,
                self.audio_latency,
            )))?;
        }
        Ok(())
    }

    /// Set whether audio is enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        if let Some(ref mut callback_tx) = self.callback_tx {
            let _ = callback_tx.try_send(CallbackMsg::Enable(enabled));
        }
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
            callback_tx.try_send(CallbackMsg::FrameSamples(samples.to_vec()))?;
        }
        Ok(())
    }
}
