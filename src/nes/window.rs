use pix_engine::prelude::*;

// enum Views {
//     Emulation,
//     Config,
//     OpenRom,
//     Keybind,
//     Help,
// }

#[derive(Debug, Clone)]
pub(crate) struct WindowBuilder {
    id: Option<WindowId>,
    win_width: u32,
    win_height: u32,
    texture_id: Option<TextureId>,
    texture_format: PixelFormat,
    texture_width: u32,
    texture_height: u32,
    texture_clip: Option<Rect>,
}

impl Default for WindowBuilder {
    fn default() -> Self {
        Self {
            id: None,
            win_width: 800,
            win_height: 600,
            texture_id: None,
            texture_format: PixelFormat::Rgb,
            texture_width: 800,
            texture_height: 600,
            texture_clip: None,
        }
    }
}

impl WindowBuilder {
    pub(crate) fn new(win_width: u32, win_height: u32) -> Self {
        Self {
            win_width,
            win_height,
            texture_width: win_width,
            texture_height: win_height,
            ..Default::default()
        }
    }

    pub(crate) fn with_id(&mut self, id: WindowId) -> &mut Self {
        self.id = Some(id);
        self
    }

    pub(crate) fn create_texture(
        &mut self,
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> &mut Self {
        self.texture_format = format;
        self.texture_width = width;
        self.texture_height = height;
        self
    }

    pub(crate) fn clip<R>(&mut self, rect: R) -> &mut Self
    where
        R: Into<Rect>,
    {
        self.texture_clip = Some(rect.into());
        self
    }

    pub(crate) fn build(&self, s: &mut PixState) -> PixResult<Window> {
        let texture_id = match self.texture_id {
            Some(id) => id,
            None => {
                s.create_texture(self.texture_format, self.texture_width, self.texture_height)?
            }
        };
        let id = match self.id {
            Some(id) => id,
            None => s.create_window(self.win_width, self.win_height).build()?,
        };

        Ok(Window {
            id,
            win_width: self.win_width,
            win_height: self.win_height,
            texture_id,
            texture_format: self.texture_format,
            texture_width: self.texture_width,
            texture_height: self.texture_height,
            texture_clip: self.texture_clip,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Window {
    id: WindowId,
    win_width: u32,
    win_height: u32,
    texture_id: TextureId,
    texture_format: PixelFormat,
    texture_width: u32,
    texture_height: u32,
    texture_clip: Option<Rect>,
}

impl Window {
    pub(crate) fn update_texture(&mut self, s: &mut PixState, bytes: &[u8]) -> PixResult<()> {
        let channels = match self.texture_format {
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
            _ => panic!("invalid texture_format"),
        };
        s.update_texture(
            self.texture_id,
            Some(rect!(0, 0, self.texture_width, self.texture_height)),
            bytes,
            channels * self.texture_width as usize,
        )?;
        s.draw_texture(self.texture_id, self.texture_clip, None)?;
        Ok(())
    }
}
