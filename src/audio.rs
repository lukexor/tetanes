use crate::{audio::filter::Filter, NesResult};
use anyhow::anyhow;
#[cfg(not(target_arch = "wasm32"))]
use pix_engine::prelude::*;
use ringbuf::{Consumer, HeapRb, Producer, SharedRb};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use std::{fmt, mem::MaybeUninit, sync::Arc};

pub mod filter;
pub mod window_sinc;

type RbRef = Arc<SharedRb<f32, Vec<MaybeUninit<f32>>>>;

pub trait Audio {
    fn output(&self) -> f32;
}

pub struct NesAudioCallback {
    initialized: bool,
    buffer: Consumer<f32, RbRef>,
}

impl NesAudioCallback {
    const fn new(buffer: Consumer<f32, RbRef>) -> Self {
        Self {
            initialized: false,
            buffer,
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn read(&mut self, out: &mut [f32]) {
        if !self.initialized && self.buffer.len() < out.len() {
            out.fill(0.0);
            return;
        }
        self.initialized = true;

        for val in out {
            if let Some(sample) = self.buffer.pop() {
                *val = sample;
            } else {
                *val = 0.0;
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AudioCallback for NesAudioCallback {
    type Channel = f32;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        self.read(out);
    }
}

impl fmt::Debug for NesAudioCallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NesAudioCallback")
            .field("initialized", &self.initialized)
            .field("buffer_len", &self.buffer.len())
            .field("buffer_capacity", &self.buffer.capacity())
            .finish()
    }
}

#[must_use]
pub struct AudioMixer {
    #[cfg(not(target_arch = "wasm32"))]
    device: Option<AudioDevice<NesAudioCallback>>,
    producer: Producer<f32, RbRef>,
    consumer: Option<Consumer<f32, RbRef>>,
    input_frequency: f32,
    output_frequency: f32,
    decim_ratio: f32,
    pitch_ratio: f32,
    fraction: f32,
    avg: f32,
    count: f32,
    filters: [Filter; 3],
}

impl AudioMixer {
    pub fn new(input_frequency: f32, output_frequency: f32, buffer_size: usize) -> Self {
        let buffer = HeapRb::<f32>::new(buffer_size);
        let (producer, consumer) = buffer.split();
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            device: None,
            producer,
            consumer: Some(consumer),
            input_frequency,
            output_frequency,
            decim_ratio: input_frequency / output_frequency,
            pitch_ratio: 1.0,
            fraction: 0.0,
            avg: 0.0,
            count: 0.0,
            filters: [
                Filter::high_pass(output_frequency, 90.0, 1500.0),
                Filter::high_pass(output_frequency, 440.0, 1500.0),
                // Should be 14k, but this allows 2X speed within the Nyquist limit
                Filter::low_pass(output_frequency, 12_000.0, 1500.0),
            ],
        }
    }

    #[must_use]
    pub const fn output_frequency(&self) -> f32 {
        self.output_frequency
    }

    /// Opens audio callback device for playback
    ///
    /// # Errors
    ///
    /// This function will return an error if the audio device fails to be opened, or if
    /// `open_playback` is called more than once.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_playback(&mut self, s: &mut PixState) -> NesResult<()> {
        match self.consumer.take() {
            Some(consumer) => {
                let spec = AudioSpecDesired {
                    freq: Some(self.output_frequency as i32),
                    channels: Some(1),
                    samples: Some((self.capacity() / 2) as u16),
                };
                self.device =
                    Some(s.open_playback(None, &spec, |_| NesAudioCallback::new(consumer))?);
                Ok(())
            }
            None => Err(anyhow!("can only open_playback once")),
        }
    }

    /// Returns audio buffer device for consuming audio samples.
    ///
    /// # Errors
    ///
    /// This function will return an error if `open_buffer` is called more than once.
    pub fn open_callback(&mut self) -> NesResult<NesAudioCallback> {
        match self.consumer.take() {
            Some(consumer) => Ok(NesAudioCallback::new(consumer)),
            None => Err(anyhow!("can only open_buffer exactly once")),
        }
    }

    /// Resets the audio callback device.
    ///
    /// # Errors
    ///
    /// This function will return an error if the audio device fails to be opened.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reset(&mut self, buffer_size: usize) {
        self.decim_ratio = self.input_frequency / self.output_frequency;
        self.pitch_ratio = 1.0;
        self.fraction = 0.0;
        let buffer = HeapRb::<f32>::new(buffer_size);
        let (producer, consumer) = buffer.split();
        self.producer = producer;
        self.consumer = Some(consumer);
    }

    #[inline]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn resume(&mut self) {
        if let Some(ref mut device) = self.device {
            device.resume();
        }
    }

    #[inline]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn pause(&mut self) {
        if let Some(ref mut device) = self.device {
            device.pause();
        }
    }

    #[inline]
    pub fn set_input_frequency(&mut self, input_frequency: f32) {
        self.input_frequency = input_frequency;
    }

    #[inline]
    pub fn set_output_frequency(&mut self, output_frequency: f32) {
        self.output_frequency = output_frequency;
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.producer.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.producer.is_empty()
    }

    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.producer.capacity()
    }

    #[inline]
    #[must_use]
    pub const fn pitch_ratio(&self) -> f32 {
        self.pitch_ratio
    }

    /// Outputs audio using multi-rate-control re-sampling.
    ///
    /// Sources:
    /// - <https://near.sh/articles/audio/dynamic-rate-control>
    /// - <https://github.com/libretro/docs/blob/master/archive/ratecontrol.pdf>
    pub fn consume(
        &mut self,
        samples: &[f32],
        dynamic_rate_control: bool,
        max_delta: f32,
    ) -> usize {
        self.pitch_ratio = if dynamic_rate_control {
            let size = self.producer.len() as f32;
            let capacity = self.producer.capacity() as f32;
            ((capacity - 2.0 * size) / capacity).mul_add(max_delta, 1.0)
        } else {
            1.0
        };
        self.decim_ratio = self.input_frequency / (self.output_frequency * self.pitch_ratio);
        let mut sample_count = 0;
        for sample in samples {
            self.avg += *sample;
            self.count += 1.0;
            while self.fraction <= 0.0 {
                let sample = self
                    .filters
                    .iter_mut()
                    .fold(self.avg / self.count, |sample, filter| filter.apply(sample));
                if self.producer.push(sample).is_err() {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        std::thread::sleep(Duration::from_micros(10));
                    }
                }
                self.avg = 0.0;
                self.count = 0.0;
                sample_count += 1;
                self.fraction += self.decim_ratio;
            }
            self.fraction -= 1.0;
        }
        sample_count
    }
}

impl fmt::Debug for AudioMixer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioMixer")
            .field("producer_len", &self.producer.len())
            .field("producer_capacity", &self.producer.capacity())
            .field("input_frequency", &self.input_frequency)
            .field("output_frequency", &self.output_frequency)
            .field("decim_ratio", &self.decim_ratio)
            .field("pitch_ratio", &self.pitch_ratio)
            .field("fraction", &self.fraction)
            .field("filters", &self.filters)
            .finish()
    }
}
