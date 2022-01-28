use crate::{
    common::{CONFIG_DIR, SAVE_DIR, SRAM_DIR},
    nes::Nes,
};
use pix_engine::prelude::*;
use std::path::PathBuf;

impl Nes {
    pub(super) fn render_help(&mut self, s: &mut PixState) -> PixResult<()> {
        s.collapsing_header("Shortcuts", |s: &mut PixState| {
            s.collapsing_tree("Gamepad", |s: &mut PixState| {
                s.fill(s.theme().colors.primary_variant);
                s.text("Button    | Keyboard    | Controller      ")?;
                s.text("--------- | ----------- | ----------------")?;
                s.text("A         | Z           | A               ")?;
                s.text("B         | X           | B               ")?;
                s.text("A (Turbo) | A           | X               ")?;
                s.text("B (Turbo) | S           | Y               ")?;
                s.text("Start     | Return      | Start           ")?;
                s.text("Select    | Right Shift | Back            ")?;
                s.text("D-Pad     | Arrow Keys  | Left Stick/D-Pad")?;
                Ok(())
            })?;

            s.collapsing_tree("Gameplay", |s: &mut PixState| {
                s.fill(s.theme().colors.primary_variant);
                s.text("Action          | Keyboard         | Controller        ")?;
                s.text("--------------- | ---------------- | ------------------")?;
                s.text("+ Speed by 25%  | Ctrl-=           | Right Shoulder    ")?;
                s.text("- Speed by 25%  | Ctrl--           | Left Shoulder     ")?;
                s.text("Fast-Forward 2x | Space            |                   ")?;
                s.text("Set Save Slot # | Ctrl-(1-4)       |                   ")?;
                s.text("Save State      | Ctrl-S           |                   ")?;
                s.text("Load State      | Ctrl-L           |                   ")?;
                s.text("Instant Rewind  | R                |                   ")?;
                s.text("Visual Rewind   | R                |                   ")?;
                Ok(())
            })?;

            s.collapsing_tree("Emulator", |s: &mut PixState| {
                s.fill(s.theme().colors.primary_variant);
                s.text("Action                 | Keyboard         | Controller        ")?;
                s.text("---------------------- | ---------------- | ------------------")?;
                s.text("Help Menu              | Ctrl-H or F1     |                   ")?;
                s.text("Config Menu            | Ctrl-C or F2     |                   ")?;
                s.text("Load/Open ROM          | Ctrl-O or F3     |                   ")?;
                s.text("Pause                  | Escape           | Guide Button      ")?;
                s.text("Quit                   | Ctrl-Q           |                   ")?;
                s.text("Reset                  | Ctrl-R           |                   ")?;
                s.text("Power Cycle            | Ctrl-P           |                   ")?;
                s.text("Take Screenshot        | F10              |                   ")?;
                s.text("Toggle Game Recording  | Shift-V          |                   ")?;
                s.text("Toggle Music Recording | Shift-R          |                   ")?;
                s.text("Toggle Music           | Ctrl-M           |                   ")?;
                s.text("Toggle Fullscreen      | Ctrl-Return      |                   ")?;
                s.text("Toggle Vsync           | Ctrl-V           |                   ")?;
                s.text("Toggle NTSC Filter     | Ctrl-N           |                   ")?;
                Ok(())
            })?;

            s.collapsing_tree("Debugging", |s: &mut PixState| {
                s.fill(s.theme().colors.primary_variant);
                s.text("Action                  | Keyboard         | Controller        ")?;
                s.text("----------------------- | ---------------- | ------------------")?;
                s.text("Toggle Pulse Channel 1  | Shift-1          |                   ")?;
                s.text("Toggle Pulse Channel 2  | Shift-2          |                   ")?;
                s.text("Toggle Triangle Channel | Shift-3          |                   ")?;
                s.text("Toggle Noise Channel    | Shift-4          |                   ")?;
                s.text("Toggle DMC Channel      | Shift-5          |                   ")?;
                s.text("Toggle CPU Debugger     | Ctrl-D           |                   ")?;
                s.text("Toggle PPU Viewer       | Shift-P          |                   ")?;
                s.text("Toggle Nametable Viewer | Shift-N          |                   ")?;
                Ok(())
            })?;

            s.collapsing_tree("CPU Debugger", |s: &mut PixState| {
                s.fill(s.theme().colors.primary_variant);
                s.text("Action                  | Keyboard        ")?;
                s.text("----------------------- | ----------------")?;
                s.text("Step instruction        | C               ")?;
                s.text("Step over instruction   | O               ")?;
                s.text("Step out of instruction | Shift-O         ")?;
                s.text("Step a single scanline  | L               ")?;
                s.text("Step an entire frame    | F               ")?;
                s.text("Move scanline up        | Shift-Up        ")?;
                s.text("Move scanline down      | Shift-Down      ")?;
                Ok(())
            })?;

            Ok(())
        })?;

        s.collapsing_header("Directories", |s: &mut PixState| {
            let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("./"));

            s.bullet("Save states: ")?;
            s.same_line(None);
            s.monospace(home_dir.join(CONFIG_DIR).join(SAVE_DIR).to_string_lossy())?;

            s.bullet("Battery-Backed Save RAM: ")?;
            s.same_line(None);
            s.monospace(home_dir.join(CONFIG_DIR).join(SRAM_DIR).to_string_lossy())?;

            Ok(())
        })?;

        Ok(())
    }
}
