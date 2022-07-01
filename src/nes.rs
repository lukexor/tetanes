//! User Interface representing the the NES Control Deck

use crate::{
    audio::AudioMixer,
    common::Regional,
    control_deck::ControlDeck,
    input::Slot,
    mem::RamState,
    nes::{
        apu_viewer::ApuViewer,
        debug::Debugger,
        event::{Action, Input},
        ppu_viewer::PpuViewer,
        state::{Replay, ReplayMode},
    },
    ppu::Ppu,
    NesResult,
};
use config::Config;
use menu::Menu;
use pix_engine::prelude::*;
use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    env,
    path::PathBuf,
    time::Instant,
};

pub(crate) mod apu_viewer;
pub(crate) mod config;
pub(crate) mod debug;
pub(crate) mod event;
pub(crate) mod filesystem;
pub(crate) mod menu;
pub(crate) mod ppu_viewer;
pub(crate) mod state;

const APP_NAME: &str = "TetaNES";
#[cfg(not(target_arch = "wasm32"))]
const ICON: &[u8] = include_bytes!("../assets/tetanes_icon.png");
const WINDOW_WIDTH_NTSC: f32 = Ppu::WIDTH as f32 * 8.0 / 7.0 + 0.5; // for 8:7 Aspect Ratio
const WINDOW_WIDTH_PAL: f32 = Ppu::WIDTH as f32 * 18.0 / 13.0 + 0.5; // for 18:13 Aspect Ratio
const WINDOW_HEIGHT: f32 = Ppu::HEIGHT as f32;
// Trim top and bottom 8 scanlines
pub(crate) const NES_FRAME_SRC: Rect<i32> = rect![0, 8, Ppu::WIDTH as i32, Ppu::HEIGHT as i32 - 16];

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
        self.path = path.map_or_else(|| env::current_dir().unwrap_or_default(), Into::into);
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

        let mut control_deck = ControlDeck::new(config.ram_state);
        control_deck.set_region(config.region);
        control_deck.set_filter(config.filter);
        for (&input, &action) in config.input_map.iter() {
            if action == Action::ZapperTrigger {
                if let Input::Mouse((slot, ..))
                | Input::Key((slot, ..))
                | Input::Button((slot, ..)) = input
                {
                    control_deck.connect_zapper(slot, true);
                }
            }
        }

        Ok(Nes::new(
            control_deck,
            config,
            self.replay.clone(),
            self.debug,
        ))
    }
}

impl Default for NesBuilder {
    fn default() -> Self {
        Self::new()
    }
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
        Self::InMenu(Menu::LoadRom)
    }
}

/// Represents all the NES Emulation state.
#[derive(Debug)]
pub struct Nes {
    control_deck: ControlDeck,
    audio: AudioMixer,
    players: HashMap<Slot, ControllerId>,
    emulation: Option<(WindowId, TextureId)>,
    debugger: Option<Debugger>,
    ppu_viewer: Option<PpuViewer>,
    apu_viewer: Option<ApuViewer>,
    config: Config,
    mode: Mode,
    replay_path: Option<PathBuf>,
    record_sound: bool,
    debug: bool,
    rewind_frame: u32,
    rewind_buffer: VecDeque<Vec<u8>>,
    replay: Replay,
    messages: Vec<(String, Instant)>,
    paths: Vec<PathBuf>,
    selected_path: usize,
    error: Option<String>,
    confirm_quit: Option<(String, bool)>,
}

impl Nes {
    pub(crate) fn new(
        control_deck: ControlDeck,
        config: Config,
        replay_path: Option<PathBuf>,
        debug: bool,
    ) -> Self {
        let audio = AudioMixer::new(
            control_deck.sample_rate(),
            config.audio_sample_rate / config.speed,
            config.audio_buffer_size,
        );
        Self {
            control_deck,
            audio,
            players: HashMap::new(),
            emulation: None,
            debugger: None,
            ppu_viewer: None,
            apu_viewer: None,
            config,
            mode: if debug { Mode::Paused } else { Mode::default() },
            replay_path,
            record_sound: false,
            debug,
            rewind_frame: 0,
            rewind_buffer: VecDeque::new(),
            replay: Replay::default(),
            messages: vec![],
            paths: vec![],
            selected_path: 0,
            error: None,
            confirm_quit: None,
        }
    }

    /// Begins emulation by starting the game engine loop.
    ///
    /// # Errors
    ///
    /// If engine fails to build or run, then an error is returned.
    pub fn run(&mut self) -> NesResult<()> {
        let title = APP_NAME.to_owned();
        let (width, height) = self.config.get_dimensions();
        let mut engine = PixEngine::builder();
        engine
            .with_dimensions(width, height)
            .with_title(title)
            .with_frame_rate()
            .target_frame_rate(60)
            .resizable();

        #[cfg(not(target_arch = "wasm32"))]
        {
            engine.icon(Image::from_read(ICON)?);
        }

        if self.config.fullscreen {
            engine.fullscreen();
        }
        if self.config.vsync {
            engine.vsync_enabled();
        }

        engine.build()?.run(self)
    }

    /// Update rendering textures with emulation state
    fn render_views(&mut self, s: &mut PixState) -> PixResult<()> {
        if let Some((_, texture_id)) = self.emulation {
            s.update_texture(
                texture_id,
                None,
                self.control_deck.frame_buffer(),
                4 * Ppu::WIDTH as usize,
            )?;

            for slot in [Slot::One, Slot::Two] {
                if self.control_deck.zapper_connected(slot) {
                    s.with_texture(texture_id, |s: &mut PixState| {
                        let (x, y) = self.control_deck.zapper_pos(slot);
                        s.stroke(Color::GRAY);
                        s.line([x - 8, y, x + 8, y])?;
                        s.line([x, y - 8, x, y + 8])?;
                        Ok(())
                    })?;
                }
            }
            s.texture(texture_id, NES_FRAME_SRC, None)?;
        }
        self.render_debugger(s)?;
        self.render_ppu_viewer(s)?;
        Ok(())
    }
}

impl AppState for Nes {
    fn on_start(&mut self, s: &mut PixState) -> PixResult<()> {
        self.update_frame_rate(s)?;
        if self.set_zapper_pos(s.mouse_pos()) {
            s.cursor(None)?;
        }
        self.audio.open_playback(s)?;
        self.set_scale(s, self.config.scale);
        for code in self.config.genie_codes.clone() {
            if let Err(err) = self.control_deck.add_genie_code(code.clone()) {
                log::warn!("{}", err);
                self.add_message(format!("Invalid Genie Code: '{}'", code));
                break;
            }
        }

        self.emulation = Some((
            s.window_id(),
            s.create_texture(Ppu::WIDTH, Ppu::HEIGHT, PixelFormat::Rgba)?,
        ));
        self.load_rom(s)?;

        if self.debug {
            self.toggle_debugger(s)?;
        }

        Ok(())
    }

    fn on_update(&mut self, s: &mut PixState) -> PixResult<()> {
        s.clear()?;

        if self.replay.mode == ReplayMode::Playback {
            self.replay_action(s)?;
        }

        if self.mode == Mode::Playing {
            // Clamp prevents wide swings in emulation speed and audio clipping due to jitter
            let seconds_to_run = (self.config.speed * s.delta_time().as_secs_f32())
                .clamp(0.0, self.config.speed * (1.0 / 20.0));
            let prev_frame = self.control_deck.frame_number();
            match self
                .control_deck
                .clock_seconds_inspect(seconds_to_run, |cpu| {
                    if let Some(ref mut viewer) = self.ppu_viewer {
                        if cpu.ppu().cycle() <= 3 && cpu.ppu().scanline() == viewer.scanline() {
                            viewer.load_nametables(cpu.ppu());
                            viewer.load_pattern_tables(cpu.ppu());
                            viewer.load_palettes(cpu.ppu());
                        }
                    }
                }) {
                Ok(_) => {
                    if prev_frame != self.control_deck.frame_number() {
                        self.update_rewind();
                    }
                    if self.config.sound {
                        self.audio.consume(
                            self.control_deck.audio_samples(),
                            self.config.dynamic_rate_control,
                            self.config.dynamic_rate_delta,
                        );
                    }
                    self.control_deck.clear_audio_samples();
                }
                Err(err) => return self.handle_emulation_error(s, &err),
            }
        }

        self.render_views(s)?;
        match self.mode {
            Mode::Paused | Mode::PausedBg => {
                if self.confirm_quit.is_some() {
                    if self.render_confirm_quit(s)? {
                        s.quit();
                    }
                } else {
                    self.render_status(s, "Paused")?;
                }
            }
            Mode::InMenu(menu) => self.render_menu(s, menu)?,
            Mode::Rewinding => {
                self.render_status(s, "Rewinding")?;
                self.rewind();
            }
            Mode::Playing => match self.replay.mode {
                ReplayMode::Recording => self.render_status(s, "Recording Replay")?,
                ReplayMode::Playback => self.render_status(s, "Replay Playback")?,
                ReplayMode::Off => (),
            },
        }
        if (self.config.speed - 1.0).abs() > f32::EPSILON {
            self.render_status(s, &format!("Speed {:.2}", self.config.speed))?;
        }
        self.render_messages(s)?;
        Ok(())
    }

    fn on_stop(&mut self, s: &mut PixState) -> PixResult<()> {
        if self.control_deck.loaded_rom().is_some() {
            if self.confirm_quit.is_none() {
                if let Err(err) = self.save_sram() {
                    log::error!("{}", err);
                    self.messages.clear();
                    self.confirm_quit = Some((
                        "Failed to save game state. Do you still want to quit?".to_string(),
                        false,
                    ));
                    self.pause_play();
                    s.abort_quit();
                    return Ok(());
                }
            }
            // TODO: Convert to config
            let save_on_exit = true;
            if save_on_exit {
                self.save_state(1);
            }

            if self.replay.mode == ReplayMode::Recording {
                self.stop_replay();
            }
        }
        self.save_config();
        Ok(())
    }

    fn on_key_pressed(&mut self, s: &mut PixState, event: KeyEvent) -> PixResult<bool> {
        Ok(self.handle_key_event(s, event, true))
    }

    fn on_key_released(&mut self, s: &mut PixState, event: KeyEvent) -> PixResult<bool> {
        Ok(self.handle_key_event(s, event, false))
    }

    fn on_mouse_pressed(
        &mut self,
        s: &mut PixState,
        btn: Mouse,
        _pos: Point<i32>,
    ) -> PixResult<bool> {
        Ok(self.handle_mouse_click(s, btn))
    }

    fn on_mouse_motion(
        &mut self,
        _s: &mut PixState,
        pos: Point<i32>,
        _rel_pos: Point<i32>,
    ) -> PixResult<bool> {
        Ok(self.handle_mouse_motion(pos))
    }

    fn on_controller_update(
        &mut self,
        _s: &mut PixState,
        controller_id: ControllerId,
        update: ControllerUpdate,
    ) -> PixResult<bool> {
        match update {
            ControllerUpdate::Added => {
                match self.players.entry(Slot::One) {
                    Entry::Vacant(v) => {
                        v.insert(controller_id);
                    }
                    Entry::Occupied(_) => {
                        self.players.entry(Slot::Two).or_insert(controller_id);
                    }
                }
                Ok(true)
            }
            ControllerUpdate::Removed => {
                self.players.retain(|_, &mut id| id != controller_id);
                Ok(true)
            }
            ControllerUpdate::Remapped => Ok(false),
        }
    }

    fn on_controller_pressed(
        &mut self,
        s: &mut PixState,
        event: ControllerEvent,
    ) -> PixResult<bool> {
        self.handle_controller_event(s, event, true)
    }

    fn on_controller_released(
        &mut self,
        s: &mut PixState,
        event: ControllerEvent,
    ) -> PixResult<bool> {
        self.handle_controller_event(s, event, false)
    }

    fn on_controller_axis_motion(
        &mut self,
        s: &mut PixState,
        controller_id: ControllerId,
        axis: Axis,
        value: i32,
    ) -> PixResult<bool> {
        self.handle_controller_axis(s, controller_id, axis, value)
    }

    fn on_window_event(
        &mut self,
        s: &mut PixState,
        window_id: WindowId,
        event: WindowEvent,
    ) -> PixResult<()> {
        match event {
            WindowEvent::Close => {
                if matches!(&self.emulation, Some((id, _)) if *id == window_id) {
                    s.quit();
                } else if matches!(&self.debugger, Some(ref debugger) if debugger.window_id() == window_id)
                {
                    self.debugger = None;
                } else if matches!(self.ppu_viewer, Some(ref viewer) if viewer.window_id() == window_id)
                {
                    self.ppu_viewer = None;
                } else if matches!(self.apu_viewer, Some(ref view) if view.window_id() == window_id)
                {
                    self.apu_viewer = None;
                }
            }
            WindowEvent::Hidden | WindowEvent::FocusLost => {
                if self.mode == Mode::Playing && self.config.pause_in_bg && !s.focused() {
                    self.mode = Mode::PausedBg;
                }
            }
            WindowEvent::Restored | WindowEvent::FocusGained => {
                if self.mode == Mode::PausedBg {
                    self.resume_play();
                }
            }
            _ => (),
        }
        Ok(())
    }
}
