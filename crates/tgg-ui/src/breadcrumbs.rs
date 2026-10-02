//! Where a page sits: its place, then each level down to it. Every level but
//! the last is pressable and goes back there; the last is the page.

use crate::pressable::pressable;
use crate::tokens::{density, radius, space};
use crate::{Theme, rem};
use gpui::{
    AnyElement, App, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window, div,
};
use std::rc::Rc;

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement, Default)]
pub struct Breadcrumbs {
    levels: Vec<(SharedString, Option<PressHandler>)>,
}

impl Breadcrumbs {
    pub fn new() -> Self {
        Self::default()
    }

    /// A level above the page, going back there on a press.
    pub fn level(
        mut self,
        label: impl Into<SharedString>,
        on_press: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.levels.push((label.into(), Some(Rc::new(on_press))));
        self
    }

    /// The page itself, last.
    pub fn page(mut self, label: impl Into<SharedString>) -> Self {
        self.levels.push((label.into(), None));
        self
    }
}

impl RenderOnce for Breadcrumbs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let mut parts: Vec<AnyElement> = Vec::new();
        for (index, (label, press)) in self.levels.into_iter().enumerate() {
            if index > 0 {
                parts.push(
                    div()
                        .text_color(palette.line_strong.to_gpui())
                        .child("/")
                        .into_any_element(),
                );
            }
            parts.push(match press {
                Some(press) => pressable(("crumb", index), Some(press), window, cx)
                    .px(rem(space::XXS))
                    .rounded(rem(radius::SM))
                    .text_color(palette.muted.to_gpui())
                    .hover(|style| {
                        style
                            .bg(palette.raise.to_gpui())
                            .text_color(palette.text.to_gpui())
                    })
                    .child(label)
                    .into_any_element(),
                None => div()
                    .px(rem(space::XXS))
                    .font_weight(FontWeight::MEDIUM)
                    .child(label)
                    .into_any_element(),
            });
        }
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(rem(2.0))
            .text_size(rem(density::CONTROL_TEXT + 0.5))
            .whitespace_nowrap()
            .children(parts)
    }
}
