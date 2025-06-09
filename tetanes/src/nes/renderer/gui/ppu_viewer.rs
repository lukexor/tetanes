use crate::nes::{
    event::{DebugEvent, EmulationEvent, NesEventProxy},
    renderer::{
        gui::lib::{ViewportOptions, animated_dashed_rect},
        painter::RenderState,
        texture::Texture,
    },
};
use egui::{
    CentralPanel, Color32, Context, CursorIcon, DragValue, Grid, Image, Label, Pos2, Rect,
    ScrollArea, Sense, SidePanel, Slider, StrokeKind, TopBottomPanel, Ui, Vec2, ViewportClass,
    ViewportId, show_tooltip_at_pointer,
};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tetanes_core::{
    debug::PpuDebugger,
    ppu::{Ppu, scroll::Scroll, sprite::Sprite},
};

#[derive(Debug)]
#[must_use]
struct State {
    tx: NesEventProxy,
    tab: Tab,
    // TODO: persist in config
    refresh_cycle: u32,
    refresh_scanline: u32,
    show_refresh_lines: bool,
    show_dividers: bool,
    show_tile_grid: bool,
    show_scroll_overlay: bool,
    show_attr_grid_16x: bool,
    show_attr_grid_32x: bool,
    nametables: NametablesState,
    pattern_tables: PatternTablesState,
    oam: OamState,
    palette: PalettesState,
    ppu: Ppu,
}

#[derive(Debug)]
#[must_use]
struct NametablesState {
    pixels: Vec<u8>,
    texture: Texture,
    zoom: f32,
    selected: Option<Vec2>,
}

#[derive(Debug)]
#[must_use]
struct PatternTablesState {
    pixels: Vec<u8>,
    texture: Texture,
    zoom: f32,
    selected: Option<Vec2>,
}

#[derive(Debug)]
#[must_use]
struct OamState {
    oam_pixels: Vec<u8>,
    sprite_pixels: Vec<u8>,
    sprites: Vec<Sprite>,
    oam_texture: Texture,
    sprites_texture: Texture,
    zoom: f32,
    oam_selected: Option<Vec2>,
}

#[derive(Debug)]
#[must_use]
struct PalettesState {
    size: Vec2,
    pixels: Vec<u8>,
    colors: Vec<u8>,
    zoom: f32,
    selected: Option<Vec2>,
}

#[derive(Debug, Copy, Clone)]
#[must_use]
struct NametableTile {
    index: u16,
    uv: Rect,
    col: u16,
    row: u16,
    x: u32, // 0..=248
    y: u32, // 0..=232
    nametable_addr: u16,
    tile_addr: u16,
    palette_index: u8,
    palette_addr: u16,
    attr_addr: u16,
    attr_val: u8,
}

impl Default for NametableTile {
    fn default() -> Self {
        Self {
            index: 0,
            uv: Rect::NOTHING,
            col: 0,
            row: 0,
            x: 0,
            y: 0,
            nametable_addr: 0,
            tile_addr: 0,
            palette_index: 0,
            palette_addr: 0,
            attr_addr: 0,
            attr_val: 0,
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[must_use]
struct ChrTile {
    index: u16,
    uv: Rect,
    tile_addr: u16,
}

impl Default for ChrTile {
    fn default() -> Self {
        Self {
            index: 0,
            uv: Rect::NOTHING,
            tile_addr: 0,
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[must_use]
struct PaletteColor {
    index: u8,
    value: u8,
    addr: u16,
    color: Color32,
}

impl Default for PaletteColor {
    fn default() -> Self {
        Self {
            index: 0,
            value: 0,
            addr: 0,
            color: Color32::BLACK,
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct PpuViewer {
    id: ViewportId,
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Nametables,
    PatternTables,
    Oam,
    Palette,
}

impl PpuViewer {
    const TITLE: &'static str = "PPU Viewer";

    pub fn new(tx: NesEventProxy, render_state: &mut RenderState) -> Self {
        Self {
            id: ViewportId::from_hash_of(Self::TITLE),
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                tx,
                tab: Tab::default(),
                refresh_cycle: 0,
                refresh_scanline: Ppu::VBLANK_SCANLINE_NTSC,
                show_refresh_lines: false,
                show_dividers: true,
                show_tile_grid: false,
                show_scroll_overlay: false,
                show_attr_grid_16x: false,
                show_attr_grid_32x: false,
                nametables: NametablesState {
                    // 4 nametables with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 4 * Ppu::SIZE],
                    texture: Texture::new(
                        render_state,
                        2.0 * Vec2::new(Ppu::WIDTH as f32, Ppu::HEIGHT as f32),
                        1.0,
                        Some("nes nametables"),
                    ),
                    zoom: 1.5,
                    selected: None,
                },
                pattern_tables: PatternTablesState {
                    // 2 pattern tables with 4 color channels (RGBA)
                    pixels: vec![0x00; 2 * 4 * Ppu::SIZE],
                    texture: Texture::new(
                        render_state,
                        Vec2::new(Ppu::WIDTH as f32, Ppu::WIDTH as f32 / 2.0),
                        1.0,
                        Some("nes pattern tables"),
                    ),
                    zoom: 3.0,
                    selected: None,
                },
                oam: OamState {
                    // 64 8x8 sprites with 4 color channels (RGBA)
                    oam_pixels: vec![0x00; 64 * 8 * 8 * 4],
                    // 1 nametable with 4 color channels (RGBA)
                    sprite_pixels: vec![0x00; 4 * Ppu::SIZE],
                    // 64 sprites
                    sprites: vec![Sprite::new(); 64],
                    oam_texture: Texture::new(
                        render_state,
                        Vec2::splat(64.0),
                        1.0,
                        Some("nes oam"),
                    ),
                    sprites_texture: Texture::new(
                        render_state,
                        Vec2::new(Ppu::WIDTH as f32, Ppu::HEIGHT as f32),
                        1.0,
                        Some("nes sprites"),
                    ),
                    zoom: 3.0,
                    oam_selected: None,
                },
                palette: PalettesState {
                    // 2 palette tables
                    size: Vec2::new(64.0, 32.0),
                    // 32 palette colors with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 32],
                    // 32 colors
                    colors: vec![0x00; 32],
                    zoom: 3.0,
                    selected: None,
                },
                ppu: Ppu::default(),
            })),
        }
    }

    pub const fn id(&self) -> ViewportId {
        self.id
    }

    pub fn open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    pub fn set_open(&self, open: bool, ctx: &Context) {
        self.open.store(open, Ordering::Release);
        self.state.lock().update_debugger(self.open());
        if !self.open() {
            ctx.send_viewport_cmd_to(self.id, egui::ViewportCommand::Close);
        }
    }

    pub fn toggle_open(&self, ctx: &Context) {
        let _ = self
            .open
            .fetch_update(Ordering::Release, Ordering::Acquire, |open| Some(!open));
        self.state.lock().update_debugger(self.open());
        if !self.open() {
            ctx.send_viewport_cmd_to(self.id, egui::ViewportCommand::Close);
        }
    }

    pub fn update_ppu(&mut self, queue: &wgpu::Queue, ppu: Ppu) {
        let mut state = self.state.lock();
        match state.tab {
            Tab::Nametables => {
                ppu.load_nametables(&mut state.nametables.pixels);
                let mut pixels = std::mem::take(&mut state.palette.pixels);
                let mut colors = std::mem::take(&mut state.palette.colors);
                ppu.load_palettes(&mut pixels, &mut colors);
                state.palette.pixels = pixels;
                state.palette.colors = colors;
                state
                    .nametables
                    .texture
                    .update(queue, &state.nametables.pixels);
            }
            Tab::PatternTables => {
                ppu.load_pattern_tables(&mut state.pattern_tables.pixels);
                state
                    .pattern_tables
                    .texture
                    .update(queue, &state.pattern_tables.pixels);
            }
            Tab::Oam => {
                let mut oam_pixels = std::mem::take(&mut state.oam.oam_pixels);
                let mut sprite_pixels = std::mem::take(&mut state.oam.sprite_pixels);
                let mut sprites = std::mem::take(&mut state.oam.sprites);

                // Clear to black each frame
                sprite_pixels.chunks_mut(4).for_each(|chunk| {
                    chunk[0] = 0;
                    chunk[1] = 0;
                    chunk[2] = 0;
                    chunk[3] = 255;
                });
                ppu.load_oam(&mut oam_pixels, &mut sprite_pixels, &mut sprites);

                state.oam.oam_pixels = oam_pixels;
                state.oam.sprite_pixels = sprite_pixels;
                state.oam.sprites = sprites;

                state.oam.oam_texture.update(queue, &state.oam.oam_pixels);
                state
                    .oam
                    .sprites_texture
                    .update(queue, &state.oam.sprite_pixels);
            }
            Tab::Palette => {
                let mut pixels = std::mem::take(&mut state.palette.pixels);
                let mut colors = std::mem::take(&mut state.palette.colors);
                ppu.load_palettes(&mut pixels, &mut colors);
                state.palette.pixels = pixels;
                state.palette.colors = colors;
            }
        }
        state.ppu = ppu;
    }

    pub fn show(&mut self, ctx: &Context, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);

        let mut viewport_builder = egui::ViewportBuilder::default()
            .with_title(Self::TITLE)
            .with_inner_size(Vec2::new(1024.0, 768.0));
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        ctx.show_viewport_deferred(self.id, viewport_builder, move |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(PpuViewer::TITLE)
                    .open(&mut window_open)
                    .show(ctx, |ui| state.lock().ui(ui, opts.enabled));
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ctx, |ui| state.lock().ui(ui, opts.enabled));
                if ctx.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
        });
    }
}

impl State {
    fn update_debugger(&self, open: bool) {
        let tx = self.tx.clone();
        let debugger = PpuDebugger {
            cycle: self.refresh_cycle,
            scanline: self.refresh_scanline,
            callback: Arc::new(move |ppu| tx.event(DebugEvent::Ppu(Box::new(ppu)))),
        };
        self.tx.event(if open {
            EmulationEvent::AddDebugger(debugger.into())
        } else {
            EmulationEvent::RemoveDebugger(debugger.into())
        });
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            TopBottomPanel::top("ppu_viewer_menubar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Nametables, "Nametables");
                    ui.selectable_value(&mut self.tab, Tab::PatternTables, "Pattern Tables");
                    ui.selectable_value(&mut self.tab, Tab::Oam, "OAM");
                    ui.selectable_value(&mut self.tab, Tab::Palette, "Palette");
                });
            });

            match self.tab {
                Tab::Nametables => self.nametables_tab(ui),
                Tab::PatternTables => self.pattern_tables_tab(ui),
                Tab::Oam => self.oam_tab(ui),
                Tab::Palette => self.palette_tab(ui),
            }
        });
    }

    fn grid_settings(&mut self, ui: &mut Ui) {
        let res = ui
            .checkbox(&mut self.show_dividers, "Table Dividers")
            .on_hover_text("Show divider lines between tables.");
        if res.changed() {
            // TODO: update config
        }

        let res = ui
            .checkbox(&mut self.show_tile_grid, "Tile Grid")
            .on_hover_text("Show grid lines between tiles.");
        if res.changed() {
            // TODO: update config
        }
    }

    fn general_settings(&mut self, ui: &mut Ui) {
        ui.strong("Refresh on:")
            .on_hover_cursor(CursorIcon::Help)
            .on_hover_text("Change which PPU cycle/scanline viewer state refreshes on.");

        ui.indent("refresh_settings", |ui| {
            ui.horizontal(|ui| {
                let drag = DragValue::new(&mut self.refresh_cycle)
                    .range(0..=Ppu::CYCLE_END)
                    .suffix(" cycle");
                let res = ui.add(drag);
                if res.changed() {
                    self.update_debugger(true);
                }
            });

            ui.horizontal(|ui| {
                let drag = DragValue::new(&mut self.refresh_scanline)
                    .range(0..=self.ppu.prerender_scanline)
                    .suffix(" scanline");
                let res = ui.add(drag);
                if res.changed() {
                    self.update_debugger(true);
                }
            });
        });
    }

    fn nametables_tab(&mut self, ui: &mut Ui) {
        SidePanel::right("nametable_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading("Nametable Info");
                ui.separator();

                let grid = Grid::new("nametables_info")
                    .num_columns(2)
                    .spacing([40.0, 6.0]);
                grid.show(ui, |ui| {
                    ui.strong("Mirroring:");
                    ui.label(format!("{:?}", self.ppu.mirroring()));
                    ui.end_row();
                });

                ui.add_space(16.0);
                ui.heading("Selected Tile");
                ui.separator();
                self.nametable_tile(ui, "nametable_tile_selected", self.nametables.selected);

                ui.add_space(16.0);
                ui.separator();

                ui.collapsing("Settings", |ui| {
                    self.general_settings(ui);

                    let res = ui
                        .checkbox(&mut self.show_refresh_lines, "Refresh Markers")
                        .on_hover_text(
                            "Show lines indicating the current refresh cycle and scanline.",
                        );
                    if res.changed() {
                        // TODO: update config
                    }

                    self.grid_settings(ui);

                    let res = ui
                        .checkbox(&mut self.show_scroll_overlay, "Scroll Overlay")
                        .on_hover_text("Show scroll position overlay.");
                    if res.changed() {
                        // TODO: update config
                    }

                    let res = ui
                        .checkbox(&mut self.show_attr_grid_16x, "Attribute Grid (16x16)")
                        .on_hover_text("Show grid lines within each attribute block.");
                    if res.changed() {
                        // TODO: update config
                    }

                    let res = ui
                        .checkbox(&mut self.show_attr_grid_32x, "Attribute Grid (32x32)")
                        .on_hover_text("Show grid lines between attribute blocks.");
                    if res.changed() {
                        // TODO: update config
                    }

                    zoom_slider(ui, &mut self.nametables.zoom);
                });
            });
        });

        let texture_size = self.nametables.texture.size;
        CentralPanel::default().show_inside(ui, |ui| {
            let scroll = ScrollArea::both()
                .min_scrolled_width(texture_size.x)
                .min_scrolled_height(texture_size.y);
            scroll.show(ui, |ui| {
                let image = Image::from_texture(self.nametables.texture.sized())
                    .fit_to_exact_size(self.nametables.zoom * texture_size)
                    .sense(Sense::click());

                let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                let image_rect = res.rect;

                if let Some(pos) = res.hover_pos()
                    && image_rect.contains(pos) {
                        self.nametable_hover(ui, &res, pos);
                    }

                if self.show_dividers {
                    // Split the 4x4 nametables in half vertically and horizontally
                    ui.painter().vline(
                        image_rect.center().x,
                        image_rect.y_range(),
                        (1.0, Color32::WHITE),
                    );
                    ui.painter().hline(
                        image_rect.x_range(),
                        image_rect.center().y,
                        (1.0, Color32::WHITE),
                    );
                }

                if self.show_refresh_lines {
                    let cycle_offset = self.refresh_cycle as f32 * image_rect.size().x
                        / 2.0
                        / Ppu::CYCLE_END as f32;
                    let scanline_offset = self.refresh_scanline as f32 * image_rect.size().y
                        / 2.0
                        / self.ppu.prerender_scanline as f32;
                    ui.painter().vline(
                        image_rect.left() + cycle_offset,
                        image_rect.y_range(),
                        (1.0, Color32::RED),
                    );
                    ui.painter().vline(
                        image_rect.center().x + cycle_offset,
                        image_rect.y_range(),
                        (1.0, Color32::RED),
                    );
                    ui.painter().hline(
                        image_rect.x_range(),
                        image_rect.top() + scanline_offset,
                        (1.0, Color32::GREEN),
                    );
                    ui.painter().hline(
                        image_rect.x_range(),
                        image_rect.center().y + scanline_offset,
                        (1.0, Color32::GREEN),
                    );
                }

                if self.show_tile_grid {
                    paint_grid(ui, image_rect, 60.0, 64.0, Color32::LIGHT_BLUE);
                }

                if self.show_attr_grid_16x {
                    paint_grid(ui, image_rect, 30.0, 32.0, Color32::LIGHT_RED);
                }

                if self.show_attr_grid_32x {
                    // Because 32x doesn't divide evenly into 240, split this up into two passes with a
                    // dividing line, forcing the leftover attribute space to be at the bottom. Also
                    // halve the number of rows
                    let top_rect = Rect::from_min_max(image_rect.min, image_rect.right_center());
                    let bot_rect =
                        Rect::from_min_max(image_rect.left_center(), image_rect.right_bottom());

                    paint_grid(ui, top_rect, 7.5, 16.0, Color32::LIGHT_GREEN);
                    ui.painter().hline(
                        top_rect.x_range(),
                        top_rect.bottom(),
                        (1.0, Color32::LIGHT_GREEN),
                    );
                    paint_grid(ui, bot_rect, 7.5, 16.0, Color32::LIGHT_GREEN);
                }

                if self.show_scroll_overlay {
                    self.nametable_scroll_overlay(ui, image_rect);
                }

                if let Some(offset) = self.nametables.selected {
                    let selection =
                        tile_selection(image_rect, self.nametables.texture.size, offset);
                    animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);
                }
            });
        });
    }

    fn nametable_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.nametables.texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size);
        let selection = tile_selection(image_rect, texture_size, offset);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
            self.nametable_tile(ui, "nametable_tile_hover", Some(offset));
        });
        if res.clicked() {
            self.nametables.selected = Some(offset);
        }
    }

    fn nametable_tile_from_offset(&self, offset: Vec2, texture_size: Vec2) -> NametableTile {
        let Vec2 { x, y } = offset;

        // Get row/column 8x8 tile and the nametable it's in
        let mut col = x as u16 / 8;
        let mut row = y as u16 / 8;
        let nametable = if col >= 32 { 1 } else { 0 } | if row >= 30 { 2 } else { 0 };

        // Wrap row/column to a single nametable
        col &= 31;
        if row >= 30 {
            // Not a power of two, so can't bitwise &
            row -= 30;
        }

        let nametable_index = (row << 5) + col;
        let base_nametable_addr = Ppu::NT_START | (nametable * Ppu::NT_SIZE);
        let base_attr_addr = base_nametable_addr + Ppu::ATTR_OFFSET;

        let nametable_addr = base_nametable_addr + nametable_index;
        let tile_index = u16::from(self.ppu.bus.peek_ciram(nametable_addr));
        let tile_addr = self.ppu.ctrl.bg_select + (tile_index << 4);

        let supertile = ((row & 0xFC) << 1) + (col >> 2);
        let attr_addr = base_attr_addr + supertile;
        let attr_val = self.ppu.bus.peek_ciram(attr_addr);

        let attr_shift = (col & 0x02) | ((row & 0x02) << 1);
        // TODO: handle mmc5 extended attributes
        let palette_addr = ((attr_val >> attr_shift) & 0x03) << 2;
        let palette_index = palette_addr >> 2;
        let palette_addr = Ppu::PALETTE_START + u16::from(palette_addr);

        let tile_uv = Rect::from_min_size(
            (Vec2::new(x, y) / texture_size).to_pos2(),
            Vec2::splat(8.0) / texture_size,
        );

        let x = (x as u32) % Ppu::WIDTH;
        let y = (y as u32) % Ppu::HEIGHT;

        NametableTile {
            index: tile_index,
            uv: tile_uv,
            col,
            row,
            x,
            y,
            nametable_addr,
            tile_addr,
            palette_index,
            palette_addr,
            attr_addr,
            attr_val,
        }
    }

    fn nametable_tile(&mut self, ui: &mut Ui, label: &str, offset: Option<Vec2>) {
        let tile = offset
            .map(|offset| self.nametable_tile_from_offset(offset, self.nametables.texture.size));
        let NametableTile {
            uv,
            index,
            col,
            row,
            x,
            y,
            nametable_addr,
            tile_addr,
            palette_index,
            palette_addr,
            attr_addr,
            attr_val,
            ..
        } = tile.unwrap_or_default();

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Tile:");
            let tile_image = Image::from_texture(self.nametables.texture.sized())
                .uv(uv)
                .maintain_aspect_ratio(false) // Ignore original aspect ratio
                .fit_to_exact_size(Vec2::splat(64.0))
                .sense(Sense::click());
            ui.add(tile_image);
            ui.end_row();

            ui.strong("Palette:");
            if tile.is_some() {
                self.palette_row(
                    ui,
                    palette_index.into(),
                    ui.cursor().min,
                    Vec2::splat(16.0),
                    true,
                );
            }
            ui.end_row();

            ui.strong("Column, Row:");
            if tile.is_some() {
                ui.label(format!("{col}, {row}"));
            }
            ui.end_row();

            ui.strong("X, Y:");
            if tile.is_some() {
                ui.label(format!("{x}, {y}"));
            }
            ui.end_row();

            ui.strong("Nametable Address:");
            if tile.is_some() {
                ui.label(format!("${nametable_addr:04X}"));
            }
            ui.end_row();

            ui.strong("Tile Index:");
            if tile.is_some() {
                ui.label(format!("${index:02X}"));
            }
            ui.end_row();

            ui.strong("Tile Address:");
            if tile.is_some() {
                ui.label(format!("${tile_addr:04X}"));
            }
            ui.end_row();

            ui.strong("Palette Index:");
            if tile.is_some() {
                ui.label(format!("{palette_index}"));
            }
            ui.end_row();

            ui.strong("Palette Address:");
            if tile.is_some() {
                ui.label(format!("${palette_addr:04X}"));
            }
            ui.end_row();

            ui.strong("Attribute Address:");
            if tile.is_some() {
                ui.label(format!("${attr_addr:04X}"));
            }
            ui.end_row();

            ui.strong("Attribute Value:");
            if tile.is_some() {
                ui.label(format!("${attr_val:02X}"));
            }
            ui.end_row();
        });
    }

    fn nametable_scroll_overlay(&self, ui: &mut Ui, image_rect: Rect) {
        let Ppu {
            cycle,
            scanline,
            vblank_scanline,
            prerender_scanline,
            scroll,
            ..
        } = self.ppu;
        let use_scroll_t = scanline >= vblank_scanline
            || (scanline == Ppu::VISIBLE_SCANLINE_END && cycle >= Ppu::SPR_EVAL_END)
            || (scanline == prerender_scanline && cycle < Ppu::BG_PREFETCH_START + 7);
        let scroll_v = if use_scroll_t { scroll.t } else { scroll.v };

        let mut scroll_x = ((scroll_v & Scroll::COARSE_X_MASK) << 3)
            | (((scroll_v & Scroll::NT_X_MASK) >> 10) * Ppu::WIDTH as u16);
        let scroll_y = ((scroll_v & Scroll::COARSE_Y_MASK) >> 2)
            | (((scroll_v & Scroll::NT_Y_MASK) >> 11) * Ppu::HEIGHT as u16)
            | ((scroll_v & Scroll::FINE_Y_MASK) >> 12);

        if use_scroll_t {
            scroll_x |= scroll.fine_x;
        } else {
            // During rendering, subtract according to current cycle/scanline
            if cycle <= Ppu::VISIBLE_END {
                if cycle >= 8 {
                    scroll_x = scroll_x.saturating_sub((cycle & !0x07) as u16);
                }
                // Adjust for 2x increments at end of last scanline
                scroll_x = scroll_x.saturating_sub(16);
            } else if cycle >= Ppu::BG_PREFETCH_START + 7 {
                scroll_x = scroll_x.saturating_sub(8);
                if cycle >= Ppu::BG_PREFETCH_END {
                    scroll_x = scroll_x.saturating_sub(8);
                }
            }
            scroll_x += scroll.fine_x;
        }

        // Scroll overlay
        let nametable_size = image_rect.size() / 2.0;
        // Translate scroll_x/scroll_y to image space
        let scroll = Vec2::new(scroll_x as f32, scroll_y as f32) * image_rect.size()
            / self.nametables.texture.size;
        let scroll_min = image_rect.min + scroll;
        let scroll_max = scroll_min + nametable_size;
        let overlay = Rect::from_min_max(scroll_min, scroll_max.min(image_rect.max));
        ui.painter().rect(
            overlay,
            0.0,
            Color32::from_black_alpha(75),
            (1.0, Color32::WHITE),
            egui::StrokeKind::Inside,
        );

        // Wrap overlay around the right/bottom edge
        let Vec2 { x, y } = scroll_max - image_rect.max;
        let wrapped_size = Vec2::new(
            if x > 0.0 { x } else { nametable_size.x },
            if y > 0.0 { y } else { nametable_size.y },
        );
        if wrapped_size.max_elem() > 0.0 {
            ui.painter().rect(
                Rect::from_min_size(image_rect.min, wrapped_size),
                0.0,
                Color32::from_black_alpha(75),
                (1.0, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn pattern_tables_tab(&mut self, ui: &mut Ui) {
        SidePanel::right("pattern_tables_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading("Selected Tile");
                ui.separator();
                self.pattern_tables_tile(
                    ui,
                    "pattern_tables_tile_selected",
                    self.pattern_tables.selected,
                );

                ui.add_space(16.0);
                ui.separator();

                ui.collapsing("Settings", |ui| {
                    self.general_settings(ui);
                    self.grid_settings(ui);
                    // TODO: Selectable palette/last known palette
                    zoom_slider(ui, &mut self.pattern_tables.zoom);
                });
            });
        });

        let texture_size = self.pattern_tables.texture.size;
        CentralPanel::default().show_inside(ui, |ui| {
            let scroll = ScrollArea::both()
                .min_scrolled_width(texture_size.x)
                .min_scrolled_height(texture_size.y);
            scroll.show(ui, |ui| {
                let image = Image::from_texture(self.pattern_tables.texture.sized())
                    .fit_to_exact_size(self.pattern_tables.zoom * texture_size)
                    .sense(Sense::click());

                let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                let image_rect = res.rect;

                if let Some(pos) = res.hover_pos()
                    && image_rect.contains(pos) {
                        self.pattern_tables_hover(ui, &res, pos);
                    }

                if self.show_dividers {
                    ui.painter().vline(
                        image_rect.center().x,
                        image_rect.y_range(),
                        (1.0, Color32::WHITE),
                    );
                }

                if self.show_tile_grid {
                    paint_grid(ui, image_rect, 16.0, 32.0, Color32::LIGHT_BLUE);
                }

                if let Some(offset) = self.pattern_tables.selected {
                    let selection =
                        tile_selection(image_rect, self.pattern_tables.texture.size, offset);
                    animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);
                }
            });
        });
    }

    fn pattern_tables_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.pattern_tables.texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size);
        let selection = tile_selection(image_rect, texture_size, offset);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
            self.pattern_tables_tile(ui, "pattern_tables_tile_hover", Some(offset));
        });
        if res.clicked() {
            self.pattern_tables.selected = Some(offset);
        }
    }

    fn pattern_chr_tile_from_offset(&self, offset: Vec2, texture_size: Vec2) -> ChrTile {
        let Vec2 { x, y } = offset;

        // Get row/column 8x8 tile and the pattern table it's in
        let mut col = x as u16 / 8;
        let row = y as u16 / 8;
        let pattern_table = if col >= 16 { 1 } else { 0 };

        // Wrap column to a single pattern table
        col &= 15;

        let tile_uv = Rect::from_min_size(
            (Vec2::new(x, y) / texture_size).to_pos2(),
            Vec2::splat(8.0) / texture_size,
        );
        let tile_addr = (pattern_table << 12) | ((col + (row << 4)) << 4);

        ChrTile {
            index: (tile_addr >> 4) & 0xFF,
            uv: tile_uv,
            tile_addr,
        }
    }

    fn pattern_tables_tile(&mut self, ui: &mut Ui, label: &str, offset: Option<Vec2>) {
        let tile = offset.map(|offset| {
            self.pattern_chr_tile_from_offset(offset, self.pattern_tables.texture.size)
        });
        let ChrTile {
            uv,
            index,
            tile_addr,
            ..
        } = tile.unwrap_or_default();

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Tile:");
            let tile_image = Image::from_texture(self.pattern_tables.texture.sized())
                .uv(uv)
                .maintain_aspect_ratio(false) // Ignore original aspect ratio
                .fit_to_exact_size(Vec2::splat(64.0))
                .sense(Sense::click());
            ui.add(tile_image);
            ui.end_row();

            ui.strong("Tile Index:");
            if tile.is_some() {
                ui.label(format!("${index:02X}"));
            }
            ui.end_row();

            ui.strong("Tile Address:");
            if tile.is_some() {
                ui.label(format!("${tile_addr:04X}"));
            }
            ui.end_row();
        });
    }

    fn oam_tab(&mut self, ui: &mut Ui) {
        SidePanel::right("oam_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading("Selected Tile");
                ui.separator();
                self.oam_tile(ui, "oam_selected", self.oam.oam_selected);

                ui.add_space(16.0);
                ui.separator();

                ui.collapsing("Settings", |ui| {
                    self.general_settings(ui);

                    let res = ui
                        .checkbox(&mut self.show_tile_grid, "Tile Grid")
                        .on_hover_text("Show grid lines between tiles.");
                    if res.changed() {
                        // TODO: update config
                    }

                    zoom_slider(ui, &mut self.oam.zoom);
                });
            });
        });

        CentralPanel::default().show_inside(ui, |ui| {
            let scroll = ScrollArea::both()
                .min_scrolled_width(self.oam.oam_texture.size.x)
                .min_scrolled_height(self.oam.oam_texture.size.y);
            scroll.show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Draw OAM tiles
                    let image = Image::from_texture(self.oam.oam_texture.sized())
                        .fit_to_exact_size(2.0 * self.oam.zoom * self.oam.oam_texture.size)
                        .sense(Sense::click());

                    let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                    let oam_image_rect = res.rect;

                    if let Some(pos) = res.hover_pos()
                        && oam_image_rect.contains(pos) {
                            self.oam_hover(ui, &res, pos);
                        }

                    if self.show_tile_grid {
                        paint_grid(ui, oam_image_rect, 8.0, 8.0, Color32::LIGHT_BLUE);
                    }

                    let image = Image::from_texture(self.oam.sprites_texture.sized())
                        // match OAM size
                        .shrink_to_fit()
                        .sense(Sense::click());

                    let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                    let spr_image_rect = res.rect;

                    if let Some(pos) = res.hover_pos()
                        && spr_image_rect.contains(pos) {
                            self.sprites_hover(ui, &res, pos);
                        }

                    if self.show_tile_grid {
                        paint_grid(ui, spr_image_rect, 30.0, 32.0, Color32::LIGHT_BLUE);
                    }

                    if let Some(offset) = self.oam.oam_selected {
                        let selection =
                            tile_selection(oam_image_rect, self.oam.oam_texture.size, offset);
                        animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);

                        let sprite_index =
                            (offset.x / 8.0) as usize + (offset.y / 8.0) as usize * 8;
                        let sprite = self.oam.sprites.get(sprite_index);
                        if let Some(sprite) = sprite {
                            let offset = Vec2::new(
                                ((sprite.x as f32) / 8.0).floor() * 8.0,
                                ((sprite.y as f32) / 8.0).floor() * 8.0,
                            );
                            if offset.x < Ppu::WIDTH as f32 && offset.y < Ppu::HEIGHT as f32 {
                                let selection = tile_selection(
                                    spr_image_rect,
                                    self.oam.sprites_texture.size,
                                    offset,
                                );
                                animated_dashed_rect(
                                    ui,
                                    selection,
                                    (1.0, Color32::WHITE),
                                    3.0,
                                    3.0,
                                );
                            }
                        }
                    }
                });
            });
        });
    }

    fn oam_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.oam.oam_texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size);
        let selection = tile_selection(image_rect, texture_size, offset);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        let sprite_index = (offset.x / 8.0) as usize + (offset.y / 8.0) as usize * 8;
        let sprite = self.oam.sprites.get(sprite_index);
        if sprite.is_some() {
            show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
                self.oam_tile(ui, "oam_hover", Some(offset));
            });
            if res.clicked() {
                self.oam.oam_selected = Some(offset);
            }
        }
    }

    fn sprites_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.oam.sprites_texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size);
        let selection = tile_selection(image_rect, texture_size, offset);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        let sprite_index = self.oam.sprites.iter().position(|sprite| {
            let grid_x = sprite.x as f32 / 8.0;
            let grid_y = sprite.y as f32 / 8.0;
            let x_min = grid_x.floor() * 8.0;
            let x_max = grid_x.ceil() * 8.0;
            let y_min = grid_y.floor() * 8.0;
            let y_max = grid_y.ceil() * 8.0;
            (x_min..=x_max).contains(&offset.x) && (y_min..=y_max).contains(&offset.y)
        });
        if let Some(index) = sprite_index {
            let offset = Vec2::new((index % 8) as f32, (index / 8) as f32) * 8.0;

            show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
                self.oam_tile(ui, "oam_hover", Some(offset));
            });
            if res.clicked() {
                self.oam.oam_selected = Some(offset);
            }
        }
    }

    fn oam_tile(&mut self, ui: &mut Ui, label: &str, offsets: Option<Vec2>) {
        let tile =
            offsets.map(|offset| self.oam_tile_from_offset(offset, self.oam.oam_texture.size));
        let ChrTile {
            uv,
            index,
            tile_addr,
            ..
        } = tile.unwrap_or_default();

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Tile:");
            let tile_image = Image::from_texture(self.oam.oam_texture.sized())
                .uv(uv)
                .maintain_aspect_ratio(false) // Ignore original aspect ratio
                .fit_to_exact_size(Vec2::splat(64.0))
                .sense(Sense::click());
            ui.add(tile_image);
            ui.end_row();

            ui.strong("Tile Index:");
            if tile.is_some() {
                ui.label(format!("${index:02X}"));
            }
            ui.end_row();

            ui.strong("Tile Address:");
            if tile.is_some() {
                ui.label(format!("${tile_addr:04X}"));
            }
            ui.end_row();

            // TODO: sprite index, palete address, position, horizontal/vertical flip/backgroud
            // priority, palette row
        });
    }

    fn oam_tile_from_offset(&self, offset: Vec2, texture_size: Vec2) -> ChrTile {
        let Vec2 { x, y } = offset;

        // Get row/column 8x8 tile
        let col = x as u16 / 8;
        let row = y as u16 / 8;

        let tile_uv = Rect::from_min_size(
            (Vec2::new(x, y) / texture_size).to_pos2(),
            Vec2::splat(8.0) / texture_size,
        );
        let index = col + (row * 8);
        ChrTile {
            index,
            uv: tile_uv,
            tile_addr: self.oam.sprites[index as usize].tile_addr,
        }
    }

    fn palette_tab(&mut self, ui: &mut Ui) {
        SidePanel::right("palette_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading("Selected Color");
                ui.separator();
                self.palette(ui, "palette_info_selected", self.palette.selected);
            });
        });

        CentralPanel::default().show_inside(ui, |ui| {
            ScrollArea::both().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let res = self
                        .palette_grid(ui, 4.0 * self.palette.zoom * self.palette.size)
                        .on_hover_cursor(CursorIcon::Cell);
                    let palette_rect = res.rect;

                    if let Some(pos) = res.hover_pos()
                        && palette_rect.contains(pos) {
                            self.palette_hover(ui, &res, pos);
                        }

                    if let Some(offset) = self.palette.selected {
                        let selection = tile_selection(palette_rect, self.palette.size, offset);
                        animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);
                    }
                });
            });
        });
    }

    fn palette_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;

        let offset = translate_screen_pos_to_tile(pos, image_rect, self.palette.size);
        let selection = tile_selection(image_rect, self.palette.size, offset);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
            self.palette(ui, "palette_hover", Some(offset));
        });
        if res.clicked() {
            self.palette.selected = Some(offset);
        }
    }

    fn palette_color_from_offset(&self, offset: Vec2) -> PaletteColor {
        let Vec2 { x, y } = offset;

        // Get row/column 32x32 palette and the palette table it's in
        let mut col = x as u16 / 8;
        let row = y as u16 / 8;
        let palette = if col >= 4 { 1 } else { 0 };

        // Wrap column to a single palette table
        col &= 3;

        let index = col + row * 4;
        let color_index = palette * 0x10 + index;
        let pixel_idx = color_index as usize * 4;
        PaletteColor {
            index: index as u8,
            addr: Ppu::PALETTE_START + color_index,
            value: self.palette.colors[color_index as usize],
            color: if let [red, green, blue] = self.palette.pixels[pixel_idx..pixel_idx + 3] {
                Color32::from_rgb(red, green, blue)
            } else {
                Color32::default()
            },
        }
    }

    fn palette(&mut self, ui: &mut Ui, label: &str, offset: Option<Vec2>) {
        let palette = offset.map(|offset| self.palette_color_from_offset(offset));
        let PaletteColor {
            index,
            value,
            color,
            addr,
            ..
        } = palette.unwrap_or_default();

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Color:");
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            ui.painter().rect_filled(rect, 1.0, color);
            ui.end_row();

            ui.strong("Index:");
            if palette.is_some() {
                ui.label(format!("${index:02X}"));
            }
            ui.end_row();

            ui.strong("Value:");
            if palette.is_some() {
                ui.label(format!("${value:02X}"));
            }
            ui.end_row();

            ui.strong("Palette Address:");
            if palette.is_some() {
                ui.label(format!("${addr:02X}"));
            }
            ui.end_row();

            ui.strong("Hex:");
            if palette.is_some() {
                ui.label(&color.to_hex()[0..7]); // Truncate the alpha channel
            }
            ui.end_row();

            ui.strong("RGB:");
            if palette.is_some() {
                let (r, g, b, _) = &color.to_tuple();
                ui.label(format!("({r:03}, {g:03}, {b:03})"));
            }
            ui.end_row();
        });
    }

    fn palette_row(&self, ui: &mut Ui, index: usize, pos: Pos2, size: Vec2, show_backdrop: bool) {
        for x in 0..4 {
            let mut idx = (index * 4 + x) * 4;
            if show_backdrop && x == 0 {
                idx = 0;
            }
            if let [red, green, blue] = self.palette.pixels[idx..idx + 3] {
                let pos = pos + Vec2::new(x as f32 * size.x, 0.0);
                let rect = Rect::from_min_max(pos, pos + size);
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_rgb(red, green, blue));
            }
        }
    }

    fn palette_grid(&self, ui: &mut Ui, size: Vec2) -> egui::Response {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let res = ui.add(Label::new("Background"));
                ui.add_space(size.x / 2.0 - res.rect.width());
                ui.add(Label::new("Sprites"));
            });

            let (rect, res) = ui.allocate_exact_size(size, Sense::click());
            ui.painter()
                .rect_stroke(rect, 0.0, (1.0, Color32::BLACK), StrokeKind::Inside);

            let size = Vec2::new(size.x / 8.0, size.y / 4.0);
            for offset in [0, 4] {
                for (y, index) in (offset..offset + 4).enumerate() {
                    let pos =
                        rect.min + Vec2::new(offset as f32 * size.x, y as f32 * size.y).floor();
                    self.palette_row(ui, index, pos, size, false);
                }
            }

            res
        })
        .inner
    }
}

/// A Zoom slider
fn zoom_slider(ui: &mut Ui, zoom: &mut f32) {
    ui.horizontal(|ui| {
        let drag = Slider::new(zoom, 0.1..=5.0).step_by(0.05).suffix("x");
        let res = ui.add(drag);
        if res.changed() {
            // TODO: update config
        }
        ui.label("Zoom")
            .on_hover_cursor(CursorIcon::Help)
            .on_hover_text("Zoom preview in or out.");
    });
}

/// A grid overlay.
fn paint_grid(ui: &mut Ui, rect: Rect, y_spacing: f32, x_spacing: f32, color: Color32) {
    let min = rect.min;
    let max = rect.max;
    let size = rect.size();
    let x_increment = size.x / x_spacing;
    let mut x = min.x + x_increment;
    while x < max.x {
        ui.painter().vline(x, rect.y_range(), (1.0, color));
        x += x_increment;
    }

    let y_increment = size.y / y_spacing;
    let mut y = min.y + y_increment;
    while y < max.y {
        ui.painter().hline(rect.x_range(), y, (1.0, color));
        y += y_increment;
    }
}

/// Translate position in screen space to texture space and find containing 8x8 tile offset
fn translate_screen_pos_to_tile(pos: Pos2, image_rect: Rect, texture_size: Vec2) -> Vec2 {
    let normalized_pos = (pos - image_rect.min) / image_rect.size();
    let texture_pos = normalized_pos * texture_size;
    (texture_pos / 8.0).floor() * 8.0
}

/// Return tile selection rectangle given an offset.
fn tile_selection(image_rect: Rect, texture_size: Vec2, tile_offset: Vec2) -> Rect {
    let scale = image_rect.size() / texture_size;
    Rect::from_min_size(
        image_rect.min + scale * tile_offset,
        scale * Vec2::splat(8.0),
    )
}
