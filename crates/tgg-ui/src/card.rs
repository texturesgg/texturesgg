//! A card for a thing with a picture: a fighter, a skin, a costume slot. As a
//! tile it stands in a grid, its picture over its name; as a row it lies in
//! a list, its picture beside. Under the name, a detail on the left and a
//! status (or a control) on the right. Pressable when it opens something;
//! a corner action brightens on hover.

use crate::pressable::pressable;
use crate::tokens::{density, radius, space};
use crate::{Color, Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ObjectFit, ParentElement,
    RenderImage, RenderOnce, SharedString, Styled, StyledImage, Window, div, img,
};
use std::rc::Rc;
use std::sync::Arc;

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// How a card lays out its picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardLayout {
    /// Picture over the name, `CARD_WIDTH` wide, for grids.
    Tile,
    /// Picture beside the name, full width, for lists.
    Row,
}

/// A tile's width and picture height, and a row's picture size, in web
/// pixels.
pub const CARD_WIDTH: f32 = 148.0;
const TILE_IMAGE: f32 = 104.0;
const ROW_IMAGE: f32 = 64.0;

/// What a card's right side says: a label, after a colored dot.
pub struct CardStatus {
    pub label: SharedString,
    pub dot: Option<Color>,
    /// The label in the accent's text color, for what's changed.
    pub accent: bool,
}

#[derive(IntoElement)]
pub struct Card {
    id: ElementId,
    layout: CardLayout,
    /// `None`: no picture at all (a stage); `Some(None)`: one on its way.
    image: Option<Option<Arc<RenderImage>>>,
    title: SharedString,
    detail: Option<SharedString>,
    status: Option<CardStatus>,
    control: Option<AnyElement>,
    corner: Option<AnyElement>,
    selected: bool,
    on_press: Option<PressHandler>,
}

impl Card {
    pub fn new(
        id: impl Into<ElementId>,
        layout: CardLayout,
        title: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            layout,
            image: None,
            title: title.into(),
            detail: None,
            status: None,
            control: None,
            corner: None,
            selected: false,
            on_press: None,
        }
    }

    /// Show a picture, or its place while it's on its way.
    pub fn image(mut self, image: Option<Arc<RenderImage>>) -> Self {
        self.image = Some(image);
        self
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn status(mut self, status: CardStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// A control on the right in place of the status, such as Install.
    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

    /// An action in the picture's corner, quiet until hovered.
    pub fn corner(mut self, corner: impl IntoElement) -> Self {
        self.corner = Some(corner.into_any_element());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let group = SharedString::from(format!("card-{:?}", self.id));
        let tile = self.layout == CardLayout::Tile;
        let picture = self.image.map(|image| {
            div()
                .relative()
                .flex_none()
                .map(|picture| {
                    if tile {
                        picture.h(rem(TILE_IMAGE))
                    } else {
                        picture.size(rem(ROW_IMAGE))
                    }
                })
                .rounded(rem(radius::SM))
                .bg(palette.bg.to_gpui())
                .overflow_hidden()
                .children(image.map(|image| img(image).size_full().object_fit(ObjectFit::Contain)))
                .when_some(self.corner, |picture, corner| {
                    picture.child(
                        div()
                            .absolute()
                            .top(rem(space::XXS))
                            .right(rem(space::XXS))
                            .rounded(rem(radius::SM))
                            .bg(palette.surface.to_gpui())
                            // Quiet until hovered, but always there, so the
                            // keyboard can reach and see it.
                            .opacity(0.55)
                            .group_hover(group.clone(), |style| style.opacity(1.0))
                            .child(corner),
                    )
                })
        });
        let status = self.status.map(|status| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(rem(5.0))
                .text_size(rem(density::DETAIL_TEXT))
                .text_color(if status.accent {
                    palette.accent_text.to_gpui()
                } else {
                    palette.muted.to_gpui()
                })
                .when_some(status.dot, |status, dot| {
                    status.child(div().size(rem(6.0)).rounded_full().bg(dot.to_gpui()))
                })
                .child(status.label)
        });
        let words = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(rem(2.0))
            .px(rem(2.0))
            .child(
                div()
                    .truncate()
                    .text_size(rem(density::CONTROL_TEXT))
                    .child(self.title),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rem(space::XXS))
                    .min_h(rem(if self.control.is_some() {
                        density::CONTROL_SM
                    } else {
                        0.0
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(rem(density::DETAIL_TEXT))
                            .text_color(palette.muted.to_gpui())
                            .children(self.detail),
                    )
                    .children(self.control.or(status.map(IntoElement::into_any_element))),
            );
        let selected = self.selected;
        pressable(self.id, self.on_press, window, cx)
            .group(group)
            .flex()
            .map(|card| {
                if tile {
                    card.w(rem(CARD_WIDTH)).flex_col()
                } else {
                    card.w_full().items_center()
                }
            })
            .gap(rem(space::XS))
            .p(rem(space::XS))
            .rounded(rem(radius::MD))
            .bg(palette.surface.to_gpui())
            .border_1()
            .border_color(if selected {
                palette.accent.to_gpui()
            } else {
                palette.line.to_gpui()
            })
            .when(!selected, |card| {
                card.hover(|style| style.border_color(palette.line_strong.to_gpui()))
            })
            .children(picture)
            .child(words)
    }
}
