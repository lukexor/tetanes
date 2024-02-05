use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopWindowTarget},
};

pub trait EventLoopExt<T> {
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static;
}

impl<T> EventLoopExt<T> for EventLoop<T> {
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static,
    {
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::EventLoopExtWebSys;
            Ok(self.spawn(event_handler))
        }

        #[cfg(not(target_arch = "wasm32"))]
        Ok(self.run(event_handler)?)
    }
}

pub trait WindowBuilderExt {
    fn with_platform(self) -> Self;
}

impl WindowBuilderExt for winit::window::WindowBuilder {
    fn with_platform(self) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowBuilderExtWebSys;
            // TODO: insert into specific section in the DOM
            self.with_append(true)
        }

        #[cfg(not(target_arch = "wasm32"))]
        self
    }
}
