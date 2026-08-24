//! Panes, and the columns a debugging window stacks them in.
//!
//! A window names its views with an enum implementing [`Pane`], and this module places them. The
//! layout is derived from which panes are open, how big the viewport is, and the height the window
//! asks for each pane, so a given set of open panes at a given size always produces the same
//! arrangement. A dragged splitter overrides a height until [`reset_layout`] forgets it.
//!
//! A window whose panes hold text passes [`Pane::default_size`] and gets splitters it can drag. One
//! whose panes hold images measures them instead, and the pane follows what it draws.

use egui::containers::{
    PanelState,
    menu::{MenuButton, MenuConfig},
};
use egui::{CentralPanel, Context, Panel, PopupCloseBehavior, Rect, ScrollArea, Ui, Vec2};

/// A view a window can open, close and lay out.
///
/// Implemented by an enum per window. The window keeps the open ones in a `Vec<Self>` and draws
/// each body itself, since a body reads the window's own state.
pub trait Pane: Copy + PartialEq + std::fmt::Debug + 'static {
    /// Every pane, in the order a column stacks them.
    const ALL: &'static [Self];

    /// The heading the pane draws above its view.
    fn title(self) -> &'static str;

    /// Where the pane is placed.
    fn column(self) -> Column;

    /// The pane's height where the window has no better answer.
    ///
    /// The center column's single pane takes what is left, so its height is never asked for. A
    /// window whose pane sizes follow its content passes its own measure to [`columns`] instead.
    fn default_size(self) -> f32;

    /// The [`egui::Id`] the pane's [`Panel`] and its stored size are keyed by.
    fn id(self) -> &'static str;
}

/// Where a pane is placed. Panes keep their column, and only redistribute height within it.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum Column {
    /// What is left once the other columns have taken their space.
    Center,
    /// Down the right edge, above the bottom column.
    Right,
    /// Across the full width of the window.
    Bottom,
}

impl Column {
    /// Every column, in the order it claims space. The center takes what the others leave.
    pub const ALL: [Self; 3] = [Self::Bottom, Self::Right, Self::Center];

    /// The column's own size before anything drags its splitter: a width on the right, a height
    /// along the bottom.
    const fn default_size(self) -> f32 {
        match self {
            // Wide enough for the debugger's six-column register grid without wrapping.
            Self::Center | Self::Right => 380.0,
            // Tall enough for a screenful of the debugger's hex rows above its address box.
            Self::Bottom => 200.0,
        }
    }

    /// How the column divides its space among the panes of `open` that belong to it: those sized
    /// by [`Pane::default_size`], then the one taking what is left.
    ///
    /// `None` when none of them are open, which is when the column is not drawn and so takes no
    /// space.
    pub fn tiling<P: Pane>(self, open: &[P]) -> Option<(Vec<P>, P)> {
        let mut panes = P::ALL
            .iter()
            .copied()
            .filter(|pane| pane.column() == self && open.contains(pane))
            .collect::<Vec<_>>();
        let filling = panes.pop()?;
        Some((panes, filling))
    }

    /// The [`egui::Id`] the column's [`Panel`] and its stored size are keyed by.
    const fn id(self) -> &'static str {
        match self {
            Self::Center => "column_center",
            Self::Right => "column_right",
            Self::Bottom => "column_bottom",
        }
    }

    /// The id the column is keyed by inside `window`, which keeps two windows' columns apart.
    fn scoped_id(self, window: &str) -> egui::Id {
        egui::Id::new(window).with(self.id())
    }
}

/// Forget every dragged splitter in `window`, so the next frame lays out from the default sizes.
pub fn reset_layout<P: Pane>(ctx: &Context, window: &str) {
    ctx.data_mut(|data| {
        for pane in P::ALL {
            data.remove::<PanelState>(egui::Id::new(window).with(pane.id()));
        }
        for column in Column::ALL {
            data.remove::<PanelState>(column.scoped_id(window));
        }
    });
}

/// The View menu: which panes are open, and the button that undoes every splitter drag.
///
/// Reports the pane whose checkbox was clicked and what it was set to. The center column's pane
/// has no toggle, since an empty center with the columns still drawn reads as a broken window.
pub fn view_menu<P: Pane>(ui: &mut Ui, window: &str, open: &[P]) -> Option<(P, bool)> {
    let mut toggled = None;
    // Stays open until the pointer leaves it, so several panes can be toggled in one go rather
    // than reopening the menu for each.
    MenuButton::new("View")
        .config(MenuConfig::new().close_behavior(PopupCloseBehavior::CloseOnClickOutside))
        .ui(ui, |ui| {
            for pane in P::ALL.iter().copied() {
                if pane.column() == Column::Center {
                    continue;
                }
                let mut is_open = open.contains(&pane);
                if ui.checkbox(&mut is_open, pane.title()).changed() {
                    toggled = Some((pane, is_open));
                }
            }
            ui.separator();
            if ui
                .button("Reset layout")
                .on_hover_text("Restore panes to their default sizes")
                .clicked()
            {
                reset_layout::<P>(ui.ctx(), window);
                // The one item here that is done after one click, unlike the checkboxes above.
                ui.close();
            }
        });
    toggled
}

/// Lay `open`'s panes out in their columns, drawing each body through `body`.
///
/// Reports the pane whose ✖ was clicked, which the caller closes. The bottom column claims its
/// strip first, then the right column is told where that leaves off: a side panel nested in a
/// `Ui` takes the height of its contents, so an unbounded one rules its divider down the side of
/// the bottom column.
///
/// A panel stores its rect after drawing its body, so mid-drag that rect names where the splitter
/// reached, not where the body went. Leaving the center to egui keeps a drag off the bottom
/// column.
pub fn columns<P: Pane>(
    ui: &mut Ui,
    window: &str,
    open: &[P],
    size: &dyn Fn(P) -> f32,
    body: &mut dyn FnMut(&mut Ui, P),
) -> Option<P> {
    let mut closed = None;
    let mut rest = ui.available_rect_before_wrap();
    if let Some(bottom) = column(ui, window, Column::Bottom, open, size, body, &mut closed) {
        rest.max.y = bottom.top();
    }
    right_column(ui, window, open, rest.height(), size, body, &mut closed);
    column(ui, window, Column::Center, open, size, body, &mut closed);
    closed
}

/// Stack `column`'s open panes inside a panel of its own, drawing nothing when it is empty.
///
/// Every pane but the last takes the height `size` gives it, and the last takes what is left, so a
/// splitter sits between each pair and one between the column and the center.
///
/// Reports the rect the column took, or `None` where it had nothing open to draw.
fn column<P: Pane>(
    ui: &mut Ui,
    window: &str,
    column: Column,
    open: &[P],
    size: &dyn Fn(P) -> f32,
    body: &mut dyn FnMut(&mut Ui, P),
    closed: &mut Option<P>,
) -> Option<Rect> {
    let (sized, filling) = column.tiling(open)?;
    let mut tile = |ui: &mut Ui, body: &mut dyn FnMut(&mut Ui, P)| {
        for pane in &sized {
            let close = Panel::top(egui::Id::new(window).with(pane.id()))
                .resizable(true)
                .default_size(size(*pane))
                .show(ui, |ui| heading(ui, *pane, body))
                .inner;
            if close {
                *closed = Some(*pane);
            }
        }
        if CentralPanel::default()
            .show(ui, |ui| heading(ui, filling, body))
            .inner
        {
            *closed = Some(filling);
        }
    };
    match column {
        Column::Center => tile(ui, body),
        Column::Bottom => {
            Panel::bottom(column.scoped_id(window))
                .resizable(true)
                .default_size(column.default_size())
                .show(ui, |ui| tile(ui, body));
        }
        Column::Right => unreachable!("the right column is drawn by `right_column`"),
    }
    // What the column claimed, read back from where egui records it and `reset_layout` clears it.
    // A panel's own response covers what it drew inside, not the strip it took.
    match column {
        Column::Center => Some(ui.min_rect()),
        Column::Right | Column::Bottom => {
            PanelState::load(ui.ctx(), column.scoped_id(window)).map(|state| state.outer_rect)
        }
    }
}

/// The right column: each pane at the height `size` gives it, one under the next, scrolling
/// together.
///
/// The caller measures `height`, the space the column has to work in. A window too short for all
/// of them scrolls the column rather than cutting the last ones off. The last pane takes whatever
/// the ones above it left, so a column with room to spare looks the same as one laid out to fit.
///
/// Panes here are plain sections rather than nested [`Panel`]s: a `Panel` sets a clip rect of its
/// own and would draw straight through the scroll area around it.
fn right_column<P: Pane>(
    ui: &mut Ui,
    window: &str,
    open: &[P],
    height: f32,
    size: &dyn Fn(P) -> f32,
    body: &mut dyn FnMut(&mut Ui, P),
    closed: &mut Option<P>,
) {
    let column = Column::Right;
    let Some((sized, filling)) = column.tiling(open) else {
        return;
    };
    let panes = sized.into_iter().chain([filling]).collect::<Vec<_>>();
    Panel::right(column.scoped_id(window))
        .default_size(column.default_size())
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt(column.scoped_id(window))
                // Bounded by the caller's measurement. The panel takes the height of its contents,
                // so asking it how tall it is would answer in a circle.
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // The bar floats over the content rather than taking width of its own, so the
                    // room for it comes off each pane. Without it a pane's ✖ sits under the bar.
                    let bar = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_outer_margin;
                    let last = panes.len() - 1;
                    for (index, pane) in panes.into_iter().enumerate() {
                        // The last pane takes what the ones above it left, measured rather than
                        // worked out: a separator's own height is easy to be a few pixels out on,
                        // and a stack that overshoots puts a scrollbar on a column with room to
                        // spare. Its own height wins where the column is too short, leaving the
                        // stack something to scroll.
                        let height = if index == last {
                            ui.available_height().max(size(pane))
                        } else {
                            size(pane)
                        };
                        let width = ui.available_width() - bar;
                        ui.allocate_ui(Vec2::new(width, height), |ui| {
                            ui.set_min_height(height);
                            if heading(ui, pane, body) {
                                *closed = Some(pane);
                            }
                        });
                        if index != last {
                            ui.separator();
                        }
                    }
                });
        });
}

/// Draw `pane`'s heading and its view, reporting whether the heading's ✖ was clicked.
///
/// A panel has no title bar of its own, so the heading is part of what the pane draws.
fn heading<P: Pane>(ui: &mut Ui, pane: P, body: &mut dyn FnMut(&mut Ui, P)) -> bool {
    let closed = ui
        .horizontal(|ui| {
            ui.strong(pane.title());
            if pane.column() == Column::Center {
                return false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small_button("✖").on_hover_text("Close pane").clicked()
            })
            .inner
        })
        .inner;
    body(ui, pane);
    closed
}
