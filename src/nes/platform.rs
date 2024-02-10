use crate::{
    nes::{event::Event, Nes},
    NesResult,
};
use std::future::Future;
use web_time::Duration;
use winit::{
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

    #[cfg(all(not(target_arch = "wasm32"), feature = "profiling"))]
    let _profiling = crate::profiling::start_server()?;
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
    pub fn initialize_platform(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::{closure::Closure, JsCast};

            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body().map(|body| (doc, body)))
                .map(|(doc, body)| {
                    let handle_load_rom = Closure::<dyn FnMut(web_sys::MouseEvent)>::new({
                        let event_proxy = self.event_proxy.clone();
                        move |_| {
                            const TEST_ROM: &[u8] =
                                include_bytes!("../../roms/akumajou_densetsu.nes");
                            if let Err(err) = event_proxy.send_event(Event::LoadRom((
                                "akumajou_densetsu.nes".to_string(),
                                TEST_ROM.to_vec(),
                            ))) {
                                log::error!(
                                    "failed to send load rom message to event_loop: {err:?}"
                                );
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
        if self.config.rom_path.is_file() {
            self.load_rom_path(self.config.rom_path.clone());
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
            use winit::window;

            static WINDOW_ICON: &[u8] = include_bytes!("../../assets/tetanes_icon.png");

            // TODO: file PR to winit to support macos - SDL supports this.
            // May be able to work around it with a macos app bundle.
            self.with_window_icon(
                ImageReader::with_format(Cursor::new(WINDOW_ICON), ImageFormat::Png)
                    .decode()
                    .with_context(|| "failed to decode window icon")
                    .and_then(|png| {
                        let width = png.width();
                        let height = png.height();
                        window::Icon::from_rgba(png.into_rgba8().into_vec(), width, height)
                            .with_context(|| "failed to create window icon")
                    })
                    .map_err(|err| log::error!("{err:?}"))
                    .ok(),
            )
        }
    }
}
