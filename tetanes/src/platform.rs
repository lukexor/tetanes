use crate::sys::platform;
use std::path::PathBuf;
use winit::{event::Event, event_loop::EventLoopWindowTarget};

/// Trait for any type requiring platform-specific initialization.
pub trait Initialize {
    /// Initialize type.
    fn initialize(&mut self) -> anyhow::Result<()>;
}

/// Extension trait for any builder that provides platform-specific behavior.
pub trait BuilderExt {
    /// Sets platform-specific options.
    fn with_platform(self, title: impl Into<String>) -> Self;
}

/// Extension trait for `EventLoop` that provides platform-specific behavior.
pub trait EventLoopExt<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static;
}

pub fn open_file_dialog(
    title: impl Into<String>,
    name: impl Into<String>,
    extensions: &[impl ToString],
    dir: Option<PathBuf>,
) -> anyhow::Result<Option<PathBuf>> {
    platform::open_file_dialog_impl(title, name, extensions, dir)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum Feature {
    Filesystem,
    Viewports,
    Suspend,
}

pub const fn supports(feature: Feature) -> bool {
    platform::supports_impl(feature)
}
