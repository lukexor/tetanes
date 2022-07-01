use crate::{
    cpu::STATUS_REGS,
    mem::{Access, Mem},
    nes::Nes,
};
use pix_engine::prelude::*;

#[derive(Debug)]
pub(crate) struct Debugger {
    window_id: WindowId,
}

impl Debugger {
    const fn new(window_id: WindowId) -> Self {
        Self { window_id }
    }

    pub(crate) const fn window_id(&self) -> WindowId {
        self.window_id
    }
}

impl Nes {
    pub(crate) fn toggle_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.debugger {
            None => {
                let (w, h) = s.dimensions()?;
                let window_id = s
                    .window()
                    .with_dimensions(w, h)
                    .with_title("Debugger")
                    .position(10, 10)
                    .resizable()
                    .build()?;
                self.debugger = Some(Debugger::new(window_id));
                self.pause_play();
            }
            Some(ref debugger) => {
                s.close_window(debugger.window_id())?;
                self.debugger = None;
            }
        }
        Ok(())
    }

    pub(crate) fn render_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        if let Some(ref debugger) = self.debugger {
            s.with_window(debugger.window_id(), |s: &mut PixState| {
                s.clear()?;
                s.fill(Color::WHITE);
                s.stroke(None);

                {
                    let cpu = self.control_deck.cpu();

                    s.text("Status: ")?;
                    s.push();
                    for status in STATUS_REGS {
                        s.same_line(None);
                        s.fill(if cpu.status().intersects(status) {
                            Color::RED
                        } else {
                            Color::GREEN
                        });
                        s.text(&format!("{:?}", status))?;
                    }
                    s.pop();

                    s.text(&format!("Cycle: {:8}", cpu.cycle()))?;
                    // TODO: Total running time

                    s.spacing()?;
                    s.text(&format!(
                        "PC: ${:04X}       A: ${:02X} [{:03}]",
                        cpu.pc(),
                        cpu.a(),
                        cpu.a()
                    ))?;
                    s.text(&format!(
                        "X:  ${:02X} [{:03}]   Y: ${:02X} [{:03}]",
                        cpu.x(),
                        cpu.x(),
                        cpu.y(),
                        cpu.y()
                    ))?;

                    s.spacing()?;
                    s.text(&format!("Stack: $01{:02X}", cpu.sp()))?;
                    s.push();
                    let bytes_per_row = 8;
                    for (i, offset) in (0xE0..=0xFF).rev().enumerate() {
                        let val = cpu.peek(0x0100 | offset, Access::Dummy);
                        if u16::from(cpu.sp()) == offset {
                            s.fill(Color::GREEN);
                        } else {
                            s.fill(Color::GRAY);
                        }
                        s.text(&format!("{:02X} ", val))?;
                        if i % bytes_per_row < bytes_per_row - 1 {
                            s.same_line(None);
                        }
                    }
                    s.pop();
                }

                {
                    let ppu = self.control_deck.ppu();

                    s.spacing()?;
                    s.text("PPU:")?;
                    s.text(&format!(
                        "VRAM Addr: ${:04X}  OAM Addr: ${:02X}",
                        ppu.addr(),
                        ppu.oamaddr()
                    ))?;
                    s.text(&format!(
                        "Cycle: {:3}  Scanline: {:3}  Frame: {}",
                        ppu.cycle(),
                        ppu.scanline() as i32 - 1,
                        ppu.frame_number()
                    ))?;

                    s.spacing()?;
                    if let Some((window_id, _)) = self.emulation {
                        if s.focused_window(window_id) {
                            let m = s.mouse_pos() / self.config.scale as i32;
                            let mx = (m.x() as f32 * 7.0 / 8.0) as i32; // Adjust ratio
                            s.text(&format!("Mouse: {:3}, {:3}", mx, m.y()))?;
                        } else {
                            s.text("Mouse: 0, 0")?;
                        }
                    }
                }

                //                 {
                //                     let cpu = self.control_deck.cpu();

                //                     s.spacing()?;
                //                     let disasm = disasm(cpu.pc(), cpu.pc().saturating_add(30));
                //                     for instr in disasm.iter().take(10) {
                //                         s.text(&instr)?;
                //                     }
                //                 }

                Ok(())
            })?;
        }
        Ok(())
    }
}
