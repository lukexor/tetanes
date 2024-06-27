use crate::{
    feature,
    nes::{
        config::Config,
        event::{
            ConfigEvent, EmulationEvent, NesEvent, NesEventProxy, RendererEvent, RunState, UiEvent,
        },
        input::Gamepads,
        renderer::{
            gui::{
                lib::{is_paste_command, key_from_keycode},
                Gui, MessageType,
            },
            shader::Shader,
            texture::Texture,
        },
    },
    platform::{self, BuilderExt, Initialize},
    thread,
};
use anyhow::Context;
use egui::{
    ahash::HashMap, DeferredViewportUiCallback, ImmediateViewport, SystemTheme, Vec2,
    ViewportBuilder, ViewportClass, ViewportCommand, ViewportId, ViewportIdMap, ViewportIdPair,
    ViewportIdSet, ViewportInfo, ViewportOutput, WindowLevel,
};
use egui_wgpu::{winit::Painter, RenderState};
use egui_winit::EventResponse;
use parking_lot::Mutex;
use std::{cell::RefCell, collections::hash_map::Entry, rc::Rc, sync::Arc};
use tetanes_core::{
    fs,
    ppu::Ppu,
    time::{Duration, Instant},
    video::Frame,
};
use thingbuf::{
    mpsc::{blocking::Receiver as BufReceiver, errors::TryRecvError},
    Recycle,
};
use tracing::{debug, error, info, warn};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::EventLoopWindowTarget,
    window::{Theme, Window, WindowId},
};

pub mod gui;
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
    painter: Rc<RefCell<Painter>>,
    focused: Option<ViewportId>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("viewports", &self.viewports)
            .field("viewport_from_window", &self.viewport_from_window)
            .field("focused", &self.focused)
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Viewport {
    ids: ViewportIdPair,
    class: ViewportClass,
    builder: ViewportBuilder,
    pub(crate) info: ViewportInfo,
    viewport_ui_cb: Option<Arc<DeferredViewportUiCallback>>,
    screenshot_requested: bool,
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) egui_state: Option<egui_winit::State>,
    occluded: bool,
}

impl std::fmt::Debug for Viewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Viewport")
            .field("ids", &self.ids)
            .field("builder", &self.builder)
            .field("info", &self.info)
            .field("screenshot_requested", &self.screenshot_requested)
            .field("window", &self.window)
            .field("occluded", &self.occluded)
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Renderer {
    pub(crate) state: Rc<RefCell<State>>,
    frame_rx: BufReceiver<Frame, FrameRecycle>,
    tx: NesEventProxy,
    redraw_tx: Arc<Mutex<NesEventProxy>>,
    pub(crate) gui: Rc<RefCell<Gui>>,
    pub(crate) ctx: egui::Context,
    render_state: Option<RenderState>,
    texture: Texture,
    first_frame: bool,
    pub(crate) last_save_time: Instant,
    zoom_changed: bool,
    resize_texture: bool,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("state", &self.state)
            .field("frame_rx", &self.frame_rx)
            .field("tx", &self.tx)
            .field("redraw_tx", &self.redraw_tx)
            .field("gui", &self.gui)
            .field("ctx", &self.ctx)
            .field("texture", &self.texture)
            .field("first_frame", &self.first_frame)
            .field("last_save_time", &self.last_save_time)
            .field("zoom_changed", &self.zoom_changed)
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct Resources {
    pub(crate) ctx: egui::Context,
    pub(crate) window: Arc<Window>,
    pub(crate) viewport_builder: ViewportBuilder,
    pub(crate) painter: Painter,
}

impl std::fmt::Debug for Resources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resources")
            .field("window", &self.window)
            .field("viewport_builder", &self.viewport_builder)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Initializes the renderer in a platform-agnostic way.
    pub fn new(
        tx: NesEventProxy,
        event_loop: &EventLoopWindowTarget<NesEvent>,
        resources: Resources,
        frame_rx: BufReceiver<Frame, FrameRecycle>,
        cfg: &Config,
    ) -> anyhow::Result<Self> {
        let Resources {
            ctx,
            window,
            viewport_builder,
            painter,
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
        if feature!(Viewports) {
            ctx.set_embed_viewports(cfg.renderer.embed_viewports);
        }

        let max_texture_side = painter.max_texture_side();
        #[allow(unused_mut)]
        let mut egui_state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            max_texture_side,
        );

        #[cfg(not(target_arch = "wasm32"))]
        if feature!(AccessKit) {
            egui_state.init_accesskit(&window, tx.inner().clone(), {
                let ctx = ctx.clone();
                move || {
                    ctx.enable_accesskit();
                    ctx.request_repaint();
                    ctx.accesskit_placeholder_tree_update()
                }
            });
        }

        let mut viewport_from_window = HashMap::default();
        viewport_from_window.insert(window.id(), ViewportId::ROOT);

        let mut viewports = ViewportIdMap::default();
        viewports.insert(
            ViewportId::ROOT,
            Viewport {
                ids: ViewportIdPair::ROOT,
                class: ViewportClass::Root,
                builder: viewport_builder.clone(),
                info: ViewportInfo {
                    minimized: window.is_minimized(),
                    maximized: Some(window.is_maximized()),
                    ..Default::default()
                },
                viewport_ui_cb: None,
                screenshot_requested: false,
                window: Some(Arc::clone(&window)),
                egui_state: Some(egui_state),
                occluded: false,
            },
        );

        let render_state = painter.render_state();
        let (Some(max_texture_side), Some(render_state)) = (max_texture_side, render_state) else {
            anyhow::bail!("render state is not initialized yet");
        };

        let texture_size = cfg.texture_size();
        let texture = Texture::new(
            &render_state.device,
            &mut render_state.renderer.write(),
            texture_size.x.min(max_texture_side as f32) as u32,
            texture_size.y.min(max_texture_side as f32) as u32,
            cfg.deck.region.aspect_ratio(),
            Some("nes frame"),
        );

        Self::set_shader_resource(&render_state, &texture.view, cfg.renderer.shader);

        let gui = Rc::new(RefCell::new(Gui::new(
            tx.clone(),
            texture.sized_texture(),
            cfg.clone(),
        )));

        let state = Rc::new(RefCell::new(State {
            viewports,
            painter: Rc::new(RefCell::new(painter)),
            viewport_from_window,
            focused: Some(ViewportId::ROOT),
        }));

        {
            let tx = tx.clone();
            let state = Rc::downgrade(&state);
            let event_loop: *const EventLoopWindowTarget<NesEvent> = event_loop;
            egui::Context::set_immediate_viewport_renderer(move |ctx, viewport| {
                if let Some(state) = state.upgrade() {
                    // SAFETY: the event loop lives longer than the Rcs we just upgraded above.
                    match unsafe { event_loop.as_ref() } {
                        Some(event_loop) => {
                            Self::render_immediate_viewport(&tx, event_loop, ctx, &state, viewport);
                        }
                        None => tracing::error!(
                            "failed to get event_loop in set_immediate_viewport_renderer"
                        ),
                    }
                } else {
                    warn!("set_immediate_viewport_renderer called after window closed");
                }
            });
        }

        if let Err(err) = Self::load(&ctx, cfg) {
            tracing::error!("{err:?}");
        }

        Ok(Self {
            state,
            frame_rx,
            tx,
            redraw_tx,
            ctx,
            gui,
            render_state: Some(render_state),
            texture,
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
            painter,
            ..
        } = &mut *self.state.borrow_mut();
        viewports.clear();
        viewport_from_window.clear();
        let mut painter = painter.borrow_mut();
        painter.gc_viewports(&ViewportIdSet::default());
        painter.destroy();
    }

    pub fn root_window_id(&self) -> Option<WindowId> {
        self.window_id_for_viewport(ViewportId::ROOT)
    }

    pub fn window_id_for_viewport(&self, viewport_id: ViewportId) -> Option<WindowId> {
        self.state
            .borrow()
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
            .and_then(|id| state.viewports.get(id))
            .map(|viewport| viewport.ids.this)
    }

    pub fn root_viewport<R>(&self, reader: impl FnOnce(&Viewport) -> R) -> Option<R> {
        self.state
            .borrow()
            .viewports
            .get(&ViewportId::ROOT)
            .map(reader)
    }

    pub fn root_window(&self) -> Option<Arc<Window>> {
        self.root_viewport(|viewport| viewport.window.clone())
            .flatten()
    }

    pub fn window(&self, window_id: WindowId) -> Option<Arc<Window>> {
        let state = self.state.borrow();
        state
            .viewport_from_window
            .get(&window_id)
            .and_then(|id| state.viewports.get(id))
            .and_then(|viewport| viewport.window.clone())
    }

    pub fn window_size(&self, cfg: &Config) -> Vec2 {
        self.window_size_for_scale(cfg, cfg.renderer.scale)
    }

    pub fn window_size_for_scale(&self, cfg: &Config, scale: f32) -> Vec2 {
        let gui = self.gui.borrow();
        let aspect_ratio = gui.aspect_ratio();
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
        self.state
            .borrow()
            .viewports
            .values()
            .all(|viewport| viewport.occluded)
    }

    pub fn inner_size(&self) -> Option<PhysicalSize<u32>> {
        self.root_window().map(|win| win.inner_size())
    }

    pub fn fullscreen(&self) -> bool {
        // viewport.info.fullscreen is sometimes stale, so rely on the actual winit state
        self.root_window()
            .map(|win| win.fullscreen().is_some())
            .unwrap_or(false)
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, embed_viewports: bool) {
        if feature!(Viewports) {
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
        let State { viewports, .. } = &mut *self.state.borrow_mut();

        for viewport_id in viewports.keys() {
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

    /// Handle event.
    pub fn on_event(&mut self, event: &NesEvent, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        self.gui.borrow_mut().on_event(event);

        match event {
            NesEvent::Renderer(event) => match event {
                RendererEvent::ViewportResized(_) => self.resize_window(cfg),
                RendererEvent::ResizeTexture => self.resize_texture = true,
                RendererEvent::RomLoaded(_) => {
                    if self.state.borrow_mut().focused != Some(ViewportId::ROOT) {
                        self.ctx
                            .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                    }
                }
                _ => (),
            },
            NesEvent::Config(event) => match event {
                ConfigEvent::DarkTheme(enabled) => {
                    self.ctx.set_visuals(if *enabled {
                        Gui::dark_theme()
                    } else {
                        Gui::light_theme()
                    });
                }
                ConfigEvent::EmbedViewports(embed) => {
                    if feature!(Viewports) {
                        self.ctx.set_embed_viewports(*embed);
                    }
                }
                ConfigEvent::Fullscreen(fullscreen) => {
                    if feature!(Viewports) {
                        self.ctx
                            .set_embed_viewports(*fullscreen || cfg.renderer.embed_viewports);
                    }
                    if self.fullscreen() != *fullscreen {
                        self.ctx
                            .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                        self.ctx.send_viewport_cmd_to(
                            ViewportId::ROOT,
                            ViewportCommand::Fullscreen(*fullscreen),
                        );
                    }
                }
                ConfigEvent::Region(_) | ConfigEvent::HideOverscan(_) | ConfigEvent::Scale(_) => {
                    self.resize_texture = true;
                }
                ConfigEvent::Shader(shader) => {
                    if let Some(render_state) = &self.render_state {
                        Self::set_shader_resource(render_state, &self.texture.view, *shader);
                    }
                }
                _ => (),
            },
            #[cfg(not(target_arch = "wasm32"))]
            NesEvent::AccessKit { window_id, request } => {
                if feature!(AccessKit) {
                    let State {
                        viewports,
                        viewport_from_window,
                        ..
                    } = &mut *self.state.borrow_mut();
                    let viewport_id = viewport_from_window.get(window_id);
                    if let Some(viewport_id) = viewport_id {
                        let state = viewports
                            .get_mut(viewport_id)
                            .and_then(|viewport| viewport.egui_state.as_mut());
                        if let Some(state) = state {
                            state.on_accesskit_action_request(request.clone());
                            self.ctx.request_repaint_of(*viewport_id);
                        }
                    }
                }
            }
            _ => (),
        }
    }

    fn initialize_all_windows(&mut self, event_loop: &EventLoopWindowTarget<NesEvent>) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.ctx.embed_viewports() {
            return;
        }

        let State {
            viewports,
            painter,
            viewport_from_window,
            ..
        } = &mut *self.state.borrow_mut();

        for viewport in viewports.values_mut() {
            viewport.initialize_window(
                self.tx.clone(),
                event_loop,
                &self.ctx,
                viewport_from_window,
                painter,
            );
        }
    }

    pub fn rom_loaded(&self) -> bool {
        self.gui.borrow().loaded_rom.is_some()
    }

    /// Handle window event.
    pub fn on_window_event(&mut self, window_id: WindowId, event: &WindowEvent) -> EventResponse {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let viewport_id = self.viewport_id_for_window(window_id);
        match event {
            WindowEvent::Focused(focused) => {
                self.state.borrow_mut().focused = if *focused { viewport_id } else { None };
            }
            // Note: Does not trigger on all platforms
            WindowEvent::Occluded(occluded) => {
                let mut state = self.state.borrow_mut();
                if let Some(viewport) = viewport_id
                    .as_ref()
                    .and_then(|id| state.viewports.get_mut(id))
                {
                    viewport.occluded = *occluded;
                }
            }
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                if let Some(viewport_id) = viewport_id {
                    let mut state = self.state.borrow_mut();
                    if viewport_id == ViewportId::ROOT {
                        self.tx.event(UiEvent::Terminate);
                    } else if let Some(viewport) = state.viewports.get_mut(&viewport_id) {
                        viewport.info.events.push(egui::ViewportEvent::Close);

                        // We may need to repaint both us and our parent to close the window,
                        // and perhaps twice (once to notice the close-event, once again to enforce it).
                        // `request_repaint_of` does a double-repaint though:
                        self.ctx.request_repaint_of(viewport_id);
                        self.ctx.request_repaint_of(viewport.ids.parent);
                    }
                }
            }
            // To support clipboard in wasm, we need to intercept the Paste event so that
            // egui_winit doesn't try to use it's clipboard fallback logic for paste. Associated
            // behavior in the wasm platform layer handles setting the egui_state clipboard text.
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(key),
                        ..
                    },
                ..
            } => {
                if let Some(key) = key_from_keycode(*key) {
                    use egui::Key;

                    let modifiers = self.ctx.input(|i| i.modifiers);

                    if feature!(ConsumePaste) && is_paste_command(modifiers, key) {
                        return EventResponse {
                            consumed: true,
                            repaint: true,
                        };
                    }

                    if matches!(key, Key::Plus | Key::Equals | Key::Minus | Key::Num0)
                        && (modifiers.ctrl || modifiers.command)
                    {
                        self.zoom_changed = true;
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(viewport_id) = viewport_id {
                    use std::num::NonZeroU32;
                    if let (Some(width), Some(height)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    {
                        self.state
                            .borrow_mut()
                            .painter
                            .borrow_mut()
                            .on_window_resized(viewport_id, width, height);
                    }
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                self.ctx
                    .send_viewport_cmd(ViewportCommand::SetTheme(if *theme == Theme::Light {
                        SystemTheme::Light
                    } else {
                        SystemTheme::Dark
                    }));
            }
            _ => (),
        }

        let mut state = self.state.borrow_mut();
        let mut res = viewport_id
            .and_then(|viewport_id| {
                state.viewports.get_mut(&viewport_id).and_then(|viewport| {
                    Some(
                        viewport
                            .egui_state
                            .as_mut()?
                            .on_window_event(viewport.window.as_deref()?, event),
                    )
                })
            })
            .unwrap_or_default();

        let gui_res = self.gui.borrow_mut().on_window_event(event);
        res.consumed |= gui_res.consumed;
        res.repaint |= gui_res.repaint;

        res
    }

    /// Handle gamepad event updates.
    pub fn on_gamepad_update(&self, gamepads: &Gamepads) -> EventResponse {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.gui.borrow().keybinds.wants_input() && gamepads.has_events() {
            EventResponse {
                consumed: true,
                repaint: true,
            }
        } else {
            EventResponse::default()
        }
    }

    pub fn add_message<S>(&mut self, ty: MessageType, text: S)
    where
        S: Into<String>,
    {
        self.gui.borrow_mut().add_message(ty, text);
    }

    pub fn on_error(&mut self, err: anyhow::Error) {
        error!("error: {err:?}");
        self.tx.event(EmulationEvent::RunState(RunState::Paused));
        self.gui.borrow_mut().error = Some(err.to_string());
    }

    pub fn load(ctx: &egui::Context, cfg: &Config) -> anyhow::Result<()> {
        let path = Config::default_config_dir().join("gui.dat");
        if fs::exists(&path) {
            let data = fs::load_raw(path).context("failed to load gui memory")?;
            let memory = bincode::deserialize(&data).context("failed to deserialize gui memory")?;
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
            let data = bincode::serialize(&mem).context("failed to serialize gui memory")?;
            fs::save_raw(path, &data).context("failed to save gui memory")
        })?;
        self.last_save_time = Instant::now();

        Ok(())
    }

    pub fn create_window(
        event_loop: &EventLoopWindowTarget<NesEvent>,
        ctx: &egui::Context,
        cfg: &Config,
    ) -> anyhow::Result<(Window, ViewportBuilder)> {
        let window_size = cfg.window_size(cfg.deck.region.aspect_ratio());
        let mut viewport_builder = ViewportBuilder::default()
            .with_app_id(Config::WINDOW_TITLE)
            .with_title(Config::WINDOW_TITLE)
            .with_active(true)
            .with_visible(false) // hide until first frame is rendered. required by AccessKit
            .with_inner_size(window_size)
            .with_min_inner_size(Vec2::new(Ppu::WIDTH as f32, Ppu::HEIGHT as f32))
            .with_fullscreen(cfg.renderer.fullscreen)
            .with_resizable(true);
        if cfg.renderer.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        let window_builder =
            egui_winit::create_winit_window_builder(ctx, event_loop, viewport_builder.clone());

        let window = window_builder
            .with_platform(Config::WINDOW_TITLE)
            .with_theme(Some(if cfg.renderer.dark_theme {
                Theme::Dark
            } else {
                Theme::Light
            }))
            .build(event_loop)?;

        egui_winit::apply_viewport_builder_to_window(ctx, &window, &viewport_builder);

        debug!("created new window: {:?}", window.id());

        Ok((window, viewport_builder))
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

        let mut painter = Painter::new(egui_wgpu::WgpuConfiguration::default(), 1, None, false);

        // Creating device may fail if adapter doesn't support our requested cfg above, so try to
        // recover with lower limits. Specifically max_texture_dimension_2d has a downlevel default
        // of 2048. egui_wgpu wants 8192 for 4k displays, but not all platforms support that yet.
        if let Err(err) = painter
            .set_window(ViewportId::ROOT, Some(Arc::clone(&window)))
            .await
        {
            if let egui_wgpu::WgpuError::RequestDeviceError(_) = err {
                painter = Painter::new(
                    egui_wgpu::WgpuConfiguration {
                        device_descriptor: Arc::new(|adapter| {
                            let base_limits = if adapter.get_info().backend == wgpu::Backend::Gl {
                                wgpu::Limits::downlevel_webgl2_defaults()
                            } else {
                                wgpu::Limits::default()
                            };
                            wgpu::DeviceDescriptor {
                                label: Some("egui wgpu device"),
                                required_features: wgpu::Features::default(),
                                required_limits: wgpu::Limits {
                                    max_texture_dimension_2d: 4096,
                                    ..base_limits
                                },
                            }
                        }),
                        ..Default::default()
                    },
                    1,
                    None,
                    false,
                );
                painter.set_window(ViewportId::ROOT, Some(window)).await?;
            } else {
                return Err(err.into());
            }
        }

        let adapter_info = painter.render_state().map(|state| state.adapter.get_info());
        if let Some(info) = adapter_info {
            debug!(
                "created new painter for adapter: `{}`. backend: `{}`",
                if info.name.is_empty() {
                    "unknown"
                } else {
                    &info.name
                },
                info.backend.to_str()
            );
        } else {
            debug!("created new painter. Adapter unknown.");
        }

        Ok(painter)
    }

    pub fn recreate_window(&mut self, event_loop: &EventLoopWindowTarget<NesEvent>) {
        if self.ctx.embed_viewports() {
            return;
        }

        let State {
            viewports,
            viewport_from_window,
            painter,
            ..
        } = &mut *self.state.borrow_mut();

        let viewport_builder = viewports
            .get(&ViewportId::ROOT)
            .map(|viewport| viewport.builder.clone())
            .unwrap_or_default();
        let viewport = Self::create_or_update_viewport(
            &self.ctx,
            viewports,
            ViewportIdPair::ROOT,
            ViewportClass::Root,
            viewport_builder,
            None,
            None,
        );

        viewport.initialize_window(
            self.tx.clone(),
            event_loop,
            &self.ctx,
            viewport_from_window,
            painter,
        );
    }

    pub fn drop_window(&mut self) -> Result<(), egui_wgpu::WgpuError> {
        if self.ctx.embed_viewports() {
            return Ok(());
        }
        let mut state = self.state.borrow_mut();
        state.viewports.remove(&ViewportId::ROOT);
        Renderer::set_painter_window(
            self.tx.clone(),
            Rc::clone(&state.painter),
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
        focused: Option<ViewportId>,
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
                info: Default::default(),
                viewport_ui_cb,
                screenshot_requested: false,
                window: None,
                egui_state: None,
                occluded: false,
            }),
            Entry::Occupied(mut entry) => {
                let viewport = entry.get_mut();
                viewport.class = class;
                viewport.ids.parent = ids.parent;
                viewport.viewport_ui_cb = viewport_ui_cb;

                let (delta_commands, recreate) = viewport.builder.patch(builder);
                if recreate {
                    viewport.window = None;
                    viewport.egui_state = None;
                } else if let Some(window) = &viewport.window {
                    let is_viewport_focused = focused == Some(ids.this);
                    egui_winit::process_viewport_commands(
                        ctx,
                        &mut viewport.info,
                        delta_commands,
                        window,
                        is_viewport_focused,
                        &mut viewport.screenshot_requested,
                    );
                }

                entry.into_mut()
            }
        }
    }

    fn handle_viewport_output(
        ctx: &egui::Context,
        viewports: &mut ViewportIdMap<Viewport>,
        outputs: ViewportIdMap<ViewportOutput>,
        focused: Option<ViewportId>,
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
                focused,
            );
            if let Some(window) = viewport.window.as_ref() {
                let is_viewport_focused = focused == Some(id);
                egui_winit::process_viewport_commands(
                    ctx,
                    &mut viewport.info,
                    output.commands,
                    window,
                    is_viewport_focused,
                    &mut viewport.screenshot_requested,
                );
            }
        }
    }

    fn render_immediate_viewport(
        tx: &NesEventProxy,
        event_loop: &EventLoopWindowTarget<NesEvent>,
        ctx: &egui::Context,
        state: &RefCell<State>,
        immediate_viewport: ImmediateViewport<'_>,
    ) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let ImmediateViewport {
            ids,
            builder,
            viewport_ui_cb,
        } = immediate_viewport;

        let input = {
            let State {
                viewports,
                painter,
                viewport_from_window,
                ..
            } = &mut *state.borrow_mut();

            let viewport = Self::create_or_update_viewport(
                ctx,
                viewports,
                ids,
                ViewportClass::Immediate,
                builder,
                None,
                None,
            );

            if viewport.window.is_none() {
                viewport.initialize_window(
                    tx.clone(),
                    event_loop,
                    ctx,
                    viewport_from_window,
                    painter,
                );
            }

            match (&viewport.window, &mut viewport.egui_state) {
                (Some(window), Some(egui_state)) => {
                    egui_winit::update_viewport_info(&mut viewport.info, ctx, window);

                    let mut input = egui_state.take_egui_input(window);
                    input.viewports = viewports
                        .iter()
                        .map(|(id, viewport)| (*id, viewport.info.clone()))
                        .collect();
                    input
                }
                _ => return,
            }
        };

        let output = ctx.run(input, |ctx| {
            viewport_ui_cb(ctx);
        });
        let viewport_id = ids.this;
        let State {
            viewports,
            painter,
            focused,
            ..
        } = &mut *state.borrow_mut();

        if let Some(viewport) = viewports.get_mut(&viewport_id) {
            viewport.info.events.clear();

            if let (Some(window), Some(egui_state)) = (&viewport.window, &mut viewport.egui_state) {
                Renderer::set_painter_window(
                    tx.clone(),
                    Rc::clone(painter),
                    viewport_id,
                    Some(Arc::clone(window)),
                );

                let clipped_primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
                painter.borrow_mut().paint_and_update_textures(
                    viewport_id,
                    output.pixels_per_point,
                    [0.0; 4],
                    &clipped_primitives,
                    &output.textures_delta,
                    false,
                );

                egui_state.handle_platform_output(window, output.platform_output);
                Self::handle_viewport_output(ctx, viewports, output.viewport_output, *focused);
            };
        };
    }

    pub fn prepare(&mut self, gamepads: &Gamepads, cfg: &Config) {
        self.gui.borrow_mut().prepare(gamepads, cfg);
        self.ctx.request_repaint();
    }

    /// Request redraw.
    pub fn redraw(
        &mut self,
        window_id: WindowId,
        event_loop: &EventLoopWindowTarget<NesEvent>,
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

        self.gui.borrow_mut().prepare(gamepads, cfg);

        self.handle_resize(viewport_id, cfg);

        let (viewport_ui_cb, raw_input) = {
            let State { viewports, .. } = &mut *self.state.borrow_mut();

            let Some(viewport) = viewports.get_mut(&viewport_id) else {
                return Ok(());
            };
            let Some(window) = &viewport.window else {
                return Ok(());
            };

            if viewport.occluded
                || (viewport_id != ViewportId::ROOT && viewport.viewport_ui_cb.is_none())
            {
                // This will only happen if this is an immediate viewport.
                // That means that the viewport cannot be rendered by itself and needs his parent to be rendered.
                return Ok(());
            }

            egui_winit::update_viewport_info(&mut viewport.info, &self.ctx, window);

            let viewport_ui_cb = viewport.viewport_ui_cb.clone();
            let egui_state = viewport
                .egui_state
                .as_mut()
                .context("failed to get egui_state")?;
            let mut raw_input = egui_state.take_egui_input(window);

            raw_input.viewports = viewports
                .iter()
                .map(|(id, viewport)| (*id, viewport.info.clone()))
                .collect();

            (viewport_ui_cb, raw_input)
        };

        // Copy NES frame buffer before drawing UI because a UI interaction might cause a texture
        // resize tied to a configuration change.
        if viewport_id == ViewportId::ROOT {
            if let Some(render_state) = &self.render_state {
                let mut frame_buffer = self.frame_rx.try_recv_ref();
                while self.frame_rx.remaining() < 2 {
                    debug!("skipping frame");
                    frame_buffer = self.frame_rx.try_recv_ref();
                }
                match frame_buffer {
                    Ok(frame_buffer) => {
                        self.texture.update(
                            &render_state.queue,
                            if cfg.renderer.hide_overscan
                                && self
                                    .gui
                                    .borrow()
                                    .loaded_region()
                                    .unwrap_or(cfg.deck.region)
                                    .is_ntsc()
                            {
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

        let output = self.ctx.run(raw_input, |ctx| match viewport_ui_cb {
            Some(viewport_ui_cb) => viewport_ui_cb(ctx),
            None => self.gui.borrow_mut().ui(ctx, Some(gamepads)),
        });

        {
            // Required to get mutable reference again to avoid double borrow when calling gui.ui
            // above because internally gui.ui calls show_viewport_immediate, which requires
            // borrowing state to render
            let State {
                viewports,
                painter,
                focused,
                viewport_from_window,
                ..
            } = &mut *self.state.borrow_mut();

            let Some(viewport) = viewports.get_mut(&viewport_id) else {
                return Ok(());
            };

            viewport.info.events.clear(); // they should have been processed

            let Viewport {
                window: Some(window),
                egui_state: Some(egui_state),
                screenshot_requested,
                ..
            } = viewport
            else {
                return Ok(());
            };

            window.pre_present_notify();

            let clipped_primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
            let screenshot_requested = std::mem::take(screenshot_requested);
            painter.borrow_mut().paint_and_update_textures(
                viewport_id,
                output.pixels_per_point,
                [0.0; 4],
                &clipped_primitives,
                &output.textures_delta,
                screenshot_requested,
            );

            if std::mem::take(&mut self.first_frame) {
                window.set_visible(true);
            }

            let active_viewports_ids: ViewportIdSet =
                output.viewport_output.keys().copied().collect();

            if feature!(ScreenReader) && self.ctx.options(|o| o.screen_reader) {
                platform::speak_text(&output.platform_output.events_description());
            }

            egui_state.handle_platform_output(window, output.platform_output);
            Self::handle_viewport_output(&self.ctx, viewports, output.viewport_output, *focused);

            // Prune dead viewports
            viewports.retain(|id, _| active_viewports_ids.contains(id));
            viewport_from_window.retain(|_, id| active_viewports_ids.contains(id));
            painter.borrow_mut().gc_viewports(&active_viewports_ids);

            if viewport_id == ViewportId::ROOT {
                for (viewport_id, viewport) in viewports {
                    self.gui.borrow_mut().show_viewport_info_window(
                        &self.ctx,
                        *viewport_id,
                        &viewport.info,
                    );
                }
                if std::mem::take(&mut self.zoom_changed) {
                    cfg.renderer.zoom = self.ctx.zoom_factor();
                }
            }
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

            self.resize_window(cfg);

            let State { painter, .. } = &mut *self.state.borrow_mut();

            if let (Some(max_texture_side), Some(render_state)) =
                (painter.borrow().max_texture_side(), &self.render_state)
            {
                let texture_size = cfg.texture_size();
                self.texture.resize(
                    &render_state.device,
                    &mut render_state.renderer.write(),
                    texture_size.x.min(max_texture_side as f32) as u32,
                    texture_size.y.min(max_texture_side as f32) as u32,
                    self.gui.borrow().aspect_ratio(),
                );
                self.gui.borrow_mut().texture = self.texture.sized_texture();

                Self::set_shader_resource(render_state, &self.texture.view, cfg.renderer.shader);
            }
            self.resize_texture = false;
        }
    }

    fn resize_window(&self, cfg: &Config) {
        if !self.fullscreen() {
            let desired_window_size = self.window_size(cfg);

            if cfg.renderer.scale == 1.0 && cfg.renderer.zoom == 1.0 {
                self.ctx.set_zoom_factor(0.7);
            } else {
                self.ctx.set_zoom_factor(cfg.renderer.zoom);
            }

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

    fn set_shader_resource(render_state: &RenderState, view: &wgpu::TextureView, shader: Shader) {
        if matches!(shader, Shader::None) {
            render_state
                .renderer
                .write()
                .callback_resources
                .remove::<shader::Resources>();
        } else {
            let shader_resource = shader::Resources::new(render_state, view, shader);
            render_state
                .renderer
                .write()
                .callback_resources
                .insert(shader_resource);
        }
    }
}

impl Viewport {
    pub fn initialize_window(
        &mut self,
        tx: NesEventProxy,
        event_loop: &EventLoopWindowTarget<NesEvent>,
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
        let window_builder =
            egui_winit::create_winit_window_builder(ctx, event_loop, self.builder.clone())
                .with_platform(self.builder.title.as_deref().unwrap_or_default());

        match window_builder.build(event_loop) {
            Ok(window) => {
                egui_winit::apply_viewport_builder_to_window(ctx, &window, &self.builder);

                viewport_from_window.insert(window.id(), viewport_id);
                let window = Arc::new(window);

                Renderer::set_painter_window(
                    tx,
                    Rc::clone(painter),
                    viewport_id,
                    Some(Arc::clone(&window)),
                );

                debug!("created new viewport window: {:?}", window.id());

                self.egui_state = Some(egui_winit::State::new(
                    ctx.clone(),
                    viewport_id,
                    event_loop,
                    Some(window.scale_factor() as f32),
                    painter.borrow().max_texture_side(),
                ));

                self.info.minimized = window.is_minimized();
                self.info.maximized = Some(window.is_maximized());
                self.window = Some(window);
            }
            Err(err) => error!("Failed to create window: {err}"),
        }
    }
}
