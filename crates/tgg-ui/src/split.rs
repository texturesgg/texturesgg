//! The editor's three-pane layout: resizable side panels around a flexible
//! center.
//!
//! Each side panel has a drag handle on its inner edge: a 1 px line with a
//! wider hit area. Dragging resizes the panel within its range, and a double
//! click resets it. Widths live in [`SplitState`], an entity the shell owns
//! so it can persist them.

use crate::tokens::density;
use crate::{Theme, rem};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Pixels, Render, RenderOnce, StatefulInteractiveElement, Styled, Window, div, px,
};

/// A side panel's width and its limits, in web pixels, so they scale with the
/// UI like every other token.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneSize {
    pub width: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl PaneSize {
    pub const fn new(default: f32, min: f32, max: f32) -> Self {
        Self {
            width: default,
            min,
            max,
            default,
        }
    }

    /// Set the width, clamped to the panel's range.
    pub fn set(&mut self, width: f32) {
        self.width = width.clamp(self.min, self.max);
    }

    pub fn reset(&mut self) {
        self.width = self.default;
    }
}

/// The side panels' widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitState {
    pub left: PaneSize,
    pub right: PaneSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
}

impl SplitState {
    fn pane(&mut self, edge: Edge) -> &mut PaneSize {
        match edge {
            Edge::Left => &mut self.left,
            Edge::Right => &mut self.right,
        }
    }
}

/// The drag payload of a resize handle.
struct ResizeDrag(Edge);

/// Resizing shows no drag preview; the panel itself follows the pointer.
struct NoPreview;

impl Render for NoPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

#[derive(IntoElement)]
pub struct Split {
    state: Entity<SplitState>,
    left: Option<AnyElement>,
    center: Option<AnyElement>,
    right: Option<AnyElement>,
}

impl Split {
    pub fn new(state: Entity<SplitState>) -> Self {
        Self {
            state,
            left: None,
            center: None,
            right: None,
        }
    }

    pub fn left(mut self, panel: impl IntoElement) -> Self {
        self.left = Some(panel.into_any_element());
        self
    }

    pub fn center(mut self, panel: impl IntoElement) -> Self {
        self.center = Some(panel.into_any_element());
        self
    }

    pub fn right(mut self, panel: impl IntoElement) -> Self {
        self.right = Some(panel.into_any_element());
        self
    }
}

impl RenderOnce for Split {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let widths = *self.state.read(cx);
        let state = self.state.clone();
        div()
            .size_full()
            .flex()
            .on_drag_move::<ResizeDrag>(move |event, window, cx| {
                let edge = event.drag(cx).0;
                let (x, bounds) = (event.event.position.x, event.bounds);
                let distance = match edge {
                    Edge::Left => x - bounds.left(),
                    Edge::Right => bounds.right() - x,
                };
                let width = web_pixels(distance, window);
                state.update(cx, |state, cx| {
                    state.pane(edge).set(width);
                    cx.notify();
                });
            })
            .child(
                div()
                    .h_full()
                    .flex_none()
                    .w(rem(widths.left.width))
                    .overflow_hidden()
                    .children(self.left),
            )
            .child(handle(&self.state, Edge::Left, cx))
            .child(div().h_full().flex_1().min_w_0().children(self.center))
            .child(handle(&self.state, Edge::Right, cx))
            .child(
                div()
                    .h_full()
                    .flex_none()
                    .w(rem(widths.right.width))
                    .overflow_hidden()
                    .children(self.right),
            )
    }
}

/// A distance in window pixels as web pixels at the current UI scale.
fn web_pixels(distance: Pixels, window: &Window) -> f32 {
    f32::from(distance) * 16.0 / f32::from(window.rem_size())
}

/// A 1 px line with a wider hit area that resizes its panel when dragged and
/// resets it on a double click.
fn handle(state: &Entity<SplitState>, edge: Edge, cx: &App) -> impl IntoElement {
    let palette = Theme::global(cx).palette;
    let state = state.clone();
    let id = match edge {
        Edge::Left => "split-handle-left",
        Edge::Right => "split-handle-right",
    };
    let hit = rem(density::HANDLE_HIT);
    div()
        .relative()
        .h_full()
        .w(px(1.0))
        .flex_none()
        .bg(palette.line.to_gpui())
        .child(
            div()
                .id(id)
                .absolute()
                .top_0()
                .bottom_0()
                .left(-hit / 2.0)
                .w(hit)
                .cursor_col_resize()
                .hover(|style| style.bg(palette.line_strong.to_gpui()))
                .on_drag(ResizeDrag(edge), |_, _, _, cx| cx.new(|_| NoPreview))
                .on_click(move |event, _, cx| {
                    if event.click_count() == 2 {
                        state.update(cx, |state, cx| {
                            state.pane(edge).reset();
                            cx.notify();
                        });
                    }
                }),
        )
}
