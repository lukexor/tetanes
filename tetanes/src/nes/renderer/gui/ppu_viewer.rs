use crate::nes::{
    event::{ConfigEvent, DebugEvent, EmulationEvent, NesEventProxy},
    renderer::{
        gui::{
            lib::{ViewportOptions, animated_dashed_rect},
            panes::{self, Column, Pane as _},
        },
        painter::RenderState,
        texture::Texture,
    },
};
use egui::{
    CentralPanel, Color32, Context, CursorIcon, DragValue, Grid, Image, Label, Panel,
    PopupCloseBehavior, Pos2, Rect, ScrollArea, Sense, Slider, StrokeKind, Ui, Vec2, ViewportClass,
    ViewportId,
    containers::menu::{MenuButton, MenuConfig},
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tetanes_core::{
    bus::Bus,
    debug::Debugger,
    ppu::{self, Mirroring, Ppu, addr, cycle, scanline, scroll::Scroll, sprite::Sprite},
};

/// Bytes of the PPU address space a snapshot carries: `$0000-$2FFF`, pattern tables plus
/// nametables.
const CHR_WINDOW: usize = 0x3000;

/// What a pane spends above its image: the heading, the header row, and the separator under it.
const PANE_CHROME: f32 = 62.0;

/// The row naming the background and sprite halves of the palette grid.
const PALETTE_LABELS: f32 = 20.0;

/// The side of one NES tile, which is the step the nametable, pattern and OAM views select by.
const TILE: f32 = 8.0;

/// The side of one palette swatch. The palette grid has no texture behind it, so its cell is
/// whatever reads well rather than a count of NES pixels.
const SWATCH: f32 = 24.0;

/// Which of a pane header's two menus is being drawn.
///
/// Both reach the pane's own state, so they arrive as one closure rather than two, which a single
/// `&mut self` can serve.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
enum HeaderMenu {
    /// The toggles that change how the view draws.
    Settings,
    /// What the pane knows about the tile or color the last click selected.
    Detail,
}

/// What this viewer takes from the console when the debugger fires.
///
/// The callback is handed the whole [`Bus`], so this is the viewer's own choice of what to copy
/// across to the render thread - the PPU's registers, and its address space resolved through the
/// board so no mapper knowledge is needed here. A few KiB per frame; rendering the views produces
/// ~1 MiB of pixels and stays on this side.
#[derive(Debug, Clone)]
#[must_use]
pub struct PpuSnapshot {
    pub ppu: Ppu,
    pub chr: Box<[u8; CHR_WINDOW]>,
    pub mirroring: Mirroring,
}

impl Default for PpuSnapshot {
    fn default() -> Self {
        Self {
            ppu: Ppu::default(),
            chr: Box::new([0; CHR_WINDOW]),
            mirroring: Mirroring::default(),
        }
    }
}

impl PpuSnapshot {
    /// Copy what the viewer needs out of the console at a break point.
    pub fn capture(bus: &Bus) -> Self {
        let mut chr = Box::new([0; CHR_WINDOW]);
        bus.copy_ppu_bus(chr.as_mut_slice());
        Self {
            ppu: bus.ppu.snapshot(),
            chr,
            mirroring: bus.mirroring(),
        }
    }
}

#[derive(Debug)]
#[must_use]
struct State {
    tx: NesEventProxy,
    panes: Vec<Pane>,
    // TODO: persist in config
    refresh_cycle: u16,
    refresh_scanline: u16,
    nametables: NametablesState,
    pattern_tables: PatternTablesState,
    oam: OamState,
    palette: PalettesState,
    snapshot: PpuSnapshot,
}

#[derive(Debug)]
#[must_use]
struct NametablesState {
    pixels: Vec<u8>,
    texture: Texture,
    zoom: f32,
    selected: Option<Vec2>,
    show_dividers: bool,
    show_tile_grid: bool,
    show_refresh_lines: bool,
    show_scroll_overlay: bool,
    show_attr_grid_16x: bool,
    show_attr_grid_32x: bool,
}

#[derive(Debug)]
#[must_use]
struct PatternTablesState {
    pixels: Vec<u8>,
    texture: Texture,
    zoom: f32,
    selected: Option<Vec2>,
    show_dividers: bool,
    show_tile_grid: bool,
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
    show_tile_grid: bool,
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
    x: u16, // 0..=248
    y: u16, // 0..=232
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
    pub id: ViewportId,
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
}

/// A view in the PPU viewer.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Pane {
    /// The four nametables as the background they describe.
    Nametables,
    /// Both pattern tables as tiles.
    PatternTables,
    /// The OAM entries as tiles, beside the screen they place.
    Oam,
    /// The background and sprite palettes as swatches.
    Palette,
}

impl Pane {
    /// The id the window's panes and columns are keyed by, which keeps them apart from another
    /// window's.
    const WINDOW: &'static str = "ppu_viewer";

    /// The panes a window opens with.
    ///
    /// All of them. The window exists to show the four at once, and each is cheap enough that
    /// none of them waits to be asked for.
    pub const DEFAULT: [Self; 4] = [
        Self::Nametables,
        Self::PatternTables,
        Self::Oam,
        Self::Palette,
    ];
}

impl panes::Pane for Pane {
    const ALL: &'static [Self] = &Self::DEFAULT;

    fn title(self) -> &'static str {
        match self {
            Self::Nametables => "Nametables",
            Self::PatternTables => "Pattern tables",
            Self::Oam => "OAM",
            Self::Palette => "Palette",
        }
    }

    fn column(self) -> Column {
        match self {
            // A nametable pair is 512x480, which is more than the right column spans.
            Self::Nametables => Column::Center,
            Self::PatternTables | Self::Oam | Self::Palette => Column::Right,
        }
    }

    fn default_size(self) -> f32 {
        // Every pane here draws an image whose height follows its zoom, so `State::pane_size`
        // measures it and applies this as the floor.
        120.0
    }

    fn id(self) -> &'static str {
        match self {
            Self::Nametables => "pane_nametables",
            Self::PatternTables => "pane_pattern_tables",
            Self::Oam => "pane_oam",
            Self::Palette => "pane_palette",
        }
    }
}

impl PpuViewer {
    const TITLE: &'static str = "TetaNES - PPU Viewer";

    /// Build the viewer with `panes` open. The window itself starts closed.
    pub fn new(tx: NesEventProxy, panes: &[Pane], render_state: &mut RenderState) -> Self {
        // The center pane has no close button, so a config that lost it gets it back rather than
        // a window with the toolbar and nothing under it.
        let mut panes = Pane::ALL
            .iter()
            .copied()
            .filter(|pane| panes.contains(pane))
            .collect::<Vec<_>>();
        if !panes.contains(&Pane::Nametables) {
            panes.insert(0, Pane::Nametables);
        }
        Self {
            id: ViewportId::from_hash_of(Self::TITLE),
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                tx,
                panes,
                refresh_cycle: 0,
                refresh_scanline: scanline::VBLANK_NTSC,
                nametables: NametablesState {
                    // 4 nametables with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 4 * ppu::size::FRAME],
                    texture: Texture::new(
                        render_state,
                        2.0 * Vec2::new(ppu::size::WIDTH as f32, ppu::size::HEIGHT as f32),
                        1.0,
                        Some("nes nametables"),
                    ),
                    zoom: 1.5,
                    selected: None,
                    show_dividers: true,
                    show_tile_grid: false,
                    show_refresh_lines: false,
                    show_scroll_overlay: false,
                    show_attr_grid_16x: false,
                    show_attr_grid_32x: false,
                },
                pattern_tables: PatternTablesState {
                    // 2 pattern tables with 4 color channels (RGBA)
                    pixels: vec![0x00; 2 * 4 * ppu::size::FRAME],
                    texture: Texture::new(
                        render_state,
                        Vec2::new(ppu::size::WIDTH as f32, ppu::size::WIDTH as f32 / 2.0),
                        1.0,
                        Some("nes pattern tables"),
                    ),
                    // 256 wide at 1x, which fits the right column without scrolling it.
                    zoom: 1.0,
                    selected: None,
                    show_dividers: true,
                    show_tile_grid: false,
                },
                oam: OamState {
                    // 64 8x8 sprites with 4 color channels (RGBA)
                    oam_pixels: vec![0x00; 64 * 8 * 8 * 4],
                    // 1 nametable with 4 color channels (RGBA)
                    sprite_pixels: vec![0x00; 4 * ppu::size::FRAME],
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
                        Vec2::new(ppu::size::WIDTH as f32, ppu::size::HEIGHT as f32),
                        1.0,
                        Some("nes sprites"),
                    ),
                    // Sets a display height both textures scale to, so the pair spans roughly
                    // the right column.
                    zoom: 1.0,
                    oam_selected: None,
                    show_tile_grid: false,
                },
                palette: PalettesState {
                    // 2 palette tables, 4 colors each, one swatch per cell.
                    size: Vec2::new(8.0 * SWATCH, 4.0 * SWATCH),
                    // 32 palette colors with 4 color channels (RGBA)
                    pixels: vec![0x00; 4 * 32],
                    // 32 colors
                    colors: vec![0x00; 32],
                    zoom: 1.0,
                    selected: None,
                },
                snapshot: PpuSnapshot::default(),
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
        self.state.lock().update_debugger(open);
        if open {
            ctx.send_viewport_cmd_to(self.id, egui::ViewportCommand::Close);
        }
    }

    pub fn toggle_open(&self, ctx: &Context) {
        let Ok(open) = self
            .open
            .try_update(Ordering::Release, Ordering::Acquire, |open| Some(!open))
        else {
            return;
        };
        self.state.lock().update_debugger(!open);
        if open {
            ctx.send_viewport_cmd_to(self.id, egui::ViewportCommand::Close);
        }
    }

    pub fn update_ppu(&mut self, queue: &wgpu::Queue, snapshot: PpuSnapshot) {
        let mut state = self.state.lock();
        let PpuSnapshot { ppu, chr, .. } = &snapshot;
        // Rendering a view walks its pixels once per frame, so a closed pane is not drawn into.
        let nametables = state.panes.contains(&Pane::Nametables);
        let pattern_tables = state.panes.contains(&Pane::PatternTables);
        let oam = state.panes.contains(&Pane::Oam);
        // The nametable view colors its tiles from the palettes, so it needs them loaded whether
        // or not the palette pane is showing them.
        let palette = nametables || state.panes.contains(&Pane::Palette);
        if nametables {
            ppu.load_nametables(chr.as_slice(), &mut state.nametables.pixels);
            state
                .nametables
                .texture
                .update(queue, &state.nametables.pixels);
        }
        if pattern_tables {
            ppu.load_pattern_tables(chr.as_slice(), &mut state.pattern_tables.pixels);
            state
                .pattern_tables
                .texture
                .update(queue, &state.pattern_tables.pixels);
        }
        if oam {
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
            ppu.load_oam(
                chr.as_slice(),
                &mut oam_pixels,
                &mut sprite_pixels,
                &mut sprites,
            );

            state.oam.oam_pixels = oam_pixels;
            state.oam.sprite_pixels = sprite_pixels;
            state.oam.sprites = sprites;

            state.oam.oam_texture.update(queue, &state.oam.oam_pixels);
            state
                .oam
                .sprites_texture
                .update(queue, &state.oam.sprite_pixels);
        }
        if palette {
            let mut pixels = std::mem::take(&mut state.palette.pixels);
            let mut colors = std::mem::take(&mut state.palette.colors);
            ppu.load_palettes(&mut pixels, &mut colors);
            state.palette.pixels = pixels;
            state.palette.colors = colors;
        }
        state.snapshot = snapshot;
    }

    pub fn show(&mut self, ui: &mut Ui, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);

        let mut viewport_builder = egui::ViewportBuilder::default()
            .with_title(Self::TITLE)
            // Wide and tall enough for all four nametables at their default zoom, with the right
            // column beside them.
            .with_inner_size(Vec2::new(1180.0, 850.0));
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        ui.show_viewport_deferred(self.id, viewport_builder, move |ui, class| {
            if class == ViewportClass::EmbeddedWindow {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(PpuViewer::TITLE)
                    .open(&mut window_open)
                    .show(ui, |ui| state.lock().ui(ui, opts.enabled));
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ui, |ui| state.lock().ui(ui, opts.enabled));
                if ui.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
        });
    }
}

impl State {
    fn update_debugger(&self, open: bool) {
        self.tx.event(if open {
            let tx = self.tx.clone();
            EmulationEvent::AddDebugger(Debugger {
                cycle: self.refresh_cycle,
                scanline: self.refresh_scanline,
                callback: Arc::new(move |bus| {
                    tx.event(DebugEvent::Ppu(Box::new(PpuSnapshot::capture(bus))))
                }),
            })
        } else {
            EmulationEvent::RemoveDebugger
        });
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool) {
        let mut closed = None;
        ui.add_enabled_ui(enabled, |ui| {
            Panel::top("ppu_viewer_toolbar").show(ui, |ui| self.toolbar(ui));
            // Drawn through a closure rather than by the layout, since a body reads the window's
            // own state. The pane the closure cannot reach is the one it reports closed.
            let open = self.panes.clone();
            // Measured before the bodies borrow `self` to draw.
            let sizes = Pane::ALL
                .iter()
                .map(|pane| (*pane, self.pane_size(*pane)))
                .collect::<Vec<_>>();
            let size = |pane| {
                sizes
                    .iter()
                    .find_map(|(other, size)| (*other == pane).then_some(*size))
                    .unwrap_or_default()
            };
            closed = panes::columns(ui, Pane::WINDOW, &open, &size, &mut |ui, pane| {
                self.pane(ui, pane)
            });
        });
        if let Some(pane) = closed {
            self.set_pane_open(pane, false);
        }
    }

    /// The dot every pane refreshes on, and which panes are open.
    ///
    /// One [`Debugger`] fires the snapshot all four views read, so the cycle and scanline belong
    /// to the window rather than to any one pane.
    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            self.refresh_settings(ui);
            ui.separator();
            ui.strong("Mirroring:");
            ui.label(format!("{:?}", self.snapshot.mirroring));
            ui.separator();
            if let Some((pane, open)) = panes::view_menu(ui, Pane::WINDOW, &self.panes) {
                self.set_pane_open(pane, open);
            }
        });
    }

    /// Open or close `pane`, rebuilding the open list in [`Pane::ALL`] order so a reopened pane
    /// goes back where it was in its column.
    fn set_pane_open(&mut self, pane: Pane, open: bool) {
        self.panes = Pane::ALL
            .iter()
            .copied()
            .filter(|other| {
                if *other == pane {
                    open
                } else {
                    self.panes.contains(other)
                }
            })
            .collect();
        self.tx
            .event(ConfigEvent::PpuViewerPanes(self.panes.clone()));
        // The console stops rendering a closed pane's view, so its texture is as old as the frame
        // it was closed on. Reopening asks for a snapshot rather than waiting for the next dot.
        if open {
            self.update_debugger(true);
        }
    }

    /// How tall `pane` needs to be to show its view whole.
    ///
    /// The right column has no splitters between its panes, so a fixed height would leave every
    /// image scrolled and no way to grow it. Measuring the image instead makes the zoom slider the
    /// resize control, and a pane is exactly as tall as what it draws.
    fn pane_size(&self, pane: Pane) -> f32 {
        let image = match pane {
            // The center column's pane takes what is left, so its height is never asked for.
            Pane::Nametables => 0.0,
            Pane::PatternTables => self.pattern_tables.zoom * self.pattern_tables.texture.size.y,
            // The view scales both textures to one shared display height.
            Pane::Oam => 2.0 * self.oam.zoom * self.oam.oam_texture.size.y,
            // Plus the row naming the two halves.
            Pane::Palette => self.palette.zoom * self.palette.size.y + PALETTE_LABELS,
        };
        (image + PANE_CHROME).max(pane.default_size())
    }

    /// Draw `pane`'s view. The layout draws the heading above it.
    fn pane(&mut self, ui: &mut Ui, pane: Pane) {
        match pane {
            Pane::Nametables => self.nametables(ui),
            Pane::PatternTables => self.pattern_tables(ui),
            Pane::Oam => self.oam(ui),
            Pane::Palette => self.palette_pane(ui),
        }
    }

    /// The PPU dot the snapshot every pane reads is taken at.
    fn refresh_settings(&mut self, ui: &mut Ui) {
        ui.strong("Refresh on:")
            .on_hover_cursor(CursorIcon::Help)
            .on_hover_text("Change which PPU cycle/scanline viewer state refreshes on.");

        let drag = DragValue::new(&mut self.refresh_cycle)
            .range(0..=cycle::END)
            .suffix(" cycle");
        let res = ui.add(drag);
        if res.changed() {
            self.update_debugger(true);
        }

        let drag = DragValue::new(&mut self.refresh_scanline)
            .range(0..=self.snapshot.ppu.prerender_scanline)
            .suffix(" scanline");
        let res = ui.add(drag);
        if res.changed() {
            self.update_debugger(true);
        }
    }

    fn nametables(&mut self, ui: &mut Ui) {
        let mut zoom = self.nametables.zoom;
        let selected = self.nametables.selected;
        pane_header(
            ui,
            "nametables",
            &mut zoom,
            true,
            selected.is_some(),
            |ui, menu| {
                if menu == HeaderMenu::Detail {
                    self.nametable_tile(ui, "nametable_tile_selected", selected);
                    return;
                }
                let res = ui
                    .checkbox(&mut self.nametables.show_refresh_lines, "Refresh Markers")
                    .on_hover_text("Show lines indicating the current refresh cycle and scanline.");
                if res.changed() {
                    // TODO: update config
                }

                grid_settings(
                    ui,
                    &mut self.nametables.show_dividers,
                    &mut self.nametables.show_tile_grid,
                );

                let res = ui
                    .checkbox(&mut self.nametables.show_scroll_overlay, "Scroll Overlay")
                    .on_hover_text("Show scroll position overlay.");
                if res.changed() {
                    // TODO: update config
                }

                let res = ui
                    .checkbox(
                        &mut self.nametables.show_attr_grid_16x,
                        "Attribute Grid (16x16)",
                    )
                    .on_hover_text("Show grid lines within each attribute block.");
                if res.changed() {
                    // TODO: update config
                }

                let res = ui
                    .checkbox(
                        &mut self.nametables.show_attr_grid_32x,
                        "Attribute Grid (32x32)",
                    )
                    .on_hover_text("Show grid lines between attribute blocks.");
                if res.changed() {
                    // TODO: update config
                }
            },
        );
        self.nametables.zoom = zoom;

        let texture_size = self.nametables.texture.size;
        {
            let scroll = ScrollArea::both()
                .id_salt("nametables_image")
                .min_scrolled_width(texture_size.x)
                .min_scrolled_height(texture_size.y);
            scroll.show(ui, |ui| {
                let image = Image::from_texture(self.nametables.texture.sized())
                    .fit_to_exact_size(self.nametables.zoom * texture_size)
                    .sense(Sense::click());

                let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                let image_rect = res.rect;

                if let Some(pos) = res.hover_pos()
                    && image_rect.contains(pos)
                {
                    self.nametable_hover(ui, &res, pos);
                }

                if self.nametables.show_dividers {
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

                if self.nametables.show_refresh_lines {
                    let cycle_offset =
                        self.refresh_cycle as f32 * image_rect.size().x / 2.0 / cycle::END as f32;
                    let scanline_offset = self.refresh_scanline as f32 * image_rect.size().y
                        / 2.0
                        / self.snapshot.ppu.prerender_scanline as f32;
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

                if self.nametables.show_tile_grid {
                    paint_grid(ui, image_rect, 60.0, 64.0, Color32::LIGHT_BLUE);
                }

                if self.nametables.show_attr_grid_16x {
                    paint_grid(ui, image_rect, 30.0, 32.0, Color32::LIGHT_RED);
                }

                if self.nametables.show_attr_grid_32x {
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

                if self.nametables.show_scroll_overlay {
                    self.nametable_scroll_overlay(ui, image_rect);
                }

                if let Some(offset) = self.nametables.selected {
                    let selection =
                        tile_selection(image_rect, self.nametables.texture.size, offset, TILE);
                    animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);
                }
            });
        }
    }

    fn nametable_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.nametables.texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size, TILE);
        let selection = tile_selection(image_rect, texture_size, offset, TILE);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        res.clone().on_hover_ui_at_pointer(|ui| {
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
        let base_nametable_addr = addr::NAMETABLE_START | (nametable * ppu::size::NAMETABLE);
        let base_attr_addr = base_nametable_addr + addr::ATTR_OFFSET;

        let nametable_addr = base_nametable_addr + nametable_index;
        let tile_index = u16::from(self.snapshot.chr[usize::from(nametable_addr)]);
        let tile_addr = self.snapshot.ppu.ctrl_bg_select + (tile_index << 4);

        let supertile = ((row & 0xFC) << 1) + (col >> 2);
        let attr_addr = base_attr_addr + supertile;
        let attr_val = self.snapshot.chr[usize::from(attr_addr)];

        let attr_shift = (col & 0x02) | ((row & 0x02) << 1);
        // TODO: handle mmc5 extended attributes
        let palette_addr = ((attr_val >> attr_shift) & 0x03) << 2;
        let palette_index = palette_addr >> 2;
        let palette_addr = addr::PALETTE_START + u16::from(palette_addr);

        let tile_uv = Rect::from_min_size(
            (Vec2::new(x, y) / texture_size).to_pos2(),
            Vec2::splat(8.0) / texture_size,
        );

        let x = (x as u16) % ppu::size::WIDTH;
        let y = (y as u16) % ppu::size::HEIGHT;

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
        } = self.snapshot.ppu;
        let use_scroll_t = scanline >= vblank_scanline
            || (scanline == scanline::VISIBLE_END && cycle >= cycle::SPR_EVAL_END)
            || (scanline == prerender_scanline && cycle < cycle::BG_PREFETCH_START + 7);
        let scroll_v = if use_scroll_t { scroll.t } else { scroll.v };

        let mut scroll_x = ((scroll_v & Scroll::COARSE_X_MASK) << 3)
            | (((scroll_v & Scroll::NT_X_MASK) >> 10) * ppu::size::WIDTH);
        let scroll_y = ((scroll_v & Scroll::COARSE_Y_MASK) >> 2)
            | (((scroll_v & Scroll::NT_Y_MASK) >> 11) * ppu::size::HEIGHT)
            | ((scroll_v & Scroll::FINE_Y_MASK) >> 12);

        if use_scroll_t {
            scroll_x |= scroll.fine_x;
        } else {
            // During rendering, subtract according to current cycle/scanline
            if cycle <= scanline::VISIBLE_END {
                if cycle >= 8 {
                    scroll_x = scroll_x.saturating_sub(cycle & !0x07);
                }
                // Adjust for 2x increments at end of last scanline
                scroll_x = scroll_x.saturating_sub(16);
            } else if cycle >= cycle::BG_PREFETCH_START + 7 {
                scroll_x = scroll_x.saturating_sub(8);
                if cycle >= cycle::BG_PREFETCH_END {
                    scroll_x = scroll_x.saturating_sub(8);
                }
            }
            scroll_x += scroll.fine_x;
        }

        // The 256x240 viewport sits on a 512x480 torus, so it can run off the right edge, the
        // bottom, or both at once. Drawing it at all four wrapped origins under a clip to the
        // image puts each piece where it belongs and cuts away the rest. No two pieces overlap,
        // since they sit a full image apart and each spans half of one.
        let texture_size = self.nametables.texture.size;
        let scroll = Vec2::new(
            f32::from(scroll_x % texture_size.x as u16),
            f32::from(scroll_y % texture_size.y as u16),
        ) * image_rect.size()
            / texture_size;
        let nametable_size = image_rect.size() / 2.0;
        let origin = image_rect.min + scroll;
        let painter = ui.painter().with_clip_rect(image_rect);
        for offset in [
            Vec2::ZERO,
            Vec2::new(-image_rect.width(), 0.0),
            Vec2::new(0.0, -image_rect.height()),
            -image_rect.size(),
        ] {
            painter.rect(
                Rect::from_min_size(origin + offset, nametable_size),
                0.0,
                Color32::from_black_alpha(75),
                (1.0, Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn pattern_tables(&mut self, ui: &mut Ui) {
        let mut zoom = self.pattern_tables.zoom;
        let selected = self.pattern_tables.selected;
        pane_header(
            ui,
            "pattern_tables",
            &mut zoom,
            true,
            selected.is_some(),
            |ui, menu| match menu {
                // TODO: Selectable palette/last known palette
                HeaderMenu::Settings => grid_settings(
                    ui,
                    &mut self.pattern_tables.show_dividers,
                    &mut self.pattern_tables.show_tile_grid,
                ),
                HeaderMenu::Detail => {
                    self.pattern_tables_tile(ui, "pattern_tables_tile_selected", selected);
                }
            },
        );
        self.pattern_tables.zoom = zoom;

        let texture_size = self.pattern_tables.texture.size;
        {
            let scroll = ScrollArea::both()
                .id_salt("pattern_tables_image")
                .min_scrolled_width(texture_size.x)
                .min_scrolled_height(texture_size.y);
            scroll.show(ui, |ui| {
                let image = Image::from_texture(self.pattern_tables.texture.sized())
                    .fit_to_exact_size(self.pattern_tables.zoom * texture_size)
                    .sense(Sense::click());

                let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                let image_rect = res.rect;

                if let Some(pos) = res.hover_pos()
                    && image_rect.contains(pos)
                {
                    self.pattern_tables_hover(ui, &res, pos);
                }

                if self.pattern_tables.show_dividers {
                    ui.painter().vline(
                        image_rect.center().x,
                        image_rect.y_range(),
                        (1.0, Color32::WHITE),
                    );
                }

                if self.pattern_tables.show_tile_grid {
                    paint_grid(ui, image_rect, 16.0, 32.0, Color32::LIGHT_BLUE);
                }

                if let Some(offset) = self.pattern_tables.selected {
                    let selection =
                        tile_selection(image_rect, self.pattern_tables.texture.size, offset, TILE);
                    animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);
                }
            });
        }
    }

    fn pattern_tables_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.pattern_tables.texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size, TILE);
        let selection = tile_selection(image_rect, texture_size, offset, TILE);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        res.clone().on_hover_ui_at_pointer(|ui| {
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

    fn oam(&mut self, ui: &mut Ui) {
        let mut zoom = self.oam.zoom;
        let selected = self.oam.oam_selected;
        pane_header(
            ui,
            "oam",
            &mut zoom,
            true,
            selected.is_some(),
            |ui, menu| {
                if menu == HeaderMenu::Detail {
                    self.oam_tile(ui, "oam_selected", selected);
                    return;
                }
                let res = ui
                    .checkbox(&mut self.oam.show_tile_grid, "Tile Grid")
                    .on_hover_text("Show grid lines between tiles.");
                if res.changed() {
                    // TODO: update config
                }
            },
        );
        self.oam.zoom = zoom;

        let oam_texture_size = self.oam.oam_texture.size;
        let sprites_texture_size = self.oam.sprites_texture.size;
        // The two textures have different dimensions - OAM is a square 8x8 grid of tiles while
        // sprites is a whole 256x240 screen - so zoom sets a shared display height and each keeps
        // its own aspect ratio. Scaling both to a common box instead would either squash the
        // screen or, since `fit_to_exact_size` maintains aspect ratio, silently shrink it to
        // whichever axis fit.
        let display_height = 2.0 * self.oam.zoom * oam_texture_size.y;
        let oam_size = oam_texture_size * (display_height / oam_texture_size.y);
        let sprites_size = sprites_texture_size * (display_height / sprites_texture_size.y);
        {
            let scroll = ScrollArea::both()
                .id_salt("oam_image")
                .min_scrolled_width(oam_texture_size.x + sprites_texture_size.x)
                .min_scrolled_height(oam_texture_size.y.max(sprites_texture_size.y));
            scroll.show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Draw OAM tiles
                    let image = Image::from_texture(self.oam.oam_texture.sized())
                        .fit_to_exact_size(oam_size)
                        .sense(Sense::click());

                    let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                    let oam_image_rect = res.rect;

                    if let Some(pos) = res.hover_pos()
                        && oam_image_rect.contains(pos)
                    {
                        self.oam_hover(ui, &res, pos);
                    }

                    if self.oam.show_tile_grid {
                        paint_grid(ui, oam_image_rect, 8.0, 8.0, Color32::LIGHT_BLUE);
                    }

                    // Draw sprites as laid out on screen, at the same height as the OAM tiles
                    let image = Image::from_texture(self.oam.sprites_texture.sized())
                        .fit_to_exact_size(sprites_size)
                        .sense(Sense::click());

                    let res = ui.add(image).on_hover_cursor(CursorIcon::Cell);
                    let spr_image_rect = res.rect;

                    if let Some(pos) = res.hover_pos()
                        && spr_image_rect.contains(pos)
                    {
                        self.sprites_hover(ui, &res, pos);
                    }

                    if self.oam.show_tile_grid {
                        paint_grid(ui, spr_image_rect, 30.0, 32.0, Color32::LIGHT_BLUE);
                    }

                    if let Some(offset) = self.oam.oam_selected {
                        let selection =
                            tile_selection(oam_image_rect, self.oam.oam_texture.size, offset, TILE);
                        animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);

                        let sprite_index =
                            (offset.x / 8.0) as usize + (offset.y / 8.0) as usize * 8;
                        let sprite = self.oam.sprites.get(sprite_index);
                        if let Some(sprite) = sprite {
                            let offset = Vec2::new(
                                ((sprite.x as f32) / 8.0).floor() * 8.0,
                                ((sprite.y as f32) / 8.0).floor() * 8.0,
                            );
                            if offset.x < ppu::size::WIDTH as f32
                                && offset.y < ppu::size::HEIGHT as f32
                            {
                                let selection = tile_selection(
                                    spr_image_rect,
                                    self.oam.sprites_texture.size,
                                    offset,
                                    TILE,
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
        }
    }

    fn oam_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;
        let texture_size = self.oam.oam_texture.size;

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size, TILE);
        let selection = tile_selection(image_rect, texture_size, offset, TILE);

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
            res.clone().on_hover_ui_at_pointer(|ui| {
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

        let offset = translate_screen_pos_to_tile(pos, image_rect, texture_size, TILE);
        let selection = tile_selection(image_rect, texture_size, offset, TILE);

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

            res.clone().on_hover_ui_at_pointer(|ui| {
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

            // TODO: sprite index, palette address, position, horizontal/vertical flip/backgroud
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

    fn palette_pane(&mut self, ui: &mut Ui) {
        let mut zoom = self.palette.zoom;
        let selected = self.palette.selected;
        pane_header(
            ui,
            "palette",
            &mut zoom,
            false,
            selected.is_some(),
            |ui, menu| {
                if menu == HeaderMenu::Detail {
                    self.palette(ui, "palette_info_selected", selected);
                }
            },
        );
        self.palette.zoom = zoom;

        {
            ScrollArea::both().id_salt("palette_image").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let res = self
                        .palette_grid(ui, self.palette.zoom * self.palette.size)
                        .on_hover_cursor(CursorIcon::Cell);
                    let palette_rect = res.rect;

                    if let Some(pos) = res.hover_pos()
                        && palette_rect.contains(pos)
                    {
                        self.palette_hover(ui, &res, pos);
                    }

                    if let Some(offset) = self.palette.selected {
                        let selection =
                            tile_selection(palette_rect, self.palette.size, offset, SWATCH);
                        animated_dashed_rect(ui, selection, (1.0, Color32::WHITE), 3.0, 3.0);
                    }
                });
            });
        }
    }

    fn palette_hover(&mut self, ui: &mut Ui, res: &egui::Response, pos: Pos2) {
        let image_rect = res.rect;

        let offset = translate_screen_pos_to_tile(pos, image_rect, self.palette.size, SWATCH);
        let selection = tile_selection(image_rect, self.palette.size, offset, SWATCH);

        animated_dashed_rect(
            ui,
            selection,
            (1.0, Color32::from_white_alpha(220)),
            3.0,
            3.0,
        );

        res.clone().on_hover_ui_at_pointer(|ui| {
            self.palette(ui, "palette_hover", Some(offset));
        });
        if res.clicked() {
            self.palette.selected = Some(offset);
        }
    }

    fn palette_color_from_offset(&self, offset: Vec2) -> PaletteColor {
        let Vec2 { x, y } = offset;

        // Get row/column 32x32 palette and the palette table it's in
        let mut col = (x as u16 / SWATCH as u16).min(7);
        let row = (y as u16 / SWATCH as u16).min(3);
        let palette = if col >= 4 { 1 } else { 0 };

        // Wrap column to a single palette table
        col &= 3;

        let index = col + row * 4;
        let color_index = palette * 0x10 + index;
        let pixel_idx = color_index as usize * 4;
        PaletteColor {
            index: index as u8,
            addr: addr::PALETTE_START + color_index,
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

/// The dividers and tile grid one view draws over its image.
fn grid_settings(ui: &mut Ui, dividers: &mut bool, tile_grid: &mut bool) {
    let res = ui
        .checkbox(dividers, "Table Dividers")
        .on_hover_text("Show divider lines between tables.");
    if res.changed() {
        // TODO: update config
    }

    let res = ui
        .checkbox(tile_grid, "Tile Grid")
        .on_hover_text("Show grid lines between tiles.");
    if res.changed() {
        // TODO: update config
    }
}

/// A pane's own controls: how far its view is zoomed, and the two menus `menu` fills.
///
/// One row rather than a side panel, since four panes are on screen at once and each has only its
/// own view's width to work in.
fn pane_header(
    ui: &mut Ui,
    id: &str,
    zoom: &mut f32,
    settings: bool,
    selected: bool,
    mut menu: impl FnMut(&mut Ui, HeaderMenu),
) {
    // Every pane draws the same three widgets, so the pane's name keeps four ⚙ menus apart.
    ui.push_id(id, |ui| {
        ui.horizontal(|ui| {
            let drag = Slider::new(zoom, 0.1..=5.0).step_by(0.05).suffix("x");
            let res = ui.add(drag);
            if res.changed() {
                // TODO: update config
            }
            if settings {
                MenuButton::new("⚙")
                    .config(
                        MenuConfig::new().close_behavior(PopupCloseBehavior::CloseOnClickOutside),
                    )
                    .ui(ui, |ui| menu(ui, HeaderMenu::Settings));
            }
            ui.add_enabled_ui(selected, |ui| {
                MenuButton::new("Selected").ui(ui, |ui| menu(ui, HeaderMenu::Detail));
            });
        });
    });
    ui.separator();
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
fn translate_screen_pos_to_tile(
    pos: Pos2,
    image_rect: Rect,
    texture_size: Vec2,
    cell: f32,
) -> Vec2 {
    let normalized_pos = (pos - image_rect.min) / image_rect.size();
    let texture_pos = normalized_pos * texture_size;
    (texture_pos / cell).floor() * cell
}

/// Return tile selection rectangle given an offset.
fn tile_selection(image_rect: Rect, texture_size: Vec2, tile_offset: Vec2, cell: f32) -> Rect {
    let scale = image_rect.size() / texture_size;
    Rect::from_min_size(
        image_rect.min + scale * tile_offset,
        scale * Vec2::splat(cell),
    )
}
