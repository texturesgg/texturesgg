//! A raised label shown on hover, with an optional shortcut hint.

use crate::tokens::{density, font, radius, space};
use crate::{Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div,
};

#[derive(Clone, Debug)]
pub struct Tooltip {
    label: SharedString,
    shortcut: Option<SharedString>,
}

impl Tooltip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
        }
    }

    /// Show a key hint after the label, as it's written for the platform
    /// ("Ctrl+O").
    pub fn shortcut(mut self, keys: impl Into<SharedString>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }

    /// The view gpui's `.tooltip(...)` builder returns.
    pub fn view(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        div()
            .flex()
            .items_center()
            .gap(rem(space::XS))
            .px(rem(space::XS))
            .py(rem(space::XXS))
            .rounded(rem(radius::SM))
            .border_1()
            .border_color(palette.line_strong.to_gpui())
            .bg(palette.raise.to_gpui())
            .shadow(vec![palette.popover_shadow.to_gpui()])
            .text_color(palette.text.to_gpui())
            .text_size(rem(density::CONTROL_TEXT))
            .font_family(font::SANS)
            .child(self.label.clone())
            .when_some(self.shortcut.clone(), |tooltip, keys| {
                tooltip.child(
                    div()
                        .font_family(font::MONO)
                        .text_color(palette.muted.to_gpui())
                        .child(keys),
                )
            })
    }
}
