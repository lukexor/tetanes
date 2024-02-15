use crate::{
    control_deck,
    input::{FourPlayer, Player},
    nes::{config::Config, event::Event, Nes},
    profiling, NesError,
};
use pixels::{wgpu, PixelsContext};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use web_time::{Duration, Instant};
use winit::{event::WindowEvent, event_loop::EventLoop, window::Window};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Menu {
    Config(ConfigTab),
    Keybind(Player),
    #[default]
    LoadRom,
    About,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfigTab {
    General,
    Emulation,
    Audio,
    Video,
}

impl ConfigTab {
    #[inline]
    #[must_use]
    pub const fn as_slice() -> &'static [Self] {
        &[Self::General, Self::Emulation, Self::Audio, Self::Video]
    }
}

impl AsRef<str> for ConfigTab {
    fn as_ref(&self) -> &str {
        match self {
            Self::General => "General",
            Self::Emulation => "Emulation",
            Self::Audio => "Audio",
            Self::Video => "Video",
        }
    }
}

impl AsRef<str> for Player {
    fn as_ref(&self) -> &str {
        match self {
            Self::One => "Player One",
            Self::Two => "Player Two",
            Self::Three => "Player Three",
            Self::Four => "Player Four",
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleRate {
    S32,
    S44_1,
    S48,
    S96,
}

impl SampleRate {
    #[inline]
    #[must_use]
    pub const fn as_slice() -> &'static [Self] {
        &[Self::S32, Self::S44_1, Self::S48, Self::S96]
    }

    #[inline]
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        match self {
            Self::S32 => 32000.0,
            Self::S44_1 => 44100.0,
            Self::S48 => 48000.0,
            Self::S96 => 96000.0,
        }
    }
}

impl AsRef<str> for SampleRate {
    fn as_ref(&self) -> &str {
        match self {
            Self::S32 => "32 kHz",
            Self::S44_1 => "44.1 kHz",
            Self::S48 => "48 kHz",
            Self::S96 => "96 kHz",
        }
    }
}

impl TryFrom<usize> for SampleRate {
    type Error = &'static str;
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::S32),
            1 => Ok(Self::S44_1),
            2 => Ok(Self::S48),
            3 => Ok(Self::S96),
            _ => Err("Invalid sample rate: {value}"),
        }
    }
}

impl TryFrom<f32> for SampleRate {
    type Error = &'static str;
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        match value as i32 {
            32000 => Ok(Self::S32),
            44100 => Ok(Self::S44_1),
            48000 => Ok(Self::S48),
            96000 => Ok(Self::S96),
            _ => Err("Invalid sample rate: {value}"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Speed {
    S25,
    S50,
    S75,
    S100,
    S125,
    S150,
    S175,
    S200,
}

impl Speed {
    #[inline]
    #[must_use]
    pub const fn as_slice() -> &'static [Self] {
        &[
            Self::S25,
            Self::S50,
            Self::S75,
            Self::S100,
            Self::S125,
            Self::S150,
            Self::S175,
            Self::S200,
        ]
    }

    #[inline]
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        match self {
            Self::S25 => 0.25,
            Self::S50 => 0.50,
            Self::S75 => 0.75,
            Self::S100 => 1.0,
            Self::S125 => 1.25,
            Self::S150 => 1.5,
            Self::S175 => 1.75,
            Self::S200 => 2.0,
        }
    }
}

impl AsRef<str> for Speed {
    fn as_ref(&self) -> &str {
        match self {
            Self::S25 => "25%",
            Self::S50 => "50%",
            Self::S75 => "75%",
            Self::S100 => "100%",
            Self::S125 => "125%",
            Self::S150 => "150%",
            Self::S175 => "175%",
            Self::S200 => "200%",
        }
    }
}

impl From<usize> for Speed {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::S25,
            1 => Self::S50,
            2 => Self::S75,
            4 => Self::S125,
            5 => Self::S150,
            6 => Self::S175,
            7 => Self::S200,
            _ => Self::S100,
        }
    }
}

impl From<f32> for Speed {
    fn from(value: f32) -> Self {
        Self::from(((4.0 * value) as usize).saturating_sub(1))
    }
}

#[must_use]
pub struct Gui {
    window: Arc<Window>,
    state: State,
    ctx: egui::Context,
    egui_state: egui_winit::State,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
    renderer: egui_wgpu::Renderer,
    paint_jobs: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
}

impl std::fmt::Debug for Gui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gui")
            .field("gui_state", &self.state)
            .finish()
    }
}

impl Gui {
    /// Create `Framework`.
    pub fn new(
        event_loop: &EventLoop<Event>,
        window: Arc<Window>,
        pixels: &pixels::Pixels<'static>,
    ) -> Self {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let ctx = egui::Context::default();
        #[cfg(not(target_arch = "wasm32"))]
        ctx.set_embed_viewports(false);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::default(),
            event_loop,
            Some(scale_factor),
            Some(pixels.device().limits().max_texture_dimension_2d as usize),
        );

        Self {
            window,
            state: State::new(),
            ctx,
            egui_state: state,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [window_size.width, window_size.height],
                pixels_per_point: scale_factor,
            },
            renderer: egui_wgpu::Renderer::new(
                pixels.device(),
                pixels.render_texture_format(),
                None,
                1,
            ),
            paint_jobs: vec![],
            textures: egui::TexturesDelta::default(),
        }
    }

    /// Handle event.
    pub fn on_event(&mut self, event: &WindowEvent) -> egui_winit::EventResponse {
        match event {
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.screen_descriptor.size_in_pixels = [size.width, size.height];
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.screen_descriptor.pixels_per_point = *scale_factor as f32;
            }
            WindowEvent::ThemeChanged(theme) => {
                use egui::{viewport::ViewportCommand, SystemTheme};
                use winit::window::Theme;
                self.ctx
                    .send_viewport_cmd(ViewportCommand::SetTheme(if *theme == Theme::Light {
                        SystemTheme::Light
                    } else {
                        SystemTheme::Dark
                    }));
            }
            _ => (),
        }
        self.egui_state.on_window_event(&self.window, event)
    }

    /// Prepare.
    pub fn prepare(&mut self, paused: bool, config: &mut Config) {
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let output = self.ctx.run(raw_input, |ctx| {
            self.state.ui(ctx, paused, config);
        });

        self.textures.append(output.textures_delta);
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        self.paint_jobs = self
            .ctx
            .tessellate(output.shapes, self.screen_descriptor.pixels_per_point);
    }

    /// Render.
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        ctx: &PixelsContext<'_>,
    ) {
        for (id, image_delta) in &self.textures.set {
            self.renderer
                .update_texture(&ctx.device, &ctx.queue, *id, image_delta);
        }
        self.renderer.update_buffers(
            &ctx.device,
            &ctx.queue,
            encoder,
            &self.paint_jobs,
            &self.screen_descriptor,
        );

        {
            let mut renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            self.renderer
                .render(&mut renderpass, &self.paint_jobs, &self.screen_descriptor);
        }

        // Cleanup
        let textures = std::mem::take(&mut self.textures);
        for id in &textures.free {
            self.renderer.free_texture(id);
        }
    }

    pub fn toggle_menu(&mut self, menu: Menu) {
        match menu {
            Menu::Config(_) => self.state.config_open = !self.state.config_open,
            Menu::Keybind(_) => self.state.keybind_open = !self.state.keybind_open,
            Menu::LoadRom => self.state.load_rom_open = !self.state.load_rom_open,
            Menu::About => self.state.about_open = !self.state.about_open,
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct State {
    config_open: bool,
    keybind_open: bool,
    load_rom_open: bool,
    about_open: bool,
    version: String,
    config_dir: String,
    save_dir: String,
    sram_dir: String,
    messages: Vec<(String, Instant)>,
    error: Option<String>,
}

impl State {
    pub const MENUBAR_HEIGHT: f32 = 18.0;

    /// Create a `Gui`.
    fn new() -> Self {
        Self {
            config_open: false,
            keybind_open: false,
            load_rom_open: false,
            about_open: false,
            version: format!("Version: {}", env!("CARGO_PKG_VERSION")),
            config_dir: control_deck::Config::directory()
                .to_string_lossy()
                .to_string(),
            save_dir: control_deck::Config::save_dir()
                .to_string_lossy()
                .to_string(),
            sram_dir: control_deck::Config::sram_dir()
                .to_string_lossy()
                .to_string(),
            messages: vec![],
            error: None,
        }
    }

    /// Create the UI.
    fn ui(&mut self, ctx: &egui::Context, paused: bool, config: &mut Config) {
        use egui::RichText;

        egui::TopBottomPanel::top("menu_bar")
            .exact_height(Self::MENUBAR_HEIGHT)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    egui::global_dark_light_mode_switch(ui);
                    ui.separator();

                    ui.menu_button("File", |ui| {
                        if ui.button("Load ROM...").clicked() || self.load_rom_open {
                            self.open_load_dialog();
                        }
                        if ui.button("Recently Played...").clicked() {
                            self.todo(ui);
                        }
                        if ui.button("Load Replay...").clicked() {
                            self.todo(ui);
                        }
                        // Load Replay
                        if ui.button("Configuration").clicked() {
                            self.config_open = true;
                            // button Restore Defaults
                            // button Clear Save State
                            //
                            // General
                            //   textedit Config Path
                            //   textedit Save Path
                            //   textedit Battey-Backed Save Path
                            //   checkbox Enable Rewind
                            //     textedit Rewind Frames
                            //     textedit Rewind Buffer Size (MB)
                            //
                            // Emulation
                            //   combobox Speed
                            //   checkbox Enable Zapper Gun
                            //   combobox Four Player Mode
                            //   combobox NES Region
                            //   combobox RAM State
                            //   checkbox Concurrent D-Pad
                            //
                            // View
                            //   combobox Scale
                            //   checkbox Show FPS
                            //   checkbox Show Messages
                            //   combobox Video Filter
                            //   checkbox Show Overscan
                            //   combobox Fullscreen Mode
                            //   checkbox Enable VSync
                            //   checkbox Always On Top
                            //
                            // Audio
                            //   checkbox Enabled
                            //   combobox Output Device
                            //   combobox Sample Rate
                            //   combobox Latency ms
                            //   checkbox APU Channels
                            ui.close_menu();
                        }
                        if ui.button("Keybinds").clicked() {
                            self.keybind_open = true;
                            // Keyboard
                            // Controllers
                            // combobox
                            //   Player 1
                            //   Player 2
                            //   Player 3
                            //   Player 4
                            ui.close_menu();
                        };
                        if ui.button("Reset").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Power Cycle").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Quit").clicked() {
                            self.todo(ui);
                        };
                    });
                    ui.menu_button("Controls", |ui| {
                        if ui.button("Pause/Unpause").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Mute/Unmute").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Save State").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Load State").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Save Slot...").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Rewind").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Begin/End Replay Recording").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Begin/End Audio Recording").clicked() {
                            self.todo(ui);
                        };
                    });
                    ui.menu_button("Emulation", |ui| {
                        if ui.button("Speed...").clicked() {
                            self.todo(ui);
                            // Increase/Decrease/Default
                        };
                        ui.checkbox(&mut config.control_deck.zapper, "Enable Zapper Gun");
                        // Four Player Mode
                        // NES Region
                        // RAM State
                        // Concurrent D-Pad
                    });
                    ui.menu_button("View", |ui| {
                        if ui.button("Scale...").clicked() {
                            self.todo(ui);
                        };
                        let mut show_fps = false;
                        ui.checkbox(&mut show_fps, "Show FPS");
                        let mut show_messages = false;
                        ui.checkbox(&mut show_messages, "Show Messages");
                        if ui.button("Video Filter...").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Take Screenshot").clicked() {
                            self.todo(ui);
                        };
                    });
                    ui.menu_button("Window", |ui| {
                        if ui.button("Maximize").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Minimize").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Toggle Fullscreen").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Hide Menu Bar").clicked() {
                            self.todo(ui);
                        };
                    });
                    ui.menu_button("Debug", |ui| {
                        #[cfg(feature = "profiling")]
                        {
                            let mut profile = puffin::are_scopes_on();
                            ui.checkbox(&mut profile, "Toggle profiling");
                            profiling::enable(profile);
                        }
                        if ui.button("Toggle CPU Debugger").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Toggle PPU Debugger").clicked() {
                            self.todo(ui);
                        };
                        if ui.button("Toggle APU Debugger").clicked() {
                            self.todo(ui);
                        };
                    });
                    if ui.button("About").clicked() {
                        self.about_open = true;
                        ui.close_menu();
                    }
                });
            });

        if !self.messages.is_empty() {
            egui::TopBottomPanel::top("messages").show(ctx, |ui| {
                const TIMEOUT: Duration = Duration::from_secs(3);

                let now = Instant::now();
                self.messages
                    .retain(|(_, created)| (now - *created) < TIMEOUT);
                self.messages.dedup_by(|a, b| a.0.eq(&b.0));
                for (message, _) in &self.messages {
                    ui.label(message);
                }
            });
        }
        // TODO: show confirm quit dialog?

        if paused || self.error.is_some() {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                // TODO: Render framerate if enabled
                // TODO: maybe show other statuses like rewinding/playback/recording - bitflags?
                if paused {
                    ui.label("Paused");
                }
                let mut clear_error = false;
                if let Some(ref error) = self.error {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(error).color(egui::Color32::RED));
                        clear_error = ui.button("Clear").clicked();
                    });
                }
                if clear_error {
                    self.error = None;
                }
            });
        }

        egui::Window::new("Configuration")
            .open(&mut self.config_open)
            .show(ctx, |ui| {
                // let mut save_slot = config.save_slot as usize - 1;
                // if ui.combo_box("Save Slot", &mut save_slot, &["1", "2", "3", "4"], 4)? {
                //     self.config.save_slot = save_slot as u8 + 1;
                // }
            });

        egui::Window::new("About TetaNES")
            .open(&mut self.about_open)
            .show(ctx, |ui| {
                ui.label(RichText::new(&self.version).strong());
                ui.hyperlink("https://github.com/lukexor/tetanes");

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Configuration: ").strong());
                    ui.label(&self.config_dir);
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Save States: ").strong());
                    ui.label(&self.save_dir);
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Battery-Backed Save States: ").strong());
                    ui.label(&self.sram_dir);
                });
            });

        // #[cfg(feature = "profiling")]
        // puffin_egui::show_viewport_if_enabled(ctx);
    }

    fn todo(&mut self, ui: &mut egui::Ui) {
        log::warn!("not implemented yet");
    }

    fn open_load_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("nes", &["nes"])
            .pick_file()
        {
            log::info!("loading rom: {path:?}");
            self.load_rom_open = false;
            // Send LoadROM path event
            // self.open_puffin_path(path);
        }
    }
}

impl Nes {
    #[inline]
    pub fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        log::info!("{text}");
        self.renderer
            .gui
            .state
            .messages
            .push((text, Instant::now()));
    }

    #[inline]
    pub fn on_error(&mut self, err: NesError) {
        self.pause(true);
        log::error!("{err:?}");
        self.renderer.gui.state.error = Some(err.to_string());
    }
}
