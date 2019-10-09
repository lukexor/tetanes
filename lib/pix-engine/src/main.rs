use pix_engine::*;
use std::time::Duration;

struct App {}

impl App {
    fn new() -> Self {
        Self {}
    }
}

impl State for App {
    fn on_start(&mut self, _data: &mut StateData) -> bool {
        true
    }
    fn on_update(&mut self, _elapsed: Duration, _data: &mut StateData) -> bool {
        true
    }
    fn on_stop(&mut self, _data: &mut StateData) -> bool {
        true
    }
}

pub fn main() {
    let app = App::new();
    let mut engine = PixEngine::new("App", app, 800, 600);
    engine.run().unwrap();
}
