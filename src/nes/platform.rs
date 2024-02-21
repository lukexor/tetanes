use crate::{
    nes::{config::Config, Nes},
    NesResult,
};
use std::{future::Future, io::Read};
use web_time::Duration;
use winit::{
    dpi::LogicalSize,
    event::Event as WinitEvent,
    event_loop::{EventLoop, EventLoopWindowTarget},
};

/// Spawn a future to be run until completion.
pub fn spawn<F>(future: F) -> NesResult<()>
where
    F: Future<Output = NesResult<()>> + 'static,
{
    let execute = async {
        if let Err(err) = future.await {
            log::error!("spawned future failed: {err:?}");
        }
    };

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(execute);

    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(execute);

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn sleep(_duration: Duration) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn sleep(duration: Duration) {
    std::thread::sleep(duration);
}

impl Nes {
    #[cfg(target_arch = "wasm32")]
    pub fn initialize_platform(&mut self) {
        use crate::event::Event;
        use wasm_bindgen::{closure::Closure, JsCast};

        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body().map(|body| (doc, body)))
            .map(|(doc, body)| {
                let handle_load_rom = Closure::<dyn FnMut(web_sys::MouseEvent)>::new({
                    let event_proxy = self.event_proxy.clone();
                    move |_| {
                        const TEST_ROM: &[u8] = include_bytes!("../../roms/akumajou_densetsu.nes");
                        if let Err(err) = event_proxy.send_event(Event::LoadRom((
                            "akumajou_densetsu.nes".to_string(),
                            TEST_ROM.to_vec(),
                        ))) {
                            log::error!("failed to send load rom message to event_loop: {err:?}");
                        }
                    }
                });

                let load_rom_btn = doc.create_element("button").expect("created button");
                load_rom_btn.set_text_content(Some("Load ROM"));
                load_rom_btn
                    .add_event_listener_with_callback(
                        "click",
                        handle_load_rom.as_ref().unchecked_ref(),
                    )
                    .expect("added event listener");
                body.append_child(&load_rom_btn).ok();
                handle_load_rom.forget();

                let handle_pause = Closure::<dyn FnMut(web_sys::MouseEvent)>::new({
                    let event_proxy = self.event_proxy.clone();
                    move |_| {
                        if let Err(err) = event_proxy.send_event(Event::Pause) {
                            log::error!("failed to send pause message to event_loop: {err:?}");
                        }
                    }
                });

                let pause_btn = doc.create_element("button").expect("created button");
                pause_btn.set_text_content(Some("Pause"));
                pause_btn
                    .add_event_listener_with_callback(
                        "click",
                        handle_pause.as_ref().unchecked_ref(),
                    )
                    .expect("added event listener");
                body.append_child(&pause_btn).ok();
                handle_pause.forget();
            })
            .expect("couldn't append canvas to document body");
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn initialize_platform(&mut self) {
        use crate::nes::event::DeckEvent;

        if self.config.rom_path.is_file() {
            use crate::filesystem;
            use anyhow::Context;
            use std::fs::File;

            let path = &self.config.rom_path;
            let filename = filesystem::filename(path);
            match File::open(path).with_context(|| format!("failed to open rom {path:?}")) {
                Ok(mut rom) => {
                    let mut buffer = Vec::new();
                    rom.read_to_end(&mut buffer).unwrap();
                    self.send_event(DeckEvent::LoadRom((filename.to_string(), buffer)));
                }
                Err(err) => self.on_error(err),
            }
        }
    }
}

/// Extension trait for `EventLoop` that provides platform-specific behavior.
pub trait EventLoopExt<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(WinitEvent<T>, &EventLoopWindowTarget<T>) + 'static;
}

impl<T> EventLoopExt<T> for EventLoop<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(WinitEvent<T>, &EventLoopWindowTarget<T>) + 'static,
    {
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::EventLoopExtWebSys;
            self.spawn(event_handler);
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.run(event_handler)?;

        Ok(())
    }
}

/// Extension trait for `WindowBuilder` that provides platform-specific behavior.
pub trait WindowBuilderExt {
    /// Sets platform-specific window options.
    fn with_platform(self) -> Self;
}

impl WindowBuilderExt for winit::window::WindowBuilder {
    /// Sets platform-specific window options.
    fn with_platform(self) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowBuilderExtWebSys;
            // TODO: insert into specific section in the DOM
            self.with_append(true)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            use anyhow::Context;
            use image::{io::Reader as ImageReader, ImageFormat};
            use std::io::Cursor;

            static WINDOW_ICON: &[u8] = include_bytes!("../../assets/tetanes_icon.png");

            let icon = ImageReader::with_format(Cursor::new(WINDOW_ICON), ImageFormat::Png)
                .decode()
                .context("failed to decode window icon");

            self.with_window_icon(
                icon.and_then(|png| {
                    let width = png.width();
                    let height = png.height();
                    winit::window::Icon::from_rgba(png.into_rgba8().into_vec(), width, height)
                        .with_context(|| "failed to create window icon")
                })
                .map_err(|err| log::error!("{err:?}"))
                .ok(),
            )
        }
    }
}

pub trait WindowExt {
    fn inner_dimensions(&self) -> (LogicalSize<f32>, LogicalSize<f32>);
    fn inner_dimensions_with_spacing(&self, x: f32, y: f32)
        -> (LogicalSize<f32>, LogicalSize<f32>);
}

impl WindowExt for Config {
    fn inner_dimensions(&self) -> (LogicalSize<f32>, LogicalSize<f32>) {
        let (width, height) = self.dimensions();
        let scale = f32::from(self.scale);
        (
            LogicalSize::new(width, height),
            LogicalSize::new(width / scale, height / scale),
        )
    }

    fn inner_dimensions_with_spacing(
        &self,
        x: f32,
        y: f32,
    ) -> (LogicalSize<f32>, LogicalSize<f32>) {
        let (inner_size, min_inner_size) = self.inner_dimensions();
        let scale = f32::from(self.scale);
        (
            LogicalSize::new(inner_size.width + x, inner_size.height + y),
            LogicalSize::new(
                min_inner_size.width + x / scale,
                min_inner_size.height + y / scale,
            ),
        )
    }
}
