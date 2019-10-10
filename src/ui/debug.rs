use crate::{
    console::{cpu::StatusRegs, RENDER_HEIGHT, RENDER_WIDTH},
    memory::Memory,
    ui::{Ui, WINDOW_HEIGHT, WINDOW_WIDTH},
};
use pix_engine::{
    draw::Rect,
    pixel::{self, ColorType, Sprite},
    GenericImageView, StateData,
};

const DEBUG_WIDTH: u32 = WINDOW_WIDTH / 2;
const DEBUG_HEIGHT: u32 = WINDOW_HEIGHT;

impl Ui {
    pub(super) fn toggle_debug(&mut self, data: &mut StateData) {
        if self.settings.scale == 1 {
            return;
        }
        self.settings.debug = !self.settings.debug;

        // Adjust window width and create textures if necessary
        let debug_width = DEBUG_WIDTH * self.settings.scale;
        let debug_height = DEBUG_HEIGHT * self.settings.scale;

        if self.settings.debug {
            self.width += debug_width;
            data.create_texture(
                "cpu_debug",
                ColorType::RGBA,
                Rect::new(0, 0, debug_width, debug_height),
                Rect::new(self.width - debug_width, 0, debug_width, debug_height),
            );
            data.create_texture(
                "ppu_nt",
                ColorType::RGB,
                Rect::new(0, 0, RENDER_WIDTH / 2, RENDER_HEIGHT / 2),
                Rect::new(0, 0, 0, 0),
            );
        } else {
            self.width -= debug_width;
        }
        data.set_screen_size(self.width, self.height);
        self.console.debug(self.settings.debug);
        self.draw_cpu_debug(data);
        self.console.cpu.mem.ppu.update_debug();
    }

    pub(super) fn copy_cpu_debug(&mut self, data: &mut StateData) {
        if let Some(debug_sprite) = &self.debug_sprite {
            let pixels = &debug_sprite.raw_pixels();
            data.copy_texture("cpu_debug", pixels);

            let pat_height = RENDER_HEIGHT * self.settings.scale / 3;

            // Pattern tables
            let pat0 = Rect::new(
                self.width - debug_sprite.width(),
                self.height - pat_height,
                debug_sprite.width() / 2,
                pat_height,
            );
            let pat1 = Rect::new(
                self.width - debug_sprite.width() / 2,
                self.height - pat_height,
                debug_sprite.width() / 2,
                pat_height,
            );

            let pat_tables = self.console.pattern_tables();
            data.copy_texture_dst("ppu_nt", pat0, &pat_tables[0]);
            data.copy_texture_dst("ppu_nt", pat1, &pat_tables[1]);
        }
    }

    pub(super) fn draw_cpu_debug(&mut self, data: &mut StateData) {
        let font_scaler = |s| match s {
            s if s > 1 => s - 1,
            _ => 1,
        };

        let font_scale = font_scaler(self.settings.scale);
        let small_scale = font_scaler(font_scale);

        let x = 5 * font_scale;
        let mut y = 5 * font_scale;
        let wh = pixel::WHITE;
        let debug_width = DEBUG_WIDTH * self.settings.scale;
        let debug_height = DEBUG_HEIGHT * self.settings.scale;

        data.set_font_scale(font_scale);
        if self.debug_sprite.is_none() {
            let sprite = Sprite::new_rgba8(debug_width, debug_height);
            data.set_draw_target(sprite);
        } else {
            data.set_draw_target(self.debug_sprite.take().unwrap());
        }
        data.fill(pixel::VERY_DARK_GRAY);

        // Status Registers
        let cpu = &self.console.cpu;
        data.draw_string(x, y, "Status:", wh);

        let scolor = |f| {
            if cpu.status & f as u8 > 0 {
                pixel::RED
            } else {
                pixel::GREEN
            }
        };

        let fxpad = 8 * font_scale; // Font x-padding
        let fypad = 10 * font_scale; // Font y-padding
        let ox = x + 8 * fxpad; // 8 chars from "Status: " * font padding
        data.draw_string(ox, y, "N", scolor(StatusRegs::N));
        data.draw_string(ox + fxpad, y, "V", scolor(StatusRegs::V));
        data.draw_string(ox + 2 * fxpad, y, "-", scolor(StatusRegs::U));
        data.draw_string(ox + 3 * fxpad, y, "B", scolor(StatusRegs::B));
        data.draw_string(ox + 4 * fxpad, y, "D", scolor(StatusRegs::D));
        data.draw_string(ox + 5 * fxpad, y, "I", scolor(StatusRegs::I));
        data.draw_string(ox + 6 * fxpad, y, "Z", scolor(StatusRegs::Z));
        data.draw_string(ox + 7 * fxpad, y, "C", scolor(StatusRegs::C));

        y += fypad;
        data.draw_string(x, y, &format!("Cycles: {:8}", cpu.cycle_count), wh);

        // PC, Acc, X, Y
        y += 2 * fypad;
        data.draw_string(x, y, &format!("PC: ${:04X}", cpu.pc), wh);
        data.draw_string(
            x + 13 * fxpad,
            y,
            &format!("A: ${:02X} [{:03}]", cpu.acc, cpu.acc),
            wh,
        );
        y += fypad;
        data.draw_string(x, y, &format!("X: ${:02X} [{:03}]", cpu.x, cpu.x), wh);
        data.draw_string(
            x + 13 * fxpad,
            y,
            &format!("Y: ${:02X} [{:03}]", cpu.y, cpu.y),
            wh,
        );

        // Stack
        y += 2 * fypad;
        data.draw_string(x, y, &format!("Stack: $01{:02X}", cpu.sp), wh);
        y += fypad;

        data.set_font_scale(small_scale);
        let bytes_per_row = 8;
        let xpad = 24 * small_scale; // Font x-padding
        let ypad = 10 * small_scale; // Font y-padding
        for offset in 0..32u32 {
            let val = cpu.peek(0x0100 + offset as u16);
            data.draw_string(
                x + (xpad * offset) % (bytes_per_row * xpad),
                y + ypad * (offset / bytes_per_row),
                &format!("{:02X} ", val),
                wh,
            );
        }
        data.set_font_scale(font_scale);

        // PPU
        let ppu = &self.console.cpu.mem.ppu;
        y += ypad * 4 + fypad;
        data.draw_string(x, y, &format!("PPU: ${:04X}", ppu.read_ppuaddr()), wh);
        data.draw_string(
            x + 12 * fxpad,
            y,
            &format!("Sprite: ${:02X}", ppu.read_oamaddr()),
            wh,
        );
        y += fypad;
        data.draw_string(
            x,
            y,
            &format!(
                "Dot: {:3}  Scanline: {:3}",
                ppu.cycle,
                i32::from(ppu.scanline) - 1
            ),
            wh,
        );

        // Disassembly
        data.set_font_scale(small_scale);

        // Number of instructions to show
        let instr_count = 15 * small_scale as usize;
        y += 2 * fypad;
        let pad = 10 * small_scale;
        let mut prev_count = 0;
        for pc in cpu.pc_log.iter().take(instr_count / 2) {
            let mut pc = *pc;
            let disasm = cpu.disassemble(&mut pc);
            data.draw_string(x, y, &disasm, wh);
            y += pad;
            prev_count += 1;
        }
        let mut pc = cpu.pc;
        for i in 0..(instr_count - prev_count) {
            let color = if i == 0 { pixel::CYAN } else { wh };
            let disasm = cpu.disassemble(&mut pc);
            data.draw_string(x, y, &disasm, color);
            y += pad;
        }
        data.set_font_scale(font_scale);
        self.debug_sprite = Some(data.take_draw_target().unwrap());
    }
}
