//! The 3D stage: the viewport on the renderer's own background, with the
//! playback pill floating at its foot. Tool panes float over its right side;
//! the viewport keeps clear of them, so the model stays centered in what's
//! left and never sits under a pane.

use crate::timeline::Timeline;
use crate::viewport::Viewport;
use gpui::{Div, Entity, ParentElement, Rems, Styled, div};
use tgg_ui::rem;
use tgg_ui::tokens::space;

/// The renderer's clear color, so the stage reads as one surface with the
/// viewport on it.
pub(crate) fn color() -> gpui::Rgba {
    let [red, green, blue] = hsd_render::renderer::BACKGROUND_RGB.map(u32::from);
    tgg_ui::Color::rgb(red << 16 | green << 8 | blue).to_gpui()
}

/// The stage, keeping `clear` of its right edge for panes over it.
pub(crate) fn stage(viewport: &Entity<Viewport>, timeline: &Entity<Timeline>, clear: Rems) -> Div {
    div().relative().size_full().bg(color()).child(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right(clear)
            .child(viewport.clone())
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(rem(space::MD))
                    .px(rem(space::MD))
                    .flex()
                    .justify_center()
                    .child(timeline.clone()),
            ),
    )
}
