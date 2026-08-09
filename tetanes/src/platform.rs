use crate::{
    nes::event::{NesEvent, NesEventProxy},
    sys::platform,
};
use std::path::{Path, PathBuf};

pub use platform::*;

/// Trait for any type requiring platform-specific initialization.
pub trait Initialize {
    /// Initialize type.
    fn initialize(&mut self) -> anyhow::Result<()>;
}

/// Extension trait for any builder that provides platform-specific behavior.
pub trait BuilderExt {
    /// Sets platform-specific options.
    fn with_platform(self, title: &str) -> Self;
}

/// Method for platforms supporting opening a file dialog.
///
/// This returns as soon as the dialog is up rather than when the user is done with it, so the
/// event loop stays live. The outcome arrives later as the event `on_open` builds from the chosen
/// path, or as [`crate::nes::event::UiEvent::FileDialogCancelled`] if the dialog was dismissed.
pub fn open_file_dialog(
    tx: &NesEventProxy,
    title: impl Into<String>,
    name: impl Into<String>,
    extensions: &[impl ToString],
    dir: Option<impl AsRef<Path>>,
    on_open: impl FnOnce(PathBuf) -> NesEvent + Send + 'static,
) -> anyhow::Result<()> {
    platform::open_file_dialog_impl(tx, title, name, extensions, dir, on_open)
}

/// Speak the given text out loud for platforms that support it.
#[allow(clippy::missing_const_for_fn)]
pub fn speak_text(text: &str) {
    platform::speak_text_impl(text);
}

pub mod renderer {
    use super::*;
    use crate::nes::{config::Config, event::Response, renderer::Renderer};

    pub fn constrain_window_to_viewport(
        renderer: &Renderer,
        desired_window_width: f32,
        cfg: &Config,
    ) -> Response {
        platform::renderer::constrain_window_to_viewport_impl(renderer, desired_window_width, cfg)
    }
}

/// Platform-specific feature capabilities.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum Feature {
    AbortOnExit,
    Blocking,
    ConstrainedViewport,
    ConsumePaste,
    Filesystem,
    ScreenReader,
    Storage,
    Suspend,
    OsViewports,
}

/// Checks if the current platform supports a given feature.
#[macro_export]
macro_rules! feature {
    ($feature: tt) => {{
        use $crate::platform::Feature::*;
        match $feature {
            // Wasm should never be able to exit
            AbortOnExit => cfg!(target_arch = "wasm32"),
            Blocking | Filesystem | OsViewports => {
                cfg!(not(target_arch = "wasm32"))
            }
            ConstrainedViewport | ConsumePaste | ScreenReader => {
                cfg!(target_arch = "wasm32")
            }
            Storage => true,
            Suspend => cfg!(target_os = "android"),
        }
    }};
}
