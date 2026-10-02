//! The head of a page: its title, a chip beside it, a line under it, and its
//! actions at the right. The window's top row stays empty above it, so a
//! page says its name once.

use crate::tokens::{space, text};
use crate::{Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div,
};

/// How far a page's content sits in from its sides, in web pixels.
pub const INSET: f32 = space::LG + space::XXS;

#[derive(IntoElement)]
pub struct PageHeader {
    title: SharedString,
    beside: Option<AnyElement>,
    line: Option<SharedString>,
    actions: Vec<AnyElement>,
}

impl PageHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            beside: None,
            line: None,
            actions: Vec::new(),
        }
    }

    /// Beside the title, such as a chip.
    pub fn beside(mut self, element: impl IntoElement) -> Self {
        self.beside = Some(element.into_any_element());
        self
    }

    /// Under the title: a summary of the page.
    pub fn line(mut self, line: impl Into<SharedString>) -> Self {
        self.line = Some(line.into());
        self
    }

    /// An action at the right, in order.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl RenderOnce for PageHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(rem(space::XXS))
            .px(rem(INSET))
            .pt(rem(space::XS))
            .pb(rem(space::MD))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(space::SM))
                    .child(
                        div()
                            .text_size(rem(text::XL + 2.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title),
                    )
                    .children(self.beside)
                    .child(div().flex_1())
                    .children(self.actions),
            )
            .when_some(self.line, |header, line| {
                header.child(
                    div()
                        .text_size(rem(text::XS))
                        .text_color(palette.muted.to_gpui())
                        .child(line),
                )
            })
    }
}
