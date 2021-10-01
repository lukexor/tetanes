use crate::{
    cpu::{
        instr::{AddrMode::*, Operation::*, INSTRUCTIONS},
        StatusRegs,
    },
    mapper::Mapper,
    memory::MemRead,
    nes::{Nes, WINDOW_WIDTH},
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    NesResult,
};
use pix_engine::prelude::*;

const PALETTE_HEIGHT: u32 = 64;
pub(super) const DEBUG_WIDTH: u32 = 350;
pub(super) const INFO_WIDTH: u32 = 2 * RENDER_WIDTH;
pub(super) const INFO_HEIGHT: u32 = 4 * 10;

struct Debug {
    running_time: Duration,
}

impl Nes {
    pub(super) fn toggle_ppu_viewer(&mut self, s: &mut PixState) -> NesResult<()> {
        self.ppu_viewer = !self.ppu_viewer;
        if self.ppu_viewer {
            let info_height = 4 * 10;
            let window = s
                .create_window(
                    2 * RENDER_WIDTH,
                    RENDER_HEIGHT + PALETTE_HEIGHT + info_height,
                )
                .with_title("PPU Viewer")
                .build()?;
            self.ppu_viewer_window = Some(window);

            // Set up two side-by-side textures for each palette plane
            let src = rect!(0, 0, RENDER_WIDTH / 2, RENDER_HEIGHT / 2);
            let left_dst = rect!(0, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let right_dst = rect!(RENDER_WIDTH, 0, RENDER_WIDTH, RENDER_HEIGHT);
            // TODO
            // s.create_texture(window, "left_pattern", ColorType::Rgba, src, left_dst)?;
            // s.create_texture(window, "right_pattern", ColorType::Rgba, src, right_dst)?;

            // Set up palette texture
            let src = rect!(0, 0, 16, 2);
            let dst = rect!(0, RENDER_HEIGHT, 2 * RENDER_WIDTH, PALETTE_HEIGHT);
            // s.create_texture(window, "palette", ColorType::Rgba, src, dst)?;

            // Set up info panel at the bottom
            let src = rect!(0, 0, 2 * RENDER_WIDTH, info_height);
            let dst = rect!(
                0,
                RENDER_HEIGHT + PALETTE_HEIGHT,
                2 * RENDER_WIDTH,
                info_height,
            );
            // s.create_texture(window, "ppu_info", ColorType::Rgb, src, dst)?;

            // Since debug may not have been enabled before, have PPU generate nametable data
            self.cpu.bus.ppu.update_debug();
        } else if let Some(ppu_viewer_window) = self.ppu_viewer_window {
            s.close_window(ppu_viewer_window)?;
        }
        self.cpu
            .bus
            .ppu
            .set_debug(self.nt_viewer || self.ppu_viewer);
        Ok(())
    }

    pub(super) fn copy_ppu_viewer(&mut self, s: &mut PixState) -> NesResult<()> {
        if let Some(ppu_viewer_window) = self.ppu_viewer_window {
            // Set up patterns
            let pat_tables = &self.cpu.bus.ppu.pattern_tables;
            // s.copy_texture(ppu_viewer_window, "left_pattern", &pat_tables[0])?;
            // s.copy_texture(ppu_viewer_window, "right_pattern", &pat_tables[1])?;

            // Draw Borders
            let borders = Image::new(RENDER_WIDTH / 2, RENDER_HEIGHT);
            // s.set_draw_target(borders);
            s.fill(BLACK);
            s.line((0, 0, 0, RENDER_HEIGHT as i32))?;
            // s.copy_window_draw_target(ppu_viewer_window, "right_pattern")?;
            // s.clear_draw_target();

            // Set up palette
            // s.copy_texture(ppu_viewer_window, "palette", &self.cpu.bus.ppu.palette)?;

            // Set up info
            // s.set_draw_target(self.ppu_info_image.clone());
            let mut p = point!(5, 5);
            let ypad = 10;

            // Clear
            let width = self.nt_info_image.width();
            let height = self.nt_info_image.height();
            s.rect((p, width - p.x as u32, height - p.y as u32))?;

            s.fill(WHITE);
            s.text(p, &format!("Scanline: {}", self.pat_scanline))?;
            p.y += ypad;

            let m = s.mouse_pos();
            let (tile, palette) = if self.focused_window == Some(ppu_viewer_window)
                && m >= point!(0, 0)
                && m.x < (2 * RENDER_WIDTH - 1) as i32
            {
                let tile = if m.y < RENDER_HEIGHT as i32 {
                    format!("${:02X}", (m.y / 16) << 4 | ((m.x / 16) % 16))
                } else {
                    String::new()
                };
                let palette = if m.y >= RENDER_HEIGHT as i32
                    && m.y <= (RENDER_HEIGHT + PALETTE_HEIGHT) as i32
                {
                    let py = m.y.saturating_sub(RENDER_HEIGHT as i32 + 2) / 32;
                    let px = m.x / 32;
                    let palette_id = self.cpu.bus.ppu.palette_ids[(py * 16 + px) as usize];
                    format!("${:02X}", palette_id)
                } else {
                    String::new()
                };
                (tile, palette)
            } else {
                (String::new(), String::new())
            };
            s.text(p, &format!("Tile: {}", tile))?;
            p.y += ypad;
            s.text(p, &format!("Palette: {}", palette))?;
            // s.copy_window_draw_target(ppu_viewer_window, "ppu_info")?;
            // s.clear_draw_target();
        }
        Ok(())
    }

    pub(super) fn toggle_nt_viewer(&mut self, s: &mut PixState) -> NesResult<()> {
        self.nt_viewer = !self.nt_viewer;
        if self.nt_viewer {
            let info_height = 4 * 10;
            let window = s
                .create_window(2 * RENDER_WIDTH, 2 * RENDER_HEIGHT + info_height)
                .with_title("Nametable Viewer")
                .build()?;
            self.nt_viewer_window = Some(window);

            // Set up four NT windows
            let src = rect!(0, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let nt1_dst = rect!(0, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let nt2_dst = rect!(RENDER_WIDTH, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let nt3_dst = rect!(0, RENDER_HEIGHT, RENDER_WIDTH, RENDER_HEIGHT);
            let nt4_dst = rect!(RENDER_WIDTH, RENDER_HEIGHT, RENDER_WIDTH, RENDER_HEIGHT);
            // s.create_texture(window, "nametable1", ColorType::Rgba, src, nt1_dst)?;
            // s.create_texture(window, "nametable2", ColorType::Rgba, src, nt2_dst)?;
            // s.create_texture(window, "nametable3", ColorType::Rgba, src, nt3_dst)?;
            // s.create_texture(window, "nametable4", ColorType::Rgba, src, nt4_dst)?;

            // Set up 2 horizontal lines for scanline detection
            let src = rect!(0, 0, 2 * RENDER_WIDTH, RENDER_HEIGHT);
            let top_dst = rect!(0, 0, 2 * RENDER_WIDTH, RENDER_HEIGHT);
            let bot_dst = rect!(0, RENDER_HEIGHT, 2 * RENDER_WIDTH, RENDER_HEIGHT);
            // s.create_texture(window, "scanline_top", ColorType::Rgba, src, top_dst)?;
            // s.create_texture(window, "scanline_bot", ColorType::Rgba, src, bot_dst)?;

            // Set up info panel at the bottom
            let src = rect!(0, 0, 2 * RENDER_WIDTH, info_height);
            let dst = rect!(0, 2 * RENDER_HEIGHT, 2 * RENDER_WIDTH, info_height);
            // s.create_texture(window, "nt_info", ColorType::Rgb, src, dst)?;

            // Since debug may not have been enabled before, have PPU generate nametable data
            self.cpu.bus.ppu.update_debug();
        } else if let Some(nt_viewer_window) = self.nt_viewer_window {
            s.close_window(nt_viewer_window)?;
        }
        self.cpu
            .bus
            .ppu
            .set_debug(self.nt_viewer || self.ppu_viewer);
        Ok(())
    }

    pub(super) fn copy_nt_viewer(&mut self, s: &mut PixState) -> NesResult<()> {
        if let Some(nt_viewer_window) = self.nt_viewer_window {
            let nametables = &self.cpu.bus.ppu.nametables;
            // s.copy_texture(nt_viewer_window, "nametable1", &nametables[0])?;
            // s.copy_texture(nt_viewer_window, "nametable2", &nametables[1])?;
            // s.copy_texture(nt_viewer_window, "nametable3", &nametables[2])?;
            // s.copy_texture(nt_viewer_window, "nametable4", &nametables[3])?;

            // Draw scanlines
            let line = Image::new(2 * RENDER_WIDTH, RENDER_HEIGHT);
            // s.set_draw_target(line);
            s.fill(WHITE);
            s.line((0, self.nt_scanline, 2 * RENDER_WIDTH, self.nt_scanline))?;
            // s.copy_window_draw_target(nt_viewer_window, "scanline_top")?;
            // s.copy_window_draw_target(nt_viewer_window, "scanline_bot")?;
            // s.clear_draw_target();

            // Draw Borders
            let borders = Image::new(2 * RENDER_WIDTH, RENDER_HEIGHT);
            // s.set_draw_target(borders);
            s.fill(BLACK);
            s.line((0, RENDER_HEIGHT - 1, 2 * RENDER_WIDTH, RENDER_HEIGHT - 1))?;
            s.line((RENDER_WIDTH, 0, RENDER_WIDTH, RENDER_HEIGHT))?;
            // s.copy_window_draw_target(nt_viewer_window, "scanline_top")?;
            // s.copy_window_draw_target(nt_viewer_window, "scanline_bot")?;
            // s.clear_draw_target();

            // Draw info
            // s.set_draw_target(self.nt_info_image.clone());
            let mut p = point!(5, 5);
            let ypad = 10;

            let width = self.nt_info_image.width();
            let height = self.nt_info_image.height();
            s.rect((p, width - p.x as u32, height - p.y as u32))?;

            s.fill(WHITE);
            s.text(p, &format!("Scanline: {}", self.nt_scanline))?;
            p.y += ypad;
            let mirroring = self.cpu.bus.mapper.mirroring();
            s.text(p, &format!("Mirroring: {:?}", mirroring))?;
            p.x = RENDER_WIDTH as i32;
            p.y = 5;

            let m = s.mouse_pos();

            if self.focused_window == Some(nt_viewer_window)
                && m >= point!(0, 0)
                && m.x < 2 * (RENDER_WIDTH - 1) as i32
                && m.y < 2 * RENDER_HEIGHT as i32
            {
                let nt_addr = 0x2000
                    + (m.x / RENDER_WIDTH as i32) * 0x0400
                    + (m.y / RENDER_HEIGHT as i32) * 0x0800;
                let ppu_addr = nt_addr + ((((m.y / 8) % 30) << 5) | ((m.x / 8) % 32));
                let tile_id = self.cpu.bus.ppu.nametable_ids[(ppu_addr - 0x2000) as usize];

                s.text(p, &format!("Tile ID: ${:02X}", tile_id))?;
                p.y += ypad;
                s.text(p, &format!("(X, Y): {:?}", m))?;
                p.y += ypad;
                s.text(p, &format!("PPU Addr: ${:04X}", ppu_addr))?;
            } else {
                s.text(p, "Tile ID:")?;
                p.y += ypad;
                s.text(p, "X, Y:")?;
                p.y += ypad;
                s.text(p, "PPU Addr:")?;
            }
            // s.copy_window_draw_target(nt_viewer_window, "nt_info")?;
        }
        Ok(())
    }

    pub(super) fn set_nt_scanline(&mut self, scanline: u32) {
        let scanline = if scanline > RENDER_HEIGHT - 1 {
            RENDER_HEIGHT - 1
        } else {
            scanline
        };
        self.nt_scanline = scanline;
        self.cpu.bus.ppu.set_nt_scanline(self.nt_scanline as u16);
    }

    pub(super) fn set_pat_scanline(&mut self, scanline: u32) {
        let scanline = if scanline > RENDER_HEIGHT - 1 {
            RENDER_HEIGHT - 1
        } else {
            scanline
        };
        self.pat_scanline = scanline;
        self.cpu.bus.ppu.set_pat_scanline(self.pat_scanline as u16);
    }

    pub(super) fn toggle_debug(&mut self, s: &mut PixState) -> NesResult<()> {
        self.config.debug = !self.config.debug;
        self.paused(self.config.debug);
        let new_width = if self.config.debug {
            self.width + DEBUG_WIDTH
        } else {
            self.width
        };
        // s.set_screen_size(new_width, self.height)?;
        self.active_debug = true;
        self.draw_debug(s)?;
        Ok(())
    }

    pub(super) fn copy_debug(&mut self, s: &mut PixState) -> NesResult<()> {
        let pixels = self.debug_image.bytes();
        // s.copy_texture("debug", pixels)?;
        Ok(())
    }

    #[allow(clippy::many_single_char_names)]
    pub(super) fn draw_debug(&mut self, s: &mut PixState) -> NesResult<()> {
        let mut p = point!(5, 5);

        // s.set_draw_target(self.debug_image.clone());
        s.fill(DARK_GRAY);

        // Status Registers
        let cpu = &self.cpu;
        s.text(p, "Status:")?;

        let scolor = |f: &StatusRegs| {
            if cpu.status & *f as u8 > 0 {
                RED
            } else {
                GREEN
            }
        };

        let fxpad = 8; // Font x-padding
        let fypad = 10; // Font y-padding
        let ox = p.x + 8 * fxpad; // 8 chars from "Status: " * font padding
        use StatusRegs::*;
        for (i, status) in [N, V, U, B, D, I, C].iter().enumerate() {
            s.fill(scolor(status));
            s.text((ox + i as i32 * fxpad, p.y), &format!("{:?}", status))?;
        }

        let ppu = &self.cpu.bus.ppu;
        let cycles = format!("Cycles: {:8}", cpu.cycle_count);
        let seconds = format!("Seconds: {:7}", self.running_time.floor());
        let areg = format!("A: ${:02X} [{:03}]", cpu.acc, cpu.acc);
        let pc = format!("PC: ${:04X}", cpu.pc);
        let xreg = format!("X: ${:02X} [{:03}]", cpu.x, cpu.x);
        let yreg = format!("Y: ${:02X} [{:03}]", cpu.y, cpu.y);
        let stack = format!("Stack: $01{:02X}", cpu.sp);
        let vram = format!("Vram Addr: ${:04X}", ppu.read_ppuaddr());
        let spr = format!("Spr Addr: ${:02X}", ppu.read_oamaddr());
        let sl = i32::from(ppu.scanline) - 1;
        let cycsl = format!("Cycle: {:3}  Scanline: {:3}", ppu.cycle, sl);
        let m = s.mouse_pos() / self.config.scale as i32;
        let mouse = if m >= point!(0, 0) && m.x < WINDOW_WIDTH as i32 && m.y < RENDER_HEIGHT as i32
        {
            let mx = (m.x as f32 * 7.0 / 8.0) as u32;
            format!("Mouse: {:3}, {:3}", mx, m.y)
        } else {
            "Mouse:".to_string()
        };

        p.y += fypad;
        s.fill(WHITE);
        s.text(p, &cycles)?;
        p.y += fypad;
        s.text(p, &seconds)?;

        // PC, Acc, X, Y
        p.y += 2 * fypad;
        s.text(p, &pc)?;
        s.text((p.x + 13 * fxpad, p.y), &areg)?;
        p.y += fypad;
        s.text(p, &xreg)?;
        s.text((p.x + 13 * fxpad, p.y), &yreg)?;

        // Stack
        p.y += 2 * fypad;
        s.text(p, &stack)?;
        p.y += fypad;

        let bytes_per_row = 8;
        let xpad = 24; // Font x-padding
        let ypad = 10; // Font y-padding
        for (i, offset) in (0xE0..=0xFF).rev().enumerate() {
            let val = cpu.peek(0x0100 | offset);
            let x = p.x + (xpad * i as i32) % (bytes_per_row * xpad);
            let y = p.y + ypad * (i as i32 / bytes_per_row);
            s.text((x, y), &format!("{:02X} ", val))?;
        }

        // PPU
        p.y += ypad * 4 + fypad;
        s.text(p, &vram)?;
        p.y += fypad;
        s.text(p, &spr)?;
        p.y += fypad;
        s.text(p, &cycsl)?;
        p.y += fypad;
        s.text(p, &mouse)?;

        // Disassembly
        p.y += 2 * fypad;
        // Number of instructions to show
        let instr_count = std::cmp::min(30, (self.height - p.y as u32) as usize / 10);
        let pad = 10;
        let mut prev_count = 0;
        let instrs = cpu.pc_log.iter().take(instr_count / 2).rev();
        for pc in instrs {
            let mut pc = *pc;
            let disasm = cpu.disassemble(&mut pc);
            s.text(p, &disasm)?;
            p.y += pad;
            prev_count += 1;
        }
        let mut pc = cpu.pc;
        for i in 0..(instr_count - prev_count) {
            let color = if i == 0 { CYAN } else { WHITE };
            let opcode = cpu.peek(pc);
            let instr = INSTRUCTIONS[opcode as usize];
            let byte = cpu.peekw(pc.wrapping_add(1));
            let disasm = cpu.disassemble(&mut pc);
            s.fill(color);
            s.text(p, &disasm)?;
            p.y += pad;
            match instr.op() {
                JMP => {
                    pc = byte;
                    if cpu.instr.addr_mode() == IND {
                        if pc & 0x00FF == 0x00FF {
                            // Simulate bug
                            pc = (u16::from(cpu.peek(pc & 0xFF00)) << 8) | u16::from(cpu.peek(pc));
                        } else {
                            // Normal behavior
                            pc = (u16::from(cpu.peek(pc + 1)) << 8) | u16::from(cpu.peek(pc));
                        }
                    }
                }
                RTS | RTI => pc = cpu.peek_stackw().wrapping_add(1),
                _ => (),
            }
        }

        // CPU Memory TODO move this to Hex window
        // y += 2 * fypad;
        // let addr_start: u32 = 0x6000;
        // let addr_len: u32 = 0x00A0;
        // for addr in addr_start..addr_start + addr_len {
        //     let val = cpu.peek(addr as u16);
        //     s.text(
        //         x + (xpad * (addr - addr_start)) % (bytes_per_row * xpad),
        //         y + ypad * ((addr - addr_start) / bytes_per_row),
        //         &format!("{:02X} ", val),
        //         wh,
        //     );
        // }

        // s.clear_draw_target();
        Ok(())
    }

    pub(super) fn should_break(&self) -> bool {
        // let instr = self.cpu.next_instr();
        // TODO add breakpoint logic
        false
    }
}
