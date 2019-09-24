//! Window Management using SDL2

use crate::console::{RENDER_HEIGHT, RENDER_WIDTH, SAMPLE_RATE};
use crate::Result;
use crate::{to_nes_err, util};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::mouse::MouseUtil;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{self, FullscreenType};
use sdl2::{EventPump, GameControllerSubsystem};

const WINDOW_WIDTH: u32 = (RENDER_WIDTH as f32 * 8.0 / 7.0) as u32; // for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = RENDER_HEIGHT;
const DEBUG_PADDING: u32 = 5;

pub struct TextureMap {
    tex: Texture<'static>,
    pitch: usize,
    src: Rect,
    dst: Rect,
}

/// A Window instance
pub struct Window {
    width: u32,
    height: u32,
    pub controller_sub: GameControllerSubsystem,
    audio_device: AudioQueue<f32>,
    mouse: MouseUtil,
    canvas: Canvas<video::Window>,
    game_view: TextureMap,
    ntbls: Vec<TextureMap>,
    pats: Vec<TextureMap>,
    pals: Vec<TextureMap>,
    _texture_creator: TextureCreator<video::WindowContext>,
}

impl Window {
    /// Creates a new Window instance containing the necessary window, audio, and input components
    /// used by the UI
    pub fn init(
        title: &str,
        scale: u32,
        fullscreen: bool,
        debug: bool,
    ) -> Result<(Self, EventPump)> {
        let context = sdl2::init().map_err(to_nes_err)?;

        let width = WINDOW_WIDTH * scale;
        let height = WINDOW_HEIGHT * scale;

        // Window
        let video_sub = context.video().map_err(to_nes_err)?;
        let mut window_builder = video_sub.window(title, width, height);
        window_builder.position_centered().resizable();
        let mouse = context.mouse();
        if fullscreen {
            mouse.show_cursor(false);
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

        // Textures
        let texture_creator = canvas.texture_creator();
        let texture_creator_ptr = &texture_creator as *const TextureCreator<video::WindowContext>;

        let game_view = Self::game_view_tex_map(texture_creator_ptr, width, height, debug)?;
        let ntbls = Self::nametable_tex_maps(texture_creator_ptr, width, height)?;
        let pats = Self::pattern_tex_maps(texture_creator_ptr, width, height)?;
        let pals = Self::palette_tex_maps(texture_creator_ptr, width, height)?;

        // Set up Audio
        let audio_sub = context.audio().map_err(to_nes_err)?;
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(1),
            samples: None,
        };
        let audio_device = audio_sub
            .open_queue(None, &desired_spec)
            .map_err(to_nes_err)?;
        audio_device.resume();

        // Set up Input event pump
        let event_pump = context.event_pump().map_err(to_nes_err)?;
        let controller_sub = context.game_controller().map_err(to_nes_err)?;

        let window = Self {
            width,
            height,
            controller_sub,
            audio_device,
            mouse,
            canvas,
            game_view,
            ntbls,
            pats,
            pals,
            _texture_creator: texture_creator,
        };
        Ok((window, event_pump))
    }

    /// Updates the Window canvas texture with the passed in pixel data
    pub fn update_frame(&mut self, pixels: Vec<u8>) -> Result<()> {
        self.game_view
            .tex
            .update(None, &pixels, self.game_view.pitch)?;
        self.render_frame()
    }

    pub fn render_frame(&mut self) -> Result<()> {
        self.canvas.clear();
        self.canvas
            .copy(&self.game_view.tex, self.game_view.src, self.game_view.dst)
            .map_err(to_nes_err)?;
        self.canvas.present();
        Ok(())
    }

    pub fn set_debug_size(&mut self) -> Result<()> {
        self.canvas
            .set_logical_size(self.width, self.height - (self.height / 6))?;
        self.canvas
            .window_mut()
            .set_size(self.width, self.height - (self.height / 6))?;
        Ok(())
    }

    pub fn update_debug(
        &mut self,
        game_view: Vec<u8>,
        nametables: &Vec<Vec<u8>>,
        pattern_tables: &Vec<Vec<u8>>,
        palettes: &Vec<Vec<u8>>,
    ) -> Result<()> {
        self.game_view
            .tex
            .update(None, &game_view, self.game_view.pitch)?;
        for (i, ntbl) in self.ntbls.iter_mut().enumerate() {
            ntbl.tex.update(None, &nametables[i], ntbl.pitch)?;
        }
        for (i, pat) in self.pats.iter_mut().enumerate() {
            pat.tex.update(None, &pattern_tables[i], pat.pitch)?;
        }
        for (i, pal) in self.pals.iter_mut().enumerate() {
            pal.tex.update(None, &palettes[i], pal.pitch)?;
        }
        self.render_debug()
    }

    pub fn render_debug(&mut self) -> Result<()> {
        self.canvas.clear();
        self.canvas
            .copy(&self.game_view.tex, self.game_view.src, self.game_view.dst)
            .map_err(to_nes_err)?;
        for ntbl in self.ntbls.iter() {
            self.canvas
                .copy(&ntbl.tex, ntbl.src, ntbl.dst)
                .map_err(to_nes_err)?;
        }
        for pat in self.pats.iter() {
            self.canvas
                .copy(&pat.tex, pat.src, pat.dst)
                .map_err(to_nes_err)?;
        }
        for pal in self.pals.iter() {
            self.canvas
                .copy(&pal.tex, pal.src, pal.dst)
                .map_err(to_nes_err)?;
        }
        self.canvas.present();
        Ok(())
    }

    pub fn render_blank(&mut self) -> Result<()> {
        self.canvas.clear();
        self.canvas.set_draw_color(Color::RGB(0, 0, 0));
        self.canvas
            .draw_rect(Rect::new(0, 0, self.width, self.height))
            .map_err(to_nes_err)?;
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
            self.mouse.show_cursor(false);
            video::FullscreenType::True
        } else {
            self.mouse.show_cursor(true);
            video::FullscreenType::Off
        };
        self.canvas
            .window_mut()
            .set_fullscreen(mode)
            .map_err(to_nes_err)?;
        self.canvas.window_mut().set_size(self.width, self.height)?;
        Ok(())
    }

    /// Sets the window title
    pub fn set_title(&mut self, title: &str) -> Result<()> {
        self.canvas.window_mut().set_title(title)?;
        Ok(())
    }

    fn game_view_tex_map(
        creator: *const TextureCreator<video::WindowContext>,
        width: u32,
        height: u32,
        debug: bool,
    ) -> Result<TextureMap> {
        let half_width = (width / 2) - DEBUG_PADDING;
        let half_height = (height / 2) - DEBUG_PADDING;
        let tex_width = RENDER_WIDTH;
        let tex_height = RENDER_HEIGHT;

        let game_view = TextureMap {
            tex: unsafe { &*creator }.create_texture_streaming(
                PixelFormatEnum::RGB24,
                tex_width,
                tex_height,
            )?,
            pitch: (tex_width * 3) as usize,
            src: Rect::new(0, 8, RENDER_WIDTH, RENDER_HEIGHT - 16), // Cuts off overscan
            dst: if debug {
                Rect::new(0, 0, half_width, half_height)
            } else {
                Rect::new(0, 0, width, height)
            },
        };
        Ok(game_view)
    }

    fn nametable_tex_maps(
        creator: *const TextureCreator<video::WindowContext>,
        width: u32,
        height: u32,
    ) -> Result<Vec<TextureMap>> {
        let half_width = (width / 2) - DEBUG_PADDING;
        let half_height = (height / 2) - DEBUG_PADDING;
        let quart_width = half_width / 2;
        let quart_height = (half_height / 2) - DEBUG_PADDING / 2;
        let right_x = (half_width + DEBUG_PADDING) as i32;
        let ntbl_x_right = right_x + (quart_width + DEBUG_PADDING) as i32;
        let ntbl_y_top = 0;
        let ntbl_y_bot = (quart_height + DEBUG_PADDING) as i32;

        let ntbl_rects = vec![
            Rect::new(right_x, ntbl_y_top, quart_width, quart_height),
            Rect::new(ntbl_x_right, ntbl_y_top, quart_width, quart_height),
            Rect::new(right_x, ntbl_y_bot, quart_width, quart_height),
            Rect::new(ntbl_x_right, ntbl_y_bot, quart_width, quart_height),
        ];

        let mut ntbls = Vec::with_capacity(4);
        let tex_width = RENDER_WIDTH;
        let tex_height = RENDER_HEIGHT;
        for rect in ntbl_rects {
            let tex_map = TextureMap {
                tex: unsafe { &*creator }.create_texture_streaming(
                    PixelFormatEnum::RGB24,
                    tex_width,
                    tex_height,
                )?,
                pitch: (tex_width * 3) as usize,
                src: Rect::new(0, 0, RENDER_WIDTH, RENDER_HEIGHT),
                dst: rect,
            };
            ntbls.push(tex_map);
        }
        Ok(ntbls)
    }

    fn pattern_tex_maps(
        creator: *const TextureCreator<video::WindowContext>,
        width: u32,
        height: u32,
    ) -> Result<Vec<TextureMap>> {
        let half_width = (width / 2) - DEBUG_PADDING;
        let half_height = (height / 2) - DEBUG_PADDING;
        let quart_width = half_width / 2;
        let quart_height = (half_height / 2) - DEBUG_PADDING / 2;
        let right_x = (half_width + DEBUG_PADDING) as i32;
        let right_x2 = right_x + (quart_width + DEBUG_PADDING) as i32;
        let bottom_y = (half_height + DEBUG_PADDING) as i32;

        let pat_rects = vec![
            Rect::new(right_x, bottom_y, quart_width, quart_height),
            Rect::new(right_x2, bottom_y, quart_width, quart_height),
        ];

        let mut pats = Vec::with_capacity(2);
        let tex_width = RENDER_WIDTH / 2;
        let tex_height = tex_width;
        for rect in pat_rects {
            let tex_map = TextureMap {
                tex: unsafe { &*creator }.create_texture_streaming(
                    PixelFormatEnum::RGB24,
                    tex_width,
                    tex_height,
                )?,
                pitch: (tex_width * 3) as usize,
                src: Rect::new(0, 0, RENDER_WIDTH, RENDER_HEIGHT),
                dst: rect,
            };
            pats.push(tex_map);
        }
        Ok(pats)
    }

    fn palette_tex_maps(
        creator: *const TextureCreator<video::WindowContext>,
        width: u32,
        height: u32,
    ) -> Result<Vec<TextureMap>> {
        let half_width = (width / 2) - DEBUG_PADDING;
        let half_height = (height / 2) - DEBUG_PADDING;
        let pal_height = half_width / 4;
        let bottom_y_1 = (half_height + DEBUG_PADDING) as i32;
        let bottom_y_2 = bottom_y_1 + (pal_height + DEBUG_PADDING) as i32;

        let system_pal_tex_width = 16;
        let game_pal_tex_width = 9;
        let pal_tex_height = 4;
        let game_pal_width = half_width / system_pal_tex_width * game_pal_tex_width;
        let pal_texs = vec![
            (
                system_pal_tex_width,
                pal_tex_height,
                Rect::new(0, bottom_y_1, half_width, pal_height),
            ),
            (
                game_pal_tex_width,
                pal_tex_height,
                Rect::new(0, bottom_y_2, game_pal_width, pal_height),
            ),
        ];

        let mut pals = Vec::with_capacity(2);
        for tex in pal_texs {
            let tex_width = tex.0;
            let tex_height = tex.1;
            let rect = tex.2;
            let tex_map = TextureMap {
                tex: unsafe { &*creator }.create_texture_streaming(
                    PixelFormatEnum::RGB24,
                    tex_width,
                    tex_height,
                )?,
                pitch: (tex_width * 3) as usize,
                src: Rect::new(0, 0, RENDER_WIDTH, RENDER_HEIGHT),
                dst: rect,
            };
            pals.push(tex_map);
        }
        Ok(pals)
    }
}
