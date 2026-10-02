//! A small pill naming a fact about a page, such as which game it's about,
//! with an optional colored dot. Pressable when there's more to it.

use crate::pressable::pressable;
use crate::tokens::{density, radius, space};
use crate::{Color, Theme, Tooltip, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window, div,
};
use std::rc::Rc;

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Chip {
    id: ElementId,
    label: SharedString,
    dot: Option<Color>,
    tooltip: Option<SharedString>,
    on_press: Option<PressHandler>,
}

impl Chip {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            dot: None,
            tooltip: None,
            on_press: None,
        }
    }

    pub fn dot(mut self, color: Color) -> Self {
        self.dot = Some(color);
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn on_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Chip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let pressable_chip = self.on_press.is_some();
        pressable(self.id, self.on_press, window, cx)
            .flex_none()
            .flex()
            .items_center()
            .gap(rem(6.0))
            .h(rem(density::CONTROL_SM))
            .px(rem(space::XS + 2.0))
            .rounded(rem(radius::PILL))
            .bg(palette.line.to_gpui())
            .text_size(rem(density::DETAIL_TEXT))
            .text_color(palette.muted.to_gpui())
            .when(pressable_chip, |chip| {
                chip.hover(|style| {
                    style
                        .bg(palette.line_strong.to_gpui())
                        .text_color(palette.text.to_gpui())
                })
            })
            .when_some(self.tooltip, |chip, tooltip| {
                chip.tooltip(move |_, cx| Tooltip::new(tooltip.clone()).view(cx))
            })
            .when_some(self.dot, |chip, dot| {
                chip.child(div().size(rem(6.0)).rounded_full().bg(dot.to_gpui()))
            })
            .child(self.label)
    }
}
