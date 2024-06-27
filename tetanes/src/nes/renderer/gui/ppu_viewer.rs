use crate::{
    feature,
    nes::{config::Config, event::NesEventProxy, renderer::gui::lib::ViewportOptions},
};
use egui::{CentralPanel, Context, Ui, ViewportClass};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::warn;

#[derive(Debug)]
#[must_use]
pub struct State {
    _tx: NesEventProxy,
    tab: Tab,
}

#[derive(Debug)]
#[must_use]
pub struct PpuViewer {
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
    resources: Option<Config>,
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
    const TITLE: &'static str = "🎞 PPU Viewer";

    pub fn new(tx: NesEventProxy) -> Self {
        Self {
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                _tx: tx,
                tab: Tab::default(),
            })),
            resources: None,
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

    pub fn prepare(&mut self, cfg: &Config) {
        self.resources = Some(cfg.clone());
    }

    pub fn show(&mut self, ctx: &Context, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let mut viewport_builder = egui::ViewportBuilder::default().with_title(Self::TITLE);
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);
        let Some(cfg) = self.resources.take() else {
            warn!("PpuViewer::prepare was not called with required resources");
            return;
        };

        let viewport_id = egui::ViewportId::from_hash_of("ppu_viewer");
        fn viewport_cb(
            ctx: &Context,
            class: ViewportClass,
            open: &Arc<AtomicBool>,
            enabled: bool,
            state: &Arc<Mutex<State>>,
            cfg: &Config,
        ) {
            if class == ViewportClass::Embedded {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(PpuViewer::TITLE)
                    .open(&mut window_open)
                    .show(ctx, |ui| state.lock().ui(ui, enabled, cfg));
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ctx, |ui| state.lock().ui(ui, enabled, cfg));
                if ctx.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
        }

        if feature!(DeferredViewport) {
            ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, class| {
                viewport_cb(ctx, class, &open, opts.enabled, &state, &cfg);
            });
        } else {
            ctx.show_viewport_immediate(viewport_id, viewport_builder, move |ctx, class| {
                viewport_cb(ctx, class, &open, opts.enabled, &state, &cfg);
            });
        }
    }
}

impl State {
    fn ui(&mut self, ui: &mut Ui, enabled: bool, _cfg: &Config) {
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

    fn nametables(&mut self, _ui: &mut Ui) {}

    fn chr(&mut self, _ui: &mut Ui) {}

    fn sprites(&mut self, _ui: &mut Ui) {}

    fn palette(&mut self, _ui: &mut Ui) {}
}
