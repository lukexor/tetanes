use crate::nes::{
    config::Config,
    input::{Gamepads, Input},
    renderer::event::{
        key_from_keycode, modifiers_from_modifiers_state, pointer_button_from_mouse,
    },
};
use egui::{
    Checkbox, Context, KeyboardShortcut, Pos2, Rect, Response, Sense, TextStyle, TextWrapMode, Ui,
    Widget, WidgetText,
};
use std::ops::{Deref, DerefMut};
use tetanes_core::ppu::Ppu;
use winit::{event::ElementState, window::Window};

#[derive(Debug, Copy, Clone)]
#[must_use]
pub struct ViewportOptions {
    pub enabled: bool,
    pub always_on_top: bool,
}

#[derive(Debug, Copy, Clone)]
pub enum ShowShortcut {
    Yes,
    No,
}

impl ShowShortcut {
    pub fn then<T>(&self, f: impl FnOnce() -> T) -> Option<T> {
        match self {
            Self::Yes => Some(f()),
            Self::No => None,
        }
    }
}

pub trait ShortcutText<'a>
where
    Self: Sized + 'a,
{
    fn shortcut_text(self, shortcut_text: impl Into<WidgetText>) -> ShortcutWidget<'a, Self> {
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

pub fn input_down(ui: &mut Ui, gamepads: &Gamepads, cfg: &Config, input: Input) -> bool {
    ui.input_mut(|i| match input {
        Input::Key(keycode, modifier_state) => key_from_keycode(keycode).is_some_and(|key| {
            let modifiers = modifiers_from_modifiers_state(modifier_state);
            i.key_down(key) && i.modifiers == modifiers
        }),
        Input::Button(player, button) => cfg
            .input
            .gamepad_assigned_to(player)
            .and_then(|uuid| gamepads.gamepad_by_uuid(&uuid))
            .is_some_and(|g| g.is_pressed(button)),
        Input::Mouse(mouse_button) => pointer_button_from_mouse(mouse_button)
            .is_some_and(|pointer| i.pointer.button_down(pointer)),
        Input::Axis(player, axis, direction) => cfg
            .input
            .gamepad_assigned_to(player)
            .and_then(|uuid| gamepads.gamepad_by_uuid(&uuid))
            .and_then(|g| g.axis_data(axis).map(|data| data.value()))
            .is_some_and(|value| {
                let (dir, state) = Gamepads::axis_state(value);
                dir == Some(direction) && state == ElementState::Pressed
            }),
    })
}

#[must_use]
pub struct ShortcutWidget<'a, T> {
    inner: T,
    shortcut_text: WidgetText,
    phantom: std::marker::PhantomData<&'a ()>,
}

impl<T> Deref for ShortcutWidget<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for ShortcutWidget<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Widget for ShortcutWidget<'_, T>
where
    T: Widget,
{
    fn ui(self, ui: &mut Ui) -> Response {
        ui.horizontal(|ui| {
            let res = self.inner.ui(ui);

            if !self.shortcut_text.is_empty() {
                let shortcut_galley = self.shortcut_text.into_galley(
                    ui,
                    Some(TextWrapMode::Extend),
                    f32::INFINITY,
                    TextStyle::Button,
                );

                let available_rect = ui.available_rect_before_wrap();

                let gap_before_shortcut_text = ui.spacing().item_spacing.x;
                let mut desired_size = shortcut_galley.size();
                desired_size.x += gap_before_shortcut_text;
                // Ensure sense is set to hover so that screen readers don't try to read it,
                // consistent with `shortcut_text` on `Button`
                let (rect, _) = ui.allocate_at_least(desired_size, Sense::hover());

                if ui.is_rect_visible(rect) {
                    let text_pos = Pos2::new(
                        available_rect.max.x - shortcut_galley.size().x,
                        rect.center().y - 0.5 * shortcut_galley.size().y,
                    );
                    ui.painter()
                        .galley(text_pos, shortcut_galley, ui.visuals().weak_text_color());
                }
            }
            res
        })
        .inner
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

impl Widget for ToggleValue<'_> {
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

impl<T: PartialEq> Widget for RadioValue<'_, T> {
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

pub fn screen_size_in_pixels(window: &Window) -> egui::Vec2 {
    let size = window.inner_size();
    egui::vec2(size.width as f32, size.height as f32)
}

pub fn pixels_per_point(egui_ctx: &egui::Context, window: &Window) -> f32 {
    let native_pixels_per_point = window.scale_factor() as f32;
    let egui_zoom_factor = egui_ctx.zoom_factor();
    egui_zoom_factor * native_pixels_per_point
}

pub fn inner_rect_in_points(window: &Window, pixels_per_point: f32) -> Option<egui::Rect> {
    let inner_pos_px = window.inner_position().ok()?;
    let inner_pos_px = egui::pos2(inner_pos_px.x as f32, inner_pos_px.y as f32);

    let inner_size_px = window.inner_size();
    let inner_size_px = egui::vec2(inner_size_px.width as f32, inner_size_px.height as f32);

    let inner_rect_px = egui::Rect::from_min_size(inner_pos_px, inner_size_px);

    Some(inner_rect_px / pixels_per_point)
}

pub fn outer_rect_in_points(window: &Window, pixels_per_point: f32) -> Option<egui::Rect> {
    let outer_pos_px = window.outer_position().ok()?;
    let outer_pos_px = egui::pos2(outer_pos_px.x as f32, outer_pos_px.y as f32);

    let outer_size_px = window.outer_size();
    let outer_size_px = egui::vec2(outer_size_px.width as f32, outer_size_px.height as f32);

    let outer_rect_px = egui::Rect::from_min_size(outer_pos_px, outer_size_px);

    Some(outer_rect_px / pixels_per_point)
}

pub fn to_winit_icon(icon: &egui::IconData) -> Option<winit::window::Icon> {
    if icon.is_empty() {
        None
    } else {
        match winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height) {
            Ok(winit_icon) => Some(winit_icon),
            Err(err) => {
                tracing::warn!("Invalid IconData: {err}");
                None
            }
        }
    }
}

/// An animated dashed rectangle.
pub fn animated_dashed_rect(
    ui: &mut Ui,
    rect: Rect,
    stroke: impl Into<egui::Stroke>,
    dash_length: f32,
    gap_length: f32,
) {
    if ui.is_rect_visible(rect) {
        ui.ctx().request_repaint(); // because it is animated

        let rect = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ];
        let time = ui.input(|i| i.time as f32);
        let total_length = dash_length + gap_length;
        let dash_offset = (time * 10.0) % total_length;

        ui.painter().add(egui::Shape::dashed_line_with_offset(
            &rect,
            stroke,
            &[dash_length],
            &[gap_length],
            dash_offset,
        ));
    }
}
