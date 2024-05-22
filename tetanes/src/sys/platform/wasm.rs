use crate::{
    nes::{
        event::{EmulationEvent, NesEvent, RendererEvent, ReplayData, SendNesEvent, UiEvent},
        rom::RomData,
        Running,
    },
    platform::{BuilderExt, EventLoopExt, Feature, Initialize},
};
use anyhow::{bail, Context};
use std::path::PathBuf;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{js_sys::Uint8Array, window, FileReader, HtmlCanvasElement, HtmlInputElement};
use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopProxy, EventLoopWindowTarget},
    platform::web::{EventLoopExtWebSys, WindowBuilderExtWebSys},
    window::WindowBuilder,
};

pub const fn supports_impl(_feature: Feature) -> bool {
    false
}

pub fn open_file_dialog_impl(
    _title: impl Into<String>,
    _name: impl Into<String>,
    extensions: &[impl ToString],
    _dir: Option<PathBuf>,
) -> anyhow::Result<Option<PathBuf>> {
    let input_id = match extensions[0].to_string().as_str() {
        "nes" => html_ids::ROM_INPUT,
        "replay" => html_ids::REPLAY_INPUT,
        _ => bail!("unsupported file extension"),
    };
    let input = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(input_id))
        .and_then(|input| input.dyn_into::<HtmlInputElement>().ok());
    match input {
        Some(input) => input.click(),
        None => bail!("failed to find file input element"),
    }
    focus_canvas();
    Ok(None)
}

impl Initialize for Running {
    fn initialize(&mut self) -> anyhow::Result<()> {
        let window = web_sys::window().context("valid js window")?;
        let document = window.document().context("valid html document")?;

        let on_error = |tx: &EventLoopProxy<NesEvent>, err: JsValue| {
            tx.nes_event(UiEvent::Error(
                err.as_string()
                    .unwrap_or_else(|| "failed to load rom".to_string()),
            ));
        };

        for input_id in [html_ids::ROM_INPUT, html_ids::REPLAY_INPUT] {
            let on_change = Closure::<dyn FnMut(_)>::new({
                let tx = self.tx.clone();
                move |evt: web_sys::Event| {
                    match FileReader::new() {
                        Ok(reader) => {
                            let Some(file) = evt
                                .current_target()
                                .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
                                .and_then(|input| input.files())
                                .and_then(|files| files.item(0))
                            else {
                                tx.nes_event(UiEvent::FileDialogCancelled);
                                return;
                            };
                            match reader.read_as_array_buffer(&file) {
                                Ok(_) => {
                                    let on_load = Closure::<dyn FnMut()>::new({
                                        let reader = reader.clone();
                                        let tx = tx.clone();
                                        move || match reader.result() {
                                            Ok(result) => {
                                                let data = Uint8Array::new(&result);
                                                let event = match input_id {
                                                    html_ids::ROM_INPUT => EmulationEvent::LoadRom(
                                                        (file.name(), RomData(data.to_vec())),
                                                    ),
                                                    html_ids::REPLAY_INPUT => {
                                                        EmulationEvent::LoadReplay((
                                                            file.name(),
                                                            ReplayData(data.to_vec()),
                                                        ))
                                                    }
                                                    _ => unreachable!("unsupported input id"),
                                                };
                                                tx.nes_event(event);
                                                focus_canvas();
                                            }
                                            Err(err) => on_error(&tx, err),
                                        }
                                    });
                                    reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
                                    on_load.forget();
                                }
                                Err(err) => on_error(&tx, err),
                            }
                        }
                        Err(err) => on_error(&tx, err),
                    };
                }
            });

            let on_cancel = Closure::<dyn FnMut(_)>::new({
                let tx = self.tx.clone();
                move |_: web_sys::Event| tx.nes_event(UiEvent::FileDialogCancelled)
            });

            let input = document
                .get_element_by_id(input_id)
                .with_context(|| format!("valid {input_id} button"))?;
            if let Err(err) =
                input.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
            {
                on_error(&self.tx, err);
            }
            if let Err(err) =
                input.add_event_listener_with_callback("cancel", on_cancel.as_ref().unchecked_ref())
            {
                on_error(&self.tx, err);
            }
            on_change.forget();
            on_cancel.forget();
        }

        let on_resize = Closure::<dyn FnMut(_)>::new({
            let tx = self.tx.clone();
            move |_: web_sys::Event| {
                if let Some(window) = web_sys::window() {
                    tx.nes_event(RendererEvent::BrowserResized((
                        window
                            .inner_width()
                            .ok()
                            .and_then(|w| w.as_f64())
                            .map_or(0.0, |w| w as f32),
                        window
                            .inner_height()
                            .ok()
                            .and_then(|h| h.as_f64())
                            .map_or(0.0, |h| h as f32),
                    )));
                }
            }
        });
        if let Err(err) =
            window.add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref())
        {
            on_error(&self.tx, err);
        }
        on_resize.forget();

        if let Some(status) = document.get_element_by_id(html_ids::LOADING_STATUS) {
            tracing::info!(
                "removing hidden class from loading status: {}",
                html_ids::LOADING_STATUS
            );
            if let Err(err) = status.class_list().add_1("hidden") {
                tracing::info!("{err:?}");
                on_error(&self.tx, err);
            }
        }

        Ok(())
    }
}

impl BuilderExt for WindowBuilder {
    /// Sets platform-specific window options.
    fn with_platform(self, _title: &str) -> Self {
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

mod html_ids {
    pub(super) const CANVAS: &str = "frame";
    pub(super) const LOADING_STATUS: &str = "loading-status";
    pub(super) const ROM_INPUT: &str = "load-rom";
    pub(super) const REPLAY_INPUT: &str = "load-replay";
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
