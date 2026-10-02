//! The pill floating over the stage while a fighter plays a move: play and
//! pause, the move's name, a scrubber over its frames, and the playback
//! speed. Its container places it.

use crate::editor::{TogglePlayback, dispatch, keys};
use crate::viewport::Viewport;
use gpui::{Context, Entity, FontWeight, IntoElement, ParentElement, Render, Styled, Window, div};
use tgg_ui::tokens::{density, font, radius, space, text};
use tgg_ui::{
    ButtonSize, ButtonVariant, MenuButton, MenuItem, Slider, Theme, Tooltip, rem, shortcut,
};

/// The pill's width, in web pixels; it narrows to fit a small stage.
const PILL_WIDTH: f32 = 460.0;

/// The speeds offered, as multiples of the game's 60 Hz.
const RATES: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

/// It redraws with each viewport frame, as the status line does, so the frame
/// counter follows playback without redrawing the window.
pub(crate) struct Timeline {
    viewport: Entity<Viewport>,
}

impl Timeline {
    pub fn new(viewport: Entity<Viewport>, cx: &mut Context<Self>) -> Self {
        cx.observe(&viewport, |_, _, cx| cx.notify()).detach();
        Self { viewport }
    }
}

impl Render for Timeline {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let viewport = self.viewport.read(cx);
        let Some(clock) = viewport.clock() else {
            return div();
        };
        let bar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(rem(space::SM))
            .w(rem(PILL_WIDTH))
            .max_w_full()
            .pl(rem(space::XXS))
            .pr(rem(space::XS))
            .rounded(rem(radius::PILL))
            .bg(palette.surface.to_gpui())
            .border_1()
            .border_color(palette.line_strong.to_gpui())
            .shadow(vec![palette.popover_shadow.to_gpui()]);
        let paused = viewport.is_paused();
        let name = clock.label;
        let end = clock.end_frame;
        let frame = clock.frame.floor().min(end);
        let rate = clock.rate;

        let seek = self.viewport.clone();
        let speeds = RATES.iter().fold(
            MenuButton::new("speed", rate_label(rate))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm),
            |menu, &option| {
                let viewport = self.viewport.clone();
                menu.item(
                    MenuItem::new(rate_label(option), move |_, cx| {
                        viewport.update(cx, |viewport, cx| {
                            if let Err(error) = viewport.set_rate(option, cx) {
                                crate::log(&format!("speed unchanged: {error}"));
                            }
                        })
                    })
                    .checked(rate == option),
                )
            },
        );
        bar.h(rem(density::CONTROL_MD + space::XS))
            .child(
                tgg_ui::Button::new("playback", if paused { "Play" } else { "Pause" })
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .tooltip(
                        Tooltip::new(if paused { "Play" } else { "Pause" })
                            .shortcut(shortcut(keys::PLAYBACK)),
                    )
                    .on_press(dispatch(TogglePlayback)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(rem(160.0))
                    .truncate()
                    .text_size(rem(text::XS))
                    .font_weight(FontWeight::NORMAL)
                    .child(name),
            )
            .child(Slider::new(
                "frames",
                frame,
                end,
                1.0,
                move |frame, _, cx| {
                    seek.update(cx, |viewport, cx| {
                        if let Err(error) = viewport.seek(frame, cx) {
                            crate::log(&format!("seek failed: {error}"));
                        }
                    })
                },
            ))
            .child(
                div()
                    .flex_none()
                    .font_family(font::MONO)
                    .text_size(rem(text::XS))
                    .text_color(palette.muted.to_gpui())
                    .child(format!("{frame:>3.0} / {end:.0}")),
            )
            .child(speeds)
    }
}

/// "0.25×", "1×".
fn rate_label(rate: f32) -> String {
    format!("{rate}×")
}
