//! User Interface representing the the NES Game Deck

use crate::{
    bus::Bus,
    common::{home_dir, Clocked, LogLevel, Loggable, Powered, CONFIG_DIR},
    cpu::{Cpu, Irq, CPU_CLOCK_RATE},
    map_nes_err, mapper, memory, nes_err,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    serialization::{validate_save_header, write_save_header, Savable},
    ui::debug::DEBUG_WIDTH,
    NesResult,
};
use pix_engine::{
    event::PixEvent,
    pixel::{self, Pixel},
    sprite::Sprite,
    PixEngine, PixEngineResult, State, StateData,
};
use std::{
    collections::VecDeque,
    fmt, fs,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

mod debug;
mod event;
mod menus;
mod settings;

pub use settings::UiSettings;

const ICON_PATH: &str = "static/rustynes_icon.png";
const APP_NAME: &str = "RustyNES";
const WINDOW_WIDTH: u32 = (RENDER_WIDTH as f32 * 8.0 / 7.0) as u32; // for 8:7 Aspect Ratio
const WINDOW_HEIGHT: u32 = RENDER_HEIGHT;
const REWIND_SIZE: u8 = 20;
const REWIND_TIMER: f64 = 5.0;

struct Message {
    timer: f64,
    timed: bool,
    text: String,
}

impl Message {
    pub fn new(text: &str) -> Self {
        Self {
            timer: 5.0,
            timed: true,
            text: text.to_string(),
        }
    }
    pub fn new_static(text: &str) -> Self {
        Self {
            timer: 0.0,
            timed: false,
            text: text.to_string(),
        }
    }
}

pub struct Ui {
    roms: Vec<PathBuf>,
    loaded_rom: PathBuf,
    paused: bool,
    clock: f64,
    turbo_clock: u8,
    cpu: Cpu<Bus>,
    cycles_remaining: f64,
    ctrl: bool,
    shift: bool,
    focused_window: u32,
    lost_focus: bool,
    menu: bool,
    debug: bool,
    ppu_viewer: bool,
    nt_viewer: bool,
    nt_scanline: u32,
    ppu_viewer_window: Option<u32>,
    pat_scanline: u32,
    nt_viewer_window: Option<u32>,
    msg_box: Sprite,
    debug_sprite: Sprite,
    active_debug: bool,
    width: u32,
    height: u32,
    speed_counter: i32,
    rewind_timer: f64,
    rewind_slot: u8,
    rewind_save: u8,
    rewind_queue: VecDeque<u8>,
    record_frame: usize,
    recording: bool,
    playback: bool,
    record_buffer: Vec<Vec<PixEvent>>,
    messages: Vec<Message>,
    settings: UiSettings,
}

impl Ui {
    pub fn new() -> Self {
        let settings = UiSettings::default();
        Self::with_settings(settings).unwrap()
    }

    pub fn with_settings(settings: UiSettings) -> PixEngineResult<Self> {
        let scale = settings.scale;
        let width = scale * WINDOW_WIDTH;
        let height = scale * WINDOW_HEIGHT;

        unsafe { memory::RANDOMIZE_RAM = settings.randomize_ram }
        let cpu = Cpu::init(Bus::new());

        let record_buffer = if let Some(replay) = &settings.replay {
            let file = fs::File::open(replay)
                .map_err(|e| map_nes_err!("failed to open file {:?}: {}", replay.display(), e))?;
            let mut file = BufReader::new(file);
            let mut buffer: Vec<Vec<PixEvent>> = Vec::new();
            buffer.load(&mut file)?;
            buffer
        } else {
            Vec::new()
        };

        Ok(Self {
            roms: Vec::new(),
            loaded_rom: PathBuf::new(),
            paused: true,
            clock: 0.0,
            turbo_clock: 0,
            cpu,
            cycles_remaining: 0.0,
            ctrl: false,
            shift: false,
            focused_window: 0,
            lost_focus: false,
            menu: false,
            debug: false,
            ppu_viewer: false,
            nt_viewer: false,
            nt_scanline: 0,
            ppu_viewer_window: None,
            pat_scanline: 0,
            nt_viewer_window: None,
            msg_box: Sprite::new(width, height),
            debug_sprite: Sprite::new(DEBUG_WIDTH, height),
            active_debug: false,
            width,
            height,
            speed_counter: 0,
            rewind_timer: 3.0 * REWIND_TIMER,
            rewind_slot: 0,
            rewind_save: 0,
            rewind_queue: VecDeque::with_capacity(REWIND_SIZE as usize),
            record_frame: 0,
            recording: settings.record,
            playback: !record_buffer.is_empty(),
            record_buffer,
            messages: Vec::new(),
            settings,
        })
    }

    pub fn run(self) -> NesResult<()> {
        let width = self.width;
        let height = self.height;
        let vsync = self.settings.vsync;
        let mut engine = PixEngine::new(APP_NAME, self, width, height, vsync)?;
        engine.set_icon(ICON_PATH)?;
        engine.run()?;
        Ok(())
    }

    fn paused(&mut self, val: bool) {
        self.paused = val;
        if self.paused {
            self.add_static_message("Paused");
        } else {
            self.remove_static_message("Paused");
        }
    }

    fn add_message(&mut self, text: &str) {
        self.messages.push(Message::new(text));
    }

    fn add_static_message(&mut self, text: &str) {
        self.messages.push(Message::new_static(text));
    }

    fn remove_static_message(&mut self, text: &str) {
        self.messages.retain(|msg| msg.text != text);
    }

    fn draw_messages(&mut self, elapsed: f64, data: &mut StateData) -> NesResult<()> {
        self.messages.retain(|msg| !msg.timed || msg.timer > 0.0);
        if !self.messages.is_empty() {
            data.set_draw_target(&mut self.msg_box);
            let mut y = 5;
            data.set_draw_scale(2);
            for msg in self.messages.iter_mut() {
                msg.timer -= elapsed;
                data.fill_rect(0, y - 5, self.width, 25, Pixel([0, 0, 0, 200]));
                let mut x = 10;
                for s in msg.text.split_whitespace() {
                    let curr_width = s.len() as u32 * 16;
                    if x + curr_width >= self.width {
                        x = 10;
                        y += 20;
                        data.draw_string(x, y, s, pixel::RED);
                    } else {
                        data.draw_string(x, y, s, pixel::RED);
                    }
                    x += curr_width;
                    data.draw_string(x, y, " ", pixel::RED);
                    x += 16;
                }
                y += 20;
            }
            data.set_draw_scale(1);
            let pixels = self.msg_box.bytes();
            data.copy_texture(1, "message", &pixels)?;
            data.clear_draw_target();
        }
        Ok(())
    }

    /// Loads a ROM cartridge into memory
    pub fn load_rom(&mut self, rom_id: usize) -> NesResult<()> {
        self.loaded_rom = self.roms[rom_id].to_path_buf();
        let mapper = mapper::load_rom(&self.loaded_rom)?;
        self.cpu.bus.load_mapper(mapper);
        Ok(())
    }

    /// Powers on the console
    pub fn power_on(&mut self) -> NesResult<()> {
        self.cpu.power_on();
        if let Err(e) = self.load_sram() {
            self.add_message(&e.to_string());
        }
        self.paused = false;
        self.cycles_remaining = 0.0;
        Ok(())
    }

    /// Powers off the console
    pub fn power_off(&mut self) -> NesResult<()> {
        if self.recording {
            self.save_recording()?;
        }
        if let Err(e) = self.save_sram() {
            self.add_message(&e.to_string());
        }
        self.power_cycle();
        self.paused = true;
        Ok(())
    }

    /// Steps the console the number of instructions required to generate an entire frame
    pub fn clock_frame(&mut self) {
        while !self.cpu.bus.ppu.frame_complete {
            let _ = self.clock();
        }
        self.cpu.bus.ppu.frame_complete = false;
    }

    pub fn clock_seconds(&mut self, seconds: f64) {
        self.cycles_remaining += CPU_CLOCK_RATE * seconds;
        while self.cycles_remaining > 0.0 {
            self.cycles_remaining -= self.clock() as f64;
        }
    }

    /// Add Game Genie Codes
    pub fn add_genie_code(&mut self, val: &str) -> NesResult<()> {
        self.cpu.bus.add_genie_code(val)
    }

    /// Returns a rendered frame worth of data from the PPU
    pub fn frame(&mut self) -> &Vec<u8> {
        &self.cpu.bus.ppu.frame()
    }

    /// Returns nametable graphics
    pub fn nametables(&self) -> &Vec<Vec<u8>> {
        self.cpu.bus.ppu.nametables()
    }

    /// Returns pattern table graphics
    pub fn pattern_tables(&self) -> &Vec<Vec<u8>> {
        self.cpu.bus.ppu.pattern_tables()
    }

    /// Returns palette graphics
    pub fn palette(&self) -> &Vec<u8> {
        self.cpu.bus.ppu.palette()
    }

    /// Returns a frame worth of audio samples from the APU
    pub fn audio_samples(&mut self) -> &[f32] {
        self.cpu.bus.apu.samples()
    }

    pub fn clear_audio(&mut self) {
        self.cpu.bus.apu.clear_samples()
    }

    /// Save the current state of the console into a save file
    pub fn save_state(&mut self, slot: u8) {
        if self.settings.save_enabled {
            let save = || -> NesResult<()> {
                let save_path = save_path(&self.loaded_rom, slot)?;
                let save_dir = save_path.parent().unwrap(); // Safe to do because save_path is never root
                if !save_dir.exists() {
                    fs::create_dir_all(save_dir).map_err(|e| {
                        map_nes_err!("failed to create directory {:?}: {}", save_dir.display(), e)
                    })?;
                }
                let save_file = fs::File::create(&save_path).map_err(|e| {
                    map_nes_err!("failed to create file {:?}: {}", save_path.display(), e)
                })?;
                let mut writer = BufWriter::new(save_file);
                write_save_header(&mut writer).map_err(|e| {
                    map_nes_err!("failed to write header {:?}: {}", save_path.display(), e)
                })?;
                self.save(&mut writer)?;
                Ok(())
            };
            match save() {
                Ok(_) => self.add_message(&format!("Saved Slot {}", slot)),
                Err(e) => self.add_message(&e.to_string()),
            }
        } else {
            self.add_message("Savestates Disabled");
        }
    }

    /// Load the console with data saved from a save state
    pub fn load_state(&mut self, slot: u8) {
        if self.settings.save_enabled {
            if let Ok(save_path) = save_path(&self.loaded_rom, slot) {
                if save_path.exists() {
                    let mut load = || -> NesResult<()> {
                        let save_file = fs::File::open(&save_path).map_err(|e| {
                            map_nes_err!("Failed to open file {:?}: {}", save_path.display(), e)
                        })?;
                        let mut reader = BufReader::new(save_file);
                        match validate_save_header(&mut reader) {
                            Ok(_) => {
                                if let Err(e) = self.load(&mut reader) {
                                    self.reset();
                                    return nes_err!("Failed to load savestate #{}: {}", slot, e);
                                }
                            }
                            Err(e) => return nes_err!("Failed to load savestate #{}: {}", slot, e),
                        }
                        Ok(())
                    };
                    match load() {
                        Ok(()) => self.add_message(&format!("Loaded Slot {}", slot)),
                        Err(e) => self.add_message(&e.to_string()),
                    }
                }
            }
        } else {
            self.add_message("Saved States Disabled");
        }
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    fn save_sram(&mut self) -> NesResult<()> {
        if let Some(mapper) = &self.cpu.bus.mapper {
            let mapper = mapper.borrow();
            if mapper.battery_backed() {
                let sram_path = sram_path(&self.loaded_rom)?;
                let sram_dir = sram_path.parent().unwrap(); // Safe to do because sram_path is never root
                if !sram_dir.exists() {
                    fs::create_dir_all(sram_dir).map_err(|e| {
                        map_nes_err!("failed to create directory {:?}: {}", sram_dir.display(), e)
                    })?;
                }

                let mut sram_opts = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&sram_path)
                    .map_err(|e| {
                        map_nes_err!("failed to open file {:?}: {}", sram_path.display(), e)
                    })?;

                // Empty file means we just created it
                if sram_opts.metadata()?.len() == 0 {
                    let mut sram_file = BufWriter::new(sram_opts);
                    write_save_header(&mut sram_file).map_err(|e| {
                        map_nes_err!("failed to write header {:?}: {}", sram_path.display(), e)
                    })?;
                    mapper.save_sram(&mut sram_file)?;
                } else {
                    // Check if exists and header is different, so we avoid overwriting
                    match validate_save_header(&mut sram_opts) {
                        Ok(_) => {
                            let mut sram_file = BufWriter::new(sram_opts);
                            mapper.save_sram(&mut sram_file)?;
                        }
                        Err(e) => {
                            return nes_err!(
                                "failed to write sram due to invalid header. error: {}",
                                e
                            )
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    fn load_sram(&mut self) -> NesResult<()> {
        let load_failure = {
            if let Some(mapper) = &self.cpu.bus.mapper {
                let mut mapper = mapper.borrow_mut();
                if mapper.battery_backed() {
                    let sram_path = sram_path(&self.loaded_rom)?;
                    if sram_path.exists() {
                        let sram_file = fs::File::open(&sram_path).map_err(|e| {
                            map_nes_err!("failed to open file {:?}: {}", sram_path.display(), e)
                        })?;
                        let mut sram_file = BufReader::new(sram_file);
                        match validate_save_header(&mut sram_file) {
                            Ok(_) => {
                                if let Err(e) = mapper.load_sram(&mut sram_file) {
                                    return nes_err!("failed to load save sram: {}", e);
                                }
                            }
                            Err(e) => return nes_err!(
                                "failed to load sram: {}.\n  move or delete `{}` before exiting, otherwise sram data will be lost.",
                                e,
                                sram_path.display()
                            ),
                        }
                    }
                }
            }
            Ok(())
        };
        if load_failure.is_err() {
            self.reset();
        }
        load_failure
    }
}

impl State for Ui {
    fn on_start(&mut self, data: &mut StateData) -> PixEngineResult<()> {
        // Before rendering anything, set up our textures
        self.create_textures(data)?;

        if let Ok(mut roms) = find_roms(&self.settings.path) {
            self.roms.append(&mut roms);
        }
        if self.roms.len() == 1 {
            self.load_rom(0)?;
            self.power_on()?;
            self.load_state(self.settings.save_slot);
            let codes = self.settings.genie_codes.to_vec();
            for code in codes {
                if let Err(e) = self.add_genie_code(&code) {
                    self.add_message(&e.to_string());
                }
            }
            self.update_title(data);
        }

        if self.settings.debug {
            self.toggle_debug(data)?;
        }

        self.cpu
            .set_log_level(LogLevel::from_u8(self.settings.log_level)?);

        if self.settings.fullscreen {
            data.fullscreen(true)?;
        }

        // Smooths out startup graphic glitches for some games
        if !self.paused {
            let startup_frames = 40;
            for _ in 0..startup_frames {
                self.clock_frame();
                if self.settings.sound_enabled {
                    let samples = self.audio_samples();
                    data.enqueue_audio(&samples);
                }
                self.clear_audio();
            }
        }
        Ok(())
    }

    fn on_update(&mut self, elapsed: Duration, data: &mut StateData) -> PixEngineResult<()> {
        let elapsed = elapsed.as_secs_f64();

        self.poll_events(data)?;
        self.update_title(data);

        // Save rewind snapshot
        if self.settings.rewind_enabled {
            self.rewind_timer -= elapsed;
            if self.rewind_timer <= 0.0 {
                self.rewind_save %= REWIND_SIZE;
                if self.rewind_save < 5 {
                    self.rewind_save = 5;
                }
                self.rewind_timer = REWIND_TIMER;
                self.save_state(self.rewind_save);
                self.messages.pop(); // Remove saved message
                self.rewind_queue.push_back(self.rewind_save);
                self.rewind_save += 1;
                if self.rewind_queue.len() > REWIND_SIZE as usize {
                    let _ = self.rewind_queue.pop_front();
                }
                self.rewind_slot = self.rewind_queue.len() as u8;
            }
        }

        if !self.paused {
            self.clock += elapsed;
            // Frames that aren't multiples of the default render 1 more/less frames
            // every other frame
            let mut frames_to_run = 0;
            self.speed_counter += (100.0 * self.settings.speed) as i32;
            while self.speed_counter > 0 {
                self.speed_counter -= 100;
                frames_to_run += 1;
            }

            // Clock NES
            for _ in 0..frames_to_run as usize {
                if self.settings.unlock_fps {
                    self.clock_seconds(elapsed);
                } else {
                    self.clock_frame();
                }
                self.turbo_clock = (1 + self.turbo_clock) % 6;
            }
        }
        // Update screen
        data.copy_texture(1, "nes", self.frame())?;
        if self.menu {
            self.draw_menu(data)?;
        }

        self.draw_messages(elapsed, data)?;

        if self.debug {
            if self.active_debug || self.paused {
                self.draw_debug(data);
            }
            self.copy_debug(data)?;
        }
        if self.ppu_viewer {
            self.copy_ppu_viewer(data)?;
        }
        if self.nt_viewer {
            self.copy_nt_viewer(data)?;
        }

        // Enqueue sound
        if self.settings.sound_enabled {
            let samples = self.audio_samples();
            data.enqueue_audio(&samples);
        }
        self.clear_audio();
        Ok(())
    }

    fn on_stop(&mut self, _data: &mut StateData) -> PixEngineResult<()> {
        self.power_off()?;
        Ok(())
    }
}

impl Clocked for Ui {
    /// Steps the console a single CPU instruction at a time
    fn clock(&mut self) -> u64 {
        let cpu_cycles = self.cpu.clock();
        let ppu_cycles = 3 * cpu_cycles;

        for _ in 0..ppu_cycles {
            self.cpu.bus.ppu.clock();
            if self.cpu.bus.ppu.nmi_pending {
                self.cpu.trigger_nmi();
                self.cpu.bus.ppu.nmi_pending = false;
            }

            let irq_pending = if let Some(mapper) = &self.cpu.bus.mapper {
                mapper.borrow_mut().clock();
                mapper.borrow_mut().irq_pending()
            } else {
                false
            };
            self.cpu.set_irq(Irq::Mapper, irq_pending);
        }

        for _ in 0..cpu_cycles {
            self.cpu.bus.apu.clock();
            self.cpu
                .set_irq(Irq::FrameCounter, self.cpu.bus.apu.irq_pending);
            self.cpu.set_irq(Irq::Dmc, self.cpu.bus.apu.dmc.irq_pending);
        }

        cpu_cycles
    }
}

impl Powered for Ui {
    /// Soft-resets the console
    fn reset(&mut self) {
        self.cpu.reset();
        self.clock = 0.0;
    }

    /// Hard-resets the console
    fn power_cycle(&mut self) {
        self.cpu.power_cycle();
    }
}

impl Savable for Ui {
    fn save(&self, fh: &mut dyn Write) -> NesResult<()> {
        self.cpu.save(fh)?;
        self.clock.save(fh)?;
        self.turbo_clock.save(fh)?;
        self.cycles_remaining.save(fh)?;
        self.speed_counter.save(fh)?;
        self.settings.save(fh)?;
        Ok(())
    }
    fn load(&mut self, fh: &mut dyn Read) -> NesResult<()> {
        self.cpu.load(fh)?;
        self.clock.load(fh)?;
        self.turbo_clock.load(fh)?;
        self.cycles_remaining.load(fh)?;
        self.speed_counter.load(fh)?;
        self.settings.load(fh)?;
        Ok(())
    }
}

/// Searches for valid NES rom files ending in `.nes`
///
/// If rom_path is a `.nes` file, uses that
/// If no arg[1], searches current directory for `.nes` files
pub fn find_roms<P: AsRef<Path>>(path: P) -> NesResult<Vec<PathBuf>> {
    use std::ffi::OsStr;
    let path = path.as_ref();
    let mut roms = Vec::new();
    if path.is_dir() {
        path.read_dir()
            .map_err(|e| map_nes_err!("unable to read directory {:?}: {}", path, e))?
            .filter_map(|f| f.ok())
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .for_each(|f| roms.push(f.path()));
    } else if path.is_file() {
        roms.push(path.to_path_buf());
    } else {
        nes_err!("invalid path: {:?}", path)?;
    }
    if roms.is_empty() {
        nes_err!("no rom files found or specified")
    } else {
        Ok(roms)
    }
}

/// Returns the path where battery-backed Save RAM files are stored
///
/// # Arguments
///
/// * `path` - An object that implements AsRef<Path> that holds the path to the currently
/// running ROM
///
/// # Errors
///
/// Panics if path is not a valid path
pub fn sram_path<P: AsRef<Path>>(path: &P) -> NesResult<PathBuf> {
    let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
    path.push(CONFIG_DIR);
    path.push("sram");
    path.push(save_name);
    path.set_extension("dat");
    Ok(path)
}

/// Returns the path where Save states are stored
///
/// # Arguments
///
/// * `path` - An object that implements AsRef<Path> that holds the path to the currently
/// running ROM
///
/// # Errors
///
/// Panics if path is not a valid path
pub fn save_path<P: AsRef<Path>>(path: &P, slot: u8) -> NesResult<PathBuf> {
    let save_name = path.as_ref().file_stem().and_then(|s| s.to_str()).unwrap();
    let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
    path.push(CONFIG_DIR);
    path.push("save");
    path.push(save_name);
    path.push(format!("{}", slot));
    path.set_extension("dat");
    Ok(path)
}

impl fmt::Debug for Ui {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Ui {{\n  cpu: {:?}\n}} ", self.cpu)
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;

    fn load(rom: &str) -> Ui {
        let mut ui = Ui::new();
        ui.roms.push(PathBuf::from(rom));
        ui.load_rom(0).unwrap();
        ui.power_on().unwrap();
        ui
    }

    #[test]
    fn save_header() {
        let mut file = fs::File::create("header.dat").unwrap();
        write_save_header(&mut file).unwrap();
        fs::remove_file("header.dat").unwrap();
    }

    #[test]
    fn find_rom_cases() {
        let rom_tests = &[
            // (Test name, Path, Error)
            // CWD with no `.nes` files
            (
                "CWD with no nes files",
                "./",
                "no rom files found or specified",
            ),
            // Directory with no `.nes` files
            (
                "Dir with no nes files",
                "src/",
                "no rom files found or specified",
            ),
            // Invalid
            (
                "invalid directory",
                "invalid/",
                "invalid path: \"invalid/\"",
            ),
        ];
        for test in rom_tests {
            let roms = find_roms(test.1);
            assert!(roms.is_err(), "invalid path {}", test.0);
            assert_eq!(
                roms.err().unwrap().to_string(),
                test.2,
                "error matches {}",
                test.0
            );
        }
    }

    #[test]
    fn nestest() {
        let rom = "tests/cpu/nestest.nes";
        let mut ui = load(&rom);
        ui.cpu.pc = 0xC000; // Start automated tests
        let _ = ui.clock_seconds(0.5);
        assert_eq!(ui.cpu.peek(0x0000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_oam() {
        let rom = "tests/cpu/dummy_writes_oam.nes";
        let mut ui = load(&rom);
        let _ = ui.clock_seconds(6.0);
        assert_eq!(ui.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn dummy_writes_ppumem() {
        let rom = "tests/cpu/dummy_writes_ppumem.nes";
        let mut ui = load(&rom);
        let _ = ui.clock_seconds(4.0);
        assert_eq!(ui.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn exec_space_ppuio() {
        let rom = "tests/cpu/exec_space_ppuio.nes";
        let mut ui = load(&rom);
        let _ = ui.clock_seconds(2.0);
        assert_eq!(ui.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn instr_timing() {
        let rom = "tests/cpu/instr_timing.nes";
        let mut ui = load(&rom);
        let _ = ui.clock_seconds(22.0);
        assert_eq!(ui.cpu.peek(0x6000), 0x00, "{}", rom);
    }

    #[test]
    fn interrupts() {
        let rom = "tests/cpu/interrupts.nes";
        let mut ui = load(&rom);
        ui.cpu.set_log_level(LogLevel::Debug);
        ui.cpu.bus.apu.set_log_level(LogLevel::Debug);
        ui.cpu.bus.apu.dmc.set_log_level(LogLevel::Debug);
        // let _ = ui.clock_seconds(0.5);
        while ui.cpu.peek(0x6000) != 0x01 {
            ui.clock();
        }
        assert_eq!(ui.cpu.peek(0x6000), 0x00, "{}", rom);
    }
}
