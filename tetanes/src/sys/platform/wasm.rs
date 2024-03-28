use crate::{
    nes::{
        event::{EmulationEvent, NesEvent, RomData, UiEvent},
        Nes,
    },
    platform::{BuilderExt, EventLoopExt, Initialize},
};
use anyhow::{bail, Context};
use std::path::PathBuf;
use tracing::error;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{js_sys::Uint8Array, window, FileReader, HtmlCanvasElement, HtmlInputElement};
use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopProxy, EventLoopWindowTarget},
    platform::web::{EventLoopExtWebSys, WindowBuilderExtWebSys},
    window::WindowBuilder,
};

pub mod html_ids {
    pub const CANVAS: &str = "frame";
    pub const ROM_INPUT: &str = "load-rom";
}

pub fn get_canvas() -> Option<web_sys::HtmlCanvasElement> {
    window()
        .and_then(|win| win.document())
        .and_then(|doc| doc.get_element_by_id(html_ids::CANVAS))
        .and_then(|canvas| canvas.dyn_into::<HtmlCanvasElement>().ok())
}

pub fn focus_canvas() {
    if let Some(canvas) = get_canvas() {
        let _ = canvas.focus();
    }
}

pub fn open_file_dialog(
    _title: impl Into<String>,
    _name: impl Into<String>,
    _extensions: &[impl ToString],
    _dir: Option<PathBuf>,
) -> anyhow::Result<Option<PathBuf>> {
    let input = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(html_ids::ROM_INPUT))
        .and_then(|input| input.dyn_into::<HtmlInputElement>().ok());
    match input {
        Some(input) => input.click(),
        None => bail!("failed to find file input element"),
    }
    focus_canvas();
    Ok(None)
}

impl Initialize for Nes {
    fn initialize(&mut self) -> anyhow::Result<()> {
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
                                move || {
                                    if let Err(err) = reader.result().map(|result| {
                                        let data = Uint8Array::new(&result);
                                        event_proxy.send_event(
                                            EmulationEvent::LoadRom((
                                                file.name(),
                                                RomData::new(data.to_vec()),
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
                Ok(()) => focus_canvas(),
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
}

impl BuilderExt for WindowBuilder {
    /// Sets platform-specific window options.
    fn with_platform(self) -> Self {
        self.with_canvas(get_canvas())
    }
}

impl<T> EventLoopExt<T> for EventLoop<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static,
    {
        self.spawn(event_handler);
        Ok(())
    }
}
