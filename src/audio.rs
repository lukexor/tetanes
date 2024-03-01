use crate::{
    audio::filter::Filter, nes::config::SampleRate, platform::time::Duration, profile, NesResult,
};
use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BufferSize, Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    SupportedBufferSize,
};
use ringbuf::{HeapRb, Producer};
use std::{fmt, iter, sync::Arc};

pub mod filter;
pub mod window_sinc;

pub trait Audio {
    fn output(&self) -> f32;
}

type AudioRb = Arc<HeapRb<f32>>;

#[derive(Debug)]
#[must_use]
pub enum CallbackMsg {
    NewSamples,
    UpdateResampleRatio(f32),
    Enable(bool),
    Record(bool),
}

#[must_use]
pub struct Mixer {
    stream: Option<Stream>,
    producer: Option<Producer<f32, AudioRb>>,
    input_rate: f32,
    sample_rate: SampleRate,
    resample_ratio: f32,
    latency: Duration,
    sample_latency: usize,
    num_channels: usize,
    sample_avg: f32,
    sample_count: f32,
    decim_fraction: f32,
    filters: [Filter; 3],
    processed_samples: Vec<f32>,
    recording: bool,
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mixer")
            .field("playing", &self.stream.is_some())
            .field("buffer_len", &self.buffer_len())
            .field("input_rate", &self.input_rate)
            .field("sample_rate", &self.sample_rate)
            .field("resample_ratio", &self.resample_ratio)
            .field("latency", &self.latency)
            .field("sample_latency", &self.sample_latency)
            .field("num_channels", &self.num_channels)
            .field("sample_avg", &self.sample_avg)
            .field("sample_count", &self.sample_count)
            .field("decim_fraction", &self.decim_fraction)
            .field("filters", &self.filters)
            .field("recording", &self.recording)
            .finish()
    }
}

impl Mixer {
    /// Creates a new audio mixer.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device fails to be opened.
    pub fn new(input_rate: f32, sample_rate: SampleRate, latency: Duration) -> Self {
        let sample_latency = (f32::from(sample_rate) * latency.as_secs_f32()) as usize;
        Self {
            stream: None,
            producer: None,
            input_rate,
            sample_rate,
            resample_ratio: input_rate / f32::from(sample_rate),
            latency,
            sample_latency,
            num_channels: 1,
            sample_avg: 0.0,
            sample_count: 0.0,
            decim_fraction: 0.0,
            filters: [
                Filter::high_pass(sample_rate, 90.0, 1500.0),
                Filter::high_pass(sample_rate, 440.0, 1500.0),
                // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
                Filter::low_pass(sample_rate, 11_000.0, 1500.0),
            ],
            // x2 for fast-forward and x2 for stereo
            processed_samples: Vec::with_capacity(
                sample_latency.max((f32::from(SampleRate::MAX) / 60.0 * 2.0 * 2.0) as usize),
            ),
            recording: false,
        }
    }

    /// Processes and filters generated audio samples.
    pub fn process(&mut self, samples: &[f32]) -> NesResult<()> {
        if let Some(ref mut producer) = self.producer {
            Self::downsample(
                samples,
                self.num_channels,
                self.resample_ratio,
                &mut self.processed_samples,
                &mut self.filters,
                &mut self.sample_avg,
                &mut self.sample_count,
                &mut self.decim_fraction,
            );
            if self.recording {
                // TODO: push slice to recording thread
            }
            let len = producer.free_len().min(self.processed_samples.len());
            let queued_samples = producer.push_iter(&mut self.processed_samples.drain(..len));
            log::trace!("queued: {queued_samples}, buffer len: {}", producer.len());
        }
        Ok(())
    }

    /// Returns the number of samples queued for playback.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.producer.as_ref().map(|p| p.len()).unwrap_or(0)
    }

    /// Returns the number of audio channels.
    #[must_use]
    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    /// Returns the current of resample ratio.
    #[must_use]
    pub fn resample_ratio(&self) -> f32 {
        self.resample_ratio
    }

    /// Returns the `Duration` of audio queued for playback.
    #[must_use]
    pub fn queued_time(&self) -> Duration {
        let queued_time =
            self.buffer_len() as f32 / f32::from(self.sample_rate) / self.num_channels as f32;
        Duration::from_secs_f32(queued_time)
    }

    /// Pause or resume the audio output stream. If `paused` is false and the stream is not started
    /// yet, it will be started.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device has not been started yet or does not support pausing.
    pub fn pause(&mut self, paused: bool) -> NesResult<()> {
        if paused && self.recording {
            self.set_recording(false);
        }
        if let Some(ref stream) = self.stream {
            if paused {
                stream.pause()?;
            } else {
                stream.play()?;
            }
        }
        Ok(())
    }

    /// Change the audio resample ratio.
    pub fn set_input_rate(&mut self, input_rate: f32) {
        self.input_rate = input_rate;
        self.resample_ratio = self.input_rate / f32::from(self.sample_rate);
    }

    // TODO: add set_sample_rate

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Start/stop recording audio to a file.
    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }

    /// Start the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if audio is already started or can not be initialized.
    pub fn start(&mut self) -> NesResult<SampleRate> {
        if let Some(ref stream) = self.stream {
            stream.play()?;
            return Ok(self.sample_rate);
        }

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
        let (config, sample_format) = Self::choose_audio_config(&device, self.sample_rate)?;
        self.sample_rate = SampleRate::try_from(config.sample_rate.0)?;
        self.resample_ratio = self.input_rate / f32::from(self.sample_rate);
        self.num_channels = config.channels as usize;
        self.sample_latency = (f32::from(self.sample_rate)
            * self.latency.as_secs_f32()
            * self.num_channels as f32) as usize;

        match sample_format {
            SampleFormat::I8 => self.make_stream::<i8>(&device, config),
            SampleFormat::I16 => self.make_stream::<i16>(&device, config),
            SampleFormat::I32 => self.make_stream::<i32>(&device, config),
            SampleFormat::I64 => self.make_stream::<i64>(&device, config),
            SampleFormat::U8 => self.make_stream::<u8>(&device, config),
            SampleFormat::U16 => self.make_stream::<u16>(&device, config),
            SampleFormat::U32 => self.make_stream::<u32>(&device, config),
            SampleFormat::U64 => self.make_stream::<u64>(&device, config),
            SampleFormat::F32 => self.make_stream::<f32>(&device, config),
            SampleFormat::F64 => self.make_stream::<f64>(&device, config),
            sample_format => Err(anyhow!("Unsupported sample format {sample_format}")),
        }?;

        Ok(self.sample_rate)
    }

    /// Stop the audio output stream.
    pub fn stop(&mut self) -> NesResult<()> {
        self.pause(true)?;
        self.stream = None;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn downsample(
        samples: &[f32],
        num_channels: usize,
        resample_ratio: f32,
        buffer: &mut Vec<f32>,
        filters: &mut [Filter],
        avg: &mut f32,
        count: &mut f32,
        fraction: &mut f32,
    ) {
        for sample in samples {
            *avg += sample;
            *count += 1.0;
            *fraction -= 1.0;
            while *fraction < 1.0 {
                let sample = filters
                    .iter_mut()
                    .fold(*avg / *count, |sample, filter| filter.apply(sample));
                for _ in 0..num_channels {
                    buffer.push(sample);
                }
                *avg = 0.0;
                *count = 0.0;
                *fraction += resample_ratio;
            }
        }
    }

    /// Choose the best audio configuration for the given device and sample_rate.
    fn choose_audio_config(
        device: &Device,
        sample_rate: SampleRate,
    ) -> NesResult<(StreamConfig, SampleFormat)> {
        let mut supported_configs = device.supported_output_configs()?;
        let desired_sample_rate = cpal::SampleRate(u32::from(sample_rate));
        let desired_buffer_size = if cfg!(target_arch = "wasm32") {
            1024
        } else {
            512
        };
        let chosen_config = supported_configs
            .find(|config| {
                log::debug!("supported config: {config:?}");
                let supports_sample_rate = config.max_sample_rate() >= desired_sample_rate;
                let supports_sample_format = config.sample_format() == SampleFormat::F32;
                let supports_buffer_size = match config.buffer_size() {
                    SupportedBufferSize::Range { min, max } => {
                        (*min..=*max).contains(&desired_buffer_size)
                    }
                    SupportedBufferSize::Unknown => false,
                };
                supports_sample_rate && supports_sample_format && supports_buffer_size
            })
            .or_else(|| {
                log::debug!("falling back to first supported output");
                device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut c| c.next())
            })
            .map(|config| {
                log::debug!(
                    "desired sample rate: {desired_sample_rate:?}, chosen config: {config:?}"
                );
                let min_sample_rate = config.min_sample_rate();
                let max_sample_rate = config.max_sample_rate();
                config.with_sample_rate(desired_sample_rate.clamp(min_sample_rate, max_sample_rate))
            })
            .ok_or_else(|| anyhow!("no supported audio configurations found"))?;
        let sample_format = chosen_config.sample_format();
        let buffer_size = match chosen_config.buffer_size() {
            SupportedBufferSize::Range { min, max } => desired_buffer_size.min(*max).max(*min),
            SupportedBufferSize::Unknown => desired_buffer_size,
        };
        let mut config = StreamConfig::from(chosen_config);
        config.buffer_size = BufferSize::Fixed(buffer_size);
        Ok((config, sample_format))
    }

    fn make_stream<T>(&mut self, device: &Device, config: StreamConfig) -> NesResult<()>
    where
        T: SizedSample + FromSample<f32>,
    {
        log::info!("creating audio stream with config: {config:?}");

        self.processed_samples.reserve(self.sample_latency);
        let buffer = HeapRb::<f32>::new(self.processed_samples.capacity().next_power_of_two());
        let (producer, mut consumer) = buffer.split();
        self.producer = Some(producer);

        let stream = device.build_output_stream(
            &config,
            move |out: &mut [T], _info| {
                profile!("audio callback");

                if log::log_enabled!(log::Level::Debug) && out.len() > consumer.len() {
                    log::debug!("audio underrun: {} < {}", consumer.len(), out.len());
                }

                log::trace!("playing audio samples: {}", out.len().min(consumer.len()));
                for (sample, value) in out
                    .iter_mut()
                    .zip(consumer.pop_iter().chain(iter::repeat(0.0)))
                {
                    *sample = T::from_sample(value);
                }
            },
            |err| eprintln!("an error occurred on stream: {err}"),
            None,
        )?;
        stream.play()?;
        self.stream = Some(stream);

        Ok(())
    }
}
