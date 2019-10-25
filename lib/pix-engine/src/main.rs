use pix_engine::*;

struct App {}

impl App {
    fn new() -> Self {
        Self {}
    }
}

impl State for App {
    fn on_start(&mut self, _data: &mut StateData) -> PixEngineResult<()> {
        Ok(())
    }
    fn on_update(&mut self, _elapsed: f32, _data: &mut StateData) -> PixEngineResult<()> {
        Ok(())
    }
    fn on_stop(&mut self, _data: &mut StateData) -> PixEngineResult<()> {
        Ok(())
    }
}

pub fn main() {
    let app = App::new();
    let vsync = false;
    let mut engine = PixEngine::new("App", app, 800, 600, vsync).expect("valid engine");
    engine.run().expect("engine run");
}
