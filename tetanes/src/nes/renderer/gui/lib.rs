use crate::nes::{
    config::Config,
    input::{Gamepads, Input},
};
use egui::{
    Align, Checkbox, Context, Key, KeyboardShortcut, Layout, Modifiers, PointerButton, Pos2, Rect,
    Response, RichText, Ui, Widget, WidgetText,
};
use std::ops::{Deref, DerefMut};
use tetanes_core::ppu::Ppu;
use winit::{
    event::{ElementState, MouseButton},
    keyboard::{KeyCode, ModifiersState},
};

#[derive(Debug, Copy, Clone)]
#[must_use]
pub struct ViewportOptions {
    pub enabled: bool,
    pub always_on_top: bool,
}

pub trait ShortcutText<'a>
where
    Self: Sized + 'a,
{
    fn shortcut_text(self, shortcut_text: impl Into<RichText>) -> ShortcutWidget<'a, Self> {
        ShortcutWidget {
            inner: self,
            shortcut_text: shortcut_text.into(),
            phantom: std::marker::PhantomData,
        }
    }
}

pub fn cursor_to_zapper(x: f32, y: f32, rect: Rect) -> Option<Pos2> {
    let width = Ppu::WIDTH as f32;
    let height = Ppu::HEIGHT as f32;
    // Normalize x/y to 0..=1 and scale to PPU dimensions
    let x = ((x - rect.min.x) / rect.width()) * width;
    let y = ((y - rect.min.y) / rect.height()) * height;
    ((0.0..width).contains(&x) && (0.0..height).contains(&y)).then_some(Pos2::new(x, y))
}

pub fn input_down(ui: &mut Ui, gamepads: Option<&Gamepads>, cfg: &Config, input: Input) -> bool {
    ui.input_mut(|i| match input {
        Input::Key(keycode, modifier_state) => key_from_keycode(keycode).map_or(false, |key| {
            let modifiers = modifiers_from_modifiers_state(modifier_state);
            i.key_down(key) && i.modifiers == modifiers
        }),
        Input::Button(player, button) => cfg
            .input
            .gamepad_assigned_to(player)
            .and_then(|uuid| gamepads.map(|g| g.gamepad_by_uuid(&uuid)))
            .flatten()
            .map_or(false, |g| g.is_pressed(button)),
        Input::Mouse(mouse_button) => pointer_button_from_mouse(mouse_button)
            .map_or(false, |pointer| i.pointer.button_down(pointer)),
        Input::Axis(player, axis, direction) => cfg
            .input
            .gamepad_assigned_to(player)
            .and_then(|uuid| gamepads.map(|g| g.gamepad_by_uuid(&uuid)))
            .flatten()
            .and_then(|g| g.axis_data(axis).map(|data| data.value()))
            .map_or(false, |value| {
                let (dir, state) = Gamepads::axis_state(value);
                dir == Some(direction) && state == ElementState::Pressed
            }),
    })
}

#[must_use]
pub struct ShortcutWidget<'a, T> {
    inner: T,
    shortcut_text: RichText,
    phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a, T> Deref for ShortcutWidget<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for ShortcutWidget<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T> Widget for ShortcutWidget<'a, T>
where
    T: Widget,
{
    fn ui(self, ui: &mut Ui) -> Response {
        if self.shortcut_text.is_empty() {
            self.inner.ui(ui)
        } else {
            ui.horizontal(|ui| {
                let res = self.inner.ui(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.weak(self.shortcut_text);
                });
                res
            })
            .inner
        }
    }
}

#[must_use]
pub struct ToggleValue<'a> {
    selected: &'a mut bool,
    text: WidgetText,
}

impl<'a> ToggleValue<'a> {
    pub fn new(selected: &'a mut bool, text: impl Into<WidgetText>) -> Self {
        Self {
            selected,
            text: text.into(),
        }
    }
}

impl<'a> Widget for ToggleValue<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut res = ui.selectable_label(*self.selected, self.text);
        if res.clicked() {
            *self.selected = !*self.selected;
            res.mark_changed();
        }
        res
    }
}

#[must_use]
pub struct RadioValue<'a, T> {
    current_value: &'a mut T,
    alternative: T,
    text: WidgetText,
}

impl<'a, T: PartialEq> RadioValue<'a, T> {
    pub fn new(current_value: &'a mut T, alternative: T, text: impl Into<WidgetText>) -> Self {
        Self {
            current_value,
            alternative,
            text: text.into(),
        }
    }
}

impl<'a, T: PartialEq> Widget for RadioValue<'a, T> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut res = ui.radio(*self.current_value == self.alternative, self.text);
        if res.clicked() && *self.current_value != self.alternative {
            *self.current_value = self.alternative;
            res.mark_changed();
        }
        res
    }
}

impl<'a> ShortcutText<'a> for Checkbox<'a> {}
impl<'a> ShortcutText<'a> for ToggleValue<'a> {}
impl<'a, T> ShortcutText<'a> for RadioValue<'a, T> {}

impl TryFrom<Input> for KeyboardShortcut {
    type Error = ();

    fn try_from(val: Input) -> Result<Self, Self::Error> {
        if let Input::Key(keycode, modifier_state) = val {
            Ok(KeyboardShortcut {
                logical_key: key_from_keycode(keycode).ok_or(())?,
                modifiers: modifiers_from_modifiers_state(modifier_state),
            })
        } else {
            Err(())
        }
    }
}

impl TryFrom<(Key, Modifiers)> for Input {
    type Error = ();

    fn try_from((key, modifiers): (Key, Modifiers)) -> Result<Self, Self::Error> {
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

pub const fn key_from_keycode(keycode: KeyCode) -> Option<Key> {
    Some(match keycode {
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,

        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,

        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,

        // Punctuation
        KeyCode::Space => Key::Space,
        KeyCode::Comma => Key::Comma,
        KeyCode::Period => Key::Period,
        KeyCode::Semicolon => Key::Semicolon,
        KeyCode::Backslash => Key::Backslash,
        KeyCode::Slash | KeyCode::NumpadDivide => Key::Slash,
        KeyCode::BracketLeft => Key::OpenBracket,
        KeyCode::BracketRight => Key::CloseBracket,
        KeyCode::Backquote => Key::Backtick,

        KeyCode::Cut => Key::Cut,
        KeyCode::Copy => Key::Copy,
        KeyCode::Paste => Key::Paste,
        KeyCode::Minus | KeyCode::NumpadSubtract => Key::Minus,
        KeyCode::NumpadAdd => Key::Plus,
        KeyCode::Equal => Key::Equals,

        KeyCode::Digit0 | KeyCode::Numpad0 => Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Key::Num9,

        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,

        KeyCode::F1 => Key::F1,
        KeyCode::F2 => Key::F2,
        KeyCode::F3 => Key::F3,
        KeyCode::F4 => Key::F4,
        KeyCode::F5 => Key::F5,
        KeyCode::F6 => Key::F6,
        KeyCode::F7 => Key::F7,
        KeyCode::F8 => Key::F8,
        KeyCode::F9 => Key::F9,
        KeyCode::F10 => Key::F10,
        KeyCode::F11 => Key::F11,
        KeyCode::F12 => Key::F12,
        KeyCode::F13 => Key::F13,
        KeyCode::F14 => Key::F14,
        KeyCode::F15 => Key::F15,
        KeyCode::F16 => Key::F16,
        KeyCode::F17 => Key::F17,
        KeyCode::F18 => Key::F18,
        KeyCode::F19 => Key::F19,
        KeyCode::F20 => Key::F20,
        KeyCode::F21 => Key::F21,
        KeyCode::F22 => Key::F22,
        KeyCode::F23 => Key::F23,
        KeyCode::F24 => Key::F24,
        KeyCode::F25 => Key::F25,
        KeyCode::F26 => Key::F26,
        KeyCode::F27 => Key::F27,
        KeyCode::F28 => Key::F28,
        KeyCode::F29 => Key::F29,
        KeyCode::F30 => Key::F30,
        KeyCode::F31 => Key::F31,
        KeyCode::F32 => Key::F32,
        KeyCode::F33 => Key::F33,
        KeyCode::F34 => Key::F34,
        KeyCode::F35 => Key::F35,

        _ => {
            return None;
        }
    })
}

pub const fn keycode_from_key(key: Key) -> Option<KeyCode> {
    Some(match key {
        Key::ArrowDown => KeyCode::ArrowDown,
        Key::ArrowLeft => KeyCode::ArrowLeft,
        Key::ArrowRight => KeyCode::ArrowRight,
        Key::ArrowUp => KeyCode::ArrowUp,

        Key::Escape => KeyCode::Escape,
        Key::Tab => KeyCode::Tab,
        Key::Backspace => KeyCode::Backspace,
        Key::Enter => KeyCode::Enter,

        Key::Insert => KeyCode::Insert,
        Key::Delete => KeyCode::Delete,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,

        // Punctuation
        Key::Space => KeyCode::Space,
        Key::Comma => KeyCode::Comma,
        Key::Period => KeyCode::Period,
        Key::Semicolon => KeyCode::Semicolon,
        Key::Backslash => KeyCode::Backslash,
        Key::Slash => KeyCode::Slash,
        Key::OpenBracket => KeyCode::BracketLeft,
        Key::CloseBracket => KeyCode::BracketRight,

        Key::Cut => KeyCode::Cut,
        Key::Copy => KeyCode::Copy,
        Key::Paste => KeyCode::Paste,
        Key::Minus => KeyCode::Minus,
        Key::Plus => KeyCode::NumpadAdd,
        Key::Equals => KeyCode::Equal,

        Key::Num0 => KeyCode::Digit0,
        Key::Num1 => KeyCode::Digit1,
        Key::Num2 => KeyCode::Digit2,
        Key::Num3 => KeyCode::Digit3,
        Key::Num4 => KeyCode::Digit4,
        Key::Num5 => KeyCode::Digit5,
        Key::Num6 => KeyCode::Digit6,
        Key::Num7 => KeyCode::Digit7,
        Key::Num8 => KeyCode::Digit8,
        Key::Num9 => KeyCode::Digit9,

        Key::A => KeyCode::KeyA,
        Key::B => KeyCode::KeyB,
        Key::C => KeyCode::KeyC,
        Key::D => KeyCode::KeyD,
        Key::E => KeyCode::KeyE,
        Key::F => KeyCode::KeyF,
        Key::G => KeyCode::KeyG,
        Key::H => KeyCode::KeyH,
        Key::I => KeyCode::KeyI,
        Key::J => KeyCode::KeyJ,
        Key::K => KeyCode::KeyK,
        Key::L => KeyCode::KeyL,
        Key::M => KeyCode::KeyM,
        Key::N => KeyCode::KeyN,
        Key::O => KeyCode::KeyO,
        Key::P => KeyCode::KeyP,
        Key::Q => KeyCode::KeyQ,
        Key::R => KeyCode::KeyR,
        Key::S => KeyCode::KeyS,
        Key::T => KeyCode::KeyT,
        Key::U => KeyCode::KeyU,
        Key::V => KeyCode::KeyV,
        Key::W => KeyCode::KeyW,
        Key::X => KeyCode::KeyX,
        Key::Y => KeyCode::KeyY,
        Key::Z => KeyCode::KeyZ,

        Key::F1 => KeyCode::F1,
        Key::F2 => KeyCode::F2,
        Key::F3 => KeyCode::F3,
        Key::F4 => KeyCode::F4,
        Key::F5 => KeyCode::F5,
        Key::F6 => KeyCode::F6,
        Key::F7 => KeyCode::F7,
        Key::F8 => KeyCode::F8,
        Key::F9 => KeyCode::F9,
        Key::F10 => KeyCode::F10,
        Key::F11 => KeyCode::F11,
        Key::F12 => KeyCode::F12,
        Key::F13 => KeyCode::F13,
        Key::F14 => KeyCode::F14,
        Key::F15 => KeyCode::F15,
        Key::F16 => KeyCode::F16,
        Key::F17 => KeyCode::F17,
        Key::F18 => KeyCode::F18,
        Key::F19 => KeyCode::F19,
        Key::F20 => KeyCode::F20,
        Key::F21 => KeyCode::F21,
        Key::F22 => KeyCode::F22,
        Key::F23 => KeyCode::F23,
        Key::F24 => KeyCode::F24,
        Key::F25 => KeyCode::F25,
        Key::F26 => KeyCode::F26,
        Key::F27 => KeyCode::F27,
        Key::F28 => KeyCode::F28,
        Key::F29 => KeyCode::F29,
        Key::F30 => KeyCode::F30,
        Key::F31 => KeyCode::F31,
        Key::F32 => KeyCode::F32,
        Key::F33 => KeyCode::F33,
        Key::F34 => KeyCode::F34,
        Key::F35 => KeyCode::F35,

        _ => return None,
    })
}

pub fn modifiers_from_modifiers_state(modifier_state: ModifiersState) -> Modifiers {
    Modifiers {
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

pub fn modifiers_state_from_modifiers(modifiers: Modifiers) -> ModifiersState {
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

pub fn screen_center(ctx: &Context) -> Option<Pos2> {
    ctx.input(|i| {
        let outer_rect = i.viewport().outer_rect?;
        let size = outer_rect.size();
        let monitor_size = i.viewport().monitor_size?;
        if 1.0 < monitor_size.x && 1.0 < monitor_size.y {
            let x = (monitor_size.x - size.x) / 2.0;
            let y = (monitor_size.y - size.y) / 2.0;
            Some(Pos2::new(x, y))
        } else {
            None
        }
    })
}
