use crate::{
    nes::{
        event::{EmulationEvent, NesEvent, RendererEvent, ReplayData, SendNesEvent, UiEvent},
        renderer::{Renderer, State},
        rom::RomData,
        Running,
    },
    platform::{BuilderExt, EventLoopExt, Feature, Initialize},
    thread,
};
use anyhow::{bail, Context};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    js_sys::Uint8Array, FileReader, HtmlAnchorElement, HtmlCanvasElement, HtmlInputElement,
};
use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopProxy, EventLoopWindowTarget},
    platform::web::{EventLoopExtWebSys, WindowBuilderExtWebSys},
    window::WindowBuilder,
};

const BIN_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const OS_OPTIONS: [(Os, Arch, &str); 5] = [
    (Os::Unknown, Arch::X86_64, html_ids::SELECTED_VERSION),
    (Os::Windows, Arch::X86_64, html_ids::WINDOWS_X86_LINK),
    (Os::MacOs, Arch::Aarch64, html_ids::MACOS_AARCH64_LINK),
    (Os::MacOs, Arch::X86_64, html_ids::MACOS_X86_LINK),
    (Os::Linux, Arch::X86_64, html_ids::LINXU_X86_LINK),
];

/// Checks if the current platform supports a given feature.
pub const fn supports_impl(feature: Feature) -> bool {
    match feature {
        Feature::Storage => true,
        Feature::Filesystem | Feature::Viewports | Feature::Suspend | Feature::Blocking => false,
    }
}

/// Method for platforms supporting opening a file dialog.
pub fn open_file_dialog_impl(
    _title: impl Into<String>,
    _name: impl Into<String>,
    extensions: &[impl ToString],
    _dir: impl AsRef<Path>,
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
        Some(input) => {
            // To prevent event loop receiving events while dialog is open
            if let Some(canvas) = get_canvas() {
                let _ = canvas.blur();
            }
            input.click();
        }
        None => bail!("failed to find file input element"),
    }

    Ok(None)
}

/// Helper method to log and send errors to the UI thread from javascript.
fn on_error(tx: &EventLoopProxy<NesEvent>, err: JsValue) {
    tracing::error!("{err:?}");
    tx.nes_event(UiEvent::Error(
        err.as_string()
            .unwrap_or_else(|| "failed to load rom".to_string()),
    ));
}

/// Sets up the window resize handler for responding to changes in the viewport size.
fn set_resize_handler(window: &web_sys::Window, tx: &EventLoopProxy<NesEvent>) {
    let on_resize = Closure::<dyn FnMut(_)>::new({
        let tx = tx.clone();
        move |_: web_sys::Event| {
            if let Some(window) = web_sys::window() {
                let width = window
                    .inner_width()
                    .ok()
                    .and_then(|w| w.as_f64())
                    .map_or(0.0, |w| w as f32);
                let height = window
                    .inner_height()
                    .ok()
                    .and_then(|h| h.as_f64())
                    .map_or(0.0, |h| h as f32);
                tx.nes_event(RendererEvent::ViewportResized((width, height)));
            }
        }
    });

    let on_resize_cb = on_resize.as_ref().unchecked_ref();
    if let Err(err) = window.add_event_listener_with_callback("resize", on_resize_cb) {
        on_error(tx, err);
    }

    on_resize.forget();
}

/// Sets up the onload handler for reading loaded files.
fn set_file_onload_handler(
    tx: EventLoopProxy<NesEvent>,
    input_id: &'static str,
    reader: web_sys::FileReader,
    file_name: String,
) -> anyhow::Result<()> {
    let on_load = Closure::<dyn FnMut()>::new({
        let reader = reader.clone();
        move || match reader.result() {
            Ok(result) => {
                let data = Uint8Array::new(&result).to_vec();
                let event = match input_id {
                    html_ids::ROM_INPUT => {
                        EmulationEvent::LoadRom((file_name.clone(), RomData(data)))
                    }
                    html_ids::REPLAY_INPUT => {
                        EmulationEvent::LoadReplay((file_name.clone(), ReplayData(data)))
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

    Ok(())
}

/// Sets up the onchange and oncancel handlers for file input elements.
fn set_file_onchange_handlers(
    document: &web_sys::Document,
    tx: &EventLoopProxy<NesEvent>,
    input_id: &'static str,
) -> anyhow::Result<()> {
    let on_change = Closure::<dyn FnMut(_)>::new({
        let tx = tx.clone();
        move |evt: web_sys::Event| match FileReader::new() {
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
                if let Err(err) = reader
                    .read_as_array_buffer(&file)
                    .map(|_| set_file_onload_handler(tx.clone(), input_id, reader, file.name()))
                {
                    on_error(&tx, err);
                }
            }
            Err(err) => on_error(&tx, err),
        }
    });

    let on_cancel = Closure::<dyn FnMut(_)>::new({
        let tx = tx.clone();
        move |_: web_sys::Event| {
            focus_canvas();
            tx.nes_event(UiEvent::FileDialogCancelled);
        }
    });

    let input = document
        .get_element_by_id(input_id)
        .with_context(|| format!("valid {input_id} button"))?;
    let on_change_cb = on_change.as_ref().unchecked_ref();
    let on_cancel_cb = on_cancel.as_ref().unchecked_ref();
    if let Err(err) = input
        .add_event_listener_with_callback("change", on_change_cb)
        .and_then(|_| input.add_event_listener_with_callback("cancel", on_cancel_cb))
    {
        on_error(tx, err)
    }

    on_change.forget();
    on_cancel.forget();

    Ok(())
}

/// Sets the clipboard text to the given string.
pub fn set_clipboard_text(s: &str) {
    if let Some(clipboard) = web_sys::window().and_then(|window| window.navigator().clipboard()) {
        let promise = clipboard.write_text(s);
        let future = JsFuture::from(promise);
        let future = async move {
            if let Err(err) = future.await {
                tracing::error!(
                    "Cut/Copy failed: {}",
                    err.as_string().unwrap_or_else(|| format!("{err:#?}"))
                );
            }
        };
        thread::spawn(future);
    }
}

/// Enumeration of supported operating systems.
#[derive(Debug, Copy, Clone)]
#[must_use]
enum Os {
    Unknown,
    Windows,
    #[allow(clippy::enum_variant_names)]
    MacOs,
    Linux,
    Mobile,
}

impl std::fmt::Display for Os {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let os = match self {
            Os::Windows => "Windows",
            Os::MacOs => "macOS",
            Os::Linux => "Linux",
            _ => "Desktop",
        };
        write!(f, "{os}")
    }
}

/// Enumeration of supported CPU architectures.
#[derive(Debug, Copy, Clone)]
#[must_use]
enum Arch {
    X86_64,
    Aarch64,
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let arch = match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        };
        write!(f, "{arch}")
    }
}

/// Converts the operating system and architecture to a human-readable string.
const fn platform_to_string(os: Os, arch: Arch) -> &'static str {
    match (os, arch) {
        (Os::Windows, Arch::X86_64) => "Windows",
        (Os::MacOs, Arch::X86_64) => "Mac - Intel Chip",
        (Os::MacOs, Arch::Aarch64) => "Mac - Apple Chip",
        (Os::Linux, Arch::X86_64) => "Linux",
        (Os::Mobile, _) => "Mobile",
        _ => "Desktop",
    }
}

#[wasm_bindgen]
extern "C" {
    /// Extends the `Navigator` object to support the `userAgentData` method.
    #[wasm_bindgen(extends = web_sys::Navigator)]
    type NavigatorExt;

    /// The `NavigatorUAData` is what's returned from `navigator.userAgentData` on browsers that
    /// support it.
    type NavigatorUAData;

    /// The `HighEntropyValues` object is returned from `navigator.userAgentData.getHighEntropyValues`.
    #[derive(Debug)]
    #[wasm_bindgen(js_name = Object)]
    type HighEntropyValues;

    /// `navigator.userAgentData` for browsers that support it.
    #[wasm_bindgen(method, getter, js_name = userAgentData)]
    fn user_agent_data(this: &NavigatorExt) -> Option<NavigatorUAData>;

    /// `navigator.userAgentData.getHighEntropyValues()` for browsers that support it.
    #[wasm_bindgen(method, js_name = getHighEntropyValues)]
    async fn get_high_entropy_values(this: &NavigatorUAData, hints: Vec<String>) -> JsValue;

    /// `HighEntropyValues.mobile` indicates whether the detected platform is a mobile device.
    #[wasm_bindgen(method, getter, js_class = "HighEntropyValues")]
    fn mobile(this: &HighEntropyValues) -> bool;

    /// `HighEntropyValues.platform` indicates the detected OS platform (e.g. `Windows`).
    #[wasm_bindgen(method, getter, js_class = "HighEntropyValues")]
    fn platform(this: &HighEntropyValues) -> String;

    /// `HighEntropyValues.platform` indicates the detected CPU architecture. (e.g. `x86`).
    #[wasm_bindgen(method, getter, js_class = "HighEntropyValues")]
    fn architecture(this: &HighEntropyValues) -> String;
}

/// Detects the user's platform and architecture.
async fn detect_user_platform() -> anyhow::Result<(Os, Arch)> {
    let navigator = web_sys::window()
        .map(|win| win.navigator())
        .context("failed to get navigator")?;

    let user_agent = navigator.user_agent().unwrap_or_default();
    let mut os = if user_agent.contains("Mobile") {
        Os::Mobile
    } else if user_agent.contains("Windows") {
        Os::Windows
    } else if user_agent.contains("Mac") {
        Os::MacOs
    } else if user_agent.contains("Linux") {
        Os::Linux
    } else {
        Os::Unknown
    };
    let mut arch = Arch::X86_64;

    // FIXME: Currently unsupported on Firefox/Safari but it's the only way to derive
    // macOS aarch64
    let navigator_ext = NavigatorExt { obj: navigator };
    let Some(ua_data) = navigator_ext.user_agent_data() else {
        return Ok((os, arch));
    };
    let Ok(ua_values) = ua_data
        .get_high_entropy_values(vec![
            "architecture".into(),
            "platform".into(),
            "bitness".into(),
        ])
        .await
        .dyn_into::<HighEntropyValues>()
    else {
        return Ok((os, arch));
    };
    if ua_values.mobile() {
        os = Os::Mobile;
    } else {
        match ua_values.platform().as_str() {
            "Windows" => os = Os::Windows,
            "macOS" => {
                os = Os::MacOs;
                arch = if ua_values.architecture().starts_with("x86") {
                    Arch::X86_64
                } else {
                    Arch::Aarch64
                };
            }
            "Linux" => os = Os::Linux,
            _ => (),
        }
    };

    Ok((os, arch))
}

/// Constructs the download URL for the given operating system and architecture.
fn download_url_by_os(os: Os, arch: Arch) -> String {
    let base_url =
        format!("https://github.com/lukexor/tetanes/releases/download/tetanes-v{VERSION}");
    match os {
        Os::MacOs => format!("{base_url}/{BIN_NAME}-{arch}.dmg"),
        Os::Windows => format!("{base_url}/{BIN_NAME}-{arch}.msi"),
        Os::Linux => format!("{base_url}/{BIN_NAME}-{arch}-unknown-linux-gnu.tar.gz"),
        _ => format!("https://github.com/lukexor/tetanes/releases/tag/tetanes-v{VERSION}"),
    }
}

/// Sets the download links to the correct release artifacts.
fn set_download_versions(document: &web_sys::Document) {
    if let Some(version) = document.get_element_by_id(html_ids::VERSION) {
        version.set_inner_html(concat!("v", env!("CARGO_PKG_VERSION")));
    }

    let document = document.clone();
    thread::spawn(async move {
        // Update download links to the correct release artifacts
        for (os, arch, id) in OS_OPTIONS {
            if let Some(download_link) = document
                .get_element_by_id(id)
                .and_then(|el| el.dyn_into::<HtmlAnchorElement>().ok())
            {
                download_link.set_href(&download_url_by_os(os, arch));
                let platform = platform_to_string(os, arch);
                download_link.set_inner_text(&format!("Download for {platform}"));
            }
        }

        // Set selected version to detected platform
        if let Some(selected_version) = document
            .get_element_by_id(html_ids::SELECTED_VERSION)
            .and_then(|el| el.dyn_into::<HtmlAnchorElement>().ok())
        {
            if let Ok((os, arch)) = detect_user_platform().await {
                selected_version.set_href(&download_url_by_os(os, arch));
                let platform = platform_to_string(os, arch);
                selected_version.set_inner_text(&format!("Download for {platform}"));
            }
        }

        // Add mouseover/mouseout event listeners to version download links and make them visible
        if let (Some(version_download), Some(version_options)) = (
            document.get_element_by_id(html_ids::VERSION_DOWNLOAD),
            document.get_element_by_id(html_ids::VERSION_OPTIONS),
        ) {
            let on_mouseover = Closure::<dyn FnMut(_)>::new({
                let version_options = version_options.clone();
                move |_: web_sys::MouseEvent| {
                    if let Err(err) = version_options.class_list().remove_1("hidden") {
                        tracing::error!("{err:?}");
                    }
                }
            });
            let on_mouseout = Closure::<dyn FnMut(_)>::new(move |_: web_sys::MouseEvent| {
                if let Err(err) = version_options.class_list().add_1("hidden") {
                    tracing::error!("{err:?}");
                }
            });
            let on_mouseover_cb = on_mouseover.as_ref().unchecked_ref();
            let on_mouseout_cb = on_mouseout.as_ref().unchecked_ref();
            if let Err(err) = version_download
                .add_event_listener_with_callback("mouseover", on_mouseover_cb)
                .and_then(|_| {
                    version_download.add_event_listener_with_callback("mouseout", on_mouseout_cb)
                })
                .and_then(|_| version_download.class_list().remove_1("hidden"))
            {
                tracing::error!("{err:?}");
            }
            on_mouseover.forget();
            on_mouseout.forget();
        }
    });
}

/// Hides the loading status when the WASM module has finished loading.
fn finish_loading(
    document: &web_sys::Document,
    tx: &EventLoopProxy<NesEvent>,
) -> anyhow::Result<()> {
    if let Some(status) = document.get_element_by_id(html_ids::LOADING_STATUS) {
        if let Err(err) = status.class_list().add_1("hidden") {
            on_error(tx, err);
        }
    }

    Ok(())
}

impl Initialize for Running {
    /// Initialize JS event handlers and DOM elements.
    fn initialize(&mut self) -> anyhow::Result<()> {
        let window = web_sys::window().context("valid window")?;
        let document = window.document().context("valid html document")?;

        set_download_versions(&document);
        set_resize_handler(&window, &self.tx);
        for input_id in [html_ids::ROM_INPUT, html_ids::REPLAY_INPUT] {
            set_file_onchange_handlers(&document, &self.tx, input_id)?;
        }

        finish_loading(&document, &self.tx)?;

        Ok(())
    }
}

impl Initialize for Renderer {
    /// Initialize JS event handlers and DOM elements.
    fn initialize(&mut self) -> anyhow::Result<()> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .context("failed to get html document")?;

        let on_paste = Closure::<dyn FnMut(_)>::new({
            let ctx = self.ctx.clone();
            let state = Rc::clone(&self.state);
            move |evt: web_sys::ClipboardEvent| {
                if let Some(data) = evt.clipboard_data() {
                    if let Ok(text) = data.get_data("text") {
                        let text = text.replace("\r\n", "\n");
                        if !text.is_empty() {
                            let consumed = {
                                let State { viewports, .. } = &mut *state.borrow_mut();
                                let egui_state = viewports
                                    .get_mut(&egui::ViewportId::ROOT)
                                    .and_then(|viewport| viewport.egui_state.as_mut());
                                match egui_state {
                                    Some(egui_state) => {
                                        // Requires creating an event and setting the clipboard
                                        // here because egui_winit internally tries to manage a
                                        // fallback clipboard for platforms not supported by the
                                        // clipboard crates being used.
                                        //
                                        // This has associated behavior in the renderer to prevent
                                        // sending 'paste events' (ctrl/cmd+V) to egui_state to
                                        // bypass its internal clipboard handling.
                                        egui_state
                                            .egui_input_mut()
                                            .events
                                            .push(egui::Event::Paste(text.clone()));
                                        egui_state.set_clipboard_text(text);
                                        true
                                    }
                                    _ => false,
                                }
                            };
                            if consumed {
                                ctx.request_repaint();
                                evt.stop_propagation();
                                evt.prevent_default();
                            }
                        }
                    }
                }
            }
        });
        if let Err(err) =
            document.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref())
        {
            tracing::error!("failed to set paste handler: {err:?}");
        }
        on_paste.forget();

        let on_cut = Closure::<dyn FnMut(_)>::new({
            let ctx = self.ctx.clone();
            let state = Rc::clone(&self.state);
            let gui = Rc::clone(&self.gui);
            move |evt: web_sys::ClipboardEvent| {
                // Some browsers require transient activation, so we have to write to the clipboard
                // now
                let res = Renderer::process_input(&ctx, &state, &gui);
                if res.repaint {
                    ctx.request_repaint();
                }
                if res.consumed {
                    evt.stop_propagation();
                    evt.prevent_default();
                }
            }
        });
        if let Err(err) =
            document.add_event_listener_with_callback("cut", on_cut.as_ref().unchecked_ref())
        {
            tracing::error!("failed to set cut handler: {err:?}");
        }
        on_cut.forget();

        let on_copy = Closure::<dyn FnMut(_)>::new({
            let ctx = self.ctx.clone();
            let state = Rc::clone(&self.state);
            let gui = Rc::clone(&self.gui);
            move |evt: web_sys::ClipboardEvent| {
                // Some browsers require transient activation, so we have to write to the clipboard
                // now
                let res = Renderer::process_input(&ctx, &state, &gui);
                if res.repaint {
                    ctx.request_repaint();
                }
                if res.consumed {
                    evt.stop_propagation();
                    evt.prevent_default();
                }
            }
        });
        if let Err(err) =
            document.add_event_listener_with_callback("copy", on_copy.as_ref().unchecked_ref())
        {
            tracing::error!("failed to set copy handler: {err:?}");
        }
        on_copy.forget();

        if let Some(canvas) = get_canvas() {
            let on_keydown = Closure::<dyn FnMut(_)>::new(move |evt: web_sys::KeyboardEvent| {
                use egui::Key;

                let prevent_default = Key::from_name(&evt.key()).map_or(true, |key| {
                    // Allow ctrl/meta + X, C, V through
                    !matches!(key, Key::X | Key::C | Key::V) || !(evt.ctrl_key() || evt.meta_key())
                });

                if prevent_default {
                    evt.prevent_default();
                }
            });
            if let Err(err) = canvas
                .add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref())
            {
                tracing::error!("failed to set keydown handler: {err:?}");
            }
            on_keydown.forget();

            // Because we want to capture cut/copy/paste, `prevent_default` is disabled on winit,
            // so restore default behavior on other winit events
            for event in [
                "touchstart",
                "keyup",
                "wheel",
                "contextmenu",
                "pointerdown",
                "pointermove",
            ] {
                let on_event = Closure::<dyn FnMut(_)>::new({
                    let canvas = canvas.clone();
                    move |evt: web_sys::Event| {
                        evt.prevent_default();
                        if event == "pointerdown" {
                            let _ = canvas.focus();
                        }
                    }
                });
                if let Err(err) = canvas
                    .add_event_listener_with_callback(event, on_event.as_ref().unchecked_ref())
                {
                    tracing::error!("failed to set {event} handler: {err:?}");
                }
                on_event.forget();
            }
        }

        Ok(())
    }
}

impl BuilderExt for WindowBuilder {
    /// Sets platform-specific window options.
    fn with_platform(self, _title: &str) -> Self {
        // Prevent default false allows cut/copy/paste
        self.with_canvas(get_canvas()).with_prevent_default(false)
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
    //! HTML element IDs used to interact with the DOM.

    pub(super) const CANVAS: &str = "frame";
    pub(super) const LOADING_STATUS: &str = "loading-status";
    pub(super) const ROM_INPUT: &str = "load-rom";
    pub(super) const REPLAY_INPUT: &str = "load-replay";
    pub(super) const VERSION: &str = "version";
    pub(super) const VERSION_DOWNLOAD: &str = "version-download";
    pub(super) const VERSION_OPTIONS: &str = "version-options";
    pub(super) const SELECTED_VERSION: &str = "selected-version";
    pub(super) const WINDOWS_X86_LINK: &str = "x86_64-pc-windows-msvc";
    pub(super) const MACOS_X86_LINK: &str = "x86_64-apple-darwin";
    pub(super) const MACOS_AARCH64_LINK: &str = "aarch64-apple-darwin";
    pub(super) const LINXU_X86_LINK: &str = "x86_64-unknown-linux-gnu";
}

/// Gets the primary canvas element.
pub fn get_canvas() -> Option<web_sys::HtmlCanvasElement> {
    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| doc.get_element_by_id(html_ids::CANVAS))
        .and_then(|canvas| canvas.dyn_into::<HtmlCanvasElement>().ok())
}

/// Focuses the canvas element.
pub fn focus_canvas() {
    if let Some(canvas) = get_canvas() {
        let _ = canvas.focus();
    }
}
