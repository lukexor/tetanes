use pix_engine::prelude::*;

// enum Views {
//     Emulation,
//     Config,
//     OpenRom,
//     Keybind,
//     Help,
// }

#[derive(Debug)]
pub(crate) struct WindowBuilder {
    id: Option<WindowId>,
    win_width: u32,
    win_height: u32,
    texture: Option<Texture>,
    texture_format: PixelFormat,
    texture_width: u32,
    texture_height: u32,
    texture_clip: Option<Rect<i32>>,
}

impl Default for WindowBuilder {
    fn default() -> Self {
        Self {
            id: None,
            win_width: 800,
            win_height: 600,
            texture: None,
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
        R: Into<Rect<i32>>,
    {
        self.texture_clip = Some(rect.into());
        self
    }

    pub(crate) fn build(&mut self, s: &mut PixState) -> PixResult<Window> {
        let texture = match self.texture.take() {
            Some(texture) => texture,
            None => {
                s.create_texture(self.texture_width, self.texture_height, self.texture_format)?
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
            texture,
            texture_format: self.texture_format,
            texture_width: self.texture_width,
            texture_height: self.texture_height,
            texture_clip: self.texture_clip,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Window {
    pub(crate) id: WindowId,
    pub(crate) win_width: u32,
    pub(crate) win_height: u32,
    pub(crate) texture: Texture,
    pub(crate) texture_format: PixelFormat,
    pub(crate) texture_width: u32,
    pub(crate) texture_height: u32,
    pub(crate) texture_clip: Option<Rect<i32>>,
}

impl Window {
    pub(crate) fn update_texture(&mut self, s: &mut PixState, bytes: &[u8]) -> PixResult<()> {
        let channels = match self.texture_format {
            PixelFormat::Rgb => 3,
            PixelFormat::Rgba => 4,
        };
        s.update_texture(
            &mut self.texture,
            rect![0, 0, self.texture_width as i32, self.texture_height as i32],
            bytes,
            channels * self.texture_width as usize,
        )?;
        s.texture(&self.texture, self.texture_clip, None)?;
        Ok(())
    }
}
