//! User Interfa    ce representing the the NES Control Deck

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
use crossbeam::channel;
use pixels::{
    wgpu::{PowerPreference, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{collections::VecDeque, path::PathBuf, rc::Rc};
use web_time::Instant;
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{DeviceId, Event, Modifiers, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Fullscreen, Icon, Window, WindowBuilder},
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
pub enum EventLoopMsg {
    #[cfg(target_arch = "wasm32")]
    LoadRom,
    #[cfg(target_arch = "wasm32")]
    Pause,
    Terminate,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
#[must_use]
pub enum RenderMainMsg {
    NewFrame(Vec<u8>),
    SetVsync(bool),
    Resize(u32, u32),
    Pause(bool),
    Terminate,
}

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    control_deck: ControlDeck,
    audio: Mixer,
    window: Rc<Window>,
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
    event_loop_rx: channel::Receiver<EventLoopMsg>,
    #[cfg(not(target_arch = "wasm32"))]
    render_main_tx: channel::Sender<RenderMainMsg>,
}

impl Nes {
    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub async fn run(config: Config) -> NesResult<()> {
        puffin::set_scopes_on(config.debug);

        // Set up windowing, rendering and events
        let event_loop = EventLoopBuilder::<CustomEvent>::with_user_event().build()?;

        let (width, height) = config.get_dimensions();
        let window_builder = WindowBuilder::new();
        let window_builder = window_builder
            .with_active(true)
            .with_inner_size(LogicalSize::new(width, height))
            // .with_min_inner_size(LogicalSize::new(width, height))
            // .with_max_inner_size(LogicalSize::new(width, height))
            .with_title(WINDOW_TITLE)
            .with_fullscreen(config.fullscreen.then_some(Fullscreen::Borderless(None)))
            .with_resizable(false)
            .with_window_icon(Self::window_icon());
        #[cfg(target_arch = "wasm32")]
        let window_builder = {
            use winit::platform::web::WindowBuilderExtWebSys;
            window_builder.with_append(true)
        };
        let window = window_builder.build(&event_loop).expect("window");

        let pixels = {
            let mut window_size = window.inner_size();
            if window_size.width == 0 {
                window_size = PhysicalSize::new(width, height);
            }
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, &window);
            PixelsBuilder::new(Ppu::WIDTH, Ppu::HEIGHT - 16, surface_texture)
                .request_adapter_options(RequestAdapterOptions {
                    power_preference: PowerPreference::HighPerformance,
                    ..Default::default()
                })
                .enable_vsync(config.vsync)
                .build_async()
                .await?
        };
        #[cfg(target_arch = "wasm32")]
        let mut pixels = pixels;

        let mut frame_buffer = Video::new_frame_buffer();

        let mut control_deck = ControlDeck::new(config.ram_state);
        control_deck.set_region(config.region);
        control_deck.set_filter(config.filter);
        control_deck.set_four_player(config.four_player);
        control_deck.connect_zapper(config.zapper);
        let audio = Mixer::new(
            control_deck.sample_rate(),
            config.audio_sample_rate / config.speed,
            config.audio_buffer_size,
        );
        let mode = if config.debug {
            Mode::Paused(PauseMode::Manual)
        } else {
            Mode::default()
        };

        let window = Rc::new(window);
        let (event_loop_tx, event_loop_rx) = channel::bounded::<EventLoopMsg>(16);
        #[cfg(not(target_arch = "wasm32"))]
        let (render_main_tx, render_main_rx) = channel::bounded::<RenderMainMsg>(16);
        let mut nes = Self {
            control_deck,
            audio,
            window: Rc::clone(&window),
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
            event_loop_rx,
            #[cfg(not(target_arch = "wasm32"))]
            render_main_tx,
        };

        // Spawn render thread
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            render_main(pixels, nes.config.frame_time, render_main_rx, event_loop_tx);
        });

        nes.initialize(
            #[cfg(target_arch = "wasm32")]
            event_loop_tx,
        );

        if nes.config.debug {
            // nes.config.pause_in_bg = false;
            // TODO: debugger
            // nes.toggle_debugger(s)?;
        }

        // Keep track of last frame time so we can predict audio sync requirements for the next
        // frame.
        let mut prev_frame_time = Instant::now();
        // Keep a speed counter to accumulate partial frames for non-integer speed changes like
        // 1.5x.
        let mut speed_counter = 0.0;
        // NOTE: Only pause in the background if the app has received focus at least once by the
        // user. Winit sometimes sends a Focused(false) on initial startup before Focused(true).
        let mut previously_focused = false;

        // Start emulation loop
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run(move |event, window_target| {
            frame_begin!();
            profile!("event loop");

            while let Ok(msg) = nes.event_loop_rx.try_recv() {
                nes.handle_event_loop_messages(msg);
            }

            if nes.quitting {
                nes.quitting = true;
                window_target.exit();
                return;
            }

            match event {
                Event::WindowEvent {
                    window_id, event, ..
                } => match event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        if window_id == window.id() {
                            log::info!("quitting...");
                            puffin::set_scopes_on(false);
                            nes.audio.pause();
                            nes.quitting = true;
                            window_target.exit();
                        }
                        // TODO: check debugger windows
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    WindowEvent::Resized(window_size) => {
                        if let Err(err) = nes
                            .render_main_tx
                            .send(RenderMainMsg::Resize(window_size.width, window_size.height))
                        {
                            log::error!("failed to send resize message to render_main: {err:?}");
                        }
                    }
                    WindowEvent::Focused(focused) => {
                        if previously_focused {
                            nes.pause_in_bg(!focused);
                        } else if focused {
                            previously_focused = true;
                        }
                    }
                    WindowEvent::Occluded(occluded) => nes.pause_in_bg(occluded),
                    WindowEvent::KeyboardInput { event, .. } => {
                        nes.handle_key_event(event);
                    }
                    WindowEvent::ModifiersChanged(modifiers) => nes.modifiers = modifiers,
                    WindowEvent::MouseInput { button, state, .. } => {
                        nes.handle_mouse_event(button, state);
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        nes.handle_mouse_motion(position);
                    }
                    _ => (),
                },
                // TODO: Controller support
                // Event::DeviceEvent { device_id, event } => {
                //     if matches!(
                //         event,
                //         DeviceEvent::Added | DeviceEvent::Removed | DeviceEvent::Button { .. }
                //     ) {
                //         log::debug!("device event: {device_id:?}, {event:?}");
                //     }
                // }
                Event::AboutToWait => {
                    let now = Instant::now();
                    // log::info!("DEBUG {now:.0}, about to wait");

                    let last_frame_ms = now - prev_frame_time;
                    prev_frame_time = now;

                    if nes.replay.mode.is_playback() {
                        nes.replay_action();
                    }

                    if nes.mode.is_playing() {
                        // Frames that aren't multiples of the default render 1 more/less frames
                        // every other frame
                        speed_counter += nes.config.speed;
                        let mut speed_multiplier = 0;
                        while speed_counter > 0.0 {
                            speed_counter -= 1.0;
                            speed_multiplier += 1;
                        }
                        let audio_queued_time = nes.audio.queued_time();
                        // log::info!(
                        //     "DEBUG {:.0}, queued_ms: {audio_queued_ms:.4}, last_frame: {last_frame_ms:.4}",
                        //     Instant::now()
                        // );
                        // if nes.control_deck.frame_number() > 100 {
                        //     nes.quitting = true;
                        //     return;
                        // }

                        profile!("audio sync");
                        if audio_queued_time < nes.config.frame_time + nes.config.audio_delay_time
                            && audio_queued_time + nes.config.frame_time
                                < nes.audio.max_queued_time()
                        {
                            // let frames_to_clock = speed_multiplier
                            //     * (((last_frame_ms + nes.config.audio_delay_ms)
                            //         - nes.audio.queued_ms())
                            //         / nes.config.frame_time_ms)
                            //         .ceil() as usize;
                            // for _ in 0..frames_to_clock.min(5) {
                            profile!("frame update");

                            // log::info!("DEBUG {:.0}, clock start", Instant::now());
                            match nes.control_deck.clock_frame() {
                                // TODO: ppu viewer
                                // if let Some(ref mut viewer) = nes.ppu_viewer {
                                //     if cpu.ppu().cycle() <= 3 && cpu.ppu().scanline() == viewer.scanline() {
                                //         viewer.load_nametables(cpu.ppu());
                                //         viewer.load_pattern_tables(cpu.ppu());
                                //         viewer.load_palettes(cpu.ppu());
                                //     }
                                // }
                                Ok(_) => {
                                    // log::info!( "DEBUG {:.0}, clock end", Instant::now());
                                    nes.update_rewind();
                                    if nes.config.sound {
                                        // log::info!( "DEBUG {:.0}, audio start", Instant::now());
                                        nes.audio.process(nes.control_deck.audio_samples());
                                        // log::info!( "DEBUG {:.0}, audio end", Instant::now());
                                    }
                                    nes.control_deck.clear_audio_samples();
                                }
                                Err(err) => {
                                    nes.handle_emulation_error(&err);
                                    return;
                                }
                            }
                        }

                        nes.control_deck.frame_buffer(&mut frame_buffer);

                        #[cfg(target_arch = "wasm32")]
                        if let Err(err) = render_frame(&mut pixels, &frame_buffer) {
                            log::error!("error rending frame: {err:?}");
                            window_target.exit();
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            profile!("frame_buffer send");
                            if let Err(err) = nes
                                .render_main_tx
                                .send(RenderMainMsg::NewFrame(frame_buffer.clone()))
                            {
                                log::error!("frame queue error: {err}");
                                window_target.exit();
                            }
                        }
                    } else {
                        #[cfg(not(target_arch = "wasm32"))]
                        std::thread::sleep(nes.config.frame_time);
                    }
                }
                // Event::UserEvent(event) => match event {
                //     CustomEvent::ControllerAxisMotion {
                //         device_id,
                //         axis,
                //         value,
                //         ..
                //     } => {
                //         nes.handle_controller_axis_motion(device_id, axis, value);
                //     }
                //     CustomEvent::ControllerInput {
                //         device_id,
                //         button,
                //         state,
                //         ..
                //     } => {
                //         nes.handle_controller_event(device_id, button, state);
                //     }
                //     CustomEvent::ControllerUpdate {
                //         device_id, update, ..
                //     } => {
                //         nes.handle_controller_update(device_id, button, state);
                //     }
                // },
                Event::LoopExiting => {
                    log::info!("exiting...");
                    if nes.control_deck.loaded_rom().is_some() {
                        use crate::nes::state::ReplayMode;
                        if let Err(err) = nes.save_sram() {
                            log::error!("failed to save sram: {err:?}");
                        }
                        if nes.replay.mode == ReplayMode::Recording {
                            nes.stop_replay();
                        }
                        if nes.config.save_on_exit {
                            nes.save_state(1);
                        }
                    }
                    nes.save_config();
                }
                _ => (),
            }
        })?;

        Ok(())
    }

    // NOTE: in wasm, when true, this causes the game to pause when the canvas is unfocused, not
    // the browser, requestAnimationFrame automatically handles pausing the game loop when not
    // focused.
    #[cfg(target_arch = "wasm32")]
    fn pause_in_bg(&mut self, _pause: bool) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn pause_in_bg(&mut self, pause: bool) {
        if pause {
            if self.mode.is_playing() && self.config.pause_in_bg {
                self.pause_play(PauseMode::Unfocused);
            }
        } else if self.mode.is_paused_unfocused() {
            self.resume_play();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_event_loop_messages(&mut self, msg: EventLoopMsg) {
        match msg {
            EventLoopMsg::Terminate => self.quitting = true,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn handle_event_loop_messages(&mut self, msg: EventLoopMsg) {
        const TEST_ROM: &[u8] = include_bytes!("../roms/akumajou_densetsu.nes");

        match msg {
            EventLoopMsg::LoadRom => {
                // TODO: focus canvas
                if !self.control_deck.is_running() {
                    self.load_rom("akumajou_densetsu.nes", &mut std::io::Cursor::new(TEST_ROM));
                }
            }
            EventLoopMsg::Pause => {
                self.pause_play(PauseMode::Manual);
            }
            EventLoopMsg::Terminate => self.quitting = true,
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

#[cfg(not(target_arch = "wasm32"))]
fn render_main(
    mut pixels: Pixels,
    frame_time: web_time::Duration,
    render_main_rx: channel::Receiver<RenderMainMsg>,
    event_loop_tx: channel::Sender<EventLoopMsg>,
) {
    use std::thread;

    let mut is_paused = false;
    loop {
        profile!("render main");

        while let Ok(msg) = render_main_rx.try_recv() {
            match msg {
                RenderMainMsg::NewFrame(frame_buffer) => {
                    if let Err(err) = render_frame(&mut pixels, &frame_buffer) {
                        log::error!("error rending frame: {err:?}");
                        if let Err(err) = event_loop_tx.send(EventLoopMsg::Terminate) {
                            log::error!("failed to send terminate message to event_loop: {err:?}");
                        }
                        break;
                    }
                }
                RenderMainMsg::SetVsync(_enabled) => {
                    // TODO: feature not released yet: https://github.com/parasyte/pixels/pull/373
                    // pixels.enable_vsync(enabled),
                }
                RenderMainMsg::Resize(width, height) => {
                    if let Err(err) = pixels.resize_surface(width, height) {
                        log::error!("failed to resize render surface: {err:?}");
                    }
                }
                RenderMainMsg::Pause(paused) => is_paused = paused,
                RenderMainMsg::Terminate => break,
            }
        }

        if is_paused {
            thread::sleep(frame_time);
        }
    }
}

fn render_frame(pixels: &mut Pixels, frame_buffer: &[u8]) -> NesResult<()> {
    profile!();

    // Copy NES frame buffer
    let frame_buffer_len = frame_buffer.len();
    pixels
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
    Ok(pixels.render()?)
}
