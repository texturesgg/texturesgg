//! A tool pane: a card floating beside the 3D stage, with its name, an
//! optional count, a chevron that folds it to its header, and a close
//! button. Panes stack in a column; one fills what the others leave, the
//! others take what they need.

use crate::icon::IconName;
use crate::icon_button::IconButton;
use crate::tokens::{density, font, radius, space};
use crate::{Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, BoxShadow, ElementId, FontWeight, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, point, px, relative,
};
use std::rc::Rc;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// The space around a pane column, in web pixels.
pub const PANE_MARGIN: f32 = 10.0;

/// How a pane shares the column's height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneFit {
    /// Take the height the other panes leave.
    Fill,
    /// Take the height its content needs, up to about half the column.
    Content,
}

#[derive(IntoElement)]
pub struct Pane {
    id: ElementId,
    title: SharedString,
    count: Option<SharedString>,
    fit: PaneFit,
    folded: bool,
    on_fold: Option<Handler>,
    on_close: Option<Handler>,
    body: Option<AnyElement>,
}

impl Pane {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, fit: PaneFit) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            count: None,
            fit,
            folded: false,
            on_fold: None,
            on_close: None,
            body: None,
        }
    }

    /// A count or short note after the title.
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// Show only the header; `on_fold` toggles it.
    pub fn folded(
        mut self,
        folded: bool,
        on_fold: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.folded = folded;
        self.on_fold = Some(Rc::new(on_fold));
        self
    }

    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    pub fn child(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }
}

impl RenderOnce for Pane {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let title = self.title.clone();
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(rem(space::XXS + 2.0))
            .h(rem(38.0))
            .px(rem(space::XXS + 2.0))
            .when_some(self.on_fold, |header, fold| {
                header.child(
                    IconButton::new(
                        (self.id.clone(), "fold"),
                        if self.folded {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        },
                        if self.folded {
                            format!("Show {title}")
                        } else {
                            format!("Fold {title}")
                        },
                    )
                    .small()
                    .on_press(move |window, cx| fold(window, cx)),
                )
            })
            .child(
                div()
                    .text_size(rem(density::CONTROL_TEXT))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title.clone()),
            )
            .when_some(self.count, |header, count| {
                header.child(
                    div()
                        .font_family(font::MONO)
                        .text_size(rem(11.0))
                        .text_color(palette.muted.to_gpui())
                        .child(count),
                )
            })
            .child(div().flex_1())
            .when_some(self.on_close, |header, close| {
                header.child(
                    IconButton::new(
                        (self.id.clone(), "close"),
                        IconName::Close,
                        format!("Close {}", self.title),
                    )
                    .small()
                    .on_press(move |window, cx| close(window, cx)),
                )
            });
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .rounded(rem(radius::MD + 2.0))
            .bg(palette.surface.to_gpui())
            .border_1()
            .border_color(palette.line_strong.to_gpui())
            .shadow(vec![BoxShadow {
                color: palette.popover_shadow.color.to_gpui().into(),
                offset: point(px(0.0), px(12.0)),
                blur_radius: px(32.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .map(|pane| match (self.folded, self.fit) {
                (true, _) => pane.flex_none(),
                (false, PaneFit::Fill) => pane.flex_1(),
                (false, PaneFit::Content) => pane.flex_none().max_h(relative(0.48)),
            })
            .child(header)
            .when(!self.folded, |pane| {
                pane.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .children(self.body),
                )
            })
    }
}

/// The drag payload of the column's edge.
struct ColumnDrag;

/// Resizing shows no drag preview; the column itself follows the pointer.
struct NoPreview;

impl gpui::Render for NoPreview {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// The column the panes float in, over the right of the stage, `margin`
/// web pixels in from its edges. Its width is the state's right pane: its
/// left edge drags to resize it, and a double click resets it.
#[derive(IntoElement)]
pub struct PaneColumn {
    state: gpui::Entity<crate::SplitState>,
    margin: f32,
    panes: Vec<AnyElement>,
}

impl PaneColumn {
    pub fn new(state: gpui::Entity<crate::SplitState>, margin: f32) -> Self {
        Self {
            state,
            margin,
            panes: Vec::new(),
        }
    }

    pub fn children(mut self, panes: impl IntoIterator<Item = AnyElement>) -> Self {
        self.panes.extend(panes);
        self
    }
}

impl RenderOnce for PaneColumn {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::{AppContext, InteractiveElement, StatefulInteractiveElement};
        let palette = Theme::global(cx).palette;
        let width = self.state.read(cx).right.width;
        let resize = self.state.clone();
        let reset = self.state.clone();
        div()
            .absolute()
            .top(rem(self.margin))
            .bottom(rem(self.margin))
            .right(rem(self.margin))
            .w(rem(width))
            .flex()
            .flex_col()
            .gap(rem(space::XS))
            .on_drag_move::<ColumnDrag>(move |event, window, cx| {
                let distance = event.bounds.right() - event.event.position.x;
                let width = f32::from(distance) * 16.0 / f32::from(window.rem_size());
                resize.update(cx, |state, cx| {
                    state.right.set(width);
                    cx.notify();
                });
            })
            .child(
                div()
                    .id("pane-column-edge")
                    .absolute()
                    .top(rem(space::SM))
                    .bottom(rem(space::SM))
                    .left(rem(-7.0))
                    .w(rem(5.0))
                    .rounded(rem(3.0))
                    .cursor_col_resize()
                    .hover(|style| style.bg(palette.line_strong.to_gpui()))
                    .on_drag(ColumnDrag, |_, _, _, cx| cx.new(|_| NoPreview))
                    .on_click(move |event, _, cx| {
                        if event.click_count() == 2 {
                            reset.update(cx, |state, cx| {
                                state.right.reset();
                                cx.notify();
                            });
                        }
                    }),
            )
            .children(self.panes)
    }
}
