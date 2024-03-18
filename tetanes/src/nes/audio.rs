use crate::nes::config::SampleRate;
use anyhow::anyhow;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{consumer::Consumer, producer::Producer, HeapRb};
use std::{iter, sync::Arc};
use tetanes_util::{platform::time::Duration, NesError, NesResult};
use tracing::{debug, enabled, error, info, trace, warn, Level};

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
pub struct Audio {
    pub input_rate: f32,
    pub output_rate: SampleRate,
    pub resample_ratio: f32,
    pub latency: Duration,
    pub buffer_size: usize,
    pub host: cpal::Host,
    output: Option<Output>,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("input_rate", &self.input_rate)
            .field("output_rate", &self.output_rate)
            .field("resample_ratio", &self.resample_ratio)
            .field("buffer_size", &self.buffer_size)
            .field("output", &self.output)
            .finish_non_exhaustive()
    }
}

impl Audio {
    /// Creates a new audio mixer.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device fails to be opened.
    pub fn new(
        input_rate: f32,
        output_rate: SampleRate,
        latency: Duration,
        buffer_size: usize,
    ) -> Self {
        let host = cpal::default_host();
        let output = Output::create(&host, output_rate, latency, buffer_size);
        Self {
            input_rate,
            output_rate,
            resample_ratio: input_rate / f32::from(output_rate),
            latency,
            buffer_size,
            host,
            output,
        }
    }

    /// Whether the audio mixer is currently enabled.
    pub fn enabled(&self) -> bool {
        self.output
            .as_ref()
            .map(|output| output.mixer.is_some())
            .unwrap_or(false)
    }

    /// Processes generated audio samples.
    pub fn process(&mut self, samples: &[f32]) {
        if let Some(ref mut mixer) = self
            .output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
        {
            mixer.process(self.resample_ratio, samples);
        }
    }

    /// Returns the number of audio channels.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.output
            .as_ref()
            .map_or(0, |output| output.config.channels)
    }

    /// Returns the current of resample ratio.
    #[must_use]
    pub fn resample_ratio(&self) -> f32 {
        self.resample_ratio
    }

    /// Returns the `Duration` of audio queued for playback.
    #[must_use]
    pub fn queued_time(&self) -> Duration {
        self.output
            .as_ref()
            .and_then(|output| output.mixer.as_ref())
            .map_or(Duration::default(), |mixer| {
                let queued_seconds = mixer.producer.len() as f32
                    / f32::from(self.output_rate)
                    / mixer.channels as f32;
                Duration::from_secs_f32(queued_seconds)
            })
    }

    /// Pause or resume the audio output stream. If `paused` is false and the stream is not started
    /// yet, it will be started.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device has not been started yet or does not support pausing.
    pub fn pause(&mut self, paused: bool) -> NesResult<()> {
        if let Some(ref mut mixer) = self
            .output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
        {
            mixer.pause(paused)?;
        }
        Ok(())
    }

    /// Set the input sample rate. This is normally the clock rate of the NES based on the
    /// region. Also changes the resampling ratio for downsampling.
    pub fn set_input_rate(&mut self, sample_rate: f32) {
        self.input_rate = sample_rate;
        self.resample_ratio = self.input_rate / f32::from(self.output_rate);
    }

    /// Set the output sample rate that the audio device uses. Also changes the resampling ratio
    /// for downsampling. Requires restarting the audio stream and so may fail.
    pub fn set_output_rate(&mut self, sample_rate: SampleRate) -> NesResult<()> {
        self.output_rate = sample_rate;
        self.resample_ratio = self.input_rate / f32::from(self.output_rate);
        self.stop();
        self.output = Output::create(&self.host, self.output_rate, self.latency, self.buffer_size);
        self.start()
    }

    /// Set the buffer size used by the audio device for playback. Requires restarting the audio
    /// stream and so may fail.
    pub fn set_buffer_size(&mut self, buffer_size: usize) -> NesResult<()> {
        self.buffer_size = buffer_size;
        self.stop();
        self.output = Output::create(&self.host, self.output_rate, self.latency, self.buffer_size);
        self.start()
    }

    /// Whether the mixer is currently recording samples to a file.
    pub fn is_recording(&self) -> bool {
        self.output
            .as_ref()
            .and_then(|output| output.mixer.as_ref())
            .map_or(false, |mixer| mixer.recording)
    }

    /// Start/stop recording audio to a file.
    pub fn set_recording(&mut self, recording: bool) {
        if let Some(ref mut mixer) = self
            .output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
        {
            mixer.recording = recording;
        }
    }

    /// Start the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if audio is already started or can not be initialized.
    pub fn start(&mut self) -> NesResult<()> {
        if let Some(ref mut output) = self.output {
            output.start()?;
        }
        Ok(())
    }

    /// Stop the audio output stream.
    pub fn stop(&mut self) {
        if let Some(ref mut output) = self.output {
            output.stop();
        }
    }

    /// Returns a list of available hosts for the current platform.
    pub fn available_hosts(&self) -> Vec<cpal::HostId> {
        cpal::available_hosts()
    }

    /// Returns an iterator over the audio devices available to the host on the system. If no
    /// devices are available, `None` is returned.
    ///
    /// # Errors
    ///
    /// If the device is no longer valid (i.e. has been disconnected), an error is returned.
    pub fn available_devices(&self) -> NesResult<cpal::Devices> {
        Ok(self.host.devices()?)
    }

    /// Return an iterator over supported device configurations. If no devices are available, `None` is
    /// returned.
    ///
    /// # Errors
    ///
    /// If the device is no longer valid (i.e. has been disconnected), an error is returned.
    pub fn supported_configs(&self) -> Option<NesResult<cpal::SupportedOutputConfigs>> {
        self.output.as_ref().map(|output| {
            output
                .device
                .supported_output_configs()
                .map_err(NesError::msg)
        })
    }
}

#[must_use]
struct Output {
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    latency: Duration,
    mixer: Option<Mixer>,
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("config", &self.config)
            .field("sample_format", &self.sample_format)
            .field("mixer", &self.mixer)
            .finish_non_exhaustive()
    }
}

impl Output {
    fn create(
        host: &cpal::Host,
        sample_rate: SampleRate,
        latency: Duration,
        buffer_size: usize,
    ) -> Option<Self> {
        let Some(device) = host.default_output_device() else {
            warn!("no available audio devices found");
            return None;
        };
        debug!(
            "device name: {}",
            device
                .name()
                .as_ref()
                .map(String::as_ref)
                .unwrap_or("unknown")
        );
        let (config, sample_format) = match Self::choose_config(&device, sample_rate, buffer_size) {
            Ok(config) => config,
            Err(err) => {
                warn!("failed to find a matching device configuration: {err:?}");
                return None;
            }
        };
        Some(Self {
            device,
            config,
            sample_format,
            latency,
            mixer: None,
        })
    }

    /// Choose the best audio configuration for the given device and sample_rate.
    fn choose_config(
        device: &cpal::Device,
        sample_rate: SampleRate,
        buffer_size: usize,
    ) -> NesResult<(cpal::StreamConfig, cpal::SampleFormat)> {
        let mut supported_configs = device.supported_output_configs()?;
        let desired_sample_rate = cpal::SampleRate(u32::from(sample_rate));
        let desired_buffer_size = buffer_size as u32;
        let chosen_config = supported_configs
            .find(|config| {
                debug!("supported config: {config:?}");
                let supports_sample_rate = config.max_sample_rate() >= desired_sample_rate;
                let supports_sample_format = config.sample_format() == cpal::SampleFormat::F32;
                let supports_buffer_size = match config.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => {
                        (*min..=*max).contains(&desired_buffer_size)
                    }
                    cpal::SupportedBufferSize::Unknown => false,
                };
                supports_sample_rate && supports_sample_format && supports_buffer_size
            })
            .or_else(|| {
                debug!("falling back to first supported output");
                device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut c| c.next())
            })
            .map(|config| {
                debug!("desired sample rate: {desired_sample_rate:?}, chosen config: {config:?}");
                let min_sample_rate = config.min_sample_rate();
                let max_sample_rate = config.max_sample_rate();
                config.with_sample_rate(desired_sample_rate.clamp(min_sample_rate, max_sample_rate))
            })
            .ok_or_else(|| anyhow!("no supported audio configurations found"))?;
        let sample_format = chosen_config.sample_format();
        let buffer_size = match chosen_config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                desired_buffer_size.min(*max).max(*min)
            }
            cpal::SupportedBufferSize::Unknown => desired_buffer_size,
        };
        let mut config = cpal::StreamConfig::from(chosen_config);
        config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
        Ok((config, sample_format))
    }

    fn start(&mut self) -> NesResult<()> {
        if let Some(ref mixer) = self.mixer {
            mixer.stream.play()?;
            return Ok(());
        }

        info!("starting audio stream with config: {:?}", self.config);
        self.mixer = Some(Mixer::start(
            &self.device,
            &self.config,
            self.latency,
            self.sample_format,
        )?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(ref mut mixer) = self.mixer {
            mixer.stop();
        }
    }
}

#[must_use]
pub(crate) struct Mixer {
    stream: cpal::Stream,
    paused: bool,
    channels: u16,
    producer: Producer<f32, AudioRb>,
    processed_samples: Vec<f32>,
    sample_avg: f32,
    sample_count: f32,
    decim_fraction: f32,
    recording: bool,
}

impl std::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("paused", &self.paused)
            .field("channels", &self.channels)
            .field("queued_len", &self.producer.len())
            .field("processed_len", &self.processed_samples.len())
            .field("sample_avg", &self.sample_avg)
            .field("sample_count", &self.sample_count)
            .field("decim_fraction", &self.decim_fraction)
            .field("recording", &self.recording)
            .finish_non_exhaustive()
    }
}

impl Mixer {
    fn start(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        latency: Duration,
        sample_format: cpal::SampleFormat,
    ) -> NesResult<Self> {
        use cpal::SampleFormat;

        let channels = config.channels;
        let sample_rate = config.sample_rate.0;
        let sample_latency =
            (latency.as_secs_f32() * sample_rate as f32 * channels as f32).ceil() as usize;
        let processed_samples = Vec::with_capacity(sample_latency);
        // buffer needs to be larger than sample_latency, otherwise emulation can't queue enough
        // samples. Being a power of two makes the ring buffer more performant.
        let mut buffer_size = sample_latency.next_power_of_two();
        if buffer_size == sample_latency {
            buffer_size *= 2;
        }
        let buffer = HeapRb::<f32>::new(buffer_size);
        let (producer, consumer) = buffer.split();

        let stream = match sample_format {
            SampleFormat::I8 => Self::make_stream::<i8>(device, config, consumer),
            SampleFormat::I16 => Self::make_stream::<i16>(device, config, consumer),
            SampleFormat::I32 => Self::make_stream::<i32>(device, config, consumer),
            SampleFormat::I64 => Self::make_stream::<i64>(device, config, consumer),
            SampleFormat::U8 => Self::make_stream::<u8>(device, config, consumer),
            SampleFormat::U16 => Self::make_stream::<u16>(device, config, consumer),
            SampleFormat::U32 => Self::make_stream::<u32>(device, config, consumer),
            SampleFormat::U64 => Self::make_stream::<u64>(device, config, consumer),
            SampleFormat::F32 => Self::make_stream::<f32>(device, config, consumer),
            SampleFormat::F64 => Self::make_stream::<f64>(device, config, consumer),
            sample_format => Err(anyhow!("Unsupported sample format {sample_format}")),
        }?;
        stream.play()?;

        Ok(Self {
            stream,
            paused: false,
            channels,
            producer,
            processed_samples,
            sample_avg: 0.0,
            sample_count: 0.0,
            decim_fraction: 0.0,
            recording: false,
        })
    }

    fn stop(&mut self) {
        if let Err(err) = self.pause(true) {
            error!("failed to pause stream on stop: {err:?}");
        }
        self.processed_samples.clear();
    }

    /// Pause or resume the audio output stream. If `paused` is false and the stream is not started
    /// yet, it will be started.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device has not been started yet or does not support pausing.
    fn pause(&mut self, paused: bool) -> NesResult<()> {
        if paused && !self.paused {
            if self.recording {
                self.recording = false;
            }
            self.stream.pause()?;
        } else if self.paused {
            self.stream.play()?;
        }
        self.paused = paused;
        Ok(())
    }

    fn make_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        mut consumer: Consumer<f32, AudioRb>,
    ) -> NesResult<cpal::Stream>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        Ok(device.build_output_stream(
            config,
            move |out: &mut [T], _info| {
                #[cfg(feature = "profiling")]
                puffin::profile_scope!("audio callback");

                if enabled!(Level::DEBUG) && consumer.len() < out.len() {
                    warn!("audio underrun: {} < {}", consumer.len(), out.len());
                }

                trace!("playing audio samples: {}", out.len().min(consumer.len()));
                for (sample, value) in out
                    .iter_mut()
                    .zip(consumer.pop_iter().chain(iter::repeat(0.0)))
                {
                    *sample = T::from_sample(value);
                }
            },
            |err| eprintln!("an error occurred on stream: {err}"),
            None,
        )?)
    }

    fn process(&mut self, resample_ratio: f32, samples: &[f32]) {
        Self::downsample(
            samples,
            self.channels,
            resample_ratio,
            &mut self.processed_samples,
            &mut self.sample_avg,
            &mut self.sample_count,
            &mut self.decim_fraction,
        );
        if self.recording {
            // TODO: push slice to recording thread
        }
        let processed_len = self.processed_samples.len();
        let len = self.producer.free_len().min(processed_len);
        let queued_len = self
            .producer
            .push_iter(&mut self.processed_samples.drain(..len));
        trace!(
            "processed: {processed_len}, queued: {queued_len}, buffer len: {}",
            self.producer.len()
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn downsample(
        samples: &[f32],
        channels: u16,
        resample_ratio: f32,
        buffer: &mut Vec<f32>,
        avg: &mut f32,
        count: &mut f32,
        fraction: &mut f32,
    ) {
        for sample in samples {
            *avg += sample;
            *count += 1.0;
            *fraction -= 1.0;
            while *fraction < 1.0 {
                for _ in 0..channels {
                    buffer.push(*sample);
                }
                *avg = 0.0;
                *count = 0.0;
                *fraction += resample_ratio;
            }
        }
    }
}
