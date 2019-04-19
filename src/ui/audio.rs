use std::error::Error;

pub struct Audio {
    // pub stream: portaudioSteam,
    pub sample_rate: f64,
    pub output_channels: usize,
    pub channel: f32,
}

impl Audio {
    pub fn new() -> Result<Self, Box<Error>> {
        Ok(Audio {
            sample_rate: 0.0,
            output_channels: 0,
            channel: 44100.0,
        })
    }
}
