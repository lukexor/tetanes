use crate::{
    apu::AudioChannel,
    nes::{Nes, WINDOW_HEIGHT, WINDOW_WIDTH},
    ppu::Filter,
};
use pix_engine::prelude::*;

impl Nes {
    pub(super) fn render_config(&mut self, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("General", |s: &mut PixState| {
            s.checkbox("Pause in Background", &mut self.config.pause_in_bg)?;

            let mut save_slot = self.config.save_slot as usize - 1;
            s.next_width(50);
            if s.select_box("Save Slot", &mut save_slot, &["1", "2", "3", "4"], 4)? {
                self.config.save_slot = save_slot as u8 + 1;
            }
            Ok(())
        })?;

        s.collapsing_header("Emulation", |s: &mut PixState| {
            s.checkbox("Consistent Power-up RAM", &mut self.config.consistent_ram)?;
            s.checkbox("Concurrent D-Pad", &mut self.config.concurrent_dpad)?;

            s.next_width(s.theme().font_size * 15);
            if s.slider("Speed", &mut self.config.speed, 0.25, 2.0)? {
                self.set_speed(self.config.speed);
            }
            Ok(())
        })?;

        s.collapsing_header("Sound", |s: &mut PixState| {
            s.checkbox("Enabled", &mut self.config.sound)?;
            s.spacing()?;

            s.text("Channels:")?;
            let mut pulse1 = self.control_deck.channel_enabled(AudioChannel::Pulse1);
            if s.checkbox("Pulse 1", &mut pulse1)? {
                self.control_deck.toggle_channel(AudioChannel::Pulse1);
            }
            let mut pulse2 = self.control_deck.channel_enabled(AudioChannel::Pulse2);
            if s.checkbox("Pulse 2", &mut pulse2)? {
                self.control_deck.toggle_channel(AudioChannel::Pulse2);
            }
            let mut triangle = self.control_deck.channel_enabled(AudioChannel::Triangle);
            if s.checkbox("Triangle", &mut triangle)? {
                self.control_deck.toggle_channel(AudioChannel::Triangle);
            }
            let mut noise = self.control_deck.channel_enabled(AudioChannel::Noise);
            if s.checkbox("Noise", &mut noise)? {
                self.control_deck.toggle_channel(AudioChannel::Noise);
            }
            let mut dmc = self.control_deck.channel_enabled(AudioChannel::Dmc);
            if s.checkbox("DMC", &mut dmc)? {
                self.control_deck.toggle_channel(AudioChannel::Dmc);
            }
            Ok(())
        })?;

        s.collapsing_header("Video", |s: &mut PixState| {
            let mut scale = self.config.scale as usize - 1;
            s.next_width(50);
            if s.select_box("Scale", &mut scale, &["1", "2", "3", "4"], 4)? {
                self.config.scale = scale as f32 + 1.0;
                let width = (self.config.scale * WINDOW_WIDTH) as u32;
                let height = (self.config.scale * WINDOW_HEIGHT) as u32;
                s.set_window_dimensions((width, height))?;
                let (font_size, pad, ipady) = match scale {
                    0 => (6, 4, 3),
                    1 => (8, 6, 4),
                    2 => (12, 8, 6),
                    3 => (16, 10, 8),
                    _ => unreachable!("invalid scale"),
                };
                s.font_size(font_size)?;
                s.theme_mut().spacing.frame_pad = point!(pad, pad);
                s.theme_mut().spacing.item_pad = point!(pad, ipady);
            }

            let mut enabled = self.control_deck.filter() == Filter::Ntsc;
            if s.checkbox("NTSC Filter", &mut enabled)? {
                self.control_deck
                    .set_filter(if enabled { Filter::Ntsc } else { Filter::None });
            }

            if s.checkbox("Fullscreen", &mut self.config.fullscreen)? {
                s.fullscreen(self.config.fullscreen)?;
            }

            if s.checkbox("VSync Enabled", &mut self.config.vsync)? {
                s.vsync(self.config.vsync)?;
            }
            Ok(())
        })?;

        Ok(())
    }
}
