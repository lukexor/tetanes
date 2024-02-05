//! User Interface representing the the NES Control Deck

use crate::{
    audio::Mixer,
    control_deck::ControlDeck,
    frame_begin,
    nes::{
        config::WINDOW_TITLE,
        event::CustomEvent,
        renderer::Renderer,
        state::{Mode, PauseMode, Replay},
    },
    platform::{EventLoopExt, WindowBuilderExt},
    profile, NesResult,
};
use config::Config;
use crossbeam::channel::{self, Receiver};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use web_time::Instant;
use winit::{
    dpi::LogicalSize,
    event::{DeviceId, Event, Modifiers, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopWindowTarget},
    window::{Fullscreen, Icon, Window, WindowBuilder, WindowId},
};

pub mod config;
pub mod event;
pub mod renderer;
// pub mod apu_viewer;
// pub mod debug;
pub mod filesystem;
pub mod menu;
// pub mod ppu_viewer;
pub mod state;

#[derive(Debug, Copy, Clone)]
#[must_use]
pub enum Message {
    LoadRom,
    Pause,
    Terminate,
}

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    window: Arc<Window>,
    control_deck: ControlDeck,
    audio: Mixer,
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
    last_frame_time: Instant,
    speed_counter: f32,
    previously_focused: bool,
    rx: Receiver<Message>,
    renderer: Renderer,
}

impl Nes {
    async fn initialize(config: Config, event_loop: &EventLoop<CustomEvent>) -> NesResult<Self> {
        let (tx, rx) = channel::bounded::<Message>(64);
        let window = Nes::initialize_window(event_loop, &config)?;
        let renderer = Renderer::initialize(&window, &config, tx.clone()).await?;
        let control_deck = ControlDeck::with_config(config.clone().into());

        let audio = Mixer::new(
            control_deck.clock_rate(),
            config.audio_sample_rate / config.speed,
            config.audio_latency,
            config.audio_enabled,
        );
        let mode = if config.debug {
            Mode::Paused(PauseMode::Manual)
        } else {
            Mode::default()
        };

        let nes = Self {
            window: Arc::new(window),
            control_deck,
            audio,
            controllers: [None; 4],
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
            last_frame_time: Instant::now(),
            // Keep a speed counter to accumulate partial frames for non-integer speed changes like
            // 1.5x.
            speed_counter: 0.0,
            // NOTE: Only pause in the background if the app has received focus at least once by the
            // user. Winit sometimes sends a Focused(false) on initial startup before Focused(true).
            previously_focused: false,
            rx,
            renderer,
        };

        if nes.config.debug {
            // TODO: debugger
            // nes.toggle_debugger(s)?;
        }

        #[cfg(not(target_arch = "wasm32"))]
        let mut nes = nes;
        #[cfg(not(target_arch = "wasm32"))]
        if nes.config.rom_path.is_dir() {
            nes.mode = Mode::InMenu(menu::Menu::LoadRom);
        } else {
            nes.load_rom_path(nes.config.rom_path.clone());
        }

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::{closure::Closure, JsCast};

            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body().map(|body| (doc, body)))
                .map(|(doc, body)| {
                    let handle_load_rom = Closure::<dyn Fn()>::new({
                        let tx = tx.clone();
                        move || {
                            if let Err(err) = tx.try_send(Message::LoadRom) {
                                log::error!(
                                    "failed to send load rom message to event_loop: {err:?}"
                                );
                            }
                        }
                    });

                    let load_rom_btn = doc.create_element("button").expect("created button");
                    load_rom_btn.set_text_content(Some("Load ROM"));
                    load_rom_btn
                        .add_event_listener_with_callback(
                            "click",
                            handle_load_rom.as_ref().unchecked_ref(),
                        )
                        .expect("added event listener");
                    body.append_child(&load_rom_btn).ok();
                    handle_load_rom.forget();

                    let handle_pause = Closure::<dyn Fn()>::new({
                        let tx = tx.clone();
                        move || {
                            if let Err(err) = tx.try_send(Message::Pause) {
                                log::error!("failed to send pause message to event_loop: {err:?}");
                            }
                        }
                    });

                    let pause_btn = doc.create_element("button").expect("created button");
                    pause_btn.set_text_content(Some("Pause"));
                    pause_btn
                        .add_event_listener_with_callback(
                            "click",
                            handle_pause.as_ref().unchecked_ref(),
                        )
                        .expect("added event listener");
                    body.append_child(&pause_btn).ok();
                    handle_pause.forget();
                })
                .expect("couldn't append canvas to document body");
        }
        Ok(nes)
    }

    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub async fn run(config: Config) -> NesResult<()> {
        #[cfg(feature = "profiling")]
        crate::profiling::enable();

        // Set up window, events and NES state
        let event_loop = EventLoopBuilder::<CustomEvent>::with_user_event().build()?;
        let mut nes = Nes::initialize(config, &event_loop).await?;
        event_loop.run_platform(move |event, window_target| nes.main_loop(event, window_target))?;

        Ok(())
    }

    /// Initializes the window in a platform agnostic way.
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
            .with_window_icon(Self::window_icon())
            .with_platform();
        let window = window_builder.build(event_loop)?;

        if config.zapper {
            window.set_cursor_visible(false);
        }

        Ok(window)
    }

    pub fn main_loop(
        &mut self,
        event: Event<CustomEvent>,
        window_target: &EventLoopWindowTarget<CustomEvent>,
    ) {
        frame_begin!();
        profile!("event loop");

        while let Ok(msg) = self.rx.try_recv() {
            self.handle_event_message(msg);
        }

        if self.quitting {
            self.quitting = true;
            window_target.exit();
            return;
        } else if self.mode.is_paused() {
            window_target.set_control_flow(ControlFlow::Wait);
        }

        match event {
            Event::WindowEvent {
                window_id, event, ..
            } => match event {
                WindowEvent::CloseRequested => self.handle_close_requested(window_id),
                WindowEvent::RedrawRequested => {
                    self.renderer.redraw(self.control_deck.frame_buffer());
                }
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
            #[cfg(feature = "profiling")]
            crate::profiling::enable();
            self.audio.pause();
            self.quitting = true;
        }
        // TODO: check debugger windows
    }

    fn handle_update(&mut self, window_target: &EventLoopWindowTarget<CustomEvent>) {
        profile!();

        let now = Instant::now();
        let last_frame_duration = now - self.last_frame_time;
        self.last_frame_time = now;
        log::trace!(
            "last frame: {:.4}ms",
            1000.0 * last_frame_duration.as_secs_f32()
        );

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
            log::trace!(
                "queued_audio_time: {:.4}",
                1000.0 * queued_audio_time.as_secs_f32(),
            );
            if queued_audio_time
                < self.config.target_frame_duration * speed_multiplier + self.config.audio_latency
            {
                for _ in 0..speed_multiplier {
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
                            if let Err(err) = self.audio.process(self.control_deck.audio_samples())
                            {
                                log::error!("failed to process audio: {err:?}");
                            }
                            self.control_deck.clear_audio_samples();
                        }
                        Err(err) => {
                            self.handle_emulation_error(&err);
                        }
                    }
                }

                self.window.request_redraw();
            } else {
                window_target.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + queued_audio_time - self.config.audio_latency,
                ));
            }
        }
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
        self.config.save();
    }

    pub fn pause_in_bg(&mut self, pause: bool) {
        if pause {
            if self.mode.is_playing() && self.config.pause_in_bg {
                self.pause_play(PauseMode::Unfocused);
            }
        } else if self.mode.is_paused_unfocused() {
            self.resume_play();
        }
    }

    fn handle_event_message(&mut self, msg: Message) {
        match msg {
            Message::LoadRom => {
                #[cfg(target_arch = "wasm32")]
                {
                    const TEST_ROM: &[u8] = include_bytes!("../roms/akumajou_densetsu.nes");
                    // TODO: focus canvas
                    if !self.control_deck.is_running() {
                        self.load_rom("akumajou_densetsu.nes", &mut std::io::Cursor::new(TEST_ROM));
                        self.window.request_redraw();
                    }
                }
            }
            Message::Pause => self.pause_play(PauseMode::Manual),
            Message::Terminate => self.quitting = true,
        }
    }

    #[cfg(target_arch = "wasm32")]
    const fn window_icon() -> Option<Icon> {
        None
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
}
