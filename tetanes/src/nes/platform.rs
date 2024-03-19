use crate::nes::{config::Config, event::EmulationEvent, Nes, NesResult};
use tracing::error;
use winit::{
    dpi::LogicalSize,
    event::Event,
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::WindowBuilder,
};

#[cfg(target_arch = "wasm32")]
pub mod html_ids {
    pub const CANVAS: &str = "frame";
    pub const ROM_INPUT: &str = "load-rom";
}

#[cfg(target_arch = "wasm32")]
pub fn get_canvas() -> Option<web_sys::HtmlCanvasElement> {
    use wasm_bindgen::JsCast;
    use web_sys::{window, HtmlCanvasElement};

    window()
        .and_then(|win| win.document())
        .and_then(|doc| doc.get_element_by_id(html_ids::CANVAS))
        .and_then(|canvas| canvas.dyn_into::<HtmlCanvasElement>().ok())
}

impl Nes {
    #[cfg(target_arch = "wasm32")]
    pub fn initialize_platform(&mut self) -> NesResult<()> {
        use crate::nes::event::{NesEvent, RomData, UiEvent};
        use anyhow::Context;
        use wasm_bindgen::{closure::Closure, JsCast, JsValue};
        use web_sys::{js_sys::Uint8Array, FileReader, HtmlInputElement};
        use winit::event_loop::EventLoopProxy;

        let window = web_sys::window().context("valid js window")?;
        let document = window.document().context("valid html document")?;

        let on_error = |event_proxy: &EventLoopProxy<NesEvent>, err: JsValue| {
            if let Err(err) = event_proxy.send_event(
                UiEvent::Error(
                    err.as_string()
                        .unwrap_or_else(|| "failed to load rom".to_string()),
                )
                .into(),
            ) {
                error!("failed to send event: {err:?}");
            }
        };

        let on_load_rom = Closure::<dyn FnMut(_)>::new({
            let event_proxy = self.event_proxy.clone();
            let config = self.config.clone();
            move |evt: web_sys::MouseEvent| match FileReader::new().and_then(|reader| {
                evt.current_target()
                    .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
                    .and_then(|input| input.files())
                    .and_then(|files| files.item(0))
                    .map(|file| {
                        reader.read_as_array_buffer(&file).map(|_| {
                            let onload = Closure::<dyn FnMut()>::new({
                                let reader = reader.clone();
                                let event_proxy = event_proxy.clone();
                                let config = config.clone();
                                move || {
                                    if let Err(err) = reader.result().map(|result| {
                                        let data = Uint8Array::new(&result);
                                        event_proxy.send_event(
                                            EmulationEvent::LoadRom((
                                                file.name(),
                                                RomData::new(data.to_vec()),
                                                config.clone(),
                                            ))
                                            .into(),
                                        )
                                    }) {
                                        on_error(&event_proxy, err);
                                    }
                                }
                            });
                            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                            onload.forget();
                        })
                    })
                    .unwrap()
            }) {
                Ok(()) => {
                    if let Some(canvas) = get_canvas() {
                        let _ = canvas.focus();
                    }
                }
                Err(err) => on_error(&event_proxy, err),
            }
        });

        let load_rom_input = document
            .get_element_by_id(html_ids::ROM_INPUT)
            .context("valid load-rom button")?;
        if let Err(err) = load_rom_input
            .add_event_listener_with_callback("change", on_load_rom.as_ref().unchecked_ref())
        {
            on_error(&self.event_proxy, err);
        }
        on_load_rom.forget();

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn initialize_platform(&mut self) -> NesResult<()> {
        if self.config.rom_path.is_file() {
            let path = &self.config.rom_path;
            self.trigger_event(EmulationEvent::LoadRomPath((
                path.to_path_buf(),
                self.config.clone(),
            )));
        }
        Ok(())
    }
}

/// Extension trait for `EventLoop` that provides platform-specific behavior.
pub trait EventLoopExt<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static;
}

impl<T> EventLoopExt<T> for EventLoop<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static,
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

/// Extension trait for any builder that provides platform-specific behavior.
pub trait BuilderExt {
    /// Sets platform-specific options.
    fn with_platform(self) -> Self;
}

impl BuilderExt for WindowBuilder {
    /// Sets platform-specific window options.
    fn with_platform(self) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowBuilderExtWebSys;
            self.with_canvas(get_canvas())
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
                .map_err(|err| error!("{err:?}"))
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
        let (width, height) = self.window_dimensions();
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
