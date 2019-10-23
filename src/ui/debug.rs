use crate::{
    cpu::{AddrMode::*, Operation::*, StatusRegs, INSTRUCTIONS},
    memory::Memory,
    ppu::{RENDER_HEIGHT, RENDER_WIDTH},
    ui::Ui,
    NesResult,
};
use pix_engine::{
    draw::Rect,
    pixel::{self, ColorType},
    sprite::Sprite,
    StateData,
};

const PALETTE_HEIGHT: u32 = 64;
pub(super) const DEBUG_WIDTH: u32 = 350;

impl Ui {
    pub(super) fn toggle_ppu_viewer(&mut self, data: &mut StateData) -> NesResult<()> {
        self.ppu_viewer = !self.ppu_viewer;
        if self.ppu_viewer {
            let info_height = 4 * 10;
            let window = data.open_window(
                "PPU Viewer",
                2 * RENDER_WIDTH,
                RENDER_HEIGHT + PALETTE_HEIGHT + info_height,
            )?;
            self.ppu_viewer_window = Some(window);

            // Set up two side-by-side textures for each palette plane
            let src = Rect::new(0, 0, RENDER_WIDTH / 2, RENDER_HEIGHT / 2);
            let left_dst = Rect::new(0, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let right_dst = Rect::new(RENDER_WIDTH, 0, RENDER_WIDTH, RENDER_HEIGHT);
            data.create_texture(window, "left_pattern", ColorType::Rgb, src, left_dst)?;
            data.create_texture(window, "right_pattern", ColorType::Rgb, src, right_dst)?;

            // Set up palette texture
            let src = Rect::new(0, 0, 16, 2);
            let dst = Rect::new(0, RENDER_HEIGHT, 2 * RENDER_WIDTH, PALETTE_HEIGHT);
            data.create_texture(window, "palette", ColorType::Rgb, src, dst)?;

            // Set up info panel at the bottom
            let src = Rect::new(0, 0, 2 * RENDER_WIDTH, info_height);
            let dst = Rect::new(
                0,
                RENDER_HEIGHT + PALETTE_HEIGHT,
                2 * RENDER_WIDTH,
                info_height,
            );
            data.create_texture(window, "ppu_info", ColorType::Rgb, src, dst)?;

            // Since debug may not have been enabled before, have PPU generate nametable data
            self.cpu.bus.ppu.update_debug();
        } else if let Some(ppu_viewer_window) = self.ppu_viewer_window {
            data.close_window(ppu_viewer_window);
        }
        self.cpu.bus.ppu.debug(self.nt_viewer || self.ppu_viewer);
        Ok(())
    }

    pub(super) fn copy_ppu_viewer(&mut self, data: &mut StateData) -> NesResult<()> {
        if let Some(ppu_viewer_window) = self.ppu_viewer_window {
            // Set up patterns
            let pat_tables = self.pattern_tables();
            data.copy_texture(ppu_viewer_window, "left_pattern", &pat_tables[0])?;
            data.copy_texture(ppu_viewer_window, "right_pattern", &pat_tables[1])?;

            // Set up palette
            let palette = self.palette();
            data.copy_texture(ppu_viewer_window, "palette", &palette)?;

            // Set up info
            let wh = pixel::WHITE;
            let mut info = Sprite::rgb(2 * RENDER_WIDTH, 4 * 10);
            data.set_draw_target(&mut info);
            let x = 5;
            let mut y = 5;
            let ypad = 10;
            data.draw_string(x, y, &format!("Scanline: {}", self.pat_scanline), wh);
            y += ypad;
            // TODO translate mouse coords into tile and palette IDs
            data.draw_string(x, y, &format!("Tile ID: ${:02X}", 0), wh);
            y += ypad;
            data.draw_string(x, y, &format!("Palette: ${:02X}", 0), wh);
            data.copy_draw_target(ppu_viewer_window, "ppu_info")?;
            data.clear_draw_target();
        }
        Ok(())
    }

    pub(super) fn toggle_nt_viewer(&mut self, data: &mut StateData) -> NesResult<()> {
        self.nt_viewer = !self.nt_viewer;
        if self.nt_viewer {
            let info_height = 4 * 10;
            let window = data.open_window(
                "Nametable Viewer",
                2 * RENDER_WIDTH,
                2 * RENDER_HEIGHT + info_height,
            )?;
            self.nt_viewer_window = Some(window);

            // Set up four NT windows
            let src = Rect::new(0, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let nt1_dst = src;
            let nt2_dst = Rect::new(RENDER_WIDTH, 0, RENDER_WIDTH, RENDER_HEIGHT);
            let nt3_dst = Rect::new(0, RENDER_HEIGHT, RENDER_WIDTH, RENDER_HEIGHT);
            let nt4_dst = Rect::new(RENDER_WIDTH, RENDER_HEIGHT, RENDER_WIDTH, RENDER_HEIGHT);
            data.create_texture(window, "nametable1", ColorType::Rgb, src, nt1_dst)?;
            data.create_texture(window, "nametable2", ColorType::Rgb, src, nt2_dst)?;
            data.create_texture(window, "nametable3", ColorType::Rgb, src, nt3_dst)?;
            data.create_texture(window, "nametable4", ColorType::Rgb, src, nt4_dst)?;

            // Set up 2 horizontal lines for scanline detection
            let src = Rect::new(0, 0, 2 * RENDER_WIDTH, RENDER_HEIGHT);
            let top_dst = src;
            let bot_dst = Rect::new(0, RENDER_HEIGHT, 2 * RENDER_WIDTH, RENDER_HEIGHT);
            data.create_texture(window, "scanline_top", ColorType::Rgba, src, top_dst)?;
            data.create_texture(window, "scanline_bot", ColorType::Rgba, src, bot_dst)?;

            // Set up info panel at the bottom
            let src = Rect::new(0, 0, 2 * RENDER_WIDTH, info_height);
            let dst = Rect::new(0, 2 * RENDER_HEIGHT, 2 * RENDER_WIDTH, info_height);
            data.create_texture(window, "nt_info", ColorType::Rgb, src, dst)?;

            // Since debug may not have been enabled before, have PPU generate nametable data
            self.cpu.bus.ppu.update_debug();
        } else if let Some(nt_viewer_window) = self.nt_viewer_window {
            data.close_window(nt_viewer_window);
        }
        self.cpu.bus.ppu.debug(self.nt_viewer || self.ppu_viewer);
        Ok(())
    }

    pub(super) fn copy_nt_viewer(&mut self, data: &mut StateData) -> NesResult<()> {
        if let Some(nt_viewer_window) = self.nt_viewer_window {
            let wh = pixel::WHITE;

            let nametables = self.nametables();
            data.copy_texture(nt_viewer_window, "nametable1", &nametables[0])?;
            data.copy_texture(nt_viewer_window, "nametable2", &nametables[1])?;
            data.copy_texture(nt_viewer_window, "nametable3", &nametables[2])?;
            data.copy_texture(nt_viewer_window, "nametable4", &nametables[3])?;

            // Draw scanlines
            let mut line = Sprite::new(2 * RENDER_WIDTH, RENDER_HEIGHT);
            data.set_draw_target(&mut line);
            data.draw_line(0, self.nt_scanline, 2 * RENDER_WIDTH, self.nt_scanline, wh);
            data.copy_draw_target(nt_viewer_window, "scanline_top")?;
            data.copy_draw_target(nt_viewer_window, "scanline_bot")?;
            data.clear_draw_target();

            // Draw info
            let mut info = Sprite::rgb(2 * RENDER_WIDTH, 4 * 10);
            data.set_draw_target(&mut info);
            let mut x = 5;
            let mut y = 5;
            let ypad = 10;
            data.draw_string(x, y, &format!("Scanline: {}", self.nt_scanline), wh);
            y += ypad;
            if let Some(mapper) = &self.cpu.bus.mapper {
                let mirroring = mapper.borrow().mirroring();
                data.draw_string(x, y, &format!("Mirroring: {:?}", mirroring), wh);
            } else {
                data.draw_string(x, y, "Mirroring: N/A", wh);
            };
            x = RENDER_WIDTH;
            y = 5;
            // TODO translate mouse coords into IDs, X, Y and calc PPU addr
            data.draw_string(x, y, &format!("Tile ID: ${:02X}", 0), wh);
            y += ypad;
            data.draw_string(x, y, &format!("(X, Y): ({}, {})", 0, 0), wh);
            y += ypad;
            data.draw_string(x, y, &format!("PPU Addr: ${:04X}", 0), wh);
            data.copy_draw_target(nt_viewer_window, "nt_info")?;
            data.clear_draw_target();
        }
        Ok(())
    }

    pub(super) fn set_nt_scanline(&mut self, scanline: u32) {
        if Some(self.focused_window) == self.nt_viewer_window {
            let scanline = if scanline > RENDER_HEIGHT - 1 {
                RENDER_HEIGHT - 1
            } else {
                scanline
            };
            self.nt_scanline = scanline;
            self.cpu.bus.ppu.set_nt_scanline(self.nt_scanline as u16);
        }
    }

    pub(super) fn set_pat_scanline(&mut self, scanline: u32) {
        if Some(self.focused_window) == self.ppu_viewer_window {
            let scanline = if scanline > RENDER_HEIGHT - 1 {
                RENDER_HEIGHT - 1
            } else {
                scanline
            };
            self.pat_scanline = scanline;
            self.cpu.bus.ppu.set_pat_scanline(self.pat_scanline as u16);
        }
    }

    pub(super) fn toggle_debug(&mut self, data: &mut StateData) -> NesResult<()> {
        self.settings.debug = !self.settings.debug;
        self.paused(self.settings.debug);
        if self.settings.debug {
            self.width += DEBUG_WIDTH;
        } else {
            self.width -= DEBUG_WIDTH;
        }
        data.set_screen_size(self.width, self.height)?;
        self.draw_debug(data);
        Ok(())
    }

    pub(super) fn copy_debug(&mut self, data: &mut StateData) -> NesResult<()> {
        let pixels = self.debug_sprite.bytes();
        data.copy_texture(1, "debug", &pixels)?;
        Ok(())
    }

    pub(super) fn draw_debug(&mut self, data: &mut StateData) {
        let x = 5;
        let mut y = 5;
        let wh = pixel::WHITE;

        data.set_draw_target(&mut self.debug_sprite);
        data.fill(pixel::VERY_DARK_GRAY);

        // Status Registers
        let cpu = &self.cpu;
        data.draw_string(x, y, "Status:", wh);

        let scolor = |f| {
            if cpu.status & f as u8 > 0 {
                pixel::RED
            } else {
                pixel::GREEN
            }
        };

        let fxpad = 8; // Font x-padding
        let fypad = 10; // Font y-padding
        let ox = x + 8 * fxpad; // 8 chars from "Status: " * font padding
        data.draw_string(ox, y, "N", scolor(StatusRegs::N));
        data.draw_string(ox + fxpad, y, "V", scolor(StatusRegs::V));
        data.draw_string(ox + 2 * fxpad, y, "-", scolor(StatusRegs::U));
        data.draw_string(ox + 3 * fxpad, y, "B", scolor(StatusRegs::B));
        data.draw_string(ox + 4 * fxpad, y, "D", scolor(StatusRegs::D));
        data.draw_string(ox + 5 * fxpad, y, "I", scolor(StatusRegs::I));
        data.draw_string(ox + 6 * fxpad, y, "Z", scolor(StatusRegs::Z));
        data.draw_string(ox + 7 * fxpad, y, "C", scolor(StatusRegs::C));

        let ppu = &self.cpu.bus.ppu;
        let cycles = format!("Cycles: {:8}", cpu.cycle_count);
        let seconds = format!("Seconds: {:7}", self.clock.floor());
        let areg = format!("A: ${:02X} [{:03}]", cpu.acc, cpu.acc);
        let pc = format!("PC: ${:04X}", cpu.pc);
        let xreg = format!("X: ${:02X} [{:03}]", cpu.x, cpu.x);
        let yreg = format!("Y: ${:02X} [{:03}]", cpu.y, cpu.y);
        let irqs = format!("Pending IRQs: 0b{:03b}", cpu.pending_irq);
        let nmis = format!("Pending NMI: {}", cpu.pending_nmi);
        let stack = format!("Stack: $01{:02X}", cpu.sp);
        let vram = format!("Vram Addr: ${:04X}", ppu.read_ppuaddr());
        let spr = format!("Spr Addr: ${:02X}", ppu.read_oamaddr());
        let sl = i32::from(ppu.scanline) - 1;
        let cycsl = format!("Cycle: {:3}  Scanline: {:3}", ppu.cycle, sl);

        y += fypad;
        data.draw_string(x, y, &cycles, wh);
        y += fypad;
        data.draw_string(x, y, &seconds, wh);

        // PC, Acc, X, Y
        y += 2 * fypad;
        data.draw_string(x, y, &pc, wh);
        data.draw_string(x + 13 * fxpad, y, &areg, wh);
        y += fypad;
        data.draw_string(x, y, &xreg, wh);
        data.draw_string(x + 13 * fxpad, y, &yreg, wh);

        // IRQs
        y += 2 * fypad;
        data.draw_string(x, y, &irqs, wh);
        y += fypad;
        data.draw_string(x, y, &nmis, wh);

        // Stack
        y += 2 * fypad;
        data.draw_string(x, y, &stack, wh);
        y += fypad;

        let bytes_per_row = 8;
        let xpad = 24; // Font x-padding
        let ypad = 10; // Font y-padding
        for (i, offset) in (0xE0..=0xFF).rev().enumerate() {
            let val = cpu.peek(0x0100 | offset);
            let x = x + (xpad * i as u32) % (bytes_per_row * xpad);
            let y = y + ypad * (i as u32 / bytes_per_row);
            data.draw_string(x, y, &format!("{:02X} ", val), wh);
        }

        // PPU
        y += ypad * 4 + fypad;
        data.draw_string(x, y, &vram, wh);
        y += fypad;
        data.draw_string(x, y, &spr, wh);
        y += fypad;
        data.draw_string(x, y, &cycsl, wh);

        // Disassembly
        y += 2 * fypad;
        // Number of instructions to show
        let instr_count = std::cmp::min(30, (self.height - y) as usize / 10);
        let pad = 10;
        let mut prev_count = 0;
        let instrs = cpu.pc_log.iter().take(instr_count / 2).rev();
        for pc in instrs {
            let mut pc = *pc;
            let disasm = cpu.disassemble(&mut pc);
            data.draw_string(x, y, &disasm, wh);
            y += pad;
            prev_count += 1;
        }
        let mut pc = cpu.pc;
        for i in 0..(instr_count - prev_count) {
            let color = if i == 0 { pixel::CYAN } else { wh };
            let opcode = cpu.peek(pc);
            let instr = INSTRUCTIONS[opcode as usize];
            let byte = cpu.peekw(pc.wrapping_add(1));
            let disasm = cpu.disassemble(&mut pc);
            data.draw_string(x, y, &disasm, color);
            y += pad;
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
        //     data.draw_string(
        //         x + (xpad * (addr - addr_start)) % (bytes_per_row * xpad),
        //         y + ypad * ((addr - addr_start) / bytes_per_row),
        //         &format!("{:02X} ", val),
        //         wh,
        //     );
        // }

        data.clear_draw_target();
    }

    pub(super) fn should_break(&self) -> bool {
        // let instr = self.cpu.next_instr();
        // TODO
        false
    }
}
