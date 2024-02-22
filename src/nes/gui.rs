use crate::{
    control_deck,
    input::Player,
    nes::{config::Config, event::Event, platform::WindowExt, Nes},
    profile, NesError,
};
use egui::{
    global_dark_light_mode_switch, load::SizedTexture, menu, viewport::ViewportCommand, Align,
    Align2, Area, CentralPanel, ClippedPrimitive, Context, CursorIcon, Frame, Image, Layout,
    Margin, Order, RichText, Style, SystemTheme, TexturesDelta, TopBottomPanel, Ui, Vec2,
    ViewportId, Window,
};
use pixels::{
    wgpu::{self, TextureViewDescriptor},
    PixelsContext,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use web_time::{Duration, Instant};
use winit::window::Theme;
use winit::{event::WindowEvent, event_loop::EventLoop, window::Window as WinitWindow};

const MSG_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_MESSAGES: usize = 3;

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

#[must_use]
pub struct Gui {
    window: Arc<WinitWindow>,
    state: State,
    ctx: Context,
    egui_state: egui_winit::State,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
    renderer: egui_wgpu::Renderer,
    paint_jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,
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
        window: Arc<WinitWindow>,
        pixels: &pixels::Pixels<'static>,
    ) -> Self {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let ctx = Context::default();

        let egui_state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::default(),
            event_loop,
            Some(scale_factor),
            Some(pixels.device().limits().max_texture_dimension_2d as usize),
        );

        let texture = pixels.texture();
        let texture_view = texture.create_view(&TextureViewDescriptor::default());
        let mut renderer =
            egui_wgpu::Renderer::new(pixels.device(), pixels.render_texture_format(), None, 1);
        let egui_texture = renderer.register_native_texture(
            pixels.device(),
            &texture_view,
            wgpu::FilterMode::Nearest,
        );
        let state = State::new(
            Arc::clone(&window),
            SizedTexture::new(
                egui_texture,
                Vec2 {
                    x: window_size.width as f32,
                    y: window_size.height as f32,
                },
            ),
        );

        Self {
            window,
            state,
            ctx,
            egui_state,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [window_size.width, window_size.height],
                pixels_per_point: scale_factor,
            },
            renderer,
            paint_jobs: vec![],
            textures: TexturesDelta::default(),
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
        self.state.status = if paused { Some("Paused") } else { None };
        let output = self.ctx.run(raw_input, |ctx| {
            self.state.ui(ctx, config);
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
        profile!();

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
    window: Arc<WinitWindow>,
    texture: SizedTexture,
    show_menu: bool,
    menu_height: f32,
    config_open: bool,
    keybind_open: bool,
    load_rom_open: bool,
    about_open: bool,
    version: String,
    config_dir: String,
    save_dir: String,
    sram_dir: String,
    messages: Vec<(String, Instant)>,
    status: Option<&'static str>,
    error: Option<String>,
}

impl State {
    /// Create a `Gui`.
    fn new(window: Arc<WinitWindow>, texture: SizedTexture) -> Self {
        Self {
            window,
            texture,
            show_menu: true,
            menu_height: 0.0,
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
            status: None,
            error: None,
        }
    }

    /// Create the UI.
    fn ui(&mut self, ctx: &Context, config: &mut Config) {
        profile!();

        TopBottomPanel::top("menu_bar")
            .resizable(true)
            .show_animated(ctx, self.show_menu, |ui| self.menu_bar(ui, config));
        CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| self.nes_frame(ui, config));

        // TODO: show confirm quit dialog?

        let mut config_open = self.config_open;
        Window::new("Configuration")
            .open(&mut config_open)
            .show(ctx, |ui| self.configuration(ui));
        self.config_open = config_open;

        let mut about_open = self.about_open;
        Window::new("About TetaNES")
            .open(&mut about_open)
            .show(ctx, |ui| self.about(ui));
        self.about_open = about_open;

        #[cfg(feature = "profiling")]
        puffin_egui::show_viewport_if_enabled(ctx);
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, config: &mut Config) {
        ui.style_mut().spacing.menu_margin = Margin::ZERO;
        let inner_response = menu::bar(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                global_dark_light_mode_switch(ui);
                ui.separator();

                ui.menu_button("File", |ui| self.file_menu(ui));
                ui.menu_button("Controls", |ui| self.controls_menu(ui));
                ui.menu_button("Emulation", |ui| self.emulation_menu(ui, config));
                ui.menu_button("View", |ui| self.view_menu(ui, config));
                ui.menu_button("Window", |ui| self.window_menu(ui));
                ui.menu_button("Debug", |ui| self.debug_menu(ui));
                ui.toggle_value(&mut self.about_open, "About");
            });
        });
        let height = inner_response.response.rect.height();
        if height != self.menu_height {
            self.menu_height = height;
            self.resize_window(ui.style(), config);
        }
    }

    fn file_menu(&mut self, ui: &mut Ui) {
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
    }

    fn controls_menu(&mut self, ui: &mut Ui) {
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
    }

    fn emulation_menu(&mut self, ui: &mut Ui, config: &mut Config) {
        if ui.button("Speed...").clicked() {
            self.todo(ui);
            // Increase/Decrease/Default
        };
        ui.checkbox(&mut config.control_deck.zapper, "Enable Zapper Gun");
        // Four Player Mode
        // NES Region
        // RAM State
        // Concurrent D-Pad
    }

    fn view_menu(&mut self, ui: &mut Ui, config: &mut Config) {
        if ui.button("Scale...").clicked() {
            self.todo(ui);
        };
        ui.checkbox(&mut config.show_fps, "Show FPS");
        ui.checkbox(&mut config.show_messages, "Show Messages");
        if ui
            .checkbox(&mut config.hide_overscan, "Hide Overscan")
            .clicked()
        {
            self.resize_window(ui.style(), config);
        }
        if ui.button("Video Filter...").clicked() {
            self.todo(ui);
        };
        if ui.button("Take Screenshot").clicked() {
            self.todo(ui);
        };
    }

    fn window_menu(&mut self, ui: &mut Ui) {
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
    }

    fn debug_menu(&mut self, ui: &mut Ui) {
        #[cfg(feature = "profiling")]
        {
            let mut profile = puffin::are_scopes_on();
            ui.checkbox(&mut profile, "Toggle profiling");
            crate::profiling::enable(profile);
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
    }

    fn message_bar(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        self.messages.retain(|(_, expires)| now < *expires);
        self.messages.dedup_by(|a, b| a.0.eq(&b.0));
        for (message, _) in self.messages.iter().take(MAX_MESSAGES) {
            ui.label(message);
        }
    }

    fn error_bar(&mut self, ui: &mut Ui) {
        let mut clear_error = false;
        if let Some(ref error) = self.error {
            ui.vertical(|ui| {
                ui.label(RichText::new(error).color(egui::Color32::RED));
                clear_error = ui.button("Clear").clicked();
            });
        }
        if clear_error {
            self.error = None;
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        // TODO: Render framerate if enabled
        // TODO: maybe show other statuses like rewinding/playback/recording - bitflags?
        if let Some(status) = self.status {
            ui.label(status);
        }
    }

    fn nes_frame(&mut self, ui: &mut egui::Ui, config: &Config) {
        if !self.messages.is_empty() || self.error.is_some() {
            Area::new("messages")
                .anchor(Align2::LEFT_TOP, Vec2::ZERO)
                .order(Order::Foreground)
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(Layout::top_down(Align::LEFT).with_main_wrap(true), |ui| {
                            ui.set_width(ui.available_width());
                            self.message_bar(ui);
                            self.error_bar(ui);
                        });
                    });
                });
        }
        if self.status.is_some() {
            Area::new("status")
                .anchor(Align2::LEFT_BOTTOM, Vec2::ZERO)
                .order(Order::Foreground)
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.with_layout(Layout::top_down(Align::LEFT).with_main_wrap(true), |ui| {
                            ui.set_width(ui.available_width());
                            self.status_bar(ui);
                        });
                    });
                });
        }
        CentralPanel::default()
            .frame(Frame::none())
            .show_inside(ui, |ui| {
                ui.add_sized(
                    ui.available_size(),
                    Image::from_texture(self.texture)
                        .maintain_aspect_ratio(true)
                        .shrink_to_fit(),
                )
                .on_hover_cursor(if config.control_deck.zapper {
                    CursorIcon::Crosshair
                } else {
                    CursorIcon::Default
                });
            });
    }

    fn configuration(&mut self, ui: &mut Ui) {
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
        // let mut save_slot = config.save_slot as usize - 1;
        // if ui.combo_box("Save Slot", &mut save_slot, &["1", "2", "3", "4"], 4)? {
        //     self.config.save_slot = save_slot as u8 + 1;
        // }
    }

    fn about(&mut self, ui: &mut Ui) {
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
    }

    fn todo(&mut self, ui: &mut Ui) {
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

    fn resize_window(&mut self, style: &Style, config: &mut Config) {
        let spacing = style.spacing.item_spacing;
        let border = 1.0;
        let (inner_size, min_inner_size) =
            config.inner_dimensions_with_spacing(0.0, self.menu_height + spacing.y + border);
        let _ = self.window.request_inner_size(inner_size);
        self.window.set_min_inner_size(Some(min_inner_size));
    }
}

impl Nes {
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
            .push((text, Instant::now() + MSG_TIMEOUT));
    }

    pub fn on_error(&mut self, err: NesError) {
        self.pause(true);
        log::error!("{err:?}");
        self.renderer.gui.state.error = Some(err.to_string());
    }
}
