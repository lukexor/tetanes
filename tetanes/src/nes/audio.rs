use crate::nes::config::Config;
use anyhow::{Context, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{
    CachingCons, CachingProd, HeapRb,
    producer::Producer,
    traits::{Consumer, Observer, Split},
};
use std::{
    fs::File,
    io::BufWriter,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tetanes_core::time::Duration;
use tracing::{debug, error, info, trace, warn};

type SampleRb = Arc<HeapRb<f32>>;
/// Set while paused, so the stream callback outputs silence at once rather than playing out
/// whatever is still queued. cpal has no way to drop what the device has already taken, so the
/// stream is left running and silenced here instead.
type Silenced = Arc<AtomicBool>;
type SampleProducer = CachingProd<SampleRb>;
type SampleConsumer = CachingCons<SampleRb>;

/// Represents the state of the audio stream.
#[derive(Debug)]
#[must_use]
pub enum State {
    /// Audio is disabled.
    Disabled,
    /// No audio output device was found or no devices found to support desired configuration.
    NoOutputDevice,
    /// Audio output stream has been started.
    Started,
    /// Audio output stream has been stopped.
    Stopped,
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
pub struct Audio {
    pub enabled: bool,
    pub sample_rate: f32,
    pub latency: Duration,
    pub buffer_size: usize,
    pub host: cpal::Host,
    output: Option<Output>,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("enabled", &self.enabled)
            .field("sample_rate", &self.sample_rate)
            .field("latency", &self.latency)
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
    pub fn new(enabled: bool, mut sample_rate: f32, latency: Duration, buffer_size: usize) -> Self {
        let host = cpal::default_host();
        let output = Output::create(&host, sample_rate, latency, buffer_size);
        if let Some(output) = &output {
            let desired_sample_rate = sample_rate as u32;
            if output.config.sample_rate != desired_sample_rate {
                sample_rate = output.config.sample_rate as f32;
                debug!(
                    "Unable to match desired sample_rate: {desired_sample_rate}. Using {sample_rate} instead",
                );
            }
        }
        Self {
            enabled,
            sample_rate,
            latency,
            buffer_size,
            host,
            output,
        }
    }

    /// Whether the audio mixer is currently enabled.
    pub fn enabled(&self) -> bool {
        self.enabled
            && self
                .output
                .as_ref()
                .and_then(|output| output.mixer.as_ref())
                .is_some_and(|mixer| !mixer.paused)
    }

    /// Returns the current audio device, if any.
    pub fn device(&self) -> Option<&cpal::Device> {
        self.output.as_ref().map(|output| &output.device)
    }

    /// Set whether the audio mixer is enabled. Returns [`State`] representing the state of
    /// the audio stream as a result of being enabled/disabled.
    pub fn set_enabled(&mut self, enabled: bool) -> anyhow::Result<State> {
        self.enabled = enabled;
        if self.enabled {
            self.start()
        } else {
            Ok(self.stop())
        }
    }

    /// Processes generated audio samples.
    pub fn process(&mut self, samples: &[f32]) {
        if let Some(mixer) = &mut self
            .output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
        {
            mixer.process(samples);
        }
    }

    /// Returns the number of audio channels.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.output
            .as_ref()
            .map_or(0, |output| output.config.channels)
    }

    /// How full the output buffer is, from 0.0 empty to 1.0 full, or `None` when there is no
    /// stream to measure.
    ///
    /// The buffer is sized at twice the configured latency, so **0.5 is the target level** and is
    /// what dynamic rate control steers toward. Returns `None` while paused as well as while
    /// stopped: nothing is being queued then, so the level says nothing about the rate.
    #[must_use]
    pub fn buffer_level(&self) -> Option<f32> {
        self.output
            .as_ref()
            .and_then(|output| output.mixer.as_ref())
            .filter(|mixer| !mixer.paused)
            .map(|mixer| {
                mixer.producer.occupied_len() as f32 / mixer.producer.capacity().get() as f32
            })
    }

    /// Returns the `Duration` of audio queued for playback.
    #[must_use]
    pub fn queued_time(&self) -> Duration {
        self.output
            .as_ref()
            .and_then(|output| output.mixer.as_ref())
            .map_or(Duration::default(), |mixer| {
                let queued_seconds =
                    mixer.producer.occupied_len() as f32 / self.sample_rate / mixer.channels as f32;
                Duration::from_secs_f32(queued_seconds)
            })
    }

    /// Pause or resume the audio output stream. If `paused` is false and the stream is not started
    /// yet, it will be started.
    pub fn pause(&mut self, paused: bool) {
        if let Some(mixer) = &mut self
            .output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
        {
            mixer.pause(paused);
        }
    }

    /// Recreate audio output device.
    fn recreate_output(&mut self) -> anyhow::Result<State> {
        let _ = self.stop();
        self.output = Output::create(&self.host, self.sample_rate, self.latency, self.buffer_size);
        self.start()
    }

    /// Set the output sample rate that the audio device uses. Requires restarting the audio stream
    /// and so may fail.
    pub fn set_sample_rate(&mut self, sample_rate: f32) -> anyhow::Result<State> {
        self.sample_rate = sample_rate;
        self.recreate_output()
    }

    /// Set the buffer size used by the audio device for playback. Requires restarting the audio
    /// stream and so may fail.
    pub fn set_buffer_size(&mut self, buffer_size: usize) -> anyhow::Result<State> {
        self.buffer_size = buffer_size;
        self.recreate_output()
    }

    /// Set the latency used by the audio device for playback. Requires restarting the audio
    /// stream and so may fail.
    pub fn set_latency(&mut self, latency: Duration) -> anyhow::Result<State> {
        self.latency = latency;
        self.recreate_output()
    }

    /// Whether the mixer is currently recording samples to a file.
    pub fn is_recording(&self) -> bool {
        self.output
            .as_ref()
            .and_then(|output| output.mixer.as_ref())
            .is_some_and(|mixer| mixer.recording.is_some())
    }

    /// Start recording audio to a file.
    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        if let Some(mixer) = &mut self
            .output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
        {
            mixer.start_recording()
        } else {
            Ok(())
        }
    }

    /// Stop recording audio to a file.
    pub fn stop_recording(&mut self) -> anyhow::Result<Option<PathBuf>> {
        self.output
            .as_mut()
            .and_then(|output| output.mixer.as_mut())
            .map_or(Ok(None), |mixer| mixer.stop_recording())
    }

    /// Start the audio output stream. Returns [`State`] representing the state of the audio stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the audio stream could not be started.
    pub fn start(&mut self) -> anyhow::Result<State> {
        if self.enabled {
            if let Some(output) = &mut self.output {
                output.start()?;
                Ok(State::Started)
            } else {
                Ok(State::NoOutputDevice)
            }
        } else {
            Ok(State::Disabled)
        }
    }

    /// Stop the audio output stream.
    pub fn stop(&mut self) -> State {
        if let Some(output) = &mut self.output {
            output.stop();
            State::Stopped
        } else {
            State::NoOutputDevice
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
    pub fn available_devices(&self) -> anyhow::Result<cpal::Devices> {
        Ok(self.host.devices()?)
    }

    /// Return an iterator over supported device configurations. If no devices are available, `None` is
    /// returned.
    ///
    /// # Errors
    ///
    /// If the device is no longer valid (i.e. has been disconnected), an error is returned.
    pub fn supported_configs(&self) -> Option<anyhow::Result<cpal::SupportedOutputConfigs>> {
        self.output.as_ref().map(|output| {
            output
                .device
                .supported_output_configs()
                .context("failed to get supported configurations")
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
        sample_rate: f32,
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
                .description()
                .as_ref()
                .map(|desc| desc.name())
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
        sample_rate: f32,
        buffer_size: usize,
    ) -> anyhow::Result<(cpal::StreamConfig, cpal::SampleFormat)> {
        let mut supported_configs = device.supported_output_configs()?;
        let desired_sample_rate = sample_rate as u32;
        let desired_buffer_size = buffer_size as u32;
        debug!("desired: sample rate: {desired_sample_rate}, buffer_size: {buffer_size}");

        let chosen_config = supported_configs
            .find(|config| {
                let supports_sample_rate = config.max_sample_rate() >= desired_sample_rate
                    && config.min_sample_rate() <= desired_sample_rate;
                let supports_sample_format = config.sample_format() == cpal::SampleFormat::F32;
                let supports_buffer_size = match config.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => {
                        (*min..=*max).contains(&desired_buffer_size)
                    }
                    cpal::SupportedBufferSize::Unknown => false,
                };
                let supported =
                    supports_sample_rate && supports_sample_format && supports_buffer_size;
                if supported {
                    debug!("supported config: {config:?}",);
                } else {
                    trace!("unsupported config: {config:?}",);
                }
                supported
            })
            .or_else(|| {
                let config = device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut c| c.next());
                debug!("falling back to first supported config: {config:?}");
                config
            })
            .map(|config| {
                debug!("chosen config: {config:?}");
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

    fn start(&mut self) -> anyhow::Result<()> {
        if let Some(ref mixer) = self.mixer {
            mixer.stream.play()?;
            return Ok(());
        }

        info!("starting audio stream with config: {:?}", self.config);
        self.mixer = Some(Mixer::start(
            &self.device,
            self.config,
            self.latency,
            self.sample_format,
        )?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut mixer) = self.mixer.take() {
            mixer.pause(true);
        }
    }
}

#[must_use]
pub(crate) struct Mixer {
    stream: cpal::Stream,
    paused: bool,
    channels: u16,
    sample_rate: u32,
    sample_latency: usize,
    producer: SampleProducer,
    silenced: Silenced,
    processed_samples: Vec<f32>,
    recording: Option<(PathBuf, hound::WavWriter<BufWriter<File>>)>,
}

impl std::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("paused", &self.paused)
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("sample_latency", &self.sample_latency)
            .field("queued_len", &self.producer.occupied_len())
            .field("processed_len", &self.processed_samples.len())
            .field("recording", &self.recording.is_some())
            .finish_non_exhaustive()
    }
}

/// A gain envelope that ramps between silence and full playback over a couple of milliseconds.
///
/// Starting or stopping the stream by jumping straight to or from zero puts a step into the
/// output, and a step is heard as a pop. Ramping removes it. The ramp is deliberately short: long
/// enough that the discontinuity is gone, short enough that pausing still feels immediate.
#[derive(Debug)]
struct Fade {
    gain: f32,
    step: f32,
}

impl Fade {
    /// How long a full ramp takes. Below roughly a millisecond the step starts to be audible
    /// again; much above it and pausing feels like it lags.
    const DURATION: Duration = Duration::from_millis(2);

    fn new(sample_rate: u32, channels: u16) -> Self {
        let samples = Self::DURATION.as_secs_f32() * sample_rate as f32 * f32::from(channels);
        Self {
            gain: 0.0,
            step: samples.max(1.0).recip(),
        }
    }

    /// Advance one sample toward `target` and return the gain to apply.
    fn next(&mut self, target: f32) -> f32 {
        self.gain = if self.gain < target {
            (self.gain + self.step).min(target)
        } else {
            (self.gain - self.step).max(target)
        };
        self.gain
    }

    fn is_silent(&self) -> bool {
        self.gain <= 0.0
    }

    const fn silence(&mut self) {
        self.gain = 0.0;
    }
}

impl Mixer {
    fn start(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        latency: Duration,
        sample_format: cpal::SampleFormat,
    ) -> anyhow::Result<Self> {
        use cpal::SampleFormat;

        let channels = config.channels;
        let sample_rate = config.sample_rate;
        let sample_latency =
            (latency.as_secs_f32() * sample_rate as f32 * channels as f32).ceil() as usize;
        let processed_samples = Vec::with_capacity(2 * sample_latency);
        let buffer = HeapRb::<f32>::new(2 * sample_latency);
        let (producer, consumer) = buffer.split();
        let silenced = Silenced::default();

        macro_rules! stream {
            ($ty:ty) => {
                Self::make_stream::<$ty>(
                    device,
                    config,
                    consumer,
                    sample_latency,
                    Arc::clone(&silenced),
                )
            };
        }
        let stream = match sample_format {
            SampleFormat::I8 => stream!(i8),
            SampleFormat::I16 => stream!(i16),
            SampleFormat::I32 => stream!(i32),
            SampleFormat::I64 => stream!(i64),
            SampleFormat::U8 => stream!(u8),
            SampleFormat::U16 => stream!(u16),
            SampleFormat::U32 => stream!(u32),
            SampleFormat::U64 => stream!(u64),
            SampleFormat::F32 => stream!(f32),
            SampleFormat::F64 => stream!(f64),
            sample_format => Err(anyhow!("Unsupported sample format {sample_format}")),
        }?;
        stream.play()?;

        Ok(Self {
            stream,
            paused: false,
            silenced,
            channels,
            sample_rate,
            sample_latency,
            producer,
            processed_samples,
            recording: None,
        })
    }

    /// Pause or resume the audio output stream. If `paused` is false and the stream is not started
    /// yet, it will be started.
    fn pause(&mut self, paused: bool) {
        if paused && !self.paused {
            let _ = self.stop_recording();
            self.processed_samples.clear();
            // FIXME: Currently cpal doesn't let the underlying audio device empty samples before
            // pausing which leads to the remaining audio playing again upon resume. The only work
            // around is to leave the stream playing
        }
        // Silence is immediate in both directions: the callback drops what is queued rather than
        // playing it out, and on resume it waits for a full buffer before consuming again.
        self.silenced.store(paused, Ordering::Relaxed);
        self.paused = paused;
    }

    fn start_recording(&mut self) -> anyhow::Result<()> {
        let _ = self.stop_recording();
        let path = Config::default_audio_dir()
            .join(
                chrono::Local::now()
                    .format("recording_%Y-%m-%d_at_%H_%M_%S")
                    .to_string(),
            )
            .with_extension("wav");
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create audio recording directory: {}",
                    parent.display()
                )
            })?;
        }
        let spec = hound::WavSpec {
            channels: self.channels,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let writer =
            hound::WavWriter::create(&path, spec).context("failed to create audio recording")?;
        self.recording = Some((path, writer));
        Ok(())
    }

    fn stop_recording(&mut self) -> anyhow::Result<Option<PathBuf>> {
        if let Some((path, mut recording)) = self.recording.take() {
            match recording.flush() {
                Ok(_) => Ok(Some(path)),
                Err(err) => Err(anyhow!("failed to flush audio recording: {err:?}")),
            }
        } else {
            Ok(None)
        }
    }

    /// How much of an underrun's held sample is left after each one that follows it.
    ///
    /// About 5 ms to silence at any rate this runs at, which is the usual de-click ramp.
    const UNDERRUN_DECAY: f32 = 0.99;

    fn make_stream<T>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        mut consumer: SampleConsumer,
        sample_latency: usize,
        silenced: Silenced,
    ) -> anyhow::Result<cpal::Stream>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        // The device starts pulling as soon as the stream plays, which is before the emulation
        // thread has produced a single sample, so hold it at silence until the ring first holds a
        // full latency's worth. Consuming before then means every callback finds a nearly-empty
        // ring, hands the device a few real samples and pads the rest, and it is the padding -
        // once per callback, for as long as the queue takes to fill - that is heard as a garble.
        let mut primed = false;
        // What the ring last produced, for when it comes up short after that.
        let mut held = 0.0;
        let mut fade = Fade::new(config.sample_rate, config.channels);

        Ok(device.build_output_stream(
            config,
            move |out: &mut [T], _info| {
                let silenced = silenced.load(Ordering::Relaxed);

                // Once the ramp has reached silence there is nothing left to play out, so drop
                // what is queued: resuming re-primes from a full buffer rather than from whatever
                // was mid-flight when the pause happened.
                if silenced && fade.is_silent() {
                    consumer.clear();
                    held = 0.0;
                    primed = false;
                    out.fill(T::from_sample(0.0));
                    return;
                }

                if !primed && !silenced {
                    if consumer.occupied_len() < sample_latency {
                        out.fill(T::from_sample(0.0));
                        return;
                    }
                    primed = true;
                }

                let target = if silenced { 0.0 } else { 1.0 };
                let mut starved = true;
                for sample in out.iter_mut() {
                    // An underrun decays the last value toward silence rather than stepping to
                    // it: a step is a click, a few milliseconds of ramp is not.
                    held = match consumer.try_pop() {
                        Some(value) => {
                            starved = false;
                            value
                        }
                        None => held * Self::UNDERRUN_DECAY,
                    };
                    *sample = T::from_sample(held * fade.next(target));
                }

                // A callback that got nothing at all is a stall rather than a marginal miss:
                // occluded, or between ROMs. Prime again so playback resumes from a full ring
                // instead of clawing its way out of an empty one, and ramp back in when it does.
                if starved && !silenced {
                    primed = false;
                    fade.silence();
                }
            },
            |err| error!("an error occurred on stream: {err}"),
            None,
        )?)
    }

    fn process(&mut self, samples: &[f32]) {
        if self.paused {
            return;
        }
        for sample in samples {
            for _ in 0..self.channels {
                self.processed_samples.push(*sample);
            }
            if let Some((_, recording)) = &mut self.recording {
                // TODO: push slice to recording thread
                if let Err(err) = recording.write_sample(*sample) {
                    error!("failed to write audio sample: {err:?}");
                    let _ = self.stop_recording();
                }
            }
        }
        let processed_len = self.processed_samples.len();
        let len = self.producer.vacant_len().min(processed_len);
        let queued_len = self
            .producer
            .push_iter(&mut self.processed_samples.drain(..len));
        trace!(
            "processed: {processed_len}, queued: {queued_len}, buffer len: {}",
            self.producer.occupied_len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 48 kHz stereo, the common case.
    fn fade() -> Fade {
        Fade::new(48_000, 2)
    }

    /// Pops on pause and resume are steps in the output, so what the envelope has to guarantee is
    /// that it never takes one: every change in gain is bounded by a single ramp step.
    #[test]
    fn the_envelope_never_steps() {
        let mut fade = fade();
        let mut previous = fade.gain;
        // Ramp in, sit at full, ramp out, sit at silence, ramp back in.
        for (target, samples) in [(1.0, 500), (1.0, 100), (0.0, 500), (0.0, 100), (1.0, 500)] {
            for _ in 0..samples {
                let gain = fade.next(target);
                // A hair over `step` for the rounding in the accumulation itself.
                assert!(
                    (gain - previous).abs() <= fade.step * 1.001,
                    "stepped from {previous} to {gain}"
                );
                assert!((0.0..=1.0).contains(&gain), "gain left its range: {gain}");
                previous = gain;
            }
        }
    }

    /// The ramp has to be short enough that pausing feels immediate. Its whole reason for existing
    /// is that an instant cut pops, so this pins both ends of that trade.
    #[test]
    fn the_envelope_ramps_in_a_couple_of_milliseconds() {
        for (rate, channels) in [(44_100, 1), (48_000, 2), (96_000, 2)] {
            let mut fade = Fade::new(rate, channels);
            let mut samples = 0;
            while fade.next(1.0) < 1.0 {
                samples += 1;
                assert!(samples < 1_000_000, "ramp never completed");
            }
            let seconds = samples as f32 / (rate as f32 * f32::from(channels));
            assert!(
                (0.001..=0.004).contains(&seconds),
                "{rate} Hz x{channels} ramped in {}ms",
                seconds * 1000.0
            );
        }
    }

    /// Silencing has to be instant as a *state* even though the gain ramps, or a resume would
    /// pick up the tail of whatever was playing before the pause.
    #[test]
    fn silencing_resets_the_envelope_to_zero() {
        let mut fade = fade();
        while fade.next(1.0) < 1.0 {}
        assert!(!fade.is_silent());

        fade.silence();
        assert!(fade.is_silent(), "must report silence immediately");
        assert_eq!(fade.next(1.0), fade.step, "and ramp back in from zero");
    }
}
