use crate::{
    nes::{filesystem, menu::Menu, Nes},
    NesError, NesResult,
};
use web_time::{Duration, Instant};

/// Represents which mode the emulator is in.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Mode {
    Play,
    Replay,
    Rewind,
    Pause,
    Menu(Menu),
}

impl Default for Mode {
    fn default() -> Self {
        Self::Menu(Menu::default())
    }
}

impl Nes {
    #[inline]
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        matches!(self.mode, Mode::Play)
    }

    #[inline]
    #[must_use]
    pub const fn is_rewinding(&self) -> bool {
        matches!(self.mode, Mode::Rewind)
    }

    #[inline]
    #[must_use]
    pub const fn is_replaying(&self) -> bool {
        matches!(self.mode, Mode::Replay)
    }

    #[inline]
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        matches!(self.mode, Mode::Pause)
    }

    #[inline]
    #[must_use]
    pub const fn in_menu(&self) -> bool {
        matches!(self.mode, Mode::Menu(..))
    }

    #[inline]
    pub fn handle_error(&mut self, err: NesError) {
        self.pause_play();
        log::error!("{err:?}");
        self.error = Some(err.to_string());
    }

    #[inline]
    pub fn add_message<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        let text = text.into();
        log::info!("{text}");
        self.messages.push((text, Instant::now()));
    }

    pub fn render_messages(&mut self) {
        const TIMEOUT: Duration = Duration::from_secs(3);

        let now = Instant::now();
        self.messages
            .retain(|(_, created)| (now - *created) < TIMEOUT);
        self.messages.dedup_by(|a, b| a.0.eq(&b.0));
        for (message, _) in &self.messages {
            // render_message(message, Color::WHITE);
        }
    }

    pub fn render_confirm_quit(&mut self) {
        // TODO switch to egui
        // if let Some((ref msg, ref mut confirm)) = self.confirm_quit {
        //     s.push();
        //     s.stroke(None);
        //     s.fill(rgb!(0, 200));
        //     let pady = s.theme().spacing.frame_pad.y();
        //     let width = s.width()?;
        //     s.wrap(width);
        //     let (_, height) = s.size_of(msg)?;
        //     s.rect([
        //         0,
        //         s.cursor_pos().y() - pady,
        //         width as i32,
        //         4 * height as i32 + 2 * pady,
        //     ])?;
        //     s.fill(Color::WHITE);
        //     s.text(msg)?;
        //     if s.button("Confirm")? {
        //         *confirm = true;
        //         s.pop();
        //         return Ok(true);
        //     }
        //     s.same_line(None);
        //     if s.button("Cancel")? {
        //         self.confirm_quit = None;
        //         self.resume_play();
        //     }
        //     s.pop();
        // }
    }

    #[inline]
    pub fn render_status(&mut self, status: &str) {
        // render_message(status, Color::WHITE);
        if let Some(ref err) = self.error {
            // render_message(err, Color::RED);
        }
    }

    pub fn resume_play(&mut self) {
        self.mode = Mode::Play;
        if self.control_deck.is_running() {
            if let Err(err) = self.mixer.play() {
                self.add_message(format!("failed to start audio: {err:?}"));
            }
        }
    }

    pub fn pause_play(&mut self) {
        self.mode = Mode::Pause;
        if self.control_deck.is_running() {
            if self.replay_state.is_recording() {
                self.stop_replay();
            }
            self.mixer.pause();
        }
    }

    /// Save battery-backed Save RAM to a file (if cartridge supports it)
    pub fn save_sram(&self) -> NesResult<()> {
        if self.control_deck.cart_battery_backed() {
            if let Some(sram_path) = self.sram_path() {
                log::info!("saving SRAM...");
                filesystem::save_data(sram_path, self.control_deck.sram())?;
            }
        }
        Ok(())
    }

    /// Load battery-backed Save RAM from a file (if cartridge supports it)
    pub fn load_sram(&mut self) -> NesResult<()> {
        if self.control_deck.cart_battery_backed() {
            if let Some(sram_path) = self.sram_path() {
                if sram_path.exists() {
                    log::info!("loading SRAM...");
                    filesystem::load_data(&sram_path)
                        .map(|data| self.control_deck.load_sram(data))?;
                }
            }
        }
        Ok(())
    }

    pub fn toggle_pause(&mut self) {
        if self.is_playing() {
            self.pause_play();
        } else if self.is_paused() {
            self.resume_play();
        } else if self.in_menu() {
            self.exit_menu();
        }
    }

    pub fn toggle_sound_recording(&mut self) {
        if self.is_playing() {
            if !self.mixer.is_recording() {
                match self.mixer.start_recording() {
                    Ok(_) => {
                        self.add_message("Recording audio...");
                    }
                    Err(err) => {
                        log::error!("{err:?}");
                        self.add_message("Failed to start recording audio");
                    }
                }
            } else {
                self.mixer.stop_recording();
                self.add_message("Recording audio stopped.");
            }
        }
    }
}
