use crate::{
    feature,
    nes::{
        config::Config,
        event::{ConfigEvent, NesEvent, RendererEvent, Response, UiEvent},
        input::{Gamepads, Input},
        renderer::{
            Renderer, State, Viewport,
            gui::{Gui, lib::pixels_per_point},
        },
    },
};
use egui::{PointerButton, SystemTheme, ViewportCommand, ViewportId};
use winit::{
    dpi::PhysicalPosition,
    event::{
        ElementState, Force, KeyEvent, MouseButton, MouseScrollDelta, Touch, TouchPhase,
        WindowEvent,
    },
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey},
    window::{Theme, WindowId},
};

impl Renderer {
    /// Handle event.
    pub fn on_event(&mut self, event: &mut NesEvent, cfg: &Config) {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        {
            let painter = self.painter.borrow();
            if let Some(render_state) = painter.render_state() {
                self.gui.borrow_mut().on_event(&render_state.queue, event);
            }
        }

        match event {
            NesEvent::Renderer(event) => match event {
                RendererEvent::ViewportResized(_) => self.resize_window(cfg),
                RendererEvent::ResizeTexture => self.resize_texture = true,
                RendererEvent::RomLoaded(_) => {
                    let state = self.state.borrow();
                    if state.focused != Some(ViewportId::ROOT) {
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
                    if feature!(OsViewports) {
                        self.ctx.set_embed_viewports(*embed);
                    }
                }
                ConfigEvent::Fullscreen(fullscreen) => {
                    if feature!(OsViewports) {
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
                    self.painter.borrow_mut().set_shader(*shader);
                }
                _ => (),
            },
            #[cfg(not(target_arch = "wasm32"))]
            NesEvent::AccessKit { window_id, event } => {
                use crate::nes::event::AccessKitWindowEvent;
                if let Some(viewport_id) = self.viewport_id_for_window(*window_id) {
                    let mut state = self.state.borrow_mut();
                    if let Some(viewport) = state.viewports.get_mut(&viewport_id) {
                        match event {
                            AccessKitWindowEvent::InitialTreeRequested => {
                                self.ctx.enable_accesskit();
                                self.ctx.request_repaint_of(viewport_id);
                            }
                            AccessKitWindowEvent::ActionRequested(request) => {
                                viewport
                                    .raw_input
                                    .events
                                    .push(egui::Event::AccessKitActionRequest(request.clone()));
                                self.ctx.request_repaint_of(viewport_id);
                            }
                            AccessKitWindowEvent::AccessibilityDeactivated => {
                                self.ctx.disable_accesskit();
                            }
                        }
                    };
                }
            }
            _ => (),
        }
    }

    /// Handle window event.
    pub fn on_window_event(&mut self, window_id: WindowId, event: &WindowEvent) -> Response {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        let Some(viewport_id) = self.viewport_id_for_window(window_id) else {
            return Response::default();
        };

        let State {
            viewports,
            focused,
            pointer_touch_id,
            ..
        } = &mut *self.state.borrow_mut();
        let Some(viewport) = viewports.get_mut(&viewport_id) else {
            return Response::default();
        };

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(window) = &viewport.window {
            tracing::trace!("process accesskit event: {event:?}");
            self.accesskit.process_event(window, event);
        }

        let pixels_per_point = viewport
            .window
            .as_ref()
            .map_or(1.0, |window| pixels_per_point(&self.ctx, window));

        match event {
            WindowEvent::Focused(new_focused) => {
                *focused = if *new_focused {
                    Some(viewport_id)
                } else {
                    None
                };
            }
            // Note: Does not trigger on all platforms
            WindowEvent::Occluded(occluded) => viewport.occluded = *occluded,
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                if viewport_id == ViewportId::ROOT {
                    self.tx.event(UiEvent::Terminate);
                } else {
                    viewport.info.events.push(egui::ViewportEvent::Close);
                    self.gui.borrow_mut().close_viewport(viewport_id);

                    // We may need to repaint both us and our parent to close the window,
                    // and perhaps twice (once to notice the close-event, once again to enforce it).
                    // `request_repaint_of` does a double-repaint though:
                    self.ctx.request_repaint_of(viewport_id);
                    self.ctx.request_repaint_of(viewport.ids.parent);
                }
            }
            // To support clipboard in wasm, we need to intercept the Paste event so that
            // we don't try to use the clipboard fallback logic for paste. Associated
            // behavior in the wasm platform layer handles setting the clipboard text.
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        ..
                    },
                ..
            } => {
                if let Some(key) = key_from_keycode(*key) {
                    use egui::Key;

                    let modifiers = self.ctx.input(|i| i.modifiers);

                    if feature!(ConsumePaste) && is_paste_command(modifiers, key) {
                        return Response {
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
                self.painter
                    .borrow_mut()
                    .on_window_resized(viewport_id, size.width, size.height);
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
        };

        let res = match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let native_pixels_per_point = *scale_factor as f32;
                viewport.info.native_pixels_per_point = Some(native_pixels_per_point);
                Response {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                Self::on_mouse_button_input(viewport.cursor_pos, viewport, *state, *button);
                Response {
                    repaint: true,
                    consumed: self.ctx.wants_pointer_input(),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                Self::on_mouse_wheel(viewport, pixels_per_point, *delta);
                Response {
                    repaint: true,
                    consumed: self.ctx.wants_pointer_input(),
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                Self::on_cursor_moved(viewport, pixels_per_point, *position);
                Response {
                    repaint: true,
                    consumed: self.ctx.is_using_pointer(),
                }
            }
            WindowEvent::CursorLeft { .. } => {
                viewport.cursor_pos = None;
                viewport.raw_input.events.push(egui::Event::PointerGone);
                Response {
                    repaint: true,
                    consumed: false,
                }
            }
            // WindowEvent::TouchpadPressure {device_id, pressure, stage, ..  } => {} // TODO
            WindowEvent::Touch(touch) => {
                Self::on_touch(viewport, pointer_touch_id, pixels_per_point, touch);
                let consumed = match touch.phase {
                    TouchPhase::Started | TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.ctx.wants_pointer_input()
                    }
                    TouchPhase::Moved => self.ctx.is_using_pointer(),
                };
                Response {
                    repaint: true,
                    consumed,
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // Winit generates fake "synthetic" KeyboardInput events when the focus
                // is changed to the window, or away from it. Synthetic key presses
                // represent no real key presses and should be ignored.
                // See https://github.com/rust-windowing/winit/issues/3543
                if *is_synthetic && event.state == ElementState::Pressed {
                    Response {
                        repaint: true,
                        consumed: false,
                    }
                } else {
                    Self::on_keyboard_input(viewport, event);

                    // When pressing the Tab key, egui focuses the first focusable element, hence Tab always consumes.
                    let consumed = self.ctx.wants_keyboard_input()
                        || event.logical_key == Key::Named(NamedKey::Tab);
                    Response {
                        repaint: true,
                        consumed,
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                viewport
                    .raw_input
                    .events
                    .push(egui::Event::WindowFocused(*focused));
                Response {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::HoveredFile(path) => {
                if let Some(viewport) = viewports.get_mut(&viewport_id) {
                    viewport.raw_input.hovered_files.push(egui::HoveredFile {
                        path: Some(path.clone()),
                        ..Default::default()
                    });
                }
                Response {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::HoveredFileCancelled => {
                if let Some(viewport) = viewports.get_mut(&viewport_id) {
                    viewport.raw_input.hovered_files.clear();
                }
                Response {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::DroppedFile(path) => {
                if let Some(viewport) = viewports.get_mut(&viewport_id) {
                    viewport.raw_input.hovered_files.clear();
                    viewport.raw_input.dropped_files.push(egui::DroppedFile {
                        path: Some(path.clone()),
                        ..Default::default()
                    });
                }
                Response {
                    repaint: true,
                    consumed: false,
                }
            }
            WindowEvent::ModifiersChanged(state) => {
                let state = state.state();

                let alt = state.alt_key();
                let ctrl = state.control_key();
                let shift = state.shift_key();
                let super_ = state.super_key();

                if let Some(viewport) = viewports.get_mut(&viewport_id) {
                    viewport.raw_input.modifiers.alt = alt;
                    viewport.raw_input.modifiers.ctrl = ctrl;
                    viewport.raw_input.modifiers.shift = shift;
                    viewport.raw_input.modifiers.mac_cmd = cfg!(target_os = "macos") && super_;
                    viewport.raw_input.modifiers.command = if cfg!(target_os = "macos") {
                        super_
                    } else {
                        ctrl
                    };
                }

                Response {
                    repaint: true,
                    consumed: false,
                }
            }

            // Things that may require repaint:
            WindowEvent::RedrawRequested
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::Destroyed
            | WindowEvent::Occluded(_)
            | WindowEvent::Resized(_)
            | WindowEvent::Moved(_)
            | WindowEvent::ThemeChanged(_)
            | WindowEvent::TouchpadPressure { .. }
            | WindowEvent::CloseRequested => Response {
                repaint: true,
                consumed: false,
            },

            // Things we completely ignore:
            WindowEvent::ActivationTokenDone { .. }
            | WindowEvent::AxisMotion { .. }
            | WindowEvent::DoubleTapGesture { .. }
            | WindowEvent::RotationGesture { .. }
            | WindowEvent::PanGesture { .. } => Response {
                repaint: false,
                consumed: false,
            },

            WindowEvent::PinchGesture { delta, .. } => {
                // Positive delta values indicate magnification (zooming in).
                // Negative delta values indicate shrinking (zooming out).
                let zoom_factor = (*delta as f32).exp();
                viewport
                    .raw_input
                    .events
                    .push(egui::Event::Zoom(zoom_factor));
                Response {
                    repaint: true,
                    consumed: self.ctx.wants_pointer_input(),
                }
            }
            WindowEvent::Ime(_) => Response::default(),
        };

        let gui_res = self.gui.borrow_mut().on_window_event(event);

        Response {
            repaint: res.repaint || gui_res.repaint,
            consumed: res.consumed || gui_res.consumed,
        }
    }

    pub fn on_mouse_motion(&mut self, delta: (f64, f64)) {
        let State {
            viewports, focused, ..
        } = &mut *self.state.borrow_mut();
        if let Some(id) = *focused
            && let Some(viewport) = viewports.get_mut(&id)
        {
            viewport
                .raw_input
                .events
                .push(egui::Event::MouseMoved(egui::Vec2 {
                    x: delta.0 as f32,
                    y: delta.1 as f32,
                }));
        }
    }

    fn on_mouse_button_input(
        pointer_pos: Option<egui::Pos2>,
        viewport: &mut Viewport,
        state: ElementState,
        button: MouseButton,
    ) {
        if let Some(pos) = pointer_pos
            && let Some(button) = pointer_button_from_mouse(button)
        {
            let pressed = state == ElementState::Pressed;

            viewport.raw_input.events.push(egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: viewport.raw_input.modifiers,
            });
        }
    }

    fn on_cursor_moved(
        viewport: &mut Viewport,
        pixels_per_point: f32,
        pos_in_pixels: PhysicalPosition<f64>,
    ) {
        let pos_in_points = egui::pos2(
            pos_in_pixels.x as f32 / pixels_per_point,
            pos_in_pixels.y as f32 / pixels_per_point,
        );
        viewport.cursor_pos = Some(pos_in_points);

        viewport
            .raw_input
            .events
            .push(egui::Event::PointerMoved(pos_in_points));
    }

    fn on_touch(
        viewport: &mut Viewport,
        pointer_touch_id: &mut Option<u64>,
        pixels_per_point: f32,
        touch: &Touch,
    ) {
        // Emit touch event
        viewport.raw_input.events.push(egui::Event::Touch {
            device_id: egui::TouchDeviceId(egui::epaint::util::hash(touch.device_id)),
            id: egui::TouchId::from(touch.id),
            phase: match touch.phase {
                TouchPhase::Started => egui::TouchPhase::Start,
                TouchPhase::Moved => egui::TouchPhase::Move,
                TouchPhase::Ended => egui::TouchPhase::End,
                TouchPhase::Cancelled => egui::TouchPhase::Cancel,
            },
            pos: egui::pos2(
                touch.location.x as f32 / pixels_per_point,
                touch.location.y as f32 / pixels_per_point,
            ),
            force: match touch.force {
                Some(Force::Normalized(force)) => Some(force as f32),
                Some(Force::Calibrated {
                    force,
                    max_possible_force,
                    ..
                }) => Some((force / max_possible_force) as f32),
                None => None,
            },
        });
        // If we're not yet translating a touch or we're translating this very
        // touch …
        if pointer_touch_id.is_none()
            || pointer_touch_id.is_some_and(|touch_id| touch_id == touch.id)
        {
            // … emit PointerButton resp. PointerMoved events to emulate mouse
            match touch.phase {
                winit::event::TouchPhase::Started => {
                    *pointer_touch_id = Some(touch.id);
                    // First move the pointer to the right location
                    Self::on_cursor_moved(viewport, pixels_per_point, touch.location);
                    Self::on_mouse_button_input(
                        viewport.cursor_pos,
                        viewport,
                        ElementState::Pressed,
                        MouseButton::Left,
                    );
                }
                winit::event::TouchPhase::Moved => {
                    Self::on_cursor_moved(viewport, pixels_per_point, touch.location);
                }
                winit::event::TouchPhase::Ended => {
                    *pointer_touch_id = None;
                    Self::on_mouse_button_input(
                        viewport.cursor_pos,
                        viewport,
                        ElementState::Released,
                        MouseButton::Left,
                    );
                    // The pointer should vanish completely to not get any
                    // hover effects
                    viewport.cursor_pos = None;
                    viewport.raw_input.events.push(egui::Event::PointerGone);
                }
                winit::event::TouchPhase::Cancelled => {
                    *pointer_touch_id = None;
                    viewport.cursor_pos = None;
                    viewport.raw_input.events.push(egui::Event::PointerGone);
                }
            }
        }
    }

    fn on_mouse_wheel(viewport: &mut Viewport, pixels_per_point: f32, delta: MouseScrollDelta) {
        let modifiers = viewport.raw_input.modifiers;
        let (unit, delta) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (egui::MouseWheelUnit::Line, egui::vec2(x, y)),
            MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => (
                egui::MouseWheelUnit::Point,
                egui::vec2(x as f32, y as f32) / pixels_per_point,
            ),
        };
        viewport.raw_input.events.push(egui::Event::MouseWheel {
            unit,
            delta,
            modifiers,
        });
    }

    fn on_keyboard_input(viewport: &mut Viewport, event: &KeyEvent) {
        let KeyEvent {
            // Represents the position of a key independent of the currently active layout.
            //
            // It also uniquely identifies the physical key (i.e. it's mostly synonymous with a scancode).
            // The most prevalent use case for this is games. For example the default keys for the player
            // to move around might be the W, A, S, and D keys on a US layout. The position of these keys
            // is more important than their label, so they should map to Z, Q, S, and D on an "AZERTY"
            // layout. (This value is `KeyCode::KeyW` for the Z key on an AZERTY layout.)
            physical_key,
            // Represents the results of a keymap, i.e. what character a certain key press represents.
            // When telling users "Press Ctrl-F to find", this is where we should
            // look for the "F" key, because they may have a dvorak layout on
            // a qwerty keyboard, and so the logical "F" character may not be located on the physical `KeyCode::KeyF` position.
            logical_key,
            text,
            state,
            ..
        } = event;

        let pressed = *state == ElementState::Pressed;

        let physical_key = if let PhysicalKey::Code(keycode) = *physical_key {
            key_from_keycode(keycode)
        } else {
            None
        };

        let logical_key = key_from_winit_key(logical_key);

        // Helpful logging to enable when adding new key support
        tracing::trace!(
            "logical {:?} -> {:?},  physical {:?} -> {:?}",
            event.logical_key,
            logical_key,
            event.physical_key,
            physical_key
        );

        let modifiers = viewport.raw_input.modifiers;
        if let Some(logical_key) = logical_key {
            if pressed {
                if is_cut_command(modifiers, logical_key) {
                    viewport.raw_input.events.push(egui::Event::Cut);
                    return;
                } else if is_copy_command(modifiers, logical_key) {
                    viewport.raw_input.events.push(egui::Event::Copy);
                    return;
                } else if is_paste_command(modifiers, logical_key) {
                    if let Some(contents) = viewport.clipboard.get() {
                        let contents = contents.replace("\r\n", "\n");
                        if !contents.is_empty() {
                            viewport.raw_input.events.push(egui::Event::Paste(contents));
                        }
                    }
                    return;
                }
            }

            viewport.raw_input.events.push(egui::Event::Key {
                key: logical_key,
                physical_key,
                pressed,
                repeat: false, // egui will fill this in for us!
                modifiers,
            });
        }

        if let Some(text) = &text {
            // Make sure there is text, and that it is not control characters
            // (e.g. delete is sent as "\u{f728}" on macOS).
            if !text.is_empty() && text.chars().all(is_printable_char) {
                // On some platforms we get here when the user presses Cmd-C (copy), ctrl-W, etc.
                // We need to ignore these characters that are side-effects of commands.
                // Also make sure the key is pressed (not released). On Linux, text might
                // contain some data even when the key is released.
                let is_cmd = modifiers.ctrl || modifiers.command || modifiers.mac_cmd;
                if pressed && !is_cmd {
                    viewport
                        .raw_input
                        .events
                        .push(egui::Event::Text(text.to_string()));
                }
            }
        }
    }

    /// Handle gamepad event updates.
    pub fn on_gamepad_update(&self, gamepads: &Gamepads) -> Response {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();

        if self.gui.borrow().keybinds.wants_input() && gamepads.has_events() {
            Response {
                consumed: true,
                repaint: true,
            }
        } else {
            Response::default()
        }
    }
}

impl TryFrom<(egui::Key, egui::Modifiers)> for Input {
    type Error = ();

    fn try_from((key, modifiers): (egui::Key, egui::Modifiers)) -> Result<Self, Self::Error> {
        let keycode = keycode_from_key(key).ok_or(())?;
        let modifiers = modifiers_state_from_modifiers(modifiers);
        Ok(Input::Key(keycode, modifiers))
    }
}

impl From<PointerButton> for Input {
    fn from(button: PointerButton) -> Self {
        Input::Mouse(mouse_button_from_pointer(button))
    }
}

pub fn is_cut_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Cut
        || (modifiers.command && keycode == egui::Key::X)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Delete)
}

pub fn is_copy_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Copy
        || (modifiers.command && keycode == egui::Key::C)
        || (cfg!(target_os = "windows") && modifiers.ctrl && keycode == egui::Key::Insert)
}

pub fn is_paste_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Paste
        || (modifiers.command && keycode == egui::Key::V)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Insert)
}

/// Winit sends special keys (backspace, delete, F1, …) as characters.
/// Ignore those.
/// We also ignore '\r', '\n', '\t'.
/// Newlines are handled by the `Key::Enter` event.
pub const fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = '\u{e000}' <= chr && chr <= '\u{f8ff}'
        || '\u{f0000}' <= chr && chr <= '\u{ffffd}'
        || '\u{100000}' <= chr && chr <= '\u{10fffd}';

    !is_in_private_use_area && !chr.is_ascii_control()
}

pub fn key_from_winit_key(key: &winit::keyboard::Key) -> Option<egui::Key> {
    match key {
        winit::keyboard::Key::Named(named_key) => key_from_named_key(*named_key),
        winit::keyboard::Key::Character(str) => egui::Key::from_name(str.as_str()),
        winit::keyboard::Key::Unidentified(_) | winit::keyboard::Key::Dead(_) => None,
    }
}

pub fn key_from_named_key(named_key: winit::keyboard::NamedKey) -> Option<egui::Key> {
    use egui::Key;
    use winit::keyboard::NamedKey;

    Some(match named_key {
        NamedKey::Enter => Key::Enter,
        NamedKey::Tab => Key::Tab,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::End => Key::End,
        NamedKey::Home => Key::Home,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert => Key::Insert,
        NamedKey::Escape => Key::Escape,
        NamedKey::Cut => Key::Cut,
        NamedKey::Copy => Key::Copy,
        NamedKey::Paste => Key::Paste,

        NamedKey::Space => Key::Space,

        NamedKey::F1 => Key::F1,
        NamedKey::F2 => Key::F2,
        NamedKey::F3 => Key::F3,
        NamedKey::F4 => Key::F4,
        NamedKey::F5 => Key::F5,
        NamedKey::F6 => Key::F6,
        NamedKey::F7 => Key::F7,
        NamedKey::F8 => Key::F8,
        NamedKey::F9 => Key::F9,
        NamedKey::F10 => Key::F10,
        NamedKey::F11 => Key::F11,
        NamedKey::F12 => Key::F12,
        NamedKey::F13 => Key::F13,
        NamedKey::F14 => Key::F14,
        NamedKey::F15 => Key::F15,
        NamedKey::F16 => Key::F16,
        NamedKey::F17 => Key::F17,
        NamedKey::F18 => Key::F18,
        NamedKey::F19 => Key::F19,
        NamedKey::F20 => Key::F20,
        NamedKey::F21 => Key::F21,
        NamedKey::F22 => Key::F22,
        NamedKey::F23 => Key::F23,
        NamedKey::F24 => Key::F24,
        NamedKey::F25 => Key::F25,
        NamedKey::F26 => Key::F26,
        NamedKey::F27 => Key::F27,
        NamedKey::F28 => Key::F28,
        NamedKey::F29 => Key::F29,
        NamedKey::F30 => Key::F30,
        NamedKey::F31 => Key::F31,
        NamedKey::F32 => Key::F32,
        NamedKey::F33 => Key::F33,
        NamedKey::F34 => Key::F34,
        NamedKey::F35 => Key::F35,
        _ => {
            tracing::trace!("Unknown key: {named_key:?}");
            return None;
        }
    })
}

pub const fn key_from_keycode(keycode: KeyCode) -> Option<egui::Key> {
    Some(match keycode {
        KeyCode::ArrowDown => egui::Key::ArrowDown,
        KeyCode::ArrowLeft => egui::Key::ArrowLeft,
        KeyCode::ArrowRight => egui::Key::ArrowRight,
        KeyCode::ArrowUp => egui::Key::ArrowUp,

        KeyCode::Escape => egui::Key::Escape,
        KeyCode::Tab => egui::Key::Tab,
        KeyCode::Backspace => egui::Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => egui::Key::Enter,

        KeyCode::Insert => egui::Key::Insert,
        KeyCode::Delete => egui::Key::Delete,
        KeyCode::Home => egui::Key::Home,
        KeyCode::End => egui::Key::End,
        KeyCode::PageUp => egui::Key::PageUp,
        KeyCode::PageDown => egui::Key::PageDown,

        // Punctuation
        KeyCode::Space => egui::Key::Space,
        KeyCode::Comma => egui::Key::Comma,
        KeyCode::Period => egui::Key::Period,
        KeyCode::Semicolon => egui::Key::Semicolon,
        KeyCode::Backslash => egui::Key::Backslash,
        KeyCode::Slash | KeyCode::NumpadDivide => egui::Key::Slash,
        KeyCode::BracketLeft => egui::Key::OpenBracket,
        KeyCode::BracketRight => egui::Key::CloseBracket,
        KeyCode::Backquote => egui::Key::Backtick,

        KeyCode::Cut => egui::Key::Cut,
        KeyCode::Copy => egui::Key::Copy,
        KeyCode::Paste => egui::Key::Paste,
        KeyCode::Minus | KeyCode::NumpadSubtract => egui::Key::Minus,
        KeyCode::NumpadAdd => egui::Key::Plus,
        KeyCode::Equal => egui::Key::Equals,

        KeyCode::Digit0 | KeyCode::Numpad0 => egui::Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => egui::Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => egui::Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => egui::Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => egui::Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => egui::Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => egui::Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => egui::Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => egui::Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => egui::Key::Num9,

        KeyCode::KeyA => egui::Key::A,
        KeyCode::KeyB => egui::Key::B,
        KeyCode::KeyC => egui::Key::C,
        KeyCode::KeyD => egui::Key::D,
        KeyCode::KeyE => egui::Key::E,
        KeyCode::KeyF => egui::Key::F,
        KeyCode::KeyG => egui::Key::G,
        KeyCode::KeyH => egui::Key::H,
        KeyCode::KeyI => egui::Key::I,
        KeyCode::KeyJ => egui::Key::J,
        KeyCode::KeyK => egui::Key::K,
        KeyCode::KeyL => egui::Key::L,
        KeyCode::KeyM => egui::Key::M,
        KeyCode::KeyN => egui::Key::N,
        KeyCode::KeyO => egui::Key::O,
        KeyCode::KeyP => egui::Key::P,
        KeyCode::KeyQ => egui::Key::Q,
        KeyCode::KeyR => egui::Key::R,
        KeyCode::KeyS => egui::Key::S,
        KeyCode::KeyT => egui::Key::T,
        KeyCode::KeyU => egui::Key::U,
        KeyCode::KeyV => egui::Key::V,
        KeyCode::KeyW => egui::Key::W,
        KeyCode::KeyX => egui::Key::X,
        KeyCode::KeyY => egui::Key::Y,
        KeyCode::KeyZ => egui::Key::Z,

        KeyCode::F1 => egui::Key::F1,
        KeyCode::F2 => egui::Key::F2,
        KeyCode::F3 => egui::Key::F3,
        KeyCode::F4 => egui::Key::F4,
        KeyCode::F5 => egui::Key::F5,
        KeyCode::F6 => egui::Key::F6,
        KeyCode::F7 => egui::Key::F7,
        KeyCode::F8 => egui::Key::F8,
        KeyCode::F9 => egui::Key::F9,
        KeyCode::F10 => egui::Key::F10,
        KeyCode::F11 => egui::Key::F11,
        KeyCode::F12 => egui::Key::F12,
        KeyCode::F13 => egui::Key::F13,
        KeyCode::F14 => egui::Key::F14,
        KeyCode::F15 => egui::Key::F15,
        KeyCode::F16 => egui::Key::F16,
        KeyCode::F17 => egui::Key::F17,
        KeyCode::F18 => egui::Key::F18,
        KeyCode::F19 => egui::Key::F19,
        KeyCode::F20 => egui::Key::F20,
        KeyCode::F21 => egui::Key::F21,
        KeyCode::F22 => egui::Key::F22,
        KeyCode::F23 => egui::Key::F23,
        KeyCode::F24 => egui::Key::F24,
        KeyCode::F25 => egui::Key::F25,
        KeyCode::F26 => egui::Key::F26,
        KeyCode::F27 => egui::Key::F27,
        KeyCode::F28 => egui::Key::F28,
        KeyCode::F29 => egui::Key::F29,
        KeyCode::F30 => egui::Key::F30,
        KeyCode::F31 => egui::Key::F31,
        KeyCode::F32 => egui::Key::F32,
        KeyCode::F33 => egui::Key::F33,
        KeyCode::F34 => egui::Key::F34,
        KeyCode::F35 => egui::Key::F35,
        _ => {
            return None;
        }
    })
}

pub const fn keycode_from_key(key: egui::Key) -> Option<KeyCode> {
    Some(match key {
        egui::Key::ArrowDown => KeyCode::ArrowDown,
        egui::Key::ArrowLeft => KeyCode::ArrowLeft,
        egui::Key::ArrowRight => KeyCode::ArrowRight,
        egui::Key::ArrowUp => KeyCode::ArrowUp,

        egui::Key::Escape => KeyCode::Escape,
        egui::Key::Tab => KeyCode::Tab,
        egui::Key::Backspace => KeyCode::Backspace,
        egui::Key::Enter => KeyCode::Enter,

        egui::Key::Insert => KeyCode::Insert,
        egui::Key::Delete => KeyCode::Delete,
        egui::Key::Home => KeyCode::Home,
        egui::Key::End => KeyCode::End,
        egui::Key::PageUp => KeyCode::PageUp,
        egui::Key::PageDown => KeyCode::PageDown,

        // Punctuation
        egui::Key::Space => KeyCode::Space,
        egui::Key::Comma => KeyCode::Comma,
        egui::Key::Period => KeyCode::Period,
        egui::Key::Semicolon => KeyCode::Semicolon,
        egui::Key::Backslash => KeyCode::Backslash,
        egui::Key::Slash => KeyCode::Slash,
        egui::Key::OpenBracket => KeyCode::BracketLeft,
        egui::Key::CloseBracket => KeyCode::BracketRight,

        egui::Key::Cut => KeyCode::Cut,
        egui::Key::Copy => KeyCode::Copy,
        egui::Key::Paste => KeyCode::Paste,
        egui::Key::Minus => KeyCode::Minus,
        egui::Key::Plus => KeyCode::NumpadAdd,
        egui::Key::Equals => KeyCode::Equal,

        egui::Key::Num0 => KeyCode::Digit0,
        egui::Key::Num1 => KeyCode::Digit1,
        egui::Key::Num2 => KeyCode::Digit2,
        egui::Key::Num3 => KeyCode::Digit3,
        egui::Key::Num4 => KeyCode::Digit4,
        egui::Key::Num5 => KeyCode::Digit5,
        egui::Key::Num6 => KeyCode::Digit6,
        egui::Key::Num7 => KeyCode::Digit7,
        egui::Key::Num8 => KeyCode::Digit8,
        egui::Key::Num9 => KeyCode::Digit9,

        egui::Key::A => KeyCode::KeyA,
        egui::Key::B => KeyCode::KeyB,
        egui::Key::C => KeyCode::KeyC,
        egui::Key::D => KeyCode::KeyD,
        egui::Key::E => KeyCode::KeyE,
        egui::Key::F => KeyCode::KeyF,
        egui::Key::G => KeyCode::KeyG,
        egui::Key::H => KeyCode::KeyH,
        egui::Key::I => KeyCode::KeyI,
        egui::Key::J => KeyCode::KeyJ,
        egui::Key::K => KeyCode::KeyK,
        egui::Key::L => KeyCode::KeyL,
        egui::Key::M => KeyCode::KeyM,
        egui::Key::N => KeyCode::KeyN,
        egui::Key::O => KeyCode::KeyO,
        egui::Key::P => KeyCode::KeyP,
        egui::Key::Q => KeyCode::KeyQ,
        egui::Key::R => KeyCode::KeyR,
        egui::Key::S => KeyCode::KeyS,
        egui::Key::T => KeyCode::KeyT,
        egui::Key::U => KeyCode::KeyU,
        egui::Key::V => KeyCode::KeyV,
        egui::Key::W => KeyCode::KeyW,
        egui::Key::X => KeyCode::KeyX,
        egui::Key::Y => KeyCode::KeyY,
        egui::Key::Z => KeyCode::KeyZ,

        egui::Key::F1 => KeyCode::F1,
        egui::Key::F2 => KeyCode::F2,
        egui::Key::F3 => KeyCode::F3,
        egui::Key::F4 => KeyCode::F4,
        egui::Key::F5 => KeyCode::F5,
        egui::Key::F6 => KeyCode::F6,
        egui::Key::F7 => KeyCode::F7,
        egui::Key::F8 => KeyCode::F8,
        egui::Key::F9 => KeyCode::F9,
        egui::Key::F10 => KeyCode::F10,
        egui::Key::F11 => KeyCode::F11,
        egui::Key::F12 => KeyCode::F12,
        egui::Key::F13 => KeyCode::F13,
        egui::Key::F14 => KeyCode::F14,
        egui::Key::F15 => KeyCode::F15,
        egui::Key::F16 => KeyCode::F16,
        egui::Key::F17 => KeyCode::F17,
        egui::Key::F18 => KeyCode::F18,
        egui::Key::F19 => KeyCode::F19,
        egui::Key::F20 => KeyCode::F20,
        egui::Key::F21 => KeyCode::F21,
        egui::Key::F22 => KeyCode::F22,
        egui::Key::F23 => KeyCode::F23,
        egui::Key::F24 => KeyCode::F24,
        egui::Key::F25 => KeyCode::F25,
        egui::Key::F26 => KeyCode::F26,
        egui::Key::F27 => KeyCode::F27,
        egui::Key::F28 => KeyCode::F28,
        egui::Key::F29 => KeyCode::F29,
        egui::Key::F30 => KeyCode::F30,
        egui::Key::F31 => KeyCode::F31,
        egui::Key::F32 => KeyCode::F32,
        egui::Key::F33 => KeyCode::F33,
        egui::Key::F34 => KeyCode::F34,
        egui::Key::F35 => KeyCode::F35,

        _ => return None,
    })
}

pub fn modifiers_from_modifiers_state(modifier_state: ModifiersState) -> egui::Modifiers {
    egui::Modifiers {
        alt: modifier_state.alt_key(),
        ctrl: modifier_state.control_key(),
        shift: modifier_state.shift_key(),
        #[cfg(target_os = "macos")]
        mac_cmd: modifier_state.super_key(),
        #[cfg(not(target_os = "macos"))]
        mac_cmd: false,
        #[cfg(target_os = "macos")]
        command: modifier_state.super_key(),
        #[cfg(not(target_os = "macos"))]
        command: modifier_state.control_key(),
    }
}

pub fn modifiers_state_from_modifiers(modifiers: egui::Modifiers) -> ModifiersState {
    let mut modifiers_state = ModifiersState::empty();
    if modifiers.shift {
        modifiers_state |= ModifiersState::SHIFT;
    }
    if modifiers.ctrl {
        modifiers_state |= ModifiersState::CONTROL;
    }
    if modifiers.alt {
        modifiers_state |= ModifiersState::ALT;
    }
    #[cfg(target_os = "macos")]
    if modifiers.mac_cmd {
        modifiers_state |= ModifiersState::SUPER;
    }
    // TODO: egui doesn't seem to support SUPER on Windows/Linux
    modifiers_state
}

pub const fn pointer_button_from_mouse(button: MouseButton) -> Option<PointerButton> {
    Some(match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Extra1,
        MouseButton::Forward => PointerButton::Extra2,
        MouseButton::Other(_) => return None,
    })
}

pub const fn mouse_button_from_pointer(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Primary => MouseButton::Left,
        PointerButton::Secondary => MouseButton::Right,
        PointerButton::Middle => MouseButton::Middle,
        PointerButton::Extra1 => MouseButton::Back,
        PointerButton::Extra2 => MouseButton::Forward,
    }
}

pub const fn translate_cursor(cursor_icon: egui::CursorIcon) -> Option<winit::window::CursorIcon> {
    use egui::CursorIcon;

    match cursor_icon {
        CursorIcon::None => None,

        CursorIcon::Alias => Some(winit::window::CursorIcon::Alias),
        CursorIcon::AllScroll => Some(winit::window::CursorIcon::AllScroll),
        CursorIcon::Cell => Some(winit::window::CursorIcon::Cell),
        CursorIcon::ContextMenu => Some(winit::window::CursorIcon::ContextMenu),
        CursorIcon::Copy => Some(winit::window::CursorIcon::Copy),
        CursorIcon::Crosshair => Some(winit::window::CursorIcon::Crosshair),
        CursorIcon::Default => Some(winit::window::CursorIcon::Default),
        CursorIcon::Grab => Some(winit::window::CursorIcon::Grab),
        CursorIcon::Grabbing => Some(winit::window::CursorIcon::Grabbing),
        CursorIcon::Help => Some(winit::window::CursorIcon::Help),
        CursorIcon::Move => Some(winit::window::CursorIcon::Move),
        CursorIcon::NoDrop => Some(winit::window::CursorIcon::NoDrop),
        CursorIcon::NotAllowed => Some(winit::window::CursorIcon::NotAllowed),
        CursorIcon::PointingHand => Some(winit::window::CursorIcon::Pointer),
        CursorIcon::Progress => Some(winit::window::CursorIcon::Progress),

        CursorIcon::ResizeHorizontal => Some(winit::window::CursorIcon::EwResize),
        CursorIcon::ResizeNeSw => Some(winit::window::CursorIcon::NeswResize),
        CursorIcon::ResizeNwSe => Some(winit::window::CursorIcon::NwseResize),
        CursorIcon::ResizeVertical => Some(winit::window::CursorIcon::NsResize),

        CursorIcon::ResizeEast => Some(winit::window::CursorIcon::EResize),
        CursorIcon::ResizeSouthEast => Some(winit::window::CursorIcon::SeResize),
        CursorIcon::ResizeSouth => Some(winit::window::CursorIcon::SResize),
        CursorIcon::ResizeSouthWest => Some(winit::window::CursorIcon::SwResize),
        CursorIcon::ResizeWest => Some(winit::window::CursorIcon::WResize),
        CursorIcon::ResizeNorthWest => Some(winit::window::CursorIcon::NwResize),
        CursorIcon::ResizeNorth => Some(winit::window::CursorIcon::NResize),
        CursorIcon::ResizeNorthEast => Some(winit::window::CursorIcon::NeResize),
        CursorIcon::ResizeColumn => Some(winit::window::CursorIcon::ColResize),
        CursorIcon::ResizeRow => Some(winit::window::CursorIcon::RowResize),

        CursorIcon::Text => Some(winit::window::CursorIcon::Text),
        CursorIcon::VerticalText => Some(winit::window::CursorIcon::VerticalText),
        CursorIcon::Wait => Some(winit::window::CursorIcon::Wait),
        CursorIcon::ZoomIn => Some(winit::window::CursorIcon::ZoomIn),
        CursorIcon::ZoomOut => Some(winit::window::CursorIcon::ZoomOut),
    }
}
