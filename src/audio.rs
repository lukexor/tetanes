use crate::{audio::filter::Filter, profile, NesResult};
use anyhow::{anyhow, bail, Context};
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
use thingbuf::{recycling::WithCapacity, ThingBuf};
use web_time::Duration;

pub mod filter;
pub mod window_sinc;

pub trait Audio {
    fn output(&self) -> f32;
}

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
    resample_ratio: f32,
    sample_rate: f32,
    buffer_len: Arc<AtomicUsize>,
    samples_pool: Arc<ThingBuf<Vec<f32>, WithCapacity>>,
    tx: Option<Sender<CallbackMsg>>,
    enabled: bool,
    recording: bool,
}

impl fmt::Debug for Mixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mixer")
            .field("playing", &self.stream.is_some())
            .field("resample_ratio", &self.resample_ratio)
            .field("sample_rate", &self.sample_rate)
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
    pub fn new(resample_ratio: f32, sample_rate: f32, enabled: bool) -> Self {
        Self {
            stream: None,
            resample_ratio,
            sample_rate,
            buffer_len: Arc::new(AtomicUsize::new(0)),
            samples_pool: Arc::new(ThingBuf::with_recycle(
                16,
                // $8000 = 32768, which is the next power of two greater than a single frame of CPU-generated
                // audio samples.
                WithCapacity::new().with_min_capacity(0x8000),
            )),
            tx: None,
            enabled,
            recording: false,
        }
    }

    /// Returns the number of samples queued for playback.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        self.buffer_len.load(Ordering::Relaxed)
    }

    /// Returns the `Duration` of audio queued for playback.
    #[must_use]
    pub fn queued_time(&self) -> Duration {
        let queued_time = self.buffer_len() as f32 / self.sample_rate;
        log::trace!("queued_audio_time: {:.4}s", queued_time);
        Duration::from_secs_f32(queued_time)
    }

    /// Choose the best audio configuration for the given device and sample_rate.
    fn choose_audio_config(device: &Device, sample_rate: f32) -> NesResult<SupportedStreamConfig> {
        let mut supported_configs = device.supported_output_configs()?;
        let desired_sample_rate = SampleRate(sample_rate as u32);
        let chosen_config = supported_configs
            .find(|config| {
                log::debug!("supported config: {config:?}");
                config.max_sample_rate() >= desired_sample_rate
                    && config.sample_format() == SampleFormat::F32
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
        Ok(chosen_config)
    }

    /// Pause the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device does not support pausing.
    pub fn pause(&mut self) -> NesResult<()> {
        if self.recording {
            self.stop_recording()?;
        }
        self.set_enabled(false)?; // in case stream doesn't support pausing
        match self.stream {
            Some(ref stream) => Ok(stream.pause()?),
            None => bail!("failed to pause stream"),
        }
    }

    /// Resume the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio device does not support pausing.
    pub fn play(&mut self) -> NesResult<()> {
        self.set_enabled(self.enabled)?; // in case stream doesn't support resuming
        match self.stream {
            Some(ref stream) => Ok(stream.play()?),
            None => bail!("stream not started"),
        }
    }

    /// Start the audio output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if audio can not be resumed.
    pub fn start(&mut self) -> NesResult<()> {
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

    fn allocate_buffer(sample_rate: f32) -> Vec<f32> {
        const BYTES_PER_SAMPLE: f32 = 4.0;
        const DEFAULT_LATENCY: f32 = 30.0;
        Vec::with_capacity(
            ((sample_rate * DEFAULT_LATENCY * BYTES_PER_SAMPLE) as usize).next_power_of_two(),
        )
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
        let (callback_tx, callback_rx) = channel::bounded::<CallbackMsg>(64);
        self.tx = Some(callback_tx);
        let num_channels = config.channels as usize;
        let mut sample_avg = 0.0;
        let mut sample_count = 0.0;
        let mut decim_fraction = 0.0;
        let mut resample_ratio = self.resample_ratio;
        let mut filters = [
            Filter::high_pass(self.sample_rate, 90.0, 1500.0),
            Filter::high_pass(self.sample_rate, 440.0, 1500.0),
            // NOTE: Should be 14k, but this allows 2X speed within the Nyquist limit
            Filter::low_pass(self.sample_rate, 11_000.0, 1500.0),
        ];
        let buffer_len = Arc::clone(&self.buffer_len);
        let buffer_pool = Arc::clone(&self.samples_pool);
        let mut processed_samples = Self::allocate_buffer(self.sample_rate);
        let mut enabled = self.enabled;
        let mut recording = self.recording;
        let stream = device.build_output_stream(
            &config,
            move |out: &mut [T], _info| {
                profile!("audio callback");

                while let Ok(msg) = callback_rx.try_recv() {
                    match msg {
                        CallbackMsg::UpdateResampleRatio(new_resample_ratio) => {
                            resample_ratio = new_resample_ratio
                        }
                        CallbackMsg::Record(new_recording) => recording = new_recording,
                        CallbackMsg::Enable(e) => enabled = e,
                        CallbackMsg::NewSamples => {
                            if let Some(samples) = buffer_pool.pop_ref() {
                                Self::process_samples(
                                    &samples,
                                    &mut processed_samples,
                                    &mut filters,
                                    resample_ratio,
                                    &mut sample_avg,
                                    &mut sample_count,
                                    &mut decim_fraction,
                                );
                            }
                            if recording {
                                // TODO: Pass off to thread worker to write to file
                                // for sample in &sample_buffer {
                                //     let _ = recording_file.write_all(&sample.to_le_bytes());
                                // }
                            }
                        }
                    }
                }

                log::trace!(
                    "requested samples: {}, available: {}",
                    out.len(),
                    processed_samples.len(),
                );
                let num_samples = out.len() / num_channels;
                let len = num_samples.min(processed_samples.len());
                if enabled {
                    for (frame, sample) in out
                        .chunks_mut(num_channels)
                        .zip(processed_samples.drain(..len).chain(iter::repeat(0.0)))
                    {
                        for out in frame.iter_mut() {
                            *out = T::from_sample(sample);
                        }
                    }
                } else {
                    for (out, _) in out.iter_mut().zip(processed_samples.drain(..len)) {
                        *out = T::from_sample(0.0);
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

    /// Change the audio resample ratio.
    pub fn set_resample_ratio(&mut self, resample_ratio: f32) -> NesResult<()> {
        self.resample_ratio = resample_ratio;
        if let Some(ref callback_tx) = self.tx {
            callback_tx
                .try_send(CallbackMsg::UpdateResampleRatio(self.resample_ratio))
                .context("failed to send update resample event")?;
        }
        Ok(())
    }
    // TODO: add set_sample_rate

    /// Set whether audio is enabled.
    pub fn set_enabled(&mut self, enabled: bool) -> NesResult<()> {
        if let Some(ref callback_tx) = self.tx {
            callback_tx
                .try_send(CallbackMsg::Enable(enabled))
                .context("failed to send audio enable event")?;
        }
        Ok(())
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Start recording audio to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file can not be created.
    pub fn start_recording(&mut self) -> NesResult<()> {
        if let Some(ref callback_tx) = self.tx {
            callback_tx
                .try_send(CallbackMsg::Record(true))
                .context("failed to send start recording audio event")?;
            self.recording = true;
        }
        Ok(())
    }

    /// Stop recording audio to a file.
    pub fn stop_recording(&mut self) -> NesResult<()> {
        if let Some(ref callback_tx) = self.tx {
            callback_tx
                .try_send(CallbackMsg::Record(false))
                .context("failed to send stop recording audio event")?;
            self.recording = false;
        }
        Ok(())
    }

    /// Processes and filters generated audio samples.
    pub fn process(&mut self, samples: &[f32]) -> NesResult<()> {
        if let Some(ref callback_tx) = self.tx {
            if let Ok(mut buffer_slot) = self.samples_pool.push_ref() {
                buffer_slot.extend_from_slice(samples);
                callback_tx
                    .try_send(CallbackMsg::NewSamples)
                    .context("failed to send new audio samples event")?;
            }
        }
        Ok(())
    }
}
