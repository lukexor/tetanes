//! User Interface representing the the NES Control Deck

use crate::{
    audio::Mixer,
    common::Regional,
    control_deck::ControlDeck,
    frame_begin,
    mem::RamState,
    nes::{
        event::CustomEvent,
        menu::Menu,
        state::{Replay, ReplayMode},
    },
    ppu::Ppu,
    profile,
    video::Video,
    NesResult,
};
use anyhow::Context;
use config::Config;
use crossbeam::channel;
use image::{io::Reader as ImageReader, ImageFormat};
use pixels::{
    wgpu::{PowerPreference, RequestAdapterOptions},
    Pixels, PixelsBuilder, SurfaceTexture,
};
use std::{
    collections::VecDeque,
    env,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, RwLock},
    thread,
    time::Instant,
};
use winit::{
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, Event, Modifiers, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Fullscreen, Icon, WindowBuilder},
};

// pub(crate) mod apu_viewer;
pub(crate) mod config;
// pub(crate) mod debug;
pub(crate) mod event;
pub(crate) mod filesystem;
pub(crate) mod menu;
// pub(crate) mod ppu_viewer;
pub(crate) mod state;

const WINDOW_TITLE: &str = "TetaNES";
const WINDOW_ICON: &[u8] = include_bytes!("../assets/tetanes_icon.png");
const FRAME_TRIM_PITCH: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Debug, Clone)]
#[must_use]
pub struct NesBuilder {
    path: PathBuf,
    replay: Option<PathBuf>,
    fullscreen: bool,
    ram_state: Option<RamState>,
    scale: Option<f32>,
    speed: Option<f32>,
    genie_codes: Vec<String>,
    debug: bool,
}

impl NesBuilder {
    /// Creates a new `NesBuilder` instance.
    pub fn new() -> Self {
        Self {
            path: PathBuf::new(),
            replay: None,
            fullscreen: false,
            ram_state: None,
            scale: None,
            speed: None,
            genie_codes: vec![],
            debug: false,
        }
    }

    /// The initial ROM or path to search ROMs for.
    pub fn path<P>(&mut self, path: Option<P>) -> &mut Self
    where
        P: Into<PathBuf>,
    {
        self.path = path.map_or_else(
            || {
                dirs::home_dir()
                    .or_else(|| env::current_dir().ok())
                    .unwrap_or_else(|| PathBuf::from("/"))
            },
            Into::into,
        );
        self
    }

    /// A replay recording file.
    pub fn replay<P>(&mut self, path: Option<P>) -> &mut Self
    where
        P: Into<PathBuf>,
    {
        self.replay = path.map(Into::into);
        self
    }

    /// Enables fullscreen mode.
    pub fn fullscreen(&mut self, val: bool) -> &mut Self {
        self.fullscreen = val;
        self
    }

    /// Sets the default power-on state for RAM values.
    pub fn ram_state(&mut self, state: Option<RamState>) -> &mut Self {
        self.ram_state = state;
        self
    }

    /// Set the window scale.
    pub fn scale(&mut self, val: Option<f32>) -> &mut Self {
        self.scale = val;
        self
    }

    /// Set the emulation speed.
    pub fn speed(&mut self, val: Option<f32>) -> &mut Self {
        self.speed = val;
        self
    }

    /// Set the game genie codes to use on startup.
    pub fn genie_codes(&mut self, codes: Vec<String>) -> &mut Self {
        self.genie_codes = codes;
        self
    }

    pub fn debug(&mut self, debug: bool) -> &mut Self {
        self.debug = debug;
        self
    }

    /// Creates an Nes instance from an `NesBuilder`.
    ///
    /// # Errors
    ///
    /// If the default configuration directories and files can't be created, an error is returned.
    pub fn build(&self) -> NesResult<Nes> {
        let mut config = Config::load();
        config.rom_path = self.path.clone().canonicalize()?;
        config.fullscreen = self.fullscreen || config.fullscreen;
        config.ram_state = self.ram_state.unwrap_or(config.ram_state);
        config.scale = self.scale.unwrap_or(config.scale);
        config.speed = self.speed.unwrap_or(config.speed);
        config.genie_codes.append(&mut self.genie_codes.clone());
        config.debug = self.debug;

        let mut control_deck = ControlDeck::new(config.ram_state);
        control_deck.set_region(config.region);
        control_deck.set_filter(config.filter);
        control_deck.set_four_player(config.four_player);
        control_deck.connect_zapper(config.zapper);

        Ok(Nes::new(control_deck, config, self.replay.clone()))
    }
}

impl Default for NesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Copy, Clone)]
#[must_use]
enum ChannelMsg {
    Terminate,
}

/// Represents which mode the emulator is in.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(crate) enum Mode {
    Playing,
    Paused,
    PausedBg,
    InMenu(Menu),
    Rewinding,
}

impl Default for Mode {
    fn default() -> Self {
        Self::InMenu(Menu::default())
    }
}

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    control_deck: ControlDeck,
    audio: Mixer,
    controllers: [Option<DeviceId>; 4],
    // debugger: Option<Debugger>,
    // ppu_viewer: Option<PpuViewer>,
    // apu_viewer: Option<ApuViewer>,
    config: Config,
    mode: Mode,
    modifiers: Modifiers,
    replay_path: Option<PathBuf>,
    record_sound: bool,
    rewind_frame: u32,
    rewind_buffer: VecDeque<Vec<u8>>,
    replay: Replay,
    messages: Vec<(String, Instant)>,
    paths: Vec<PathBuf>,
    selected_path: usize,
    error: Option<String>,
    quitting: bool,
}

impl Nes {
    /// Create a new NES UI instance.
    pub(crate) fn new(
        control_deck: ControlDeck,
        config: Config,
        replay_path: Option<PathBuf>,
    ) -> Self {
        let audio = Mixer::new(
            control_deck.sample_rate(),
            config.audio_sample_rate / config.speed,
            config.audio_buffer_size,
            config
                .dynamic_rate_control
                .then_some(config.dynamic_rate_delta),
        );
        let mode = if config.debug {
            Mode::Paused
        } else {
            Mode::default()
        };
        Self {
            control_deck,
            audio,
            controllers: [None; 4],
            // players: HashMap::new(),
            // emulation: None,
            // debugger: None,
            // ppu_viewer: None,
            // apu_viewer: None,
            config,
            mode,
            modifiers: Modifiers::default(),
            replay_path,
            record_sound: false,
            rewind_frame: 0,
            rewind_buffer: VecDeque::new(),
            replay: Replay::default(),
            messages: vec![],
            paths: vec![],
            selected_path: 0,
            error: None,
            quitting: false,
        }
    }

    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub fn run(&mut self) -> NesResult<()> {
        puffin::set_scopes_on(self.config.debug);

        // Set up windowing, rendering and events
        let event_loop = EventLoopBuilder::<CustomEvent>::with_user_event().build()?;
        let (width, height) = self.config.get_dimensions();

        // TODO: file PR to winit to support macos - SDL supports this.
        // May be able to work around it with a macos app bundle.
        let window_icon = ImageReader::with_format(Cursor::new(WINDOW_ICON), ImageFormat::Png)
            .decode()
            .with_context(|| "failed to decode window icon")
            .and_then(|png| {
                let width = png.width();
                let height = png.height();
                Icon::from_rgba(png.into_rgba8().into_vec(), width, height)
                    .with_context(|| "failed to create window icon")
            });
        if let Err(ref err) = window_icon {
            log::error!("{err:?}");
        }
        let mut window = WindowBuilder::new()
            .with_active(true)
            .with_inner_size(LogicalSize::new(width, height))
            .with_title(WINDOW_TITLE)
            .with_fullscreen(
                self.config
                    .fullscreen
                    .then_some(Fullscreen::Borderless(None)),
            )
            .with_window_icon(window_icon.ok())
            .build(&event_loop)
            .expect("window");
        let pixels = {
            let window_size = window.inner_size();
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, &window);
            PixelsBuilder::new(Ppu::WIDTH, Ppu::HEIGHT - 16, surface_texture)
                .request_adapter_options(RequestAdapterOptions {
                    power_preference: PowerPreference::HighPerformance,
                    ..Default::default()
                })
                .enable_vsync(self.config.vsync)
                .build()?
        };

        // Force alpha to 255.
        let frame_buffer = Arc::new(RwLock::new(Video::new_frame_buffer()));

        // Spawn render thread
        let render_frame_buffer = frame_buffer.clone();
        let (new_frame_tx, new_frame_rx) = channel::unbounded();
        let (state_tx, state_rx) = channel::unbounded();
        let render_state_tx = state_tx.clone();
        let render_state_rx = state_rx.clone();
        thread::spawn(move || {
            render_main(
                pixels,
                render_frame_buffer,
                new_frame_rx,
                render_state_tx,
                render_state_rx,
            );
        });

        // Configure emulation based on config
        self.update_frame_rate()?;
        if self.config.zapper {
            window.set_cursor_visible(false);
        }
        self.set_scale(self.config.scale);
        for code in self.config.genie_codes.clone() {
            if let Err(err) = self.control_deck.add_genie_code(code.clone()) {
                log::warn!("{}", err);
                self.add_message(format!("Invalid Genie Code: '{code}'"));
                break;
            }
        }
        self.load_rom(&mut window)?;

        if self.config.debug {
            // TODO: debugger
            // self.toggle_debugger(s)?;
        }

        // Start emulation loop
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run(move |event, window_target| {
            frame_begin!();
            profile!("event loop");

            match event {
                Event::WindowEvent {
                    window_id, event, ..
                } => match event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        if window_id == window.id() {
                            puffin::set_scopes_on(false);
                            self.audio.pause();
                        }
                        // TODO: check debugger windows
                    }
                    WindowEvent::Focused(focused) => self.pause_in_bg(!focused),
                    WindowEvent::Occluded(occluded) => self.pause_in_bg(occluded),
                    WindowEvent::KeyboardInput { event, .. } => {
                        self.handle_key_event(event);
                        // TODO: Move events over to winit
                        // let joypad = self.control_deck.joypad_mut(Slot::One);
                        // joypad.set_button(JoypadBtnState::LEFT, input.key_held(KeyCode::ArrowLeft));
                        // joypad.set_button(JoypadBtnState::RIGHT, input.key_held(KeyCode::ArrowRight));
                        // joypad.set_button(JoypadBtnState::UP, input.key_held(KeyCode::ArrowUp));
                        // joypad.set_button(JoypadBtnState::DOWN, input.key_held(KeyCode::ArrowDown));
                        // joypad.set_button(JoypadBtnState::A, input.key_pressed(KeyCode::KeyZ));
                        // joypad.set_button(JoypadBtnState::B, input.key_pressed(KeyCode::KeyX));
                        // let turbo_a_pressed = input.key_pressed(KeyCode::KeyA);
                        // if turbo_a_pressed {
                        //     joypad.set_button(JoypadBtnState::TURBO_A, turbo_a_pressed);
                        //     joypad.set_button(JoypadBtnState::A, turbo_a_pressed);
                        // }
                        // let turbo_b_pressed = input.key_pressed(KeyCode::KeyS);
                        // if turbo_b_pressed {
                        //     joypad.set_button(JoypadBtnState::TURBO_B, turbo_b_pressed);
                        //     joypad.set_button(JoypadBtnState::B, turbo_b_pressed);
                        // }
                        // joypad.set_button(JoypadBtnState::START, input.key_pressed(KeyCode::Enter));
                        // joypad.set_button(
                        //     JoypadBtnState::SELECT,
                        //     input.key_pressed(KeyCode::ShiftRight),
                        // );
                    }
                    WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
                    WindowEvent::MouseInput { button, state, .. } => {
                        self.handle_mouse_event(button, state);
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        self.handle_mouse_motion(position);
                    }
                    _ => (),
                },
                // TODO: Controller support
                Event::DeviceEvent { device_id, event } => {
                    if matches!(
                        event,
                        DeviceEvent::Added | DeviceEvent::Removed | DeviceEvent::Button { .. }
                    ) {
                        log::info!("device event: {device_id:?}, {event:?}");
                    }
                }
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
                Event::AboutToWait => {
                    if self.quitting && state_rx.try_recv().is_ok() {
                        window_target.exit();
                        return;
                    }

                    if self.replay.mode == ReplayMode::Playback {
                        self.replay_action();
                    }

                    if self.mode == Mode::Playing {
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
                                if self.config.sound {
                                    self.audio.process(self.control_deck.audio_samples());
                                }
                                self.control_deck.clear_audio_samples();
                            }
                            Err(err) => {
                                self.handle_emulation_error(&err);
                                return;
                            }
                        }

                        {
                            let mut frame_buffer =
                                frame_buffer.write().expect("frame buffer write lock");
                            self.control_deck.frame_buffer(&mut frame_buffer);
                        }
                        if let Err(err) = new_frame_tx.send(true) {
                            log::error!("frame queue error: {err}");
                            window_target.exit();
                        }
                    }
                }
                Event::LoopExiting => {
                    if self.control_deck.loaded_rom().is_some() {
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
                    self.save_config();
                }
                _ => (),
            }
        })?;

        Ok(())
    }

    fn pause_in_bg(&mut self, pause: bool) {
        if pause {
            if self.mode == Mode::Playing && self.config.pause_in_bg {
                self.mode = Mode::PausedBg;
            }
        } else if self.mode == Mode::PausedBg {
            self.resume_play();
        }
    }
}

fn render_main(
    mut pixels: Pixels,
    frame_buffer: Arc<RwLock<Vec<u8>>>,
    new_frame_rx: channel::Receiver<bool>,
    state_tx: channel::Sender<ChannelMsg>,
    state_rx: channel::Receiver<ChannelMsg>,
) {
    loop {
        profile!("render thread");

        if new_frame_rx.recv().is_ok() {
            {
                let frame_buffer = frame_buffer.read().expect("frame buffer read lock");
                let frame_buffer_len = frame_buffer.len();
                pixels.frame_mut().copy_from_slice(
                    &frame_buffer[FRAME_TRIM_PITCH..frame_buffer_len - FRAME_TRIM_PITCH],
                );
            }
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

            if let Err(err) = pixels.render() {
                log::error!("error rending frame: {err:?}");
                let _ = state_tx.send(ChannelMsg::Terminate);
                break;
            }
        }
    }
}

// impl PixEngine for Nes {
//     fn on_controller_update(
//         &mut self,
//         _s: &mut PixState,
//         controller_id: ControllerId,
//         update: ControllerUpdate,
//     ) -> PixResult<bool> {
//     }

//     fn on_window_event(
//         &mut self,
//         s: &mut PixState,
//         window_id: WindowId,
//         event: WindowEvent,
//     ) -> PixResult<()> {
//         match event {
//             WindowEvent::Hidden | WindowEvent::FocusLost => {
//                 if self.mode == Mode::Playing && self.config.pause_in_bg && !s.focused() {
//                     self.mode = Mode::PausedBg;
//                 }
//             }
//             WindowEvent::Restored | WindowEvent::FocusGained => {
//                 if self.mode == Mode::PausedBg {
//                     self.resume_play();
//                 }
//             }
//             _ => (),
//         }
//         Ok(())
//     }
// }
