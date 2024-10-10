use crate::nes::{
    config::Config,
    event::{DebugEvent, EmulationEvent, NesEventProxy},
    renderer::{gui::lib::ViewportOptions, painter::RenderState, texture::Texture},
};
use egui::{
    CentralPanel, Color32, Context, CursorIcon, Grid, Image, Rect, ScrollArea, Sense, SidePanel,
    Slider, TopBottomPanel, Ui, Vec2, ViewportClass, ViewportId,
};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tetanes_core::{debug::PpuDebugger, ppu::Ppu};
use tracing::warn;

#[derive(Debug)]
#[must_use]
struct State {
    tx: NesEventProxy,
    tab: Tab,
    textures: Textures,
    // TODO: persist in config
    refresh_cycle: u32,
    refresh_scanline: u32,
    show_dividers: bool,
    show_tile_grid: bool,
    show_attr_grid_16x: bool,
    show_attr_grid_32x: bool,
    show_palette_values: bool,
    nametables: NametablesState,
    pattern_tables: PatternTablesState,
    palettes: PalettesState,
    ppu: Ppu,
}

#[derive(Debug)]
#[must_use]
struct NametablesState {
    pixels: Vec<u8>,
    selected: Option<TileInfo>,
}

#[derive(Debug)]
#[must_use]
struct PatternTablesState {
    pixels: Vec<u8>,
    selected: Option<TileInfo>,
}

#[derive(Debug)]
#[must_use]
struct PalettesState {
    pixels: Vec<u8>,
    selected: Option<PaletteInfo>,
}

#[derive(Debug)]
#[must_use]
struct Textures {
    nametables: Texture,
    tile: Texture,
    pattern_tables: Texture,
    sprites: Texture,
}

#[derive(Debug)]
#[must_use]
struct TileInfo {
    index: u16,
    x: u8,      // 0..=248
    y: u8,      // 0..=232
    height: u8, // 8 or 16
    nametable_addr: u16,
    ppu_addr: u16,
    chr_addr: u16,
    palette_index: u8,
    palette_addr: u16,
    attr_addr: u16,
    attr_data: u8,
}

#[derive(Debug)]
#[must_use]
struct PaletteInfo {
    index: u8,
    value: u8,
    color: Color32,
    addr: u16,
    hex: String,
    rgb: String,
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
                show_dividers: true,
                show_tile_grid: false,
                show_attr_grid_16x: false,
                show_attr_grid_32x: false,
                show_palette_values: true,
                nametables: NametablesState {
                    // 4 nametables with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 4 * Ppu::SIZE],
                    selected: None,
                },
                pattern_tables: PatternTablesState {
                    // 2 pattern tables with 4 color channels (RGBA)
                    pixels: vec![0x00; 2 * 4 * Ppu::SIZE],
                    selected: None,
                },
                palettes: PalettesState {
                    // 32 palette colors with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 32],
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
        self.attach();
    }

    pub fn attach(&self) {
        let state = self.state.lock();
        let tx = state.tx.clone();
        let debugger = PpuDebugger {
            cycle: state.refresh_cycle,
            scanline: state.refresh_scanline,
            callback: Arc::new(move |ppu| tx.event(DebugEvent::Ppu(ppu))),
        };
        state.tx.event(if self.open() {
            EmulationEvent::AddDebugger(debugger.into())
        } else {
            EmulationEvent::RemoveDebugger(debugger.into())
        });
    }

    pub fn prepare(&mut self, cfg: &Config) {
        self.resources = Some(cfg.clone());
    }

    pub fn update_ppu(&mut self, queue: &wgpu::Queue, ppu: Ppu) {
        let mut state = self.state.lock();
        match state.tab {
            Tab::Nametables => {
                ppu.load_nametables(&mut state.nametables.pixels);
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
                ppu.load_palettes(&mut state.palettes.pixels);
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
    fn ui(&mut self, ui: &mut Ui, enabled: bool, _cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            TopBottomPanel::top("ppu_viewer_menubar").show(ui.ctx(), |ui| {
                self.settings_menu(ui);
                // TODO: tab menu items
                // menu::bar(ui, |ui| match self.tab {
                //     Tab::Nametables => (),
                //     Tab::PatternTables => (),
                //     Tab::Sprites => (),
                //     Tab::Palette => (),
                // });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Nametables, "Nametables");
                    ui.selectable_value(&mut self.tab, Tab::PatternTables, "Pattern Tables");
                    ui.selectable_value(&mut self.tab, Tab::Sprites, "Sprites");
                    ui.selectable_value(&mut self.tab, Tab::Palette, "Palette");
                });
            });

            match self.tab {
                Tab::Nametables => self.nametables(ui),
                Tab::PatternTables => self.pattern_tables(ui),
                Tab::Sprites => self.sprites(ui),
                Tab::Palette => self.palette(ui),
            }
        });
    }

    fn settings_menu(&mut self, ui: &mut Ui) {
        ui.menu_button("🔧 Settings", |ui| {
            match self.tab {
                // TODO: Display - combobox
                //        - Tiles
                //        - Grayscale
                //        - Attributes
                // TODO: Show Tile Grid - checkbox
                // TODO: Show Attribute Grid (16x16) - checkbox
                // TODO: Show Attribute Grid (32x32) - checkbox
                // TODO: Show Scroll Position - checkbox
                Tab::Nametables => (),
                Tab::PatternTables => (),
                Tab::Sprites => (),
                Tab::Palette => (),
            }
            //
        });
    }

    fn grid_settings(&mut self, ui: &mut Ui) {
        let res = ui
            .checkbox(&mut self.show_dividers, "Show Dividers")
            .on_hover_text("Show divider lines between tables.");
        if res.changed() {
            // TODO: update config
        }

        let res = ui
            .checkbox(&mut self.show_tile_grid, "Show Tile Grid")
            .on_hover_text("Show grid lines between tiles.");
        if res.changed() {
            // TODO: update config
        }
    }

    fn refresh_settings(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let slider = Slider::new(&mut self.refresh_scanline, 0..=self.ppu.prerender_scanline);
            let res = ui.add(slider);
            if res.changed() {
                // TODO: update config and emulation
            }
            ui.label("Refresh Scanline")
                .on_hover_cursor(CursorIcon::Help)
                .on_hover_text("Change which PPU scanline to refresh viewer state on.");
        });

        ui.horizontal(|ui| {
            let slider = Slider::new(&mut self.refresh_cycle, 0..=Ppu::CYCLE_END);
            let res = ui.add(slider);
            if res.changed() {
                // TODO: update config and emulation
            }
            ui.label("Refresh Cycle")
                .on_hover_cursor(CursorIcon::Help)
                .on_hover_text("Change which PPU cycle to refresh viewer state on.");
        });
    }

    fn nametables(&mut self, ui: &mut Ui) {
        SidePanel::right("nametable_panel").show(ui.ctx(), |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.group(|ui| {
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
                });

                ui.group(|ui| {
                    ui.heading("Settings");

                    ui.separator();

                    self.refresh_settings(ui);
                    self.grid_settings(ui);

                    let res = ui
                        .checkbox(&mut self.show_attr_grid_16x, "Show Attribute Grid (16x16)")
                        .on_hover_text("Show grid lines within each attribute block.");
                    if res.changed() {
                        // TODO: update config
                    }

                    let res = ui
                        .checkbox(&mut self.show_attr_grid_32x, "Show Attribute Grid (32x32)")
                        .on_hover_text("Show grid lines between attribute blocks.");
                    if res.changed() {
                        // TODO: update config
                    }
                });
            });
        });

        CentralPanel::default().show(ui.ctx(), |ui| {
            let image = Image::from_texture(self.textures.nametables.sized())
                .maintain_aspect_ratio(true)
                .shrink_to_fit()
                .sense(Sense::click());

            let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
            if res.clicked() {
                if let Some(pos) = res.hover_pos() {
                    // TODO: Set current tile
                }
            }

            let rect = res.rect;

            if self.show_dividers {
                // Split the 4x4 nametables in half vertically and horizontally
                ui.painter()
                    .vline(rect.center().x, rect.y_range(), (1.0, Color32::WHITE));
                ui.painter()
                    .hline(rect.x_range(), rect.center().y, (1.0, Color32::WHITE));
            }

            if self.show_tile_grid {
                paint_grid(ui, rect, 60.0, 64.0, Color32::LIGHT_BLUE);
            }

            if self.show_attr_grid_16x {
                paint_grid(ui, rect, 30.0, 32.0, Color32::LIGHT_RED);
            }

            if self.show_attr_grid_32x {
                // Because 32x doesn't divide evenly into 240, split this up into two passes with a
                // dividing line, forcing the leftover attribute space to be at the bottom. Also
                // halve the number of rows
                let top_rect = Rect::from_min_max(rect.min, rect.right_center());
                let bot_rect = Rect::from_min_max(rect.left_center(), rect.right_bottom());

                paint_grid(ui, top_rect, 7.5, 16.0, Color32::LIGHT_GREEN);
                ui.painter().hline(
                    top_rect.x_range(),
                    top_rect.bottom(),
                    (1.0, Color32::LIGHT_GREEN),
                );
                paint_grid(ui, bot_rect, 7.5, 16.0, Color32::LIGHT_GREEN);
            }
        });
    }

    fn pattern_tables(&mut self, ui: &mut Ui) {
        SidePanel::right("nametable_panel").show(ui.ctx(), |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.group(|ui| {
                    ui.heading("Pattern Tables Info");

                    ui.separator();

                    // TODO
                });

                ui.group(|ui| {
                    ui.heading("Settings");

                    ui.separator();

                    self.refresh_settings(ui);
                    self.grid_settings(ui);
                });
            });
        });

        CentralPanel::default().show(ui.ctx(), |ui| {
            let image = Image::from_texture(self.textures.pattern_tables.sized())
                .maintain_aspect_ratio(true)
                .shrink_to_fit()
                .sense(Sense::click());

            let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
            if res.clicked() {
                if let Some(pos) = res.hover_pos() {
                    // TODO: Set current tile
                }
            }

            let rect = res.rect;
            if self.show_dividers {
                ui.painter()
                    .vline(rect.center().x, rect.y_range(), (1.0, Color32::WHITE));
            }
            if self.show_tile_grid {
                paint_grid(ui, rect, 16.0, 32.0, Color32::LIGHT_BLUE);
            }
        });
    }

    fn sprites(&mut self, _ui: &mut Ui) {}

    fn palette(&mut self, ui: &mut Ui) {
        SidePanel::right("palette_panel").show(ui.ctx(), |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.group(|ui| {
                    ui.heading("Settings");

                    ui.separator();

                    self.refresh_settings(ui);

                    let res = ui
                        .checkbox(&mut self.show_palette_values, "Show Values")
                        .on_hover_text("Overlay hexidecimal values for palettes.");
                    if res.changed() {
                        // TODO: update config
                    }
                });

                if let Some(selected) = &self.palettes.selected {
                    ui.group(|ui| {
                        ui.heading("Selected Color");

                        ui.separator();

                        let grid = Grid::new("selected_Color")
                            .num_columns(2)
                            .spacing([40.0, 6.0]);
                        grid.show(ui, |ui| {
                            ui.strong("Color:");
                            let pos = ui.cursor().min;
                            let rect = Rect::from_min_max(pos, pos + Vec2::splat(32.0));
                            ui.painter().rect_filled(rect, 1.0, selected.color);
                            ui.advance_cursor_after_rect(rect);
                            ui.end_row();

                            ui.strong("Index:");
                            ui.label(format!("{:02X}", selected.index));
                            ui.end_row();

                            ui.strong("Value:");
                            ui.label(format!("{:02X}", selected.value));
                            ui.end_row();

                            ui.strong("Address:");
                            ui.label(format!("{:04X}", selected.addr));
                            ui.end_row();

                            ui.strong("Hex:");
                            ui.label(&selected.hex);
                            ui.end_row();

                            ui.strong("RGB:");
                            ui.label(&selected.rgb);
                            ui.end_row();
                        });
                    });
                }
            });
        });

        CentralPanel::default().show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                let size =
                    Vec2::splat((ui.available_width() - ui.style().spacing.item_spacing.x) / 2.0);
                self.palette_colors(ui, "Background", size, 0);
                self.palette_colors(ui, "Sprites", size, 16);
            });
        });
    }

    fn palette_colors(&self, ui: &mut Ui, label: &str, size: Vec2, palette_offset: usize) {
        ui.vertical(|ui| {
            ui.label(label);

            let pos = ui.cursor().min;
            let rect = Rect::from_min_max(pos, pos + size);

            ui.painter().rect_stroke(rect, 0.0, (1.0, Color32::BLACK));

            let palette_size = size / 4.0;
            for i in palette_offset..palette_offset + 16 {
                let idx = i * 4;
                if let [red, green, blue] = self.palettes.pixels[idx..idx + 3] {
                    let x = (i - palette_offset) % 4;
                    let y = (i - palette_offset) / 4;

                    let pos = pos
                        + Vec2::new(x as f32 * palette_size.x, y as f32 * palette_size.y).floor();
                    let rect = Rect::from_min_max(pos, pos + palette_size);
                    ui.painter()
                        .rect_filled(rect, 0.0, Color32::from_rgb(red, green, blue));
                }
            }

            ui.advance_cursor_after_rect(rect);
        });
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
