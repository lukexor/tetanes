//! Window Management using SDL2

use crate::console::{RENDER_HEIGHT, RENDER_WIDTH, SAMPLE_RATE};
use crate::util::{self, Result};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{self, FullscreenType};
use sdl2::{EventPump, GameControllerSubsystem};

const WINDOW_WIDTH: u32 = (RENDER_WIDTH as f32 * 8.0 / 7.0) as u32; // for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = RENDER_HEIGHT as u32;

const NT_TEX_WIDTH: u32 = RENDER_WIDTH as u32;
const NT_TEX_HEIGHT: u32 = RENDER_HEIGHT as u32;
const PAT_TEX_WIDTH: u32 = 128;
const PAT_TEX_HEIGHT: u32 = 128;

/// A Window instance
pub struct Window {
    width: u32,
    height: u32,
    pub controller_sub: GameControllerSubsystem,
    audio_device: AudioQueue<f32>,
    canvas: Canvas<video::Window>,
    overscan: Rect,
    frame_tex: Texture<'static>,
    ntbl_texs: Vec<Texture<'static>>,
    pat_texs: Vec<Texture<'static>>,
    pal_texs: Vec<Texture<'static>>,
    _texture_creator: TextureCreator<video::WindowContext>,
}

impl Window {
    /// Creates a new Window instance containing the necessary window, audio, and input components
    /// used by the UI
    pub fn init(title: &str, scale: u32, fullscreen: bool) -> Result<(Self, EventPump)> {
        let context = sdl2::init().map_err(util::str_to_err)?;

        let width = WINDOW_WIDTH * scale;
        let height = WINDOW_HEIGHT * scale;

        // Window
        let video_sub = context.video().map_err(util::str_to_err)?;
        let mut window_builder = video_sub.window(title, width, height);
        window_builder.position_centered().resizable();
        if fullscreen {
            window_builder.fullscreen();
        }
        let mut window = window_builder.build()?;

        // Load window icon
        if let Ok(mut icon) = util::WindowIcon::load() {
            let surface = sdl2::surface::Surface::from_data(
                &mut icon.pixels,
                icon.width,
                icon.height,
                icon.pitch,
                PixelFormatEnum::RGB24,
            );
            if let Ok(surface) = surface {
                window.set_icon(surface);
            }
        }

        // Canvas
        let mut canvas = window.into_canvas().accelerated().present_vsync().build()?;
        canvas.set_logical_size(width, height)?;

        // Texture
        let texture_creator = canvas.texture_creator();
        let texture_creator_ptr = &texture_creator as *const TextureCreator<video::WindowContext>;
        let frame_tex = unsafe { &*texture_creator_ptr }.create_texture_streaming(
            PixelFormatEnum::RGB24,
            RENDER_WIDTH as u32,
            RENDER_HEIGHT as u32,
        )?;
        let mut ntbl_texs = Vec::new();
        for _ in 0..4 {
            let tex = unsafe { &*texture_creator_ptr }.create_texture_streaming(
                PixelFormatEnum::RGB24,
                NT_TEX_WIDTH as u32,
                NT_TEX_HEIGHT as u32,
            )?;
            ntbl_texs.push(tex);
        }

        let mut pat_texs = Vec::new();
        for _ in 0..2 {
            let tex = unsafe { &*texture_creator_ptr }.create_texture_streaming(
                PixelFormatEnum::RGB24,
                PAT_TEX_WIDTH,
                PAT_TEX_HEIGHT,
            )?;
            pat_texs.push(tex);
        }

        let mut pal_texs = Vec::new();
        let sys_pal_tex = unsafe { &*texture_creator_ptr }.create_texture_streaming(
            PixelFormatEnum::RGB24,
            16 as u32,
            4 as u32,
        )?;
        let pal_tex = unsafe { &*texture_creator_ptr }.create_texture_streaming(
            PixelFormatEnum::RGB24,
            (8 + 1) as u32,
            4 as u32,
        )?;
        pal_texs.push(sys_pal_tex);
        pal_texs.push(pal_tex);

        // Set up Audio
        let audio_sub = context.audio().map_err(util::str_to_err)?;
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .map_err(util::str_to_err)?;
        audio_device.resume();

        // Set up Input event pump
        let event_pump = context.event_pump().map_err(util::str_to_err)?;
        let controller_sub = context.game_controller().map_err(util::str_to_err)?;

        let window = Self {
            width,
            height,
            controller_sub,
            audio_device,
            canvas,
            // Takes off top 8 and bottom 8
            overscan: Rect::new(0, 8, RENDER_WIDTH as u32, RENDER_HEIGHT as u32 - 16),
            frame_tex,
            ntbl_texs,
            pat_texs,
            pal_texs,
            _texture_creator: texture_creator,
        };
        Ok((window, event_pump))
    }

    /// Updates the Window canvas texture with the passed in pixel data
    pub fn render_frame(&mut self, pixels: Vec<u8>) -> Result<()> {
        self.frame_tex.update(None, &pixels, RENDER_WIDTH * 3)?;
        self.canvas.clear();
        self.canvas
            .copy(&self.frame_tex, self.overscan, None)
            .map_err(util::str_to_err)?;
        self.canvas.present();
        Ok(())
    }

    pub fn set_debug_size(&mut self) {
        let _ = self
            .canvas
            .set_logical_size(self.width, self.height - (self.height / 6));
        let _ = self
            .canvas
            .window_mut()
            .set_size(self.width, self.height - (self.height / 6));
    }

    pub fn render_debug(
        &mut self,
        frame: Vec<u8>,
        nametables: Vec<Vec<u8>>,
        pattern_tables: Vec<Vec<u8>>,
        palettes: Vec<Vec<u8>>,
    ) -> Result<()> {
        self.canvas.clear();

        let width_pad = 5;
        let height_pad = 5;
        let half_width = (self.width / 2) - width_pad;
        let half_height = (self.height / 2) - height_pad;
        let quart_width = half_width / 2;
        let quart_height = (half_height / 2) - height_pad / 2;
        let right_x = (half_width + width_pad) as i32;
        let bottom_y = (half_height + height_pad) as i32;

        // Frame
        self.frame_tex.update(None, &frame, RENDER_WIDTH * 3)?;
        let frame_rect = Rect::new(0, 0, half_width, half_height);
        let _ = self.canvas.copy(&self.frame_tex, self.overscan, frame_rect);

        // Nametables
        let ntbl_pitch = (NT_TEX_WIDTH * 3) as usize;
        let ntbl_x_right = right_x + (quart_width + width_pad) as i32;
        let ntbl_y_top = 0;
        let ntbl_y_bot = (quart_height + height_pad) as i32;
        let ntbl1_rect = Rect::new(right_x, ntbl_y_top, quart_width, quart_height);
        let ntbl2_rect = Rect::new(ntbl_x_right, ntbl_y_top, quart_width, quart_height);
        let ntbl3_rect = Rect::new(right_x, ntbl_y_bot, quart_width, quart_height);
        let ntbl4_rect = Rect::new(ntbl_x_right, ntbl_y_bot, quart_width, quart_height);

        self.ntbl_texs[0].update(None, &nametables[0], ntbl_pitch)?;
        let _ = self.canvas.copy(&self.ntbl_texs[0], None, ntbl1_rect);
        self.ntbl_texs[1].update(None, &nametables[1], ntbl_pitch)?;
        let _ = self.canvas.copy(&self.ntbl_texs[1], None, ntbl2_rect);
        self.ntbl_texs[2].update(None, &nametables[2], ntbl_pitch)?;
        let _ = self.canvas.copy(&self.ntbl_texs[2], None, ntbl3_rect);
        self.ntbl_texs[3].update(None, &nametables[3], ntbl_pitch)?;
        let _ = self.canvas.copy(&self.ntbl_texs[3], None, ntbl4_rect);

        // Pattern tables
        let pat_pitch = (PAT_TEX_WIDTH * 3) as usize;
        let pat_x_right = right_x + (quart_width + width_pad) as i32;
        let pat1_rect = Rect::new(right_x, bottom_y, quart_width, quart_height);
        let pat2_rect = Rect::new(pat_x_right, bottom_y, quart_width, quart_height);

        self.pat_texs[0].update(None, &pattern_tables[0], pat_pitch)?;
        let _ = self.canvas.copy(&self.pat_texs[0], None, pat1_rect);
        self.pat_texs[1].update(None, &pattern_tables[1], pat_pitch)?;
        let _ = self.canvas.copy(&self.pat_texs[1], None, pat2_rect);

        // Palettes
        let sys_pal_pitch = 16 * 3;
        let pal_height = half_width / 4;
        let sys_pal_rect = Rect::new(0, bottom_y, half_width, pal_height);

        self.pal_texs[0].update(None, &palettes[0], sys_pal_pitch)?;
        let _ = self.canvas.copy(&self.pal_texs[0], None, sys_pal_rect);

        let pal_pitch = (8 + 1) * 3;
        let pal_y_bot = bottom_y + (pal_height + height_pad) as i32;
        let pal_rect = Rect::new(0, pal_y_bot, half_width / 16 * (8 + 1), pal_height);

        self.pal_texs[1].update(None, &palettes[1], pal_pitch)?;
        let _ = self.canvas.copy(&self.pal_texs[1], None, pal_rect);

        self.canvas.present();
        Ok(())
    }

    pub fn render_bg(&mut self) -> Result<()> {
        self.canvas.clear();
        self.canvas.present();
        Ok(())
    }

    /// Add audio samples to the audio queue
    pub fn enqueue_audio(&mut self, samples: &[f32]) {
        self.audio_device.queue(samples);
        // Keep audio in sync
        loop {
            let latency = self.audio_device.size() as f32 / SAMPLE_RATE as f32;
            if latency <= 1.0 {
                break;
            }
        }
    }

    /// Toggles fullscreen mode on the SDL2 window
    pub fn toggle_fullscreen(&mut self) -> Result<()> {
        let state = self.canvas.window().fullscreen_state();
        let mode = if state == FullscreenType::Off {
            video::FullscreenType::True
        } else {
            video::FullscreenType::Off
        };
        self.canvas
            .window_mut()
            .set_fullscreen(mode)
            .map_err(util::str_to_err)?;
        self.canvas.window_mut().set_size(self.width, self.height)?;
        Ok(())
    }

    /// Sets the window title
    pub fn set_title(&mut self, title: &str) -> Result<()> {
        self.canvas.window_mut().set_title(title)?;
        Ok(())
    }
}
