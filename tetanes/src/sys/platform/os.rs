use crate::{
    nes::Nes,
    platform::{BuilderExt, EventLoopExt, Feature, Initialize},
};
use std::path::PathBuf;
use tracing::error;
use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::WindowBuilder,
};

pub const fn supports_impl(feature: Feature) -> bool {
    matches!(
        feature,
        Feature::Filesystem | Feature::WindowMinMax | Feature::ToggleVsync
    )
}

pub fn open_file_dialog_impl(
    title: impl Into<String>,
    name: impl Into<String>,
    extensions: &[impl ToString],
    dir: Option<PathBuf>,
) -> anyhow::Result<Option<PathBuf>> {
    let mut dialog = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(name, extensions);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir);
    }
    Ok(dialog.pick_file())
}

impl Initialize for Nes {
    fn initialize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl BuilderExt for WindowBuilder {
    /// Sets platform-specific window options.
    fn with_platform(self) -> Self {
        use anyhow::Context;
        use image::{io::Reader as ImageReader, ImageFormat};
        use std::io::Cursor;

        static WINDOW_ICON: &[u8] = include_bytes!("../../../assets/tetanes_icon.png");

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

impl<T> EventLoopExt<T> for EventLoop<T> {
    /// Runs the event loop for the current platform.
    fn run_platform<F>(self, event_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Event<T>, &EventLoopWindowTarget<T>) + 'static,
    {
        self.run(event_handler)?;
        Ok(())
    }
}
