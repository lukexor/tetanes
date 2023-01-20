use crate::{
    mem::{Access, Mem},
    nes::Nes,
    ppu::{scroll::PpuScroll, Mirroring, Ppu},
};
use pix_engine::prelude::*;

#[derive(Debug)]
pub(crate) struct PpuViewer {
    window_id: WindowId,
    texture_id: TextureId,
    mirroring: Mirroring,
    scanline: u32,
    nametables: [Vec<u8>; 4],
    nametable_ids: Vec<u8>,
    pattern_tables: [Vec<u8>; 2],
    palette: [u8; Self::PALETTE_SIZE],
    palette_ids: [u8; Self::PALETTE_SIZE],
}

impl PpuViewer {
    const NAMETABLE_SIZE: usize = 4 * Ppu::SIZE;
    const NAMETABLE_IDS_SIZE: usize = 4 * Ppu::NT_SIZE as usize;
    const PATTERN_SIZE: usize = 4 * (Ppu::WIDTH * Ppu::WIDTH) as usize / 2;
    const PALETTE_SIZE: usize = (32 + 4) * 4;
    const PALETTE_HEIGHT: i32 = 64;

    fn new(window_id: WindowId, texture_id: TextureId) -> Self {
        Self {
            window_id,
            texture_id,
            mirroring: Mirroring::default(),
            scanline: 0,
            nametables: [
                vec![0x00; Self::NAMETABLE_SIZE],
                vec![0x00; Self::NAMETABLE_SIZE],
                vec![0x00; Self::NAMETABLE_SIZE],
                vec![0x00; Self::NAMETABLE_SIZE],
            ],
            nametable_ids: vec![0; Self::NAMETABLE_IDS_SIZE],
            pattern_tables: [
                vec![0x00; Self::PATTERN_SIZE],
                vec![0x00; Self::PATTERN_SIZE],
            ],
            palette: [0; Self::PALETTE_SIZE],
            palette_ids: [0; Self::PALETTE_SIZE],
        }
    }

    pub(crate) const fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub(crate) const fn texture_id(&self) -> TextureId {
        self.texture_id
    }

    pub(crate) const fn scanline(&self) -> u32 {
        self.scanline
    }

    pub(crate) fn inc_scanline(&mut self, increment: u32) {
        self.scanline = (self.scanline + increment).clamp(0, Ppu::HEIGHT - 1);
    }

    pub(crate) fn dec_scanline(&mut self, decrement: u32) {
        self.scanline = self.scanline.saturating_sub(decrement);
    }

    pub(crate) fn load_nametables(&mut self, ppu: &Ppu) {
        self.mirroring = ppu.mirroring();
        for (i, nametable) in self.nametables.iter_mut().enumerate() {
            let base_addr = Ppu::NT_START + (i as u16) * Ppu::NT_SIZE;
            for addr in base_addr..(base_addr + Ppu::NT_SIZE - 64) {
                let x_scroll = addr & PpuScroll::COARSE_X_MASK;
                let y_scroll = (addr & PpuScroll::COARSE_Y_MASK) >> 5;

                let nt_base_addr =
                    Ppu::NT_START + (addr & (PpuScroll::NT_X_MASK | PpuScroll::NT_Y_MASK));
                let tile = ppu.peek(addr, Access::Dummy);
                let tile_addr = ppu.ctrl().bg_select() + u16::from(tile) * 16;
                let supertile = (x_scroll / 4) + (y_scroll / 4) * 8;
                let attr = u16::from(ppu.peek(nt_base_addr + 0x03C0 + supertile, Access::Dummy));
                let corner = ((x_scroll % 4) / 2 + (y_scroll % 4) / 2 * 2) << 1;
                let mask = 0x03 << corner;
                let palette = (attr & mask) >> corner;

                let tile_num = x_scroll + y_scroll * 32;
                let tile_x = (tile_num % 32) * 8;
                let tile_y = (tile_num / 32) * 8;

                self.nametable_ids[(addr - Ppu::NT_START) as usize] = tile;
                for y in 0..8 {
                    let lo = u16::from(ppu.peek(tile_addr + y, Access::Dummy));
                    let hi = u16::from(ppu.peek(tile_addr + y + 8, Access::Dummy));
                    for x in 0..8 {
                        let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                        let palette_idx =
                            ppu.peek(Ppu::PALETTE_START + palette * 4 + pix_type, Access::Dummy);
                        let x = u32::from(tile_x + (7 - x));
                        let y = u32::from(tile_y + y);
                        Self::set_pixel(palette_idx.into(), x, y, Ppu::WIDTH, nametable);
                    }
                }
            }
        }
    }

    pub(crate) fn load_pattern_tables(&mut self, ppu: &Ppu) {
        let width = Ppu::WIDTH / 2;
        for (i, pattern_table) in self.pattern_tables.iter_mut().enumerate() {
            let start = (i as u16) * 0x1000;
            let end = start + 0x1000;
            for tile_addr in (start..end).step_by(16) {
                let tile_x = ((tile_addr % 0x1000) % 256) / 2;
                let tile_y = ((tile_addr % 0x1000) / 256) * 8;
                for y in 0..8 {
                    let lo = u16::from(ppu.peek(tile_addr + y, Access::Dummy));
                    let hi = u16::from(ppu.peek(tile_addr + y + 8, Access::Dummy));
                    for x in 0..8 {
                        let pix_type = ((lo >> x) & 1) + (((hi >> x) & 1) << 1);
                        let palette_idx = ppu.peek(Ppu::PALETTE_START + pix_type, Access::Dummy);
                        let x = u32::from(tile_x + (7 - x));
                        let y = u32::from(tile_y + y);
                        Self::set_pixel(palette_idx.into(), x, y, width, pattern_table);
                    }
                }
            }
        }
    }

    pub(crate) fn load_palettes(&mut self, ppu: &Ppu) {
        // Global  // BG 0 ----------------------------------  // Unused    // SPR 0 -------------------------------
        // 0x3F00: 0,0  0x3F01: 1,0  0x3F02: 2,0  0x3F03: 3,0  0x3F10: 5,0  0x3F11: 6,0  0x3F12: 7,0  0x3F13: 8,0
        // Unused  // BG 1 ----------------------------------  // Unused    // SPR 1 -------------------------------
        // 0x3F04: 0,1  0x3F05: 1,1  0x3F06: 2,1  0x3F07: 3,1  0x3F14: 5,1  0x3F15: 6,1  0x3F16: 7,1  0x3F17: 8,1
        // Unused  // BG 2 ----------------------------------  // Unused    // SPR 2 -------------------------------
        // 0x3F08: 0,2  0x3F09: 1,2  0x3F0A: 2,2  0x3F0B: 3,2  0x3F18: 5,2  0x3F19: 6,2  0x3F1A: 7,2  0x3F1B: 8,2
        // Unused  // BG 3 ----------------------------------  // Unused    // SPR 3 -------------------------------
        // 0x3F0C: 0,3  0x3F0D: 1,3  0x3F0E: 2,3  0x3F0F: 3,3  0x3F1C: 5,3  0x3F1D: 6,3  0x3F1E: 7,3  0x3F1F: 8,3
        let width = 16;
        for addr in Ppu::PALETTE_START..Ppu::PALETTE_END {
            let x = u32::from((addr - Ppu::PALETTE_START) % 16);
            let y = u32::from((addr - Ppu::PALETTE_START) / 16);
            let palette_idx = ppu.peek(addr, Access::Dummy);
            self.palette_ids[y as usize * width + x as usize] = palette_idx;
            Self::set_pixel(palette_idx.into(), x, y, width as u32, &mut self.palette);
        }
    }

    fn set_pixel(color: u16, x: u32, y: u32, width: u32, pixels: &mut [u8]) {
        let (red, green, blue) = Ppu::system_palette(color);
        let idx = 4 * (x + y * width) as usize;
        assert!(idx + 2 < pixels.len());
        pixels[idx] = red;
        pixels[idx + 1] = green;
        pixels[idx + 2] = blue;
    }
}

impl Nes {
    pub(crate) fn toggle_ppu_viewer(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.ppu_viewer {
            None => {
                let w = 4 * Ppu::WIDTH + 30; // right padding
                let h = 3 * Ppu::HEIGHT;
                let window_id = s
                    .window()
                    .dimensions(w, h)
                    .title("PPU Viewer")
                    .position(10, 10)
                    .resizable()
                    .build()?;
                s.set_window_target(window_id)?;
                let texture_id = s.create_texture(w, h, PixelFormat::Rgba)?;
                self.ppu_viewer = Some(PpuViewer::new(window_id, texture_id));
                s.reset_window_target();
            }
            Some(ref viewer) => {
                s.close_window(viewer.window_id())?;
                self.ppu_viewer = None;
            }
        }
        Ok(())
    }

    pub(crate) fn render_ppu_viewer(&mut self, s: &mut PixState) -> PixResult<()> {
        if let Some(ref viewer) = self.ppu_viewer {
            s.set_window_target(viewer.window_id())?;
            s.clear()?;
            s.fill(Color::WHITE);
            s.stroke(None);

            let width = Ppu::WIDTH as i32;
            let height = Ppu::HEIGHT as i32;
            let m = s.mouse_pos();

            // Nametables

            let nametables = &viewer.nametables;
            let nametable1 = rect![0, 0, width, height];
            let nametable2 = rect![width, 0, width, height];
            let nametable3 = rect![0, height, width, height];
            let nametable4 = rect![width, height, width, height];
            let nametable_src = rect![0, 0, 2 * width, 2 * height];
            let nametable_dst = rect![10, 10, 2 * width, 2 * height];
            let nametable_pitch = 4 * Ppu::WIDTH as usize;

            let texture_id = viewer.texture_id();
            s.update_texture(texture_id, nametable1, &nametables[0], nametable_pitch)?;
            s.update_texture(texture_id, nametable2, &nametables[1], nametable_pitch)?;
            s.update_texture(texture_id, nametable3, &nametables[2], nametable_pitch)?;
            s.update_texture(texture_id, nametable4, &nametables[3], nametable_pitch)?;
            s.texture(texture_id, nametable_src, nametable_dst)?;

            // Scanline
            let scanline = viewer.scanline as i32;
            s.push();
            s.stroke(Color::WHITE);
            s.stroke_weight(2);
            s.line([10, scanline + 10, 2 * width + 10, scanline + 10])?;
            s.line([
                10,
                scanline + height + 10,
                2 * width + 10,
                scanline + height + 10,
            ])?;
            s.pop();

            // Nametable Info

            s.set_cursor_pos([s.cursor_pos().x(), nametable_dst.bottom() + 4]);

            s.text(&format!("Scanline: {}", viewer.scanline))?;
            s.text(&format!("Mirroring: {:?}", viewer.mirroring))?;

            if s.focused_window(viewer.window_id())
                && rect![0, 0, 2 * width, 2 * height].contains(m)
            {
                let x = m.x() - nametable_src.x();
                let y = m.y() - nametable_src.y();
                let nt_addr = (x / width) * 0x0400 + (y / height) * 0x0800;
                let ppu_addr = nt_addr + ((((y / 8) % 30) << 5) | ((x / 8) % 32));
                let tile_id = viewer.nametable_ids.get(ppu_addr as usize).unwrap_or(&0x00);
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

            let patterns = &viewer.pattern_tables;
            let pattern_x = nametable_dst.right() + 8;
            let pattern_w = Ppu::WIDTH as i32 / 2;
            let pattern_h = pattern_w;
            let pattern_left = rect![pattern_x, 0, pattern_w, pattern_h];
            let pattern_right = rect![pattern_x + pattern_w, 0, pattern_w, pattern_h];
            let pattern_src = rect![pattern_x, 0, 2 * pattern_w, pattern_h];
            let pattern_dst = rect![pattern_x, 10, 4 * pattern_w, 2 * pattern_h];
            let pattern_pitch = 4 * pattern_w as usize;
            s.update_texture(texture_id, pattern_left, &patterns[0], pattern_pitch)?;
            s.update_texture(texture_id, pattern_right, &patterns[1], pattern_pitch)?;
            s.texture(texture_id, pattern_src, pattern_dst)?;

            // Palette

            let palette = &viewer.palette;
            let palette_w = 16;
            let palette_h = 2;
            let palette_src = rect![0, pattern_src.bottom(), palette_w, palette_h];
            let palette_dst = rect![
                pattern_x,
                pattern_dst.bottom(),
                2 * width,
                PpuViewer::PALETTE_HEIGHT
            ];
            let palette_pitch = 4 * palette_w as usize;
            s.update_texture(texture_id, palette_src, palette, palette_pitch)?;
            s.texture(texture_id, palette_src, palette_dst)?;

            // Borders

            s.push();

            s.stroke(Color::DIM_GRAY);
            s.fill(None);
            s.stroke_weight(2);

            s.rect(nametable1.offset([10, 10]))?;
            s.rect(nametable2.offset([10, 10]))?;
            s.rect(nametable3.offset([10, 10]))?;
            s.rect(nametable4.offset([10, 10]))?;
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
                let palette = viewer
                    .palette_ids
                    .get((py * 16 + px) as usize)
                    .unwrap_or(&0x00);
                s.text(&format!("Palette: ${:02X}", palette))?;
            } else {
                s.text("Palette: $00")?;
            }

            s.reset_column_offset();
            s.reset_window_target();
        }
        Ok(())
    }
}
