pub struct Audio {
    // pub stream: portaudioSteam,
    pub channel: f32,
}

impl Audio {
    pub fn new() -> Self {
        Audio { channel: 44100.0 }
    }
}
