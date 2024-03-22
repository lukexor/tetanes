use std::path::PathBuf;

use crate::{
    nes::{event::EmulationEvent, Nes},
    platform::{BuilderExt, EventLoopExt, Initialize},
};
use tracing::error;
use winit::{
    event::Event,
    event_loop::{EventLoop, EventLoopWindowTarget},
    window::WindowBuilder,
};

pub fn open_file_dialog(
    name: impl Into<String>,
    extensions: &[impl ToString],
) -> anyhow::Result<Option<PathBuf>> {
    Ok(rfd::FileDialog::new()
        .add_filter(name, extensions)
        .pick_file())
}

impl Initialize for Nes {
    fn initialize(&mut self) -> anyhow::Result<()> {
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
