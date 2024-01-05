//! User Interface representing the the NES Control Deck

use crate::{
    audio::Mixer,
    common::Regional,
    control_deck::ControlDeck,
    frame_begin,
    nes::{
        config::{FRAME_TRIM_PITCH, WINDOW_TITLE},
        event::CustomEvent,
        state::{Mode, PauseMode, Replay},
    },
    ppu::Ppu,
    profile,
    video::Video,
    NesResult,
};
use config::Config;
use crossbeam::channel::{self, Receiver, Sender};
use pixels::{
    wgpu::{PowerPreference, PresentMode, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc, thread};
use web_time::Instant;
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{DeviceId, Event, Modifiers, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopWindowTarget},
    window::{Fullscreen, Icon, Window, WindowBuilder, WindowId},
};

pub mod config;
pub mod event;

// pub(crate) mod apu_viewer;
// pub(crate) mod debug;
pub(crate) mod filesystem;
pub mod menu;
// pub mod ppu_viewer;
pub mod state;

#[derive(Debug, Copy, Clone)]
#[must_use]
pub enum EventMsg {
    #[cfg(target_arch = "wasm32")]
    LoadRom,
    #[cfg(target_arch = "wasm32")]
    Pause,
    Terminate,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
#[must_use]
pub enum RenderMsg {
    NewFrame(Vec<u8>),
    SetVsync(bool),
    Resize(u32, u32),
    Pause(bool),
    Terminate,
}

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    window: Arc<Window>,
    control_deck: ControlDeck,
    audio: Mixer,
    frame_buffer: Vec<u8>,
    controllers: [Option<DeviceId>; 4],
    // debugger: Option<Debugger>,
    // ppu_viewer: Option<PpuViewer>,
    // apu_viewer: Option<ApuViewer>,
    config: Config,
    mode: Mode,
    modifiers: Modifiers,
    rewind_frame: u32,
    rewind_buffer: VecDeque<Vec<u8>>,
    replay: Replay,
    messages: Vec<(String, Instant)>,
    paths: Vec<PathBuf>,
    selected_path: usize,
    error: Option<String>,
    quitting: bool,
    prev_frame_time: Instant,
    speed_counter: f32,
    previously_focused: bool,
    event_rx: Receiver<EventMsg>,
    render_tx: Option<Sender<RenderMsg>>,
}

impl Nes {
    pub async fn new(
        window: Window,
        config: Config,
        event_tx: Sender<EventMsg>,
        event_rx: Receiver<EventMsg>,
    ) -> NesResult<Self> {
        let mut control_deck = ControlDeck::new(config.ram_state);
        control_deck.set_region(config.region);
        control_deck.set_filter(config.filter);
        control_deck.set_four_player(config.four_player);
        control_deck.connect_zapper(config.zapper);

        let window = Arc::new(window);
        let audio = Mixer::new(
            control_deck.sample_rate(),
            config.audio_sample_rate / config.speed,
            config.audio_latency,
            config.audio_enabled,
        );
        let mode = if config.debug {
            Mode::Paused(PauseMode::Manual)
        } else {
            Mode::default()
        };

        let mut nes = Self {
            window,
            control_deck,
            audio,
            frame_buffer: Video::new_frame_buffer(),
            controllers: [None; 4],
            // players: HashMap::new(),
            // emulation: None,
            // debugger: None,
            // ppu_viewer: None,
            // apu_viewer: None,
            config,
            mode,
            modifiers: Modifiers::default(),
            rewind_frame: 0,
            rewind_buffer: VecDeque::new(),
            replay: Replay::default(),
            messages: vec![],
            paths: vec![],
            selected_path: 0,
            error: None,
            quitting: false,
            // Keep track of last frame time so we can predict audio sync requirements for the next
            // frame.
            prev_frame_time: Instant::now(),
            // Keep a speed counter to accumulate partial frames for non-integer speed changes like
            // 1.5x.
            speed_counter: 0.0,
            // NOTE: Only pause in the background if the app has received focus at least once by the
            // user. Winit sometimes sends a Focused(false) on initial startup before Focused(true).
            previously_focused: false,
            event_rx,
            render_tx: None,
        };

        nes.initialize(event_tx);
        if nes.config.debug {
            // TODO: debugger
            // nes.toggle_debugger(s)?;
        }

        Ok(nes)
    }

    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub async fn run(config: Config) -> NesResult<()> {
        puffin::set_scopes_on(config.debug);

        // Set up window, events and NES state
        let event_loop = EventLoopBuilder::<CustomEvent>::with_user_event().build()?;
        let window = Self::initialize_window(&event_loop, &config)?;
        let mut renderer = Self::initialize_renderer(&window, &config).await?;

        let (event_tx, event_rx) = channel::bounded::<EventMsg>(8);
        let mut nes = if thread::available_parallelism().map_or(false, |count| count.get() > 1) {
            let (render_tx, render_rx) = channel::bounded::<RenderMsg>(8);
            thread::spawn({
                let target_frame_time = config.target_frame_time;
                let event_tx = event_tx.clone();
                move || {
                    let mut is_paused = false;
                    loop {
                        profile!("render main");

                        while let Ok(msg) = render_rx.try_recv() {
                            match msg {
                                RenderMsg::NewFrame(frame_buffer) => {
                                    if let Err(err) =
                                        Self::render_frame(&mut renderer, &frame_buffer)
                                    {
                                        log::error!("error rending frame: {err:?}");
                                        if let Err(err) = event_tx.try_send(EventMsg::Terminate) {
                                            log::error!("failed to send terminate message to event_loop: {err:?}");
                                        }
                                        break;
                                    }
                                }
                                RenderMsg::SetVsync(_enabled) => {
                                    // TODO: feature not released yet: https://github.com/parasyte/pixels/pull/373
                                    // pixels.enable_vsync(enabled),
                                }
                                RenderMsg::Resize(width, height) => {
                                    if let Err(err) = renderer.resize_surface(width, height) {
                                        log::error!("failed to resize render surface: {err:?}");
                                    }
                                }
                                RenderMsg::Pause(paused) => is_paused = paused,
                                RenderMsg::Terminate => break,
                            }
                        }

                        if is_paused {
                            thread::sleep(target_frame_time);
                        }
                    }
                }
            });
            let mut nes = Self::new(window, config, event_tx, event_rx).await?;
            nes.render_tx = Some(render_tx);
            nes
        } else {
            Self::new(window, config, event_tx, event_rx).await?
        };

        // Start event loop
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::EventLoopExtWebSys;
            event_loop.spawn(move |event, window_target| nes.main_loop(event, window_target));
        }
        #[cfg(not(target_arch = "wasm32"))]
        event_loop.run(move |event, window_target| nes.main_loop(event, window_target))?;

        Ok(())
    }

    pub fn initialize_window(
        event_loop: &EventLoop<CustomEvent>,
        config: &Config,
    ) -> NesResult<Window> {
        let (width, height) = config.get_dimensions();
        let window_builder = WindowBuilder::new();
        let window_builder = window_builder
            .with_active(true)
            .with_inner_size(LogicalSize::new(width, height))
            .with_title(WINDOW_TITLE)
            // TODO: fullscreen mode based on config
            .with_fullscreen(config.fullscreen.then_some(Fullscreen::Borderless(None)))
            .with_resizable(false)
            .with_window_icon(Self::window_icon());

        #[cfg(target_arch = "wasm32")]
        let window_builder = {
            use winit::platform::web::WindowBuilderExtWebSys;
            // TODO: insert into specific section in the DOM
            window_builder.with_append(true)
        };

        Ok(window_builder.build(event_loop)?)
    }

    pub async fn initialize_renderer(window: &Window, config: &Config) -> NesResult<Pixels> {
        let mut window_size = window.inner_size();
        if window_size.width == 0 {
            let (width, height) = config.get_dimensions();
            window_size = PhysicalSize::new(width, height);
        }
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Ok(
            PixelsBuilder::new(Ppu::WIDTH, Ppu::HEIGHT - 16, surface_texture)
                .request_adapter_options(RequestAdapterOptions {
                    power_preference: PowerPreference::HighPerformance,
                    ..Default::default()
                })
                .present_mode(if config.vsync {
                    PresentMode::Mailbox
                } else {
                    PresentMode::AutoNoVsync
                })
                .build_async()
                .await?,
        )
    }

    pub fn main_loop(
        &mut self,
        event: Event<CustomEvent>,
        window_target: &EventLoopWindowTarget<CustomEvent>,
    ) {
        frame_begin!();
        profile!("event loop");

        while let Ok(msg) = self.event_rx.try_recv() {
            self.handle_event_message(msg);
        }

        if self.quitting {
            self.quitting = true;
            window_target.exit();
            return;
        }

        match event {
            Event::WindowEvent {
                window_id, event, ..
            } => match event {
                WindowEvent::CloseRequested => self.handle_close_requested(window_id),
                WindowEvent::RedrawRequested => self.handle_redraw(window_target),
                WindowEvent::Focused(focused) => {
                    if self.previously_focused {
                        self.pause_in_bg(!focused);
                    } else if focused {
                        self.previously_focused = true;
                    }
                }
                WindowEvent::Occluded(occluded) => self.pause_in_bg(occluded),
                WindowEvent::KeyboardInput { event, .. } => {
                    self.handle_key_event(window_id, event);
                }
                WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
                WindowEvent::MouseInput { button, state, .. } => {
                    self.handle_mouse_event(window_id, button, state);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.handle_mouse_motion(window_id, position);
                }
                WindowEvent::DroppedFile(_path) => {
                    // TODO: load rom
                }
                _ => {}
            },
            Event::AboutToWait => self.handle_update(window_target),
            // TODO: Controller support
            // Event::UserEvent(event) => match event {
            //     CustomEvent::ControllerAxisMotion {
            //         device_id,
            //         axis,
            //         value,
            //         ..
            //     } => {
            //         self.handle_controller_axis_motion(device_id, axis, value);
            //     }
            //     CustomEvent::ControllerInput {
            //         device_id,
            //         button,
            //         state,
            //         ..
            //     } => {
            //         self.handle_controller_event(device_id, button, state);
            //     }
            //     CustomEvent::ControllerUpdate {
            //         device_id, update, ..
            //     } => {
            //         self.handle_controller_update(device_id, button, state);
            //     }
            // },
            Event::LoopExiting => self.handle_exit(),
            _ => {}
        }
    }

    fn handle_close_requested(&mut self, window_id: WindowId) {
        if window_id == self.window.id() {
            log::info!("quitting...");
            puffin::set_scopes_on(false);
            self.audio.pause();
            self.quitting = true;
        }
        // TODO: check debugger windows
    }

    fn handle_update(&mut self, window_target: &EventLoopWindowTarget<CustomEvent>) {
        let now = Instant::now();
        let last_frame_time = now - self.prev_frame_time;
        self.prev_frame_time = now;
        // log::debug!("last frame: {:.4}ms", 1000.0 * last_frame.as_secs_f32());

        if self.replay.mode.is_playback() {
            self.replay_action();
        }

        if self.mode.is_playing() {
            // Frames that aren't multiples of the default render 1 more/less frames
            // every other frame
            self.speed_counter += self.config.speed;
            let mut speed_multiplier = 0;
            while self.speed_counter > 0.0 {
                self.speed_counter -= 1.0;
                speed_multiplier += 1;
            }

            let queued_audio_time = self.audio.queued_time();
            // log::debug!(
            //     "{:.0}, queued_ms: {:.4}, last_frame: {:.4}",
            //     web_time::SystemTime::now()
            //         .duration_since(web_time::UNIX_EPOCH)
            //         .expect("valid unix time")
            //         .as_secs_f32()
            //         * 1000.0,
            //     queued_audio_time.as_secs_f32() * 1000.0,
            //     last_frame_time.as_secs_f32() * 1000.0,
            // );
            if queued_audio_time
                < self.config.target_frame_time.max(last_frame_time) + self.config.audio_latency
            {
                profile!("frame update");

                match self.control_deck.clock_frame() {
                    // TODO: ppu viewer
                    // if let Some(ref mut viewer) = self.ppu_viewer {
                    //     if cpu.ppu().cycle() <= 3 && cpu.ppu().scanline() == viewer.scanline() {
                    //         viewer.load_nametables(cpu.ppu());
                    //         viewer.load_pattern_tables(cpu.ppu());
                    //         viewer.load_palettes(cpu.ppu());
                    //     }
                    // }
                    Ok(_) => {
                        self.update_rewind();
                        if let Err(err) = self.audio.process(self.control_deck.audio_samples()) {
                            log::error!("failed to process audio: {err:?}");
                        }
                        self.control_deck.clear_audio_samples();

                        self.control_deck.frame_buffer(&mut self.frame_buffer);
                        self.window.request_redraw();
                    }
                    Err(err) => {
                        self.handle_emulation_error(&err);
                    }
                }
            } else {
                window_target.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + queued_audio_time - self.config.audio_latency,
                ));
            }
        }
    }

    fn handle_redraw(&mut self, window_target: &EventLoopWindowTarget<CustomEvent>) {
        if let Some(ref mut render_tx) = self.render_tx {
            render_tx.try_send(RenderMsg::NewFrame(self.frame_buffer.to_vec()));
        }
        // if let Err(err) = Self.render_frame() {
        //     log::error!("error rending frame: {err:?}");
        //     window_target.exit();
        // }
    }

    fn render_frame(renderer: &mut Pixels, frame_buffer: &[u8]) -> NesResult<()> {
        profile!();

        // Copy NES frame buffer
        let frame_buffer_len = frame_buffer.len();
        renderer
            .frame_mut()
            .copy_from_slice(&frame_buffer[FRAME_TRIM_PITCH..frame_buffer_len - FRAME_TRIM_PITCH]);

        // TODO: Render framerate
        // TODO: Draw zapper crosshair
        // if self.config.zapper {
        //     s.set_texture_target(texture_id)?;
        //     let (x, y) = self.control_deck.zapper_pos();
        //     s.stroke(Color::GRAY);
        //     s.line([x - 8, y, x + 8, y])?;
        //     s.line([x, y - 8, x, y + 8])?;
        //     s.clear_texture_target();
        // }
        // TODO: Render menus
        // TODO: Render debug windows
        // self.render_debugger(s)?;
        // self.render_ppu_viewer(s)?;
        //         match self.mode {
        //             Mode::Paused | Mode::PausedBg => {
        //                 if self.confirm_quit.is_some() {
        //                     if self.render_confirm_quit(s)? {
        //                         s.quit();
        //                     }
        //                 } else {
        //                     self.render_status(s, "Paused")?;
        //                 }
        //             }
        //             Mode::InMenu(menu) => self.render_menu(s, menu)?,
        //             Mode::Rewinding => {
        //                 self.render_status(s, "Rewinding")?;
        //                 self.rewind();
        //             }
        //             Mode::Playing => match self.replay.mode {
        //                 ReplayMode::Recording => self.render_status(s, "Recording Replay")?,
        //                 ReplayMode::Playback => self.render_status(s, "Replay Playback")?,
        //                 ReplayMode::Off => (),
        //             },
        //         }
        //         if (self.config.speed - 1.0).abs() > f32::EPSILON {
        //             self.render_status(s, &format!("Speed {:.2}", self.config.speed))?;
        //         }
        //         self.render_messages(s)?;

        profile!("video sync");
        Ok(renderer.render()?)
    }

    fn handle_exit(&mut self) {
        log::info!("exiting...");
        if self.control_deck.loaded_rom().is_some() {
            use crate::nes::state::ReplayMode;
            if let Err(err) = self.save_sram() {
                log::error!("failed to save sram: {err:?}");
            }
            if self.replay.mode == ReplayMode::Recording {
                self.stop_replay();
            }
            if self.config.save_on_exit {
                self.save_state(1);
            }
        }

        // TOOD: Only save when config changes
        #[cfg(not(debug_assertions))]
        self.save_config();
    }

    // NOTE: in wasm, when true, this causes the game to pause when the canvas is unfocused, not
    // the browser, requestAnimationFrame automatically handles pausing the game loop when not
    // focused.
    pub fn pause_in_bg(&mut self, pause: bool) {
        if cfg!(not(target_arch = "wasm32")) {
            if pause {
                if self.mode.is_playing() && self.config.pause_in_bg {
                    self.pause_play(PauseMode::Unfocused);
                }
            } else if self.mode.is_paused_unfocused() {
                self.resume_play();
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_event_message(&mut self, msg: EventMsg) {
        match msg {
            EventMsg::Terminate => self.quitting = true,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_event_message(&mut self, msg: EventMsg) {
        match msg {
            EventMsg::LoadRom => {
                const TEST_ROM: &[u8] = include_bytes!("../roms/akumajou_densetsu.nes");

                // TODO: focus canvas
                if !self.control_deck.is_running() {
                    self.load_rom("akumajou_densetsu.nes", &mut std::io::Cursor::new(TEST_ROM));
                }
            }
            EventMsg::Pause => self.pause_play(PauseMode::Manual),
            EventMsg::Terminate => self.quitting = true,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn window_icon() -> Option<Icon> {
        use anyhow::Context;
        use image::{io::Reader as ImageReader, ImageFormat};
        use std::io::Cursor;

        const WINDOW_ICON: &[u8] = include_bytes!("../assets/tetanes_icon.png");

        // TODO: file PR to winit to support macos - SDL supports this.
        // May be able to work around it with a macos app bundle.
        match ImageReader::with_format(Cursor::new(WINDOW_ICON), ImageFormat::Png)
            .decode()
            .with_context(|| "failed to decode window icon")
            .and_then(|png| {
                let width = png.width();
                let height = png.height();
                Icon::from_rgba(png.into_rgba8().into_vec(), width, height)
                    .with_context(|| "failed to create window icon")
            }) {
            Ok(icon) => Some(icon),
            Err(ref err) => {
                log::error!("{err:?}");
                None
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    const fn window_icon() -> Option<Icon> {
        None
    }
}

// #[cfg(not(target_arch = "wasm32"))]
// fn render_main(
//     mut pixels: Pixels,
//     frame_time: web_time::Duration,
//     render_main_rx: channel::Receiver<RenderMainMsg>,
//     event_loop_tx: channel::Sender<EventMsg>,
// ) {
//     use std::thread;
// }
