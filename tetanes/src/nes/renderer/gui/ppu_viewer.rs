use crate::nes::{
    config::Config,
    event::{DebugEvent, EmulationEvent, NesEventProxy, Response},
    renderer::{
        gui::lib::{dashed_rect, ViewportOptions},
        painter::RenderState,
        texture::Texture,
    },
};
use egui::{
    show_tooltip_at_pointer, CentralPanel, Color32, Context, CursorIcon, DragValue, Grid, Image,
    Label, Pos2, Rect, ScrollArea, Sense, SidePanel, TopBottomPanel, Ui, Vec2, ViewportClass,
    ViewportId,
};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tetanes_core::{
    debug::PpuDebugger,
    mem::Access,
    ppu::{scroll::Scroll, Ppu},
};
use tracing::warn;
use winit::event::WindowEvent;

#[derive(Debug)]
#[must_use]
struct State {
    tx: NesEventProxy,
    tab: Tab,
    textures: Textures,
    // TODO: persist in config
    refresh_cycle: u32,
    refresh_scanline: u32,
    show_refresh_lines: bool,
    show_dividers: bool,
    show_tile_grid: bool,
    show_scroll_overlay: bool,
    show_attr_grid_16x: bool,
    show_attr_grid_32x: bool,
    show_palette_values: bool,
    nametables: NametablesState,
    pattern_tables: PatternTablesState,
    palette: PalettesState,
    ppu: Ppu,
}

#[derive(Debug)]
#[must_use]
struct NametablesState {
    pixels: Vec<u8>,
    hovered: Option<NametableTile>,
    selected: Option<NametableTile>,
}

#[derive(Debug)]
#[must_use]
struct PatternTablesState {
    pixels: Vec<u8>,
    hovered: Option<PatternTableTile>,
    selected: Option<PatternTableTile>,
}

#[derive(Debug)]
#[must_use]
struct PalettesState {
    pixels: Vec<u8>,
    hovered: Option<PaletteColor>,
    selected: Option<PaletteColor>,
}

#[derive(Debug)]
#[must_use]
struct Textures {
    nametables: Texture,
    tile: Texture,
    pattern_tables: Texture,
    sprites: Texture,
}

#[derive(Debug, Copy, Clone)]
#[must_use]
struct NametableTile {
    index: u16,
    uv: Rect,
    selection: Rect,
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

#[derive(Debug, Copy, Clone)]
#[must_use]
struct PatternTableTile {
    index: u16,
    uv: Rect,
    selection: Rect,
    tile_addr: u16,
}

#[derive(Debug, Copy, Clone)]
#[must_use]
struct PaletteColor {
    index: u8,
    selection: Rect,
    value: u8,
    color: Color32,
}

#[derive(Debug)]
#[must_use]
pub struct PpuViewer {
    id: ViewportId,
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    resources: Option<Config>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Nametables,
    PatternTables,
    Sprites,
    Palette,
}

impl PpuViewer {
    const TITLE: &'static str = "🎞 PPU Viewer";

    pub fn new(tx: NesEventProxy, render_state: &mut RenderState) -> Self {
        let textures = Textures {
            nametables: Texture::new(
                render_state,
                2.0 * Vec2::new(Ppu::WIDTH as f32, Ppu::HEIGHT as f32),
                1.0,
                Some("nes nametables"),
            ),
            tile: Texture::new(render_state, Vec2::new(8.0, 8.0), 1.0, Some("nes tile")),
            pattern_tables: Texture::new(
                render_state,
                Vec2::new(256.0, 128.0),
                1.0,
                Some("nes pattern tables"),
            ),
            sprites: Texture::new(
                render_state,
                Vec2::new(512.0, 512.0),
                1.0,
                Some("nes sprites"),
            ),
        };

        Self {
            id: ViewportId::from_hash_of(Self::TITLE),
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                tx,
                tab: Tab::default(),
                textures,
                refresh_cycle: 0,
                refresh_scanline: Ppu::VBLANK_SCANLINE_NTSC,
                show_refresh_lines: false,
                show_dividers: true,
                show_tile_grid: false,
                show_scroll_overlay: false,
                show_attr_grid_16x: false,
                show_attr_grid_32x: false,
                show_palette_values: true,
                nametables: NametablesState {
                    // 4 nametables with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 4 * Ppu::SIZE],
                    hovered: None,
                    selected: None,
                },
                pattern_tables: PatternTablesState {
                    // 2 pattern tables with 4 color channels (RGBA)
                    pixels: vec![0x00; 2 * 4 * Ppu::SIZE],
                    hovered: None,
                    selected: None,
                },
                palette: PalettesState {
                    // 32 palette colors with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 32],
                    hovered: None,
                    selected: None,
                },
                ppu: Ppu::default(),
            })),
            resources: None,
        }
    }

    pub const fn id(&self) -> ViewportId {
        self.id
    }

    pub fn open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    pub fn set_open(&self, open: bool) {
        self.open.store(open, Ordering::Release);
    }

    pub fn toggle_open(&self) {
        let _ = self
            .open
            .fetch_update(Ordering::Release, Ordering::Acquire, |open| Some(!open));
        self.state.lock().update_debugger(self.open());
    }

    pub fn prepare(&mut self, cfg: &Config) {
        self.resources = Some(cfg.clone());
    }

    pub fn update_ppu(&mut self, queue: &wgpu::Queue, ppu: Ppu) {
        let mut state = self.state.lock();
        match state.tab {
            Tab::Nametables => {
                ppu.load_nametables(&mut state.nametables.pixels);
                ppu.load_palettes(&mut state.palette.pixels);
                state
                    .textures
                    .nametables
                    .update(queue, &state.nametables.pixels);
            }
            Tab::PatternTables => {
                ppu.load_pattern_tables(&mut state.pattern_tables.pixels);
                state
                    .textures
                    .pattern_tables
                    .update(queue, &state.pattern_tables.pixels);
            }
            Tab::Sprites => (),
            Tab::Palette => {
                ppu.load_palettes(&mut state.palette.pixels);
            }
        }
        state.ppu = ppu;
    }

    pub fn on_window_event(&mut self, event: &WindowEvent) -> Response {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if matches!(event, WindowEvent::Resized(_)) {
            // TODO: preserve selection, just remap the selection Rect
            self.state.lock().nametables.selected = None;
            self.state.lock().pattern_tables.selected = None;
            self.state.lock().palette.selected = None;
        }

        Response::default()
    }

    pub fn show(&mut self, ctx: &Context, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);
        let Some(cfg) = self.resources.take() else {
            warn!("PpuViewer::prepare was not called with required resources");
            return;
        };

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
                    .show(ctx, |ui| state.lock().ui(ui, opts.enabled, &cfg));
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ctx, |ui| state.lock().ui(ui, opts.enabled, &cfg));
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
            callback: Arc::new(move |ppu| tx.event(DebugEvent::Ppu(ppu))),
        };
        self.tx.event(if open {
            EmulationEvent::AddDebugger(debugger.into())
        } else {
            EmulationEvent::RemoveDebugger(debugger.into())
        });
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            TopBottomPanel::top("ppu_viewer_menubar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Nametables, "Nametables");
                    ui.selectable_value(&mut self.tab, Tab::PatternTables, "Pattern Tables");
                    ui.selectable_value(&mut self.tab, Tab::Sprites, "Sprites");
                    ui.selectable_value(&mut self.tab, Tab::Palette, "Palette");
                });
            });

            match self.tab {
                Tab::Nametables => self.nametables_tab(ui, cfg),
                Tab::PatternTables => self.pattern_tables_tab(ui, cfg),
                Tab::Sprites => self.sprites_tab(ui, cfg),
                Tab::Palette => self.palette_tab(ui, cfg),
            }
        });
    }

    fn grid_settings(&mut self, ui: &mut Ui) {
        let res = ui
            .checkbox(&mut self.show_dividers, "Nametable Dividers")
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

    fn refresh_settings(&mut self, ui: &mut Ui) {
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

    fn nametables_tab(&mut self, ui: &mut Ui, cfg: &Config) {
        SidePanel::right("nametable_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
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

                if let Some(tile) = self.nametables.selected {
                    ui.add_space(16.0);
                    ui.heading("Selected Tile");
                    ui.separator();

                    self.nametable_tile(ui, "nametable_tile_selected", tile)
                }

                ui.add_space(16.0);
                ui.separator();

                ui.collapsing("Settings", |ui| {
                    self.refresh_settings(ui);

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
                });
            });
        });

        CentralPanel::default().show_inside(ui, |ui| {
            let scroll = ScrollArea::both()
                .min_scrolled_width(self.textures.nametables.size.x)
                .min_scrolled_height(self.textures.nametables.size.y);
            scroll.show(ui, |ui| {
                let image = Image::from_texture(self.textures.nametables.sized())
                    // TODO: Add nametable-specific scale
                    .fit_to_exact_size((cfg.renderer.scale / 2.0) * self.textures.nametables.size)
                    .sense(Sense::click());

                let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                let image_rect = res.rect;

                if let Some(pos) = res.hover_pos() {
                    self.nametable_hover(ui, &res, pos);
                    if res.clicked() {
                        self.nametables.selected = self.nametables.hovered;
                    }
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

                if let Some(tile) = self.nametables.selected {
                    ui.painter()
                        .rect_stroke(tile.selection, 0.0, (2.0, Color32::WHITE));
                    dashed_rect(ui, tile.selection, (2.0, Color32::BLACK), 3.0, 3.0);
                }
            });
        });
    }

    fn nametable_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.textures.nametables.size;

        let tile_offset = translate_screen_pos_to_tile(pos, image_rect, texture_size);
        let tile_selection = tile_selection(image_rect, texture_size, tile_offset);

        ui.painter()
            .rect_stroke(tile_selection, 0.0, (2.0, Color32::from_white_alpha(220)));
        dashed_rect(
            ui,
            tile_selection,
            (2.0, Color32::from_black_alpha(220)),
            3.0,
            3.0,
        );

        show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
            let Vec2 { x, y } = tile_offset;

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
            let tile_index = u16::from(self.ppu.bus.peek_ciram(nametable_addr, Access::Dummy));
            let tile_addr = self.ppu.ctrl.bg_select + (tile_index << 4);

            let supertile = ((row & 0xFC) << 1) + (col >> 2);
            let attr_addr = base_attr_addr + supertile;
            let attr_val = self.ppu.bus.peek_ciram(attr_addr, Access::Dummy);

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

            let tile = NametableTile {
                index: tile_index,
                uv: tile_uv,
                selection: tile_selection,
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
            };

            self.nametables.hovered = Some(tile);
            self.nametable_tile(ui, "nametable_tile_hover", tile);
        });
    }

    fn nametable_tile(&mut self, ui: &mut Ui, label: &str, tile: NametableTile) {
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
        } = tile;

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Tile:");
            let tile = Image::from_texture(self.textures.nametables.sized())
                .uv(uv)
                .maintain_aspect_ratio(false) // Ignore original aspect ratio
                .fit_to_exact_size(Vec2::splat(64.0))
                .sense(Sense::click());
            ui.add(tile);
            ui.end_row();

            ui.strong("Palette:");
            self.palette_row(
                ui,
                palette_index.into(),
                ui.cursor().min,
                Vec2::splat(16.0),
                true,
            );
            ui.end_row();

            ui.strong("Column, Row:");
            ui.label(format!("{col}, {row}"));
            ui.end_row();

            ui.strong("X, Y:");
            ui.label(format!("{x}, {y}"));
            ui.end_row();

            ui.strong("Nametable Address:");
            ui.label(format!("${nametable_addr:04X}"));
            ui.end_row();

            ui.strong("Tile Index:");
            ui.label(format!("${index:02X}"));
            ui.end_row();

            ui.strong("Tile Address:");
            ui.label(format!("${tile_addr:04X}"));
            ui.end_row();

            ui.strong("Palette Index:");
            ui.label(format!("{palette_index}"));
            ui.end_row();

            ui.strong("Palette Address:");
            ui.label(format!("${palette_addr:04X}"));
            ui.end_row();

            ui.strong("Attribute Address:");
            ui.label(format!("${attr_addr:04X}"));
            ui.end_row();

            ui.strong("Attribute Value:");
            ui.label(format!("${attr_val:02X}"));
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
                    scroll_x -= (cycle & !0x07) as u16;
                }
                // Adjust for 2x increments at end of last scanline
                scroll_x -= 16;
            } else if cycle >= Ppu::BG_PREFETCH_START + 7 {
                scroll_x -= 8;
                if cycle >= Ppu::BG_PREFETCH_END {
                    scroll_x -= 8;
                }
            }
            scroll_x += scroll.fine_x;
        }

        // Scroll overlay
        let nametable_size = image_rect.size() / 2.0;
        // Translate scroll_x/scroll_y to image space
        let scroll = Vec2::new(scroll_x as f32, scroll_y as f32) * image_rect.size()
            / self.textures.nametables.size;
        let scroll_min = image_rect.min + scroll;
        let scroll_max = scroll_min + nametable_size;
        let overlay = Rect::from_min_max(scroll_min, scroll_max.min(image_rect.max));
        ui.painter().rect(
            overlay,
            0.0,
            Color32::from_black_alpha(75),
            (1.0, Color32::WHITE),
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
            );
        }
    }

    fn pattern_tables_tab(&mut self, ui: &mut Ui, cfg: &Config) {
        SidePanel::right("pattern_tables_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                if let Some(tile) = self.pattern_tables.selected {
                    ui.add_space(16.0);
                    ui.heading("Selected Color");
                    ui.separator();

                    self.pattern_tables_tile(ui, "pattern_tables_tile_selected", tile)
                }

                ui.add_space(16.0);
                ui.separator();

                ui.collapsing("Settings", |ui| {
                    self.refresh_settings(ui);
                    self.grid_settings(ui);

                    // TODO: Selectable palette/last known palette
                });
            });
        });

        CentralPanel::default().show_inside(ui, |ui| {
            let scroll = ScrollArea::both()
                .min_scrolled_width(self.textures.pattern_tables.size.x)
                .min_scrolled_height(self.textures.pattern_tables.size.y);
            scroll.show(ui, |ui| {
                let image = Image::from_texture(self.textures.pattern_tables.sized())
                    // TODO: Add pattern table-specific scale
                    .fit_to_exact_size(cfg.renderer.scale * self.textures.pattern_tables.size)
                    .sense(Sense::click());

                let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                let image_rect = res.rect;

                if let Some(pos) = res.hover_pos() {
                    self.pattern_tables_hover(ui, &res, pos);
                    if res.clicked() {
                        self.pattern_tables.selected = self.pattern_tables.hovered;
                    }
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

                if let Some(tile) = self.pattern_tables.selected {
                    ui.painter()
                        .rect_stroke(tile.selection, 0.0, (2.0, Color32::WHITE));
                    dashed_rect(ui, tile.selection, (2.0, Color32::BLACK), 3.0, 3.0);
                }
            });
        });
    }

    fn pattern_tables_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.textures.pattern_tables.size;

        let tile_offset = translate_screen_pos_to_tile(pos, image_rect, texture_size);
        let tile_selection = tile_selection(image_rect, texture_size, tile_offset);

        ui.painter()
            .rect_stroke(tile_selection, 0.0, (2.0, Color32::from_white_alpha(220)));
        dashed_rect(
            ui,
            tile_selection,
            (2.0, Color32::from_black_alpha(220)),
            3.0,
            3.0,
        );

        show_tooltip_at_pointer(ui.ctx(), res.layer_id, res.id, |ui| {
            let Vec2 { x, y } = tile_offset;

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

            let tile = PatternTableTile {
                index: (tile_addr >> 4) & 0xFF,
                uv: tile_uv,
                selection: tile_selection,
                tile_addr,
            };

            self.pattern_tables.hovered = Some(tile);
            self.pattern_tables_tile(ui, "pattern_tables_tile_hover", tile);
        });
    }

    fn pattern_tables_tile(&mut self, ui: &mut Ui, label: &str, tile: PatternTableTile) {
        let PatternTableTile {
            uv,
            index,
            tile_addr,
            ..
        } = tile;

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Tile:");
            let tile = Image::from_texture(self.textures.pattern_tables.sized())
                .uv(uv)
                .maintain_aspect_ratio(false) // Ignore original aspect ratio
                .fit_to_exact_size(Vec2::splat(64.0))
                .sense(Sense::click());
            ui.add(tile);
            ui.end_row();

            ui.strong("Tile Index:");
            ui.label(format!("${index:02X}"));
            ui.end_row();

            ui.strong("Tile Address:");
            ui.label(format!("${tile_addr:04X}"));
            ui.end_row();
        });
    }

    fn sprites_tab(&mut self, ui: &mut Ui, cfg: &Config) {
        ui.label("Coming soon...");
    }

    fn palette_tab(&mut self, ui: &mut Ui, cfg: &Config) {
        SidePanel::right("palette_panel").show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                if let Some(palette_info) = self.palette.selected {
                    ui.add_space(16.0);
                    ui.heading("Selected Color");
                    ui.separator();

                    self.palette(ui, "palette_info_selected", palette_info);
                }

                ui.add_space(16.0);
                ui.separator();

                ui.collapsing("Settings", |ui| {
                    self.refresh_settings(ui);

                    let res = ui
                        .checkbox(&mut self.show_palette_values, "Palette Values")
                        .on_hover_text("Overlay hexidecimal values for palettes.");
                    if res.changed() {
                        // TODO: update config
                    }
                });
            });
        });

        CentralPanel::default().show_inside(ui, |ui| {
            ScrollArea::both().show(ui, |ui| {
                ui.horizontal(|ui| {
                    // TODO: Add palette-specific scale
                    let size = Vec2::splat(cfg.renderer.scale * 64.0);
                    let background_res = self.palette_grid(ui, "Background", size, 0);
                    let sprites_res = self.palette_grid(ui, "Sprites", size, 4);

                    for res in [background_res, sprites_res] {
                        if let Some(pos) = res.hover_pos() {
                            // allocate_exact_size response rect may be larger than requested, so
                            // check pos is inside
                            if res.rect.contains(pos) {
                                self.palette_hover(ui, &res, pos);
                                if res.clicked() {
                                    self.palette.selected = self.palette.hovered;
                                }
                            }
                        }
                    }

                    if self.show_palette_values {
                        // TODO
                    }

                    if let Some(color) = self.palette.selected {
                        ui.painter()
                            .rect_stroke(color.selection, 0.0, (2.0, Color32::WHITE));
                        dashed_rect(ui, color.selection, (2.0, Color32::BLACK), 3.0, 3.0);
                    }
                });
            });
        });
    }

    fn palette_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;

        // let pos = pos - image_rect.min;
        // let palette_offset = (pos / image_rect.size().x * 4.0).floor() * image_rect.size().x * 4.0;
        // let palette_selection =
        //     Rect::from_min_size(image_rect.min + palette_offset, image_rect.size() / 4.0);
        let palette_offset = translate_screen_pos_to_tile(pos, image_rect, Vec2::splat(32.0));
        let palette_selection = tile_selection(image_rect, Vec2::splat(32.0), palette_offset);

        ui.painter().rect_stroke(
            palette_selection,
            0.0,
            (2.0, Color32::from_white_alpha(220)),
        );
        dashed_rect(
            ui,
            palette_selection,
            (2.0, Color32::from_black_alpha(220)),
            3.0,
            3.0,
        );
    }

    fn palette(&mut self, ui: &mut Ui, label: &str, palette: PaletteColor) {
        let PaletteColor {
            index,
            value,
            color,
            ..
        } = palette;

        let grid = Grid::new(label).num_columns(2).spacing([40.0, 6.0]);
        grid.show(ui, |ui| {
            ui.strong("Color:");
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            ui.painter().rect_filled(rect, 1.0, color);
            ui.end_row();

            ui.strong("Index:");
            ui.label(format!("${index:02X}"));
            ui.end_row();

            ui.strong("Value:");
            ui.label(format!("${value:02X}"));
            ui.end_row();

            ui.strong("Hex:");
            // ui.label(&selected.hex);
            ui.end_row();

            ui.strong("RGB:");
            // ui.label(&selected.rgb);
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

    fn palette_grid(
        &self,
        ui: &mut Ui,
        label: &str,
        size: Vec2,
        index_offset: usize,
    ) -> egui::Response {
        ui.vertical(|ui| {
            ui.add(Label::new(label).extend());

            let (rect, res) = ui.allocate_exact_size(size, Sense::click());
            ui.painter().rect_stroke(rect, 0.0, (1.0, Color32::BLACK));

            let size = size / 4.0;
            for (y, index) in (index_offset..index_offset + 4).enumerate() {
                let pos = rect.min + Vec2::new(0.0, y as f32 * size.y).floor();
                self.palette_row(ui, index, pos, size, false);
            }

            res
        })
        .inner
    }
}

fn paint_grid(ui: &mut Ui, rect: Rect, rows: f32, cols: f32, color: Color32) {
    let min = rect.min;
    let max = rect.max;
    let size = rect.size();
    let x_increment = size.x / cols;
    let mut x = min.x + x_increment;
    while x < max.x {
        ui.painter().vline(x, rect.y_range(), (1.0, color));
        x += x_increment;
    }

    let y_increment = size.y / rows;
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

fn tile_selection(image_rect: Rect, texture_size: Vec2, tile_offset: Vec2) -> Rect {
    let scale = image_rect.size() / texture_size;
    Rect::from_min_size(
        image_rect.min + scale * tile_offset,
        scale * Vec2::splat(8.0),
    )
}
