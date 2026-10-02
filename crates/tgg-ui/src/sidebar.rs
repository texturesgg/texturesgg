//! The app's sidebar: its places at the top and foot, and between them a
//! titled list of things to go back to (the costumes being edited). It
//! hides completely rather than folding to icons; the caller draws the
//! toggle beside the window buttons, where it stays in both states.
//!
//! Up and Down move between places while one has focus. Which place is
//! current belongs to the caller.

use crate::icon::{Icon, IconName};
use crate::tokens::{density, font, radius, space, text};
use crate::{Theme, rem, title_bar};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ObjectFit, ParentElement, RenderImage, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, StyledImage, Window, div, img, px,
};
use std::rc::Rc;
use std::sync::Arc;

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The sidebar's width, in web pixels.
pub const WIDTH: f32 = 232.0;

struct Place {
    icon: IconName,
    label: SharedString,
    /// A count after the label, such as the library's skins.
    count: Option<usize>,
    /// At the foot of the sidebar rather than the top.
    foot: bool,
}

/// A thing to go back to, listed under the places.
pub struct SidebarEntry {
    pub image: Option<Arc<RenderImage>>,
    pub label: SharedString,
    /// A short note after the label, in the muted color.
    pub detail: Option<SharedString>,
}

#[derive(IntoElement)]
pub struct Sidebar {
    id: ElementId,
    places: Vec<Place>,
    selected: Option<usize>,
    on_select: SelectHandler,
    title: SharedString,
    entries: Vec<SidebarEntry>,
    on_open: Option<SelectHandler>,
}

impl Sidebar {
    /// A sidebar with place `selected` current (`None`: none of them),
    /// calling `on_select` with a place's index.
    pub fn new(
        id: impl Into<ElementId>,
        selected: Option<usize>,
        on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            places: Vec::new(),
            selected,
            on_select: Rc::new(on_select),
            title: SharedString::default(),
            entries: Vec::new(),
            on_open: None,
        }
    }

    /// A place at the top of the sidebar.
    pub fn place(mut self, icon: IconName, label: impl Into<SharedString>) -> Self {
        self.places.push(Place {
            icon,
            label: label.into(),
            count: None,
            foot: false,
        });
        self
    }

    /// A place at the top, with a count after its label.
    pub fn place_counted(
        mut self,
        icon: IconName,
        label: impl Into<SharedString>,
        count: usize,
    ) -> Self {
        self.places.push(Place {
            icon,
            label: label.into(),
            count: Some(count),
            foot: false,
        });
        self
    }

    /// A place at the foot of the sidebar.
    pub fn foot(mut self, icon: IconName, label: impl Into<SharedString>) -> Self {
        self.places.push(Place {
            icon,
            label: label.into(),
            count: None,
            foot: true,
        });
        self
    }

    /// List `entries` under `title`, calling `on_open` with an entry's index.
    /// Nothing shows when there are none.
    pub fn entries(
        mut self,
        title: impl Into<SharedString>,
        entries: Vec<SidebarEntry>,
        on_open: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.title = title.into();
        self.entries = entries;
        self.on_open = Some(Rc::new(on_open));
        self
    }
}

/// A sidebar row: its look, before what it holds.
fn row(id: ElementId, current: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let palette = Theme::global(cx).palette;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(rem(space::SM - 2.0))
        .h(rem(density::CONTROL_MD + space::XXS))
        .px(rem(space::XS + 2.0))
        .rounded(rem(radius::SM))
        .text_size(rem(density::CONTROL_TEXT))
        .cursor_pointer()
        .map(|row| {
            if current {
                row.bg(palette.raise.to_gpui())
                    .text_color(palette.text.to_gpui())
            } else {
                row.text_color(palette.muted.to_gpui()).hover(|style| {
                    style
                        .bg(palette.raise.to_gpui())
                        .text_color(palette.text.to_gpui())
                })
            }
        })
        .focus_visible(|style| {
            style
                .ring(px(2.0))
                .ring_color(palette.accent_text.to_gpui())
        })
}

impl RenderOnce for Sidebar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let count = self.places.len();
        let focus = window.use_keyed_state(self.id.clone(), cx, |_, _| Vec::<FocusHandle>::new());
        let handles = focus.update(cx, |handles, cx| {
            handles.resize_with(count, || cx.focus_handle());
            handles.clone()
        });
        let mut top: Vec<AnyElement> = Vec::new();
        let mut foot: Vec<AnyElement> = Vec::new();
        for (index, place) in self.places.into_iter().enumerate() {
            let current = self.selected == Some(index);
            let click = self.on_select.clone();
            let keys = self.on_select.clone();
            let key_handles = handles.clone();
            let element = row(("sidebar-place", index).into(), current, cx)
                .track_focus(&crate::pressable::tab_stop(&handles[index]))
                .tab_index(0)
                .on_click(move |_, window, cx| click(index, window, cx))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    let next = match event.keystroke.key.as_str() {
                        "down" => (index + 1).min(count - 1),
                        "up" => index.saturating_sub(1),
                        _ => return,
                    };
                    cx.stop_propagation();
                    window.focus(&key_handles[next], cx);
                    keys(next, window, cx);
                })
                .child(Icon::new(place.icon).size(rem(16.0)).color(if current {
                    palette.text
                } else {
                    palette.muted
                }))
                .child(div().flex_1().min_w_0().truncate().child(place.label))
                .when_some(place.count, |row, count| {
                    row.child(
                        div()
                            .font_family(font::MONO)
                            .text_size(rem(11.0))
                            .text_color(palette.muted.to_gpui())
                            .child(count.to_string()),
                    )
                })
                .into_any_element();
            if place.foot {
                foot.push(element);
            } else {
                top.push(element);
            }
        }
        let entries: Vec<AnyElement> = self
            .entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                let press = self.on_open.clone().map(|open| {
                    Rc::new(move |window: &mut Window, cx: &mut App| open(index, window, cx))
                        as Rc<dyn Fn(&mut Window, &mut App)>
                });
                let focus =
                    crate::pressable::focus_handle(&("sidebar-entry", index).into(), window, cx);
                row(("sidebar-entry", index).into(), false, cx)
                    .track_focus(&focus)
                    .tab_index(0)
                    .when_some(press, |row, press| {
                        row.on_click(move |_, window, cx| press(window, cx))
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(rem(20.0))
                            .rounded(rem(5.0))
                            .overflow_hidden()
                            .bg(palette.bg.to_gpui())
                            .children(
                                entry.image.map(|image| {
                                    img(image).size_full().object_fit(ObjectFit::Cover)
                                }),
                            ),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(entry.label))
                    .when_some(entry.detail, |row, detail| {
                        row.child(
                            div()
                                .flex_none()
                                .text_size(rem(text::XS - 1.0))
                                .text_color(palette.muted.to_gpui())
                                .child(detail),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        div()
            .id(self.id)
            .flex_none()
            .w(rem(WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(rem(space::XS))
            .pb(rem(space::XS))
            // Clear of the window buttons and the toggle beside them.
            .pt(rem(title_bar::HEIGHT + space::XXS))
            .bg(palette.surface.to_gpui())
            .border_r_1()
            .border_color(palette.line.to_gpui())
            .children(top)
            .when(!entries.is_empty(), |sidebar| {
                sidebar
                    .child(
                        div()
                            .px(rem(space::XS + 2.0))
                            .pt(rem(space::MD))
                            .pb(rem(space::XXS))
                            .text_size(rem(text::XS - 1.0))
                            .text_color(palette.muted.to_gpui())
                            .child(self.title),
                    )
                    .children(entries)
            })
            .child(div().flex_1())
            .children(foot)
    }
}
