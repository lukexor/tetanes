use crate::{driver::Driver, state::StateData};

impl StateData {
    pub fn enqueue_audio(&mut self, samples: &[f32]) {
        self.driver.enqueue_audio(samples);
    }
}
