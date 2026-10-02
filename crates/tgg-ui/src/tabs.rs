//! Tabs that switch between views of one thing, as the web's `Tabs`: muted
//! labels over a line, the selected one in the text color with an accent
//! underline, each with an optional count.
//!
//! Selection belongs to the caller. Left and Right move between tabs while
//! one has focus; Enter and Space on a focused tab select it.

use crate::tokens::{density, font, space};
use crate::{Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use std::rc::Rc;

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

struct Tab {
    label: SharedString,
    count: Option<usize>,
}

#[derive(IntoElement)]
pub struct Tabs {
    id: ElementId,
    tabs: Vec<Tab>,
    selected: usize,
    on_select: SelectHandler,
}

impl Tabs {
    pub fn new(
        id: impl Into<ElementId>,
        selected: usize,
        on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            tabs: Vec::new(),
            selected,
            on_select: Rc::new(on_select),
        }
    }

    /// Add a tab, with a count after its label when `count` is set.
    pub fn tab(mut self, label: impl Into<SharedString>, count: Option<usize>) -> Self {
        self.tabs.push(Tab {
            label: label.into(),
            count,
        });
        self
    }
}

impl RenderOnce for Tabs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let count = self.tabs.len();
        let focus = window.use_keyed_state(self.id.clone(), cx, |_, _| Vec::<FocusHandle>::new());
        let handles = focus.update(cx, |handles, cx| {
            handles.resize_with(count, || cx.focus_handle());
            handles.clone()
        });
        let selected = self.selected;
        div()
            .id(self.id)
            .flex_none()
            .flex()
            .gap(rem(space::MD))
            .px(rem(space::SM))
            .border_b_1()
            .border_color(palette.line.to_gpui())
            .children(self.tabs.into_iter().enumerate().map(|(index, tab)| {
                let is_selected = index == selected;
                let click = self.on_select.clone();
                let keys = self.on_select.clone();
                let key_handles = handles.clone();
                div()
                    .id(("tab", index))
                    .flex()
                    .items_center()
                    .gap(rem(space::XXS))
                    .py(rem(space::XS))
                    // The underline sits on the tabs' line, replacing it.
                    .mb(px(-1.0))
                    .border_b_2()
                    .border_color(if is_selected {
                        palette.accent.to_gpui()
                    } else {
                        crate::Color::CLEAR.to_gpui()
                    })
                    .text_size(rem(density::CONTROL_TEXT))
                    .font_weight(if is_selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if is_selected {
                        palette.text.to_gpui()
                    } else {
                        palette.muted.to_gpui()
                    })
                    .cursor_pointer()
                    .when(!is_selected, |tab| {
                        tab.hover(|style| style.text_color(palette.text.to_gpui()))
                    })
                    .track_focus(&crate::pressable::tab_stop(&handles[index]))
                    .tab_index(0)
                    .focus_visible(|style| {
                        style
                            .border_color(palette.accent_text.to_gpui())
                            .text_color(palette.text.to_gpui())
                    })
                    .on_click(move |_, window, cx| click(index, window, cx))
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        let next = match event.keystroke.key.as_str() {
                            "right" => (index + 1) % count,
                            "left" => (index + count - 1) % count,
                            _ => return,
                        };
                        cx.stop_propagation();
                        window.focus(&key_handles[next], cx);
                        keys(next, window, cx);
                    })
                    .child(tab.label)
                    .when_some(tab.count, |tab, count| {
                        tab.child(
                            div()
                                .font_family(font::MONO)
                                .font_weight(FontWeight::NORMAL)
                                .text_color(palette.muted.to_gpui())
                                .child(count.to_string()),
                        )
                    })
            }))
    }
}
