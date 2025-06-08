use crate::{
    feature,
    nes::{
        RunState,
        config::Config,
        event::{EmulationEvent, NesEvent, NesEventProxy, RendererEvent, UiEvent},
        input::Gamepads,
        renderer::{
            clipboard::Clipboard,
            event::translate_cursor,
            gui::{Gui, MessageType},
            painter::Painter,
        },
    },
    platform::{self, BuilderExt, Initialize},
    thread,
};
use anyhow::Context;
use crossbeam::channel::{self, Receiver};
use egui::{
    DeferredViewportUiCallback, OutputCommand, Vec2, ViewportBuilder, ViewportClass,
    ViewportCommand, ViewportId, ViewportIdMap, ViewportIdPair, ViewportIdSet, ViewportInfo,
    ViewportOutput, WindowLevel, ahash::HashMap,
};
use parking_lot::Mutex;
use std::{cell::RefCell, collections::hash_map::Entry, rc::Rc, sync::Arc};
use tetanes_core::{
    fs,
    ppu::Ppu,
    time::{Duration, Instant},
    video::Frame,
};
use thingbuf::{
    Recycle,
    mpsc::{blocking::Receiver as BufReceiver, errors::TryRecvError},
};
use tracing::{debug, error, info, trace};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{CursorGrabMode, Theme, Window, WindowButtons, WindowId},
};

pub mod clipboard;
pub mod event;
pub mod gui;
pub mod painter;
pub mod shader;
pub mod texture;

pub const OVERSCAN_TRIM: usize = (4 * Ppu::WIDTH * 8) as usize;

#[derive(Debug)]
#[must_use]
pub struct FrameRecycle;

impl Recycle<Frame> for FrameRecycle {
    fn new_element(&self) -> Frame {
        Frame::new()
    }

    fn recycle(&self, _frame: &mut Frame) {}
}

#[must_use]
pub struct State {
    pub(crate) viewports: ViewportIdMap<Viewport>,
    viewport_from_window: HashMap<WindowId, ViewportId>,
    pub(crate) focused: Option<ViewportId>,
    pointer_touch_id: Option<u64>,
    pub(crate) start_time: Instant,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("viewports", &self.viewports)
            .field("viewport_from_window", &self.viewport_from_window)
            .field("focused", &self.focused)
            .field("start_time", &self.focused)
            .finish()
    }
}

#[derive(Default)]
#[must_use]
pub struct Viewport {
    pub(crate) ids: ViewportIdPair,
    class: ViewportClass,
    builder: ViewportBuilder,
    pub(crate) info: ViewportInfo,
    pub(crate) raw_input: egui::RawInput,
    pub(crate) viewport_ui_cb: Option<Arc<DeferredViewportUiCallback>>,
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) occluded: bool,
    cursor_icon: Option<egui::CursorIcon>,
    cursor_pos: Option<egui::Pos2>,
    pub(crate) clipboard: Clipboard,
}

impl std::fmt::Debug for Viewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Viewport")
            .field("ids", &self.ids)
            // .field("class", &self.class) // why not?!
            .field("builder", &self.builder)
            .field("info", &self.info)
            .field("raw_input", &self.raw_input)
            .field(
                "viewport_ui_cb",
                &self.viewport_ui_cb.as_ref().map(|_| "fn"),
            )
            .field("window", &self.window)
            .field("occluded", &self.occluded)
            .field("cursor_icon", &self.cursor_icon)
            .field("clipboard", &self.clipboard)
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Renderer {
    pub(crate) state: Rc<RefCell<State>>,
    painter: Rc<RefCell<Painter>>,
    frame_rx: BufReceiver<Frame, FrameRecycle>,
    tx: NesEventProxy,
    redraw_tx: Arc<Mutex<NesEventProxy>>,
    pub(crate) gui: Rc<RefCell<Gui>>,
    pub(crate) ctx: egui::Context,
    #[cfg(not(target_arch = "wasm32"))]
    accesskit: accesskit_winit::Adapter,
    first_frame: bool,
    pub(crate) last_save_time: Instant,
    zoom_changed: bool,
    resize_texture: bool,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("state", &self.state)
            .field("painter", &self.painter)
            .field("frame_rx", &self.frame_rx)
            .field("tx", &self.tx)
            .field("redraw_tx", &self.redraw_tx)
            .field("gui", &self.gui)
            .field("ctx", &self.ctx)
            .field("first_frame", &self.first_frame)
            .field("last_save_time", &self.last_save_time)
            .field("zoom_changed", &self.zoom_changed)
            .field("resize_texture", &self.resize_texture)
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Resources {
    pub(crate) ctx: egui::Context,
    pub(crate) window: Arc<Window>,
    pub(crate) painter: Painter,
}

impl std::fmt::Debug for Resources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resources")
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Initializes the renderer in a platform-agnostic way.
    pub fn new(
        _event_loop: &ActiveEventLoop,
        tx: NesEventProxy,
        resources: Resources,
        frame_rx: BufReceiver<Frame, FrameRecycle>,
        cfg: &Config,
    ) -> anyhow::Result<Self> {
        let Resources {
            ctx,
            window,
            mut painter,
        } = resources;

        let redraw_tx = Arc::new(Mutex::new(tx.clone()));
        ctx.set_request_repaint_callback({
            let redraw_tx = redraw_tx.clone();
            move |info| {
                // IMPORTANT: Wasm can't block
                if let Some(tx) = redraw_tx.try_lock() {
                    tx.event(RendererEvent::RequestRedraw {
                        viewport_id: info.viewport_id,
                        when: Instant::now() + info.delay,
                    });
                } else {
                    tracing::warn!("failed to lock redraw_tx");
                }
            }
        });

        // Platforms like wasm don't easily support multiple viewports, and even if it could spawn
        // multiple canvases for each viewport, the async requirements of wgpu would make it
        // impossible to render until wasm-bindgen gets proper non-blocking async/await support.
        if feature!(OsViewports) {
            ctx.set_embed_viewports(cfg.renderer.embed_viewports);
        }

        let mut viewport_from_window = HashMap::default();
        viewport_from_window.insert(window.id(), ViewportId::ROOT);

        let mut viewports = ViewportIdMap::default();
        let mut viewport = Viewport {
            ids: ViewportIdPair::ROOT,
            class: ViewportClass::Root,
            info: ViewportInfo {
                title: Some(Config::WINDOW_TITLE.to_string()),
                ..Default::default()
            },
            window: Some(Arc::clone(&window)),
            ..Default::default()
        };
        Viewport::update_info(&mut viewport.info, &ctx, &window);
        viewports.insert(viewport.ids.this, viewport);

        painter.set_shader(cfg.renderer.shader);
        let render_state = painter.render_state_mut();
        let Some(render_state) = render_state else {
            anyhow::bail!("painter state is not initialized yet");
        };

        let gui = Rc::new(RefCell::new(Gui::new(
            ctx.clone(),
            tx.clone(),
            render_state,
            cfg,
        )));

        if let Err(err) = Self::load(&ctx, cfg) {
            tracing::error!("{err:?}");
        }

        // Must be done before the window is shown for the first time, which is true here, because
        // first_frame is set to true below
        #[cfg(not(target_arch = "wasm32"))]
        let accesskit =
            { accesskit_winit::Adapter::with_event_loop_proxy(&window, tx.inner().clone()) };

        let state = State {
            viewports,
            viewport_from_window,
            focused: None,
            pointer_touch_id: None,
            start_time: Instant::now(),
        };

        Ok(Self {
            state: Rc::new(RefCell::new(state)),
            painter: Rc::new(RefCell::new(painter)),
            frame_rx,
            tx,
            redraw_tx,
            ctx,
            #[cfg(not(target_arch = "wasm32"))]
            accesskit,
            gui,
            first_frame: true,
            last_save_time: Instant::now(),
            zoom_changed: false,
            resize_texture: false,
        })
    }

    pub fn destroy(&mut self) {
        let State {
            viewports,
            viewport_from_window,
            focused,
            ..
        } = &mut *self.state.borrow_mut();
        viewports.clear();
        viewport_from_window.clear();
        *focused = None;
        self.painter.borrow_mut().destroy();
    }

    pub fn root_window_id(&self) -> Option<WindowId> {
        self.window_id_for_viewport(ViewportId::ROOT)
    }

    pub fn window_id_for_viewport(&self, viewport_id: ViewportId) -> Option<WindowId> {
        let state = self.state.borrow();
        state
            .viewports
            .get(&viewport_id)
            .and_then(|viewport| viewport.window.as_ref())
            .map(|window| window.id())
    }

    pub fn viewport_id_for_window(&self, window_id: WindowId) -> Option<ViewportId> {
        let state = self.state.borrow();
        state
            .viewport_from_window
            .get(&window_id)
            .and_then(|id| state.viewports.get(id).map(|viewport| viewport.ids.this))
    }

    pub fn root_viewport<R>(&self, reader: impl FnOnce(&Viewport) -> R) -> Option<R> {
        let state = self.state.borrow();
        state.viewports.get(&ViewportId::ROOT).map(reader)
    }

    pub fn root_window(&self) -> Option<Arc<Window>> {
        self.root_viewport(|viewport| viewport.window.clone())
            .flatten()
    }

    pub fn window(&self, window_id: WindowId) -> Option<Arc<Window>> {
        let state = self.state.borrow();
        state.viewport_from_window.get(&window_id).and_then(|id| {
            state
                .viewports
                .get(id)
                .and_then(|viewport| viewport.window.clone())
        })
    }

    pub fn window_size(&self, cfg: &Config) -> Vec2 {
        self.window_size_for_scale(cfg, cfg.renderer.scale)
    }

    pub fn window_size_for_scale(&self, cfg: &Config, scale: f32) -> Vec2 {
        let gui = self.gui.borrow();
        let aspect_ratio = gui.aspect_ratio(cfg);
        let mut window_size = cfg.window_size_for_scale(aspect_ratio, scale);
        window_size.y += gui.menu_height;
        window_size
    }

    pub fn find_max_scale_for_width(&self, width: f32, cfg: &Config) -> f32 {
        let mut scale = cfg.renderer.scale;
        let mut size = self.window_size_for_scale(cfg, scale);
        while scale > 1.0 && size.x > width {
            scale -= 1.0;
            size = self.window_size_for_scale(cfg, scale);
        }
        scale
    }

    pub fn all_viewports_occluded(&self) -> bool {
        let state = self.state.borrow();
        state.viewports.values().all(|viewport| viewport.occluded)
    }

    pub fn inner_size(&self) -> Option<PhysicalSize<u32>> {
        self.root_window().map(|win| win.inner_size())
    }

    pub fn fullscreen(&self) -> bool {
        self.root_window()
            .map(|win| win.fullscreen().is_some())
            .unwrap_or(false)
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, embed_viewports: bool) {
        if feature!(OsViewports) {
            self.ctx.set_embed_viewports(fullscreen || embed_viewports);
        }
        self.ctx
            .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
        self.ctx
            .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Fullscreen(fullscreen));
    }

    pub fn set_embed_viewports(&mut self, embed: bool) {
        self.ctx.set_embed_viewports(embed);
    }

    pub fn set_always_on_top(&mut self, always_on_top: bool) {
        let state = self.state.borrow();
        for viewport_id in state.viewports.keys() {
            self.ctx.send_viewport_cmd_to(
                *viewport_id,
                ViewportCommand::WindowLevel(if always_on_top {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                }),
            );
        }
    }

    fn initialize_all_windows(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.ctx.embed_viewports() {
            return;
        }

        let State {
            viewports,
            viewport_from_window,
            ..
        } = &mut *self.state.borrow_mut();
        for viewport in viewports.values_mut() {
            viewport.initialize_window(
                self.tx.clone(),
                event_loop,
                &self.ctx,
                viewport_from_window,
                &self.painter,
            );
        }
    }

    pub fn rom_loaded(&self) -> bool {
        self.gui.borrow().loaded_rom.is_some()
    }

    pub fn add_message<S>(&mut self, ty: MessageType, text: S)
    where
        S: Into<String>,
    {
        self.gui.borrow_mut().add_message(ty, text);
        self.ctx.request_repaint();
    }

    pub fn on_error(&mut self, err: anyhow::Error) {
        error!("error: {err:?}");
        self.tx
            .event(EmulationEvent::RunState(RunState::AutoPaused));
        self.gui.borrow_mut().error = Some(err.to_string());
    }

    pub fn load(ctx: &egui::Context, cfg: &Config) -> anyhow::Result<()> {
        let path = Config::default_config_dir().join("gui.dat");
        if fs::exists(&path) {
            let data = fs::load_raw(path).context("failed to load gui memory")?;
            let config = bincode::config::legacy();
            let (memory, _) = bincode::serde::decode_from_slice(&data, config)
                .context("failed to deserialize gui memory")?;
            ctx.memory_mut(|mem| {
                *mem = memory;
            });
            info!("Loaded UI state");
        }
        ctx.memory_mut(|mem| {
            mem.options.zoom_factor = cfg.renderer.zoom;
        });
        Ok(())
    }

    pub fn auto_save(&mut self, cfg: &Config) -> anyhow::Result<()> {
        let time_since_last_save = Instant::now() - self.last_save_time;
        if time_since_last_save > Duration::from_secs(10) {
            self.save(cfg)?;
        }
        Ok(())
    }

    pub fn save(&mut self, cfg: &Config) -> anyhow::Result<()> {
        cfg.save()?;

        let path = Config::default_config_dir().join("gui.dat");
        self.ctx.memory(|mem| {
            let config = bincode::config::legacy();
            let data = bincode::serde::encode_to_vec(mem, config)
                .context("failed to serialize gui memory")?;
            fs::save_raw(path, &data).context("failed to save gui memory")
        })?;
        self.last_save_time = Instant::now();

        Ok(())
    }

    /// Request renderer resources (creating gui context, window, painter, etc).
    ///
    /// # Errors
    ///
    /// Returns an error if any resources can't be created correctly or `init_running` has already
    /// been called.
    pub fn request_resources(
        event_loop: &ActiveEventLoop,
        tx: &NesEventProxy,
        cfg: &Config,
    ) -> anyhow::Result<(egui::Context, Arc<Window>, Receiver<Painter>)> {
        let ctx = egui::Context::default();

        let window_size = cfg.window_size(cfg.deck.region.aspect_ratio());
        let mut builder = egui::ViewportBuilder::default()
            .with_title(Config::WINDOW_TITLE)
            .with_visible(false) // hide until first frame is rendered. required by AccessKit
            .with_fullscreen(cfg.renderer.fullscreen)
            .with_active(true)
            .with_resizable(true)
            .with_inner_size(window_size)
            .with_min_inner_size(Vec2::new(Ppu::WIDTH as f32, Ppu::HEIGHT as f32));
        if cfg.renderer.always_on_top {
            builder = builder.with_always_on_top();
        }
        let window = Arc::new(Self::create_window(&ctx, event_loop, builder)?);
        window.set_theme(Some(if cfg.renderer.dark_theme {
            Theme::Dark
        } else {
            Theme::Light
        }));

        let (painter_tx, painter_rx) = channel::bounded(1);
        thread::spawn({
            let window = Arc::clone(&window);
            let event_tx = tx.clone();
            async move {
                debug!("creating painter...");
                match Self::create_painter(window).await {
                    Ok(painter) => {
                        painter_tx.send(painter).expect("failed to send painter");
                        event_tx.event(RendererEvent::ResourcesReady);
                    }
                    Err(err) => {
                        error!("failed to create painter: {err:?}");
                        event_tx.event(UiEvent::Terminate);
                    }
                }
            }
        });

        Ok((ctx, window, painter_rx))
    }

    pub fn create_window(
        ctx: &egui::Context,
        event_loop: &ActiveEventLoop,
        builder: ViewportBuilder,
    ) -> anyhow::Result<Window> {
        let native_pixels_per_point = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next())
            .map_or_else(
                || {
                    tracing::debug!(
                        "Failed to find a monitor - assuming native_pixels_per_point of 1.0"
                    );
                    1.0
                },
                |m| m.scale_factor() as f32,
            );
        let zoom_factor = ctx.zoom_factor();
        let pixels_per_point = zoom_factor * native_pixels_per_point;

        let ViewportBuilder {
            title,
            position,
            inner_size,
            min_inner_size,
            max_inner_size,
            fullscreen,
            maximized,
            resizable,
            icon,
            active,
            visible,
            window_level,
            ..
        } = builder;

        let title = title.unwrap_or_else(|| Config::WINDOW_TITLE.to_owned());
        let mut window_attrs = Window::default_attributes()
            .with_title(title.clone())
            .with_resizable(resizable.unwrap_or(true))
            .with_visible(visible.unwrap_or(true))
            .with_maximized(maximized.unwrap_or(false))
            .with_window_level(match window_level.unwrap_or_default() {
                WindowLevel::AlwaysOnBottom => winit::window::WindowLevel::AlwaysOnBottom,
                WindowLevel::AlwaysOnTop => winit::window::WindowLevel::AlwaysOnTop,
                WindowLevel::Normal => winit::window::WindowLevel::Normal,
            })
            .with_fullscreen(
                fullscreen.and_then(|e| e.then_some(winit::window::Fullscreen::Borderless(None))),
            )
            .with_active(active.unwrap_or(true))
            .with_platform(&title);

        if let Some(size) = inner_size {
            window_attrs = window_attrs.with_inner_size(PhysicalSize::new(
                pixels_per_point * size.x,
                pixels_per_point * size.y,
            ));
        }

        if let Some(size) = min_inner_size {
            window_attrs = window_attrs.with_min_inner_size(PhysicalSize::new(
                pixels_per_point * size.x,
                pixels_per_point * size.y,
            ));
        }

        if let Some(size) = max_inner_size {
            window_attrs = window_attrs.with_max_inner_size(PhysicalSize::new(
                pixels_per_point * size.x,
                pixels_per_point * size.y,
            ));
        }

        if let Some(pos) = position {
            window_attrs = window_attrs.with_position(PhysicalPosition::new(
                pixels_per_point * pos.x,
                pixels_per_point * pos.y,
            ));
        }

        if let Some(icon) = icon {
            let winit_icon = gui::lib::to_winit_icon(&icon);
            window_attrs = window_attrs.with_window_icon(winit_icon);
        }

        let window = event_loop.create_window(window_attrs)?;

        if let Some(size) = inner_size {
            if window
                .request_inner_size(PhysicalSize::new(
                    pixels_per_point * size.x,
                    pixels_per_point * size.y,
                ))
                .is_some()
            {
                debug!("Failed to set window size");
            }
        }
        if let Some(size) = min_inner_size {
            window.set_min_inner_size(Some(PhysicalSize::new(
                pixels_per_point * size.x,
                pixels_per_point * size.y,
            )));
        }

        debug!("created new window: {:?}", window.id());

        Ok(window)
    }

    pub async fn create_painter(window: Arc<Window>) -> anyhow::Result<Painter> {
        // The window must be ready with a non-zero size before `Painter::set_window` is called,
        // otherwise the wgpu surface won't be configured correctly.
        let start = Instant::now();
        loop {
            let size = window.inner_size();
            if size.width > 0 && size.height > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(10)).await;
        }
        debug!(
            "waited {:.02}s for window creation",
            start.elapsed().as_secs_f32()
        );

        let mut painter = Painter::new();
        painter
            .set_window(ViewportId::ROOT, Some(Arc::clone(&window)))
            .await?;

        Ok(painter)
    }

    pub fn recreate_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.ctx.embed_viewports() {
            return;
        }

        let State {
            viewports,
            viewport_from_window,
            ..
        } = &mut *self.state.borrow_mut();
        let builder = viewports
            .get(&ViewportId::ROOT)
            .map(|viewport| viewport.builder.clone())
            .unwrap_or_default();
        let viewport = Self::create_or_update_viewport(
            &self.ctx,
            viewports,
            ViewportIdPair::ROOT,
            ViewportClass::Root,
            builder,
            None,
        );

        viewport.initialize_window(
            self.tx.clone(),
            event_loop,
            &self.ctx,
            viewport_from_window,
            &self.painter,
        );
    }

    pub fn drop_window(&mut self) -> anyhow::Result<()> {
        if self.ctx.embed_viewports() {
            return Ok(());
        }
        let mut state = self.state.borrow_mut();
        state.viewports.remove(&ViewportId::ROOT);
        Renderer::set_painter_window(
            self.tx.clone(),
            Rc::clone(&self.painter),
            ViewportId::ROOT,
            None,
        );
        Ok(())
    }

    fn set_painter_window(
        tx: NesEventProxy,
        painter: Rc<RefCell<Painter>>,
        viewport_id: ViewportId,
        window: Option<Arc<Window>>,
    ) {
        // This is fine because we won't be yielding. Native platforms call `block_on` and
        // wasm is single-threaded with `spawn_local` and runs on the next microtick.
        #[allow(clippy::await_holding_refcell_ref)]
        thread::spawn(async move {
            if let Err(err) = painter.borrow_mut().set_window(viewport_id, window).await {
                error!("failed to set painter window on viewport id {viewport_id:?}: {err:?}");
                tx.event(NesEvent::Ui(UiEvent::Terminate));
            }
        });
    }

    fn create_or_update_viewport<'a>(
        ctx: &egui::Context,
        viewports: &'a mut ViewportIdMap<Viewport>,
        ids: ViewportIdPair,
        class: ViewportClass,
        mut builder: ViewportBuilder,
        viewport_ui_cb: Option<Arc<DeferredViewportUiCallback>>,
    ) -> &'a mut Viewport {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if builder.icon.is_none() {
            builder.icon = viewports
                .get_mut(&ids.parent)
                .and_then(|viewport| viewport.builder.icon.clone());
        }

        match viewports.entry(ids.this) {
            Entry::Vacant(entry) => entry.insert(Viewport {
                ids,
                class,
                builder,
                viewport_ui_cb,
                ..Default::default()
            }),
            Entry::Occupied(mut entry) => {
                let viewport = entry.get_mut();
                viewport.class = class;
                viewport.ids.parent = ids.parent;
                viewport.info.parent = Some(ids.parent);
                viewport.viewport_ui_cb = viewport_ui_cb;

                let (delta_commands, recreate) = viewport.builder.patch(builder);
                if recreate {
                    viewport.window = None;
                    viewport.raw_input = Default::default();
                    viewport.cursor_icon = None;
                } else if let Some(window) = &viewport.window {
                    Self::process_viewport_commands(
                        ctx,
                        &mut viewport.info,
                        delta_commands,
                        window,
                    );
                }

                entry.into_mut()
            }
        }
    }

    pub fn handle_platform_output(viewport: &mut Viewport, platform_output: egui::PlatformOutput) {
        let egui::PlatformOutput {
            cursor_icon,
            commands,
            ..
        } = platform_output;

        viewport.set_cursor(cursor_icon);

        for command in commands {
            match command {
                OutputCommand::OpenUrl(open_url) => Self::open_url_in_browser(&open_url.url),
                OutputCommand::CopyText(copied_text) => {
                    if !copied_text.is_empty() {
                        viewport.clipboard.set(copied_text);
                    }
                }
                OutputCommand::CopyImage(_) => (),
            }
        }
    }

    fn open_url_in_browser(url: &str) {
        if let Err(err) = webbrowser::open(url) {
            tracing::warn!("failed to open url: {err:?}");
        }
    }

    fn handle_viewport_output(
        ctx: &egui::Context,
        viewports: &mut ViewportIdMap<Viewport>,
        outputs: ViewportIdMap<ViewportOutput>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        for (id, output) in outputs {
            let ids = ViewportIdPair::from_self_and_parent(id, output.parent);
            let viewport = Self::create_or_update_viewport(
                ctx,
                viewports,
                ids,
                output.class,
                output.builder,
                output.viewport_ui_cb,
            );
            if let Some(window) = viewport.window.as_ref() {
                Self::process_viewport_commands(ctx, &mut viewport.info, output.commands, window);
            }
        }
    }

    fn process_viewport_commands(
        ctx: &egui::Context,
        info: &mut ViewportInfo,
        commands: impl IntoIterator<Item = ViewportCommand>,
        window: &Window,
    ) {
        let pixels_per_point = gui::lib::pixels_per_point(ctx, window);
        for command in commands {
            match command {
                ViewportCommand::Close => {
                    info.events.push(egui::ViewportEvent::Close);
                }
                ViewportCommand::StartDrag => {
                    // If `.has_focus()` is not checked on x11 the input will be permanently taken until the app is killed!
                    if window.has_focus() {
                        if let Err(err) = window.drag_window() {
                            tracing::warn!("{command:?}: {err}");
                        }
                    }
                }
                ViewportCommand::InnerSize(size) => {
                    let width_px = pixels_per_point * size.x.max(1.0);
                    let height_px = pixels_per_point * size.y.max(1.0);
                    let requested_size = PhysicalSize::new(width_px, height_px);
                    if let Some(_returned_inner_size) = window.request_inner_size(requested_size) {
                        // On platforms where the size is entirely controlled by the user the
                        // applied size will be returned immediately, resize event in such case
                        // may not be generated.
                        // e.g. Linux

                        // On platforms where resizing is disallowed by the windowing system, the current
                        // inner size is returned immediately, and the user one is ignored.
                        // e.g. Android, iOS, …

                        // However, comparing the results is prone to numerical errors
                        // because the linux backend converts physical to logical and back again.
                        // So let's just assume it worked:

                        info.inner_rect = gui::lib::inner_rect_in_points(window, pixels_per_point);
                        info.outer_rect = gui::lib::outer_rect_in_points(window, pixels_per_point);
                    } else {
                        // e.g. macOS, Windows
                        // The request went to the display system,
                        // and the actual size will be delivered later with the [`WindowEvent::Resized`].
                    }
                }
                ViewportCommand::BeginResize(direction) => {
                    use egui::viewport::ResizeDirection as EguiResizeDirection;
                    use winit::window::ResizeDirection;

                    if let Err(err) = window.drag_resize_window(match direction {
                        EguiResizeDirection::North => ResizeDirection::North,
                        EguiResizeDirection::South => ResizeDirection::South,
                        EguiResizeDirection::East => ResizeDirection::East,
                        EguiResizeDirection::West => ResizeDirection::West,
                        EguiResizeDirection::NorthEast => ResizeDirection::NorthEast,
                        EguiResizeDirection::SouthEast => ResizeDirection::SouthEast,
                        EguiResizeDirection::NorthWest => ResizeDirection::NorthWest,
                        EguiResizeDirection::SouthWest => ResizeDirection::SouthWest,
                    }) {
                        tracing::warn!("{command:?}: {err}");
                    }
                }
                ViewportCommand::Title(title) => {
                    window.set_title(&title);
                }
                ViewportCommand::Transparent(v) => window.set_transparent(v),
                ViewportCommand::Visible(v) => window.set_visible(v),
                ViewportCommand::OuterPosition(pos) => {
                    window.set_outer_position(PhysicalPosition::new(
                        pixels_per_point * pos.x,
                        pixels_per_point * pos.y,
                    ));
                }
                ViewportCommand::MinInnerSize(s) => {
                    window.set_min_inner_size((s.is_finite() && s != Vec2::ZERO).then_some(
                        PhysicalSize::new(pixels_per_point * s.x, pixels_per_point * s.y),
                    ));
                }
                ViewportCommand::MaxInnerSize(s) => {
                    window.set_max_inner_size((s.is_finite() && s != Vec2::INFINITY).then_some(
                        PhysicalSize::new(pixels_per_point * s.x, pixels_per_point * s.y),
                    ));
                }
                ViewportCommand::ResizeIncrements(s) => {
                    window.set_resize_increments(s.map(|s| {
                        PhysicalSize::new(pixels_per_point * s.x, pixels_per_point * s.y)
                    }));
                }
                ViewportCommand::Resizable(v) => window.set_resizable(v),
                ViewportCommand::EnableButtons {
                    close,
                    minimized,
                    maximize,
                } => window.set_enabled_buttons(
                    if close {
                        WindowButtons::CLOSE
                    } else {
                        WindowButtons::empty()
                    } | if minimized {
                        WindowButtons::MINIMIZE
                    } else {
                        WindowButtons::empty()
                    } | if maximize {
                        WindowButtons::MAXIMIZE
                    } else {
                        WindowButtons::empty()
                    },
                ),
                ViewportCommand::Minimized(v) => {
                    window.set_minimized(v);
                    info.minimized = Some(v);
                }
                ViewportCommand::Maximized(v) => {
                    window.set_maximized(v);
                    info.maximized = Some(v);
                }
                ViewportCommand::Fullscreen(v) => {
                    window.set_fullscreen(v.then_some(winit::window::Fullscreen::Borderless(None)));
                    info.fullscreen = Some(v);
                }
                ViewportCommand::Decorations(v) => window.set_decorations(v),
                ViewportCommand::WindowLevel(l) => {
                    use egui::viewport::WindowLevel as EguiWindowLevel;
                    use winit::window::WindowLevel;
                    window.set_window_level(match l {
                        EguiWindowLevel::AlwaysOnBottom => WindowLevel::AlwaysOnBottom,
                        EguiWindowLevel::AlwaysOnTop => WindowLevel::AlwaysOnTop,
                        EguiWindowLevel::Normal => WindowLevel::Normal,
                    });
                }
                ViewportCommand::Icon(icon) => {
                    let winit_icon = icon.and_then(|icon| gui::lib::to_winit_icon(&icon));
                    window.set_window_icon(winit_icon);
                }
                ViewportCommand::IMERect(rect) => {
                    window.set_ime_cursor_area(
                        PhysicalPosition::new(
                            pixels_per_point * rect.min.x,
                            pixels_per_point * rect.min.y,
                        ),
                        PhysicalSize::new(
                            pixels_per_point * rect.size().x,
                            pixels_per_point * rect.size().y,
                        ),
                    );
                }
                ViewportCommand::IMEAllowed(v) => window.set_ime_allowed(v),
                ViewportCommand::IMEPurpose(p) => window.set_ime_purpose(match p {
                    egui::viewport::IMEPurpose::Password => winit::window::ImePurpose::Password,
                    egui::viewport::IMEPurpose::Terminal => winit::window::ImePurpose::Terminal,
                    egui::viewport::IMEPurpose::Normal => winit::window::ImePurpose::Normal,
                }),
                ViewportCommand::Focus => {
                    if !window.has_focus() {
                        window.focus_window();
                    }
                }
                ViewportCommand::RequestUserAttention(a) => {
                    window.request_user_attention(match a {
                        egui::UserAttentionType::Reset => None,
                        egui::UserAttentionType::Critical => {
                            Some(winit::window::UserAttentionType::Critical)
                        }
                        egui::UserAttentionType::Informational => {
                            Some(winit::window::UserAttentionType::Informational)
                        }
                    });
                }
                ViewportCommand::SetTheme(t) => window.set_theme(match t {
                    egui::SystemTheme::Light => Some(winit::window::Theme::Light),
                    egui::SystemTheme::Dark => Some(winit::window::Theme::Dark),
                    egui::SystemTheme::SystemDefault => None,
                }),
                ViewportCommand::ContentProtected(v) => window.set_content_protected(v),
                ViewportCommand::CursorPosition(pos) => {
                    if let Err(err) = window.set_cursor_position(PhysicalPosition::new(
                        pixels_per_point * pos.x,
                        pixels_per_point * pos.y,
                    )) {
                        tracing::warn!("{command:?}: {err}");
                    }
                }
                ViewportCommand::CursorGrab(o) => {
                    if let Err(err) = window.set_cursor_grab(match o {
                        egui::viewport::CursorGrab::None => CursorGrabMode::None,
                        egui::viewport::CursorGrab::Confined => CursorGrabMode::Confined,
                        egui::viewport::CursorGrab::Locked => CursorGrabMode::Locked,
                    }) {
                        tracing::warn!("{command:?}: {err}");
                    }
                }
                ViewportCommand::CursorVisible(v) => window.set_cursor_visible(v),
                ViewportCommand::MousePassthrough(passthrough) => {
                    if let Err(err) = window.set_cursor_hittest(!passthrough) {
                        tracing::warn!("{command:?}: {err}");
                    }
                }
                _ => (),
            }
        }
    }

    /// Request redraw.
    pub fn redraw(
        &mut self,
        window_id: WindowId,
        event_loop: &ActiveEventLoop,
        gamepads: &mut Gamepads,
        cfg: &mut Config,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.first_frame {
            self.initialize()?;
            self.resize_window(cfg);
        }
        self.initialize_all_windows(event_loop);

        if self.all_viewports_occluded() {
            return Ok(());
        }

        let Some(viewport_id) = self.viewport_id_for_window(window_id) else {
            return Ok(());
        };

        #[cfg(feature = "profiling")]
        puffin::GlobalProfiler::lock().new_frame();

        self.handle_resize(viewport_id, cfg);

        let (viewport_ui_cb, viewport_info, raw_input) = {
            let State {
                viewports,
                start_time,
                ..
            } = &mut *self.state.borrow_mut();

            let Some(viewport) = viewports.get_mut(&viewport_id) else {
                return Ok(());
            };
            let Some(window) = &viewport.window else {
                return Ok(());
            };
            // Always render the root viewport unless all viewports are occluded to ensure deferred
            // viewports correctly get Config and Gamepads updates.
            if viewport.occluded && viewport_id != ViewportId::ROOT {
                return Ok(());
            }

            Viewport::update_info(&mut viewport.info, &self.ctx, window);

            let viewport_ui_cb = viewport.viewport_ui_cb.clone();

            // On Windows, a minimized window will have 0 width and height.
            // See: https://github.com/rust-windowing/winit/issues/208
            // This solves an issue where egui window positions would be changed when minimizing on Windows.
            let screen_size_in_pixels = gui::lib::screen_size_in_pixels(window);
            let screen_size_in_points =
                screen_size_in_pixels / gui::lib::pixels_per_point(&self.ctx, window);

            let viewport_info = viewport.info.clone();
            let mut raw_input = viewport.raw_input.take();
            raw_input.time = Some(start_time.elapsed().as_secs_f64());
            raw_input.screen_rect = (screen_size_in_points.x > 0.0
                && screen_size_in_points.y > 0.0)
                .then(|| egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size_in_points));
            raw_input.viewport_id = viewport_id;
            raw_input
                .viewports
                .entry(viewport_id)
                .or_default()
                .native_pixels_per_point = Some(window.scale_factor() as f32);

            (viewport_ui_cb, viewport_info, raw_input)
        };

        // Copy NES frame buffer before drawing UI because a UI interaction might cause a texture
        // resize tied to a configuration change.
        if viewport_id == ViewportId::ROOT {
            if let Some(render_state) = &self.painter.borrow().render_state() {
                let mut frame_buffer = self.frame_rx.try_recv_ref();
                while self.frame_rx.remaining() < 2 {
                    trace!("skipping frame");
                    frame_buffer = self.frame_rx.try_recv_ref();
                }
                match frame_buffer {
                    Ok(frame_buffer) => {
                        let gui = self.gui.borrow_mut();
                        let is_ntsc = gui.loaded_region().unwrap_or(cfg.deck.region).is_ntsc();
                        gui.nes_texture.update(
                            &render_state.queue,
                            if cfg.renderer.hide_overscan && is_ntsc {
                                &frame_buffer[OVERSCAN_TRIM..frame_buffer.len() - OVERSCAN_TRIM]
                            } else {
                                &frame_buffer
                            },
                        );
                    }
                    Err(TryRecvError::Closed) => {
                        error!("frame channel closed unexpectedly, exiting");
                        event_loop.exit();
                        return Ok(());
                    }
                    // Empty frames are fine as we may repaint more often than 60fps due to
                    // UI interactions with keyboard/mouse
                    _ => (),
                }
            }
        }

        // Mutated by accesskit below on platforms that support it
        #[allow(unused_mut)]
        let mut output = self.ctx.run(raw_input, |ctx| {
            match &viewport_ui_cb {
                Some(viewport_ui_cb) => viewport_ui_cb(ctx),
                None => self.gui.borrow_mut().ui(ctx, cfg, gamepads),
            }
            self.gui
                .borrow_mut()
                .show_viewport_info_window(&self.ctx, viewport_id, &viewport_info);
        });

        {
            let State {
                viewports,
                viewport_from_window,
                ..
            } = &mut *self.state.borrow_mut();

            let Some(viewport) = viewports.get_mut(&viewport_id) else {
                return Ok(());
            };

            viewport.info.events.clear(); // they should have been processed

            let Viewport {
                window: Some(window),
                ..
            } = viewport
            else {
                return Ok(());
            };

            let clipped_primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

            window.pre_present_notify();
            self.painter.borrow_mut().paint(
                viewport_id,
                output.pixels_per_point,
                &clipped_primitives,
                &output.textures_delta,
            );

            if std::mem::take(&mut self.first_frame) {
                window.set_visible(true);
            }

            let active_viewports_ids = output
                .viewport_output
                .keys()
                .copied()
                .collect::<ViewportIdSet>();

            if feature!(ScreenReader) && self.ctx.options(|o| o.screen_reader) {
                platform::speak_text(&output.platform_output.events_description());
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(update) = output.platform_output.accesskit_update.take() {
                tracing::trace!("update accesskit: {update:?}");
                self.accesskit.update_if_active(|| update);
            }

            Self::handle_platform_output(viewport, output.platform_output);
            Self::handle_viewport_output(&self.ctx, viewports, output.viewport_output);
            if std::mem::take(&mut self.zoom_changed) {
                cfg.renderer.zoom = self.ctx.zoom_factor();
            }

            // Prune dead viewports
            viewports.retain(|id, _| active_viewports_ids.contains(id));
            viewport_from_window.retain(|_, id| active_viewports_ids.contains(id));
            self.painter
                .borrow_mut()
                .retain_surfaces(&active_viewports_ids);
        }

        if let Err(err) = self.auto_save(cfg) {
            error!("failed to auto save UI state: {err:?}");
        }

        Ok(())
    }

    fn handle_resize(&mut self, viewport_id: ViewportId, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if viewport_id == ViewportId::ROOT && self.resize_texture {
            tracing::debug!("resizing window and texture");

            self.tx.event(EmulationEvent::RequestFrame);
            self.resize_window(cfg);

            if let Some(render_state) = self.painter.borrow_mut().render_state_mut() {
                let texture_size = cfg.texture_size();
                let mut gui = self.gui.borrow_mut();
                let aspect_ratio = gui.aspect_ratio(cfg);
                gui.nes_texture
                    .resize(render_state, texture_size, aspect_ratio);
            }
            self.resize_texture = false;
        }
    }

    fn resize_window(&self, cfg: &Config) {
        if !self.fullscreen() {
            let desired_window_size = self.window_size(cfg);

            // On some platforms, e.g. wasm, window width is constrained by the
            // viewport width, so try to find the max scale that will fit
            if feature!(ConstrainedViewport) {
                let res = platform::renderer::constrain_window_to_viewport(
                    self,
                    desired_window_size.x,
                    cfg,
                );
                if res.consumed {
                    return;
                }
            }

            if let Some(window) = self.root_window() {
                tracing::debug!("resizing window: {desired_window_size:?}");

                let _ = window.request_inner_size(LogicalSize::new(
                    desired_window_size.x,
                    desired_window_size.y,
                ));
            }
        }
    }
}

impl Viewport {
    pub fn initialize_window(
        &mut self,
        tx: NesEventProxy,
        event_loop: &ActiveEventLoop,
        ctx: &egui::Context,
        viewport_from_window: &mut HashMap<WindowId, ViewportId>,
        painter: &Rc<RefCell<Painter>>,
    ) {
        if self.window.is_some() {
            return;
        }

        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let viewport_id = self.ids.this;

        match Renderer::create_window(ctx, event_loop, self.builder.clone()) {
            Ok(window) => {
                viewport_from_window.insert(window.id(), viewport_id);
                let window = Arc::new(window);

                Renderer::set_painter_window(
                    tx,
                    Rc::clone(painter),
                    viewport_id,
                    Some(Arc::clone(&window)),
                );

                debug!(
                    "created new viewport window: {:?} ({:?})",
                    self.builder.title,
                    window.id()
                );

                self.info.title = self.builder.title.clone();
                self.info.minimized = window.is_minimized();
                self.info.maximized = Some(window.is_maximized());
                self.window = Some(window);
            }
            Err(err) => error!("Failed to create window: {err}"),
        }
    }

    pub fn update_info(info: &mut ViewportInfo, ctx: &egui::Context, window: &Window) {
        let pixels_per_point = gui::lib::pixels_per_point(ctx, window);
        let has_position = window.is_minimized().is_none_or(|minimized| !minimized);

        let inner_rect = has_position
            .then(|| gui::lib::inner_rect_in_points(window, pixels_per_point))
            .flatten();
        let outer_rect = has_position
            .then(|| gui::lib::outer_rect_in_points(window, pixels_per_point))
            .flatten();

        let monitor_size = window.current_monitor().map(|monitor| {
            let size = monitor.size().to_logical::<f32>(pixels_per_point.into());
            egui::vec2(size.width, size.height)
        });

        let title = window.title();
        if !title.is_empty() {
            info.title = Some(title);
        }
        info.native_pixels_per_point = Some(window.scale_factor() as f32);

        info.monitor_size = monitor_size;
        info.inner_rect = inner_rect;
        info.outer_rect = outer_rect;

        if !cfg!(target_os = "macos") {
            // Asking for minimized/maximized state at runtime can lead to a deadlock on macOS
            info.maximized = Some(window.is_maximized());
            info.minimized = Some(window.is_minimized().unwrap_or(false));
        }

        info.fullscreen = Some(window.fullscreen().is_some());
        info.focused = Some(window.has_focus());
    }

    fn set_cursor(&mut self, cursor_icon: egui::CursorIcon) {
        if self.cursor_icon == Some(cursor_icon) {
            // Prevent flickering near frame boundary when Windows OS tries to control cursor icon for window resizing.
            // On other platforms: just early-out to save CPU.
            return;
        }
        let Some(window) = &self.window else {
            return;
        };

        let is_pointer_in_window = self.cursor_pos.is_some();
        if is_pointer_in_window {
            self.cursor_icon = Some(cursor_icon);

            if let Some(cursor) = translate_cursor(cursor_icon) {
                window.set_cursor_visible(true);
                window.set_cursor(cursor);
            } else {
                window.set_cursor_visible(false);
            }
        } else {
            self.cursor_icon = None;
        }
    }
}
