use crate::{
    common::Clocked,
    cpu::StatusRegs,
    memory::{MemAccess, MemRead},
    nes::{Mode, Nes, View},
    ppu::{PATTERN_WIDTH, RENDER_CHANNELS, RENDER_HEIGHT, RENDER_WIDTH},
};
use pix_engine::prelude::*;

const PALETTE_HEIGHT: u32 = 64;

impl Nes {
    pub(crate) fn clock_debug(&mut self) -> bool {
        // FIXME: Temporary CPU breakpoint stopgap
        // Types of breakpoints:
        // - Address: Read/Write/Execute
        // - Address Range: R/W/E
        // - Any
        //
        // Conditions:
        // - A/X/Y/P/SP
        // - PC
        // - Opcode
        // - Scanline
        // - Cycle
        // - Memory value
        // - Branched
        // - IRQ/NMI
        // - Spr0 Hit/Spr Overflow
        // - VBlank
        //
        // Break enabled: bool
        let breakpoints = [];

        while !self.control_deck.frame_complete() {
            if breakpoints.contains(&self.control_deck.pc()) {
                self.mode = Mode::Paused;
                return true;
            }
            if let (Some(addr), _) = self.control_deck.next_addr(MemAccess::Read) {
                if breakpoints.contains(&addr) {
                    self.mode = Mode::Paused;
                    return true;
                }
            }
            self.control_deck.clock();
            if self.control_deck.cpu_corrupted() {
                self.mode = Mode::Paused;
                self.error = Some("CPU encountered invalid opcode.".into());
                return true;
            }
        }
        self.control_deck.start_new_frame();

        false
    }

    pub(crate) fn toggle_cpu_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.cpu_debugger {
            None => {
                let (w, h) = s.dimensions()?;
                let window_id = s
                    .window()
                    .with_dimensions(w, h)
                    .with_title("CPU Debugger")
                    .position(10, 10)
                    .resizable()
                    .build()?;
                self.cpu_debugger = Some(View::new(window_id, None));
                self.control_deck.cpu_mut().debugging = true;
                if self.control_deck.is_running() {
                    self.mode = Mode::Paused;
                }
            }
            Some(debugger) => {
                s.close_window(debugger.window_id)?;
                self.control_deck.cpu_mut().debugging = false;
                self.cpu_debugger = None;
            }
        }
        Ok(())
    }

    pub(crate) fn render_cpu_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        if let Some(view) = self.cpu_debugger {
            s.with_window(view.window_id, |s: &mut PixState| {
                s.clear()?;
                s.stroke(None);

                {
                    let cpu = self.control_deck.cpu();

                    s.text("Status: ")?;
                    use StatusRegs::{B, C, D, I, N, U, V, Z};
                    s.push();
                    for status in &[N, V, U, B, D, I, Z, C] {
                        s.same_line(None);
                        s.fill(if cpu.status & *status as u8 > 0 {
                            Color::RED
                        } else {
                            Color::GREEN
                        });
                        s.text(&format!("{:?}", status))?;
                    }
                    s.pop();

                    s.text(&format!("Cycles: {:8}", cpu.cycle_count))?;
                    // TODO: Total running time

                    s.spacing()?;
                    s.text(&format!(
                        "PC: ${:04X}           A: ${:02X} [{:03}]",
                        cpu.pc, cpu.acc, cpu.acc
                    ))?;
                    s.text(&format!(
                        "X:  ${:02X} [{:03}]   Y: ${:02X} [{:03}]",
                        cpu.x, cpu.x, cpu.y, cpu.y
                    ))?;

                    s.spacing()?;
                    s.text(&format!("Stack: $01{:02X}", cpu.sp))?;
                    let bytes_per_row = 8;
                    for (i, offset) in (0xE0..=0xFF).rev().enumerate() {
                        let val = cpu.peek(0x0100 | offset);
                        s.text(&format!("{:02X} ", val))?;
                        if i % bytes_per_row < bytes_per_row - 1 {
                            s.same_line(None);
                        }
                    }
                }

                {
                    let ppu = self.control_deck.ppu();

                    s.text(&format!("VRAM Addr: ${:04X}", ppu.read_ppuaddr()))?;
                    s.text(&format!("OAM Addr:  ${:02X}", ppu.read_oamaddr()))?;
                    s.text(&format!(
                        "PPU Cycle: {:3}  Scanline: {:3}",
                        ppu.cycle,
                        i32::from(ppu.scanline) - 1
                    ))?;

                    s.spacing()?;
                    if let Some(view) = self.emulation {
                        if s.focused_window(view.window_id) {
                            let m = s.mouse_pos() / self.config.scale as i32;
                            let mx = (m.x() as f32 * 7.0 / 8.0) as i32; // Adjust ratio
                            s.text(&format!("Mouse: {:3}, {:3}", mx, m.y()))?;
                        } else {
                            s.text("Mouse: 0, 0")?;
                        }
                    }
                }

                s.spacing()?;
                let disasm = self.control_deck.disasm(
                    self.control_deck.pc(),
                    self.control_deck.pc().saturating_add(20),
                );
                for instr in &disasm {
                    s.text(&instr)?;
                }

                Ok(())
            })?;
        }
        Ok(())
    }

    pub(crate) fn toggle_ppu_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.ppu_debugger {
            None => {
                let w = 4 * RENDER_WIDTH;
                let h = 3 * RENDER_HEIGHT;
                let window_id = s
                    .window()
                    .with_dimensions(w, h)
                    .with_title("PPU Debugger")
                    .position(10, 10)
                    .resizable()
                    .build()?;
                s.with_window(window_id, |s: &mut PixState| {
                    let texture_id = s.create_texture(w, h, PixelFormat::Rgba)?;
                    self.ppu_debugger = Some(View::new(window_id, Some(texture_id)));
                    Ok(())
                })?;
                self.control_deck.ppu_mut().update_debug();
                self.control_deck.ppu_mut().set_debugging(true);
            }
            Some(debugger) => {
                s.close_window(debugger.window_id)?;
                self.ppu_debugger = None;
                self.control_deck.ppu_mut().set_debugging(false);
            }
        }
        Ok(())
    }

    pub(crate) fn render_ppu_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        if let Some(view) = self.ppu_debugger {
            if let Some(texture_id) = view.texture_id {
                s.with_window(view.window_id, |s: &mut PixState| {
                    s.clear()?;

                    let width = RENDER_WIDTH as i32;
                    let height = RENDER_HEIGHT as i32;
                    let m = s.mouse_pos();

                    // Nametables

                    let nametables = &self.control_deck.ppu().nametables;
                    let nametable1 = rect![0, 0, width, height];
                    let nametable2 = rect![width, 0, width, height];
                    let nametable3 = rect![0, height, width, height];
                    let nametable4 = rect![width, height, width, height];
                    let nametable_src = rect![0, 0, 2 * width, 2 * height];
                    let nametable_pitch = RENDER_CHANNELS * RENDER_WIDTH as usize;

                    s.update_texture(texture_id, nametable1, &nametables[0], nametable_pitch)?;
                    s.update_texture(texture_id, nametable2, &nametables[1], nametable_pitch)?;
                    s.update_texture(texture_id, nametable3, &nametables[2], nametable_pitch)?;
                    s.update_texture(texture_id, nametable4, &nametables[3], nametable_pitch)?;
                    s.texture(texture_id, nametable_src, nametable_src)?;

                    // Scanline
                    let scanline = self.scanline as i32;
                    s.push();
                    s.stroke(Color::WHITE);
                    s.stroke_weight(2);
                    s.line([0, scanline, 2 * width, scanline])?;
                    s.line([0, scanline + height, 2 * width, scanline + height])?;
                    s.pop();

                    // Nametable Info

                    s.set_cursor_pos([s.cursor_pos().x(), nametable3.bottom() + 4]);

                    s.text(&format!("Scanline: {}", self.scanline))?;
                    let mirroring = self.control_deck.cart().mirroring();
                    s.text(&format!("Mirroring: {:?}", mirroring))?;

                    if s.focused_window(view.window_id)
                        && rect![0, 0, 2 * width, 2 * height].contains(m)
                    {
                        let x = m.x() - nametable_src.x();
                        let y = m.y() - nametable_src.y();
                        let nt_addr = (x / width) * 0x0400 + (y / height) * 0x0800;
                        let ppu_addr = nt_addr + ((((y / 8) % 30) << 5) | ((x / 8) % 32));
                        let tile_id = self
                            .control_deck
                            .ppu()
                            .nametable_ids
                            .get(ppu_addr as usize)
                            .unwrap_or(&0x00);
                        s.text(&format!("Tile ID: ${:02X}", tile_id))?;
                        s.text(&format!("(X, Y): ({}, {})", x, y))?;
                        s.text(&format!("Nametable: ${:04X}", nt_addr))?;
                        s.text(&format!("PPU Addr: ${:04X}", ppu_addr))?;
                    } else {
                        s.text("Tile ID: $00")?;
                        s.text("(X, Y): (0, 0)")?;
                        s.text("Nametable: $0000")?;
                        s.text("PPU Addr: $0000")?;
                    }

                    // Pattern Tables

                    let patterns = &self.control_deck.ppu().pattern_tables;
                    let pattern_x = nametable_src.right() + 8;
                    let pattern_w = PATTERN_WIDTH as i32;
                    let pattern_h = pattern_w;
                    let pattern_left = rect![pattern_x, 0, pattern_w, pattern_h];
                    let pattern_right = rect![pattern_x + pattern_w, 0, pattern_w, pattern_h];
                    let pattern_src = rect![pattern_x, 0, 2 * pattern_w, pattern_h];
                    let pattern_dst = rect![pattern_x, 0, 4 * pattern_w, 2 * pattern_h];
                    let pattern_pitch = RENDER_CHANNELS * pattern_w as usize;
                    s.update_texture(texture_id, pattern_left, &patterns[0], pattern_pitch)?;
                    s.update_texture(texture_id, pattern_right, &patterns[1], pattern_pitch)?;
                    s.texture(texture_id, pattern_src, pattern_dst)?;

                    // Palette

                    let palette = &self.control_deck.ppu().palette;
                    let palette_w = 16;
                    let palette_h = 2;
                    let palette_src = rect![0, pattern_src.bottom(), palette_w, palette_h];
                    let palette_dst = rect![
                        pattern_x,
                        pattern_dst.bottom(),
                        2 * width,
                        PALETTE_HEIGHT as i32
                    ];
                    let palette_pitch = RENDER_CHANNELS * palette_w as usize;
                    s.update_texture(texture_id, palette_src, &palette, palette_pitch)?;
                    s.texture(texture_id, palette_src, palette_dst)?;

                    // Borders

                    s.push();

                    s.stroke(Color::DIM_GRAY);
                    s.fill(None);
                    s.stroke_weight(2);

                    s.rect(nametable1)?;
                    s.rect(nametable2)?;
                    s.rect(nametable3)?;
                    s.rect(nametable4)?;
                    s.rect(pattern_dst)?;
                    s.line([
                        pattern_dst.center().x(),
                        pattern_dst.top(),
                        pattern_dst.center().x(),
                        pattern_dst.bottom(),
                    ])?;

                    s.pop();

                    // PPU Address Info

                    s.set_cursor_pos([s.cursor_pos().x(), palette_dst.bottom() + 4]);
                    s.set_column_offset(pattern_x);

                    if pattern_dst.contains(m) {
                        let x = m.x() - pattern_dst.x();
                        let y = m.y() - pattern_dst.y();
                        let tile = (y / 16) << 4 | ((x / 16) % 16);
                        s.text(&format!("Tile: ${:02X}", tile))?;
                    } else {
                        s.text("Tile: $00")?;
                    }

                    if palette_dst.contains(m) {
                        let py = m.y().saturating_sub(height + 2) / 32;
                        let px = m.x() / 32;
                        let palette = self
                            .control_deck
                            .ppu()
                            .palette_ids
                            .get((py * 16 + px) as usize)
                            .unwrap_or(&0x00);
                        s.text(&format!("Palette: ${:02X}", palette))?;
                    } else {
                        s.text("Palette: $00")?;
                    }

                    s.reset_column_offset();
                    Ok(())
                })?;
            }
        }
        Ok(())
    }

    pub(crate) fn toggle_apu_debugger(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.apu_debugger {
            None => {
                // let window_id = s
                //     .window()
                //     .with_dimensions(w, h)
                //     .with_title("APU Debugger")
                //     .position(10, 10)
                //     .build()?;
                // self.apu_debugger = Some(View::new(window_id, Some(texture_id)));
            }
            Some(debugger) => {
                s.close_window(debugger.window_id)?;
                self.apu_debugger = None;
            }
        }
        Ok(())
    }
}
