use crate::nes::{event::NesEventProxy, renderer::gui::lib::ViewportOptions};
use egui::{CentralPanel, Context, Ui, ViewportClass};
use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug)]
#[must_use]
pub struct State {
    tx: NesEventProxy,
    tab: Tab,
}

#[derive(Debug)]
#[must_use]
pub struct PpuViewer {
    open: Arc<AtomicBool>,
    state: Arc<RwLock<State>>,
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Nametables,
    Chr,
    Sprites,
    Palette,
}

impl PpuViewer {
    pub fn new(tx: NesEventProxy) -> Self {
        Self {
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(RwLock::new(State {
                tx,
                tab: Tab::default(),
            })),
        }
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
    }

    pub fn show(&mut self, ctx: &Context, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let title = "PPU Viewer";
        let mut viewport_builder = egui::ViewportBuilder::default().with_title(title);
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);

        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("ppu_viewer"),
            viewport_builder,
            move |ctx, class| {
                if class == ViewportClass::Embedded {
                    let mut window_open = open.load(Ordering::Acquire);
                    egui::Window::new(title)
                        .open(&mut window_open)
                        .show(ctx, |ui| state.write().ui(ui, opts.enabled));
                    open.store(window_open, Ordering::Release);
                } else {
                    CentralPanel::default().show(ctx, |ui| state.write().ui(ui, opts.enabled));
                    if ctx.input(|i| i.viewport().close_requested()) {
                        open.store(false, Ordering::Release);
                    }
                }
            },
        );
    }
}

impl State {
    fn ui(&mut self, ui: &mut Ui, enabled: bool) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Nametables, "Nametables");
                ui.selectable_value(&mut self.tab, Tab::Chr, "CHR");
                ui.selectable_value(&mut self.tab, Tab::Sprites, "Sprites");
                ui.selectable_value(&mut self.tab, Tab::Palette, "Palette");
            });

            ui.separator();

            match self.tab {
                Tab::Nametables => self.nametables(ui),
                Tab::Chr => self.chr(ui),
                Tab::Sprites => self.sprites(ui),
                Tab::Palette => self.palette(ui),
            }
        });
    }

    fn nametables(&mut self, ui: &mut Ui) {}

    fn chr(&mut self, ui: &mut Ui) {}

    fn sprites(&mut self, ui: &mut Ui) {}

    fn palette(&mut self, ui: &mut Ui) {}
}
