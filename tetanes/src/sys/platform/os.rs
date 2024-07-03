use crate::{
    nes::{event::EmulationEvent, renderer::Renderer, Running},
    platform::{BuilderExt, Initialize},
};
use std::path::{Path, PathBuf};
use tracing::error;
use winit::window::WindowAttributes;

/// Method for platforms supporting opening a file dialog.
pub fn open_file_dialog_impl(
    title: impl Into<String>,
    name: impl Into<String>,
    extensions: &[impl ToString],
    dir: Option<impl AsRef<Path>>,
) -> anyhow::Result<Option<PathBuf>> {
    let mut dialog = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(name, extensions);
    if let Some(dir) = dir {
        dialog = dialog.set_directory(dir.as_ref());
    }
    Ok(dialog.pick_file())
}

/// Speak the given text out loud.
pub const fn speak_text_impl(_text: &str) {}

impl Initialize for Running {
    /// Initialize by loading a ROM from the command line, if provided.
    fn initialize(&mut self) -> anyhow::Result<()> {
        if let Some(path) = self.cfg.renderer.roms_path.take() {
            if path.is_file() {
                if let Some(parent) = path.parent() {
                    self.cfg.renderer.roms_path = Some(parent.to_path_buf());
                }
                self.event(EmulationEvent::LoadRomPath(path));
            } else if path.exists() {
                self.cfg.renderer.roms_path = Some(path);
            }
        }

        Ok(())
    }
}

impl Initialize for Renderer {
    fn initialize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl BuilderExt for WindowAttributes {
    /// Sets platform-specific window options.
    fn with_platform(self, _title: &str) -> Self {
        use anyhow::Context;
        use image::{io::Reader as ImageReader, ImageFormat};
        use std::io::Cursor;

        static WINDOW_ICON: &[u8] = include_bytes!("../../../assets/tetanes_icon.png");

        let icon = ImageReader::with_format(Cursor::new(WINDOW_ICON), ImageFormat::Png)
            .decode()
            .context("failed to decode window icon");

        let window_attrs = self.with_window_icon(
            icon.and_then(|png| {
                let width = png.width();
                let height = png.height();
                winit::window::Icon::from_rgba(png.into_rgba8().into_vec(), width, height)
                    .with_context(|| "failed to create window icon")
            })
            .map_err(|err| error!("{err:?}"))
            .ok(),
        );

        #[cfg(target_os = "linux")]
        let window_attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland as _;

            window_attrs.with_name(_title, "")
        };

        // Ensures that viewport windows open in a separate window instead of a tab, which has
        // issues with certain preference toggles like fullscreen that effect the root viewport.
        #[cfg(target_os = "macos")]
        let window_attrs = {
            use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS};

            window_attrs
                .with_tabbing_identifier(_title)
                .with_option_as_alt(OptionAsAlt::Both)
        };

        window_attrs
    }
}

pub mod renderer {
    use super::*;
    use crate::nes::{config::Config, event::Response};

    pub fn constrain_window_to_viewport_impl(
        _renderer: &Renderer,
        _desired_window_width: f32,
        _cfg: &Config,
    ) -> Response {
        Response::default()
    }
}
