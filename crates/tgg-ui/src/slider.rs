//! A horizontal slider over a range from 0, such as an animation's frames.
//!
//! Press or drag anywhere on the track to set the value; Left and Right step
//! it while the slider has focus, Home and End jump to the ends. The value
//! belongs to the caller, which is asked to change it.

use crate::tokens::{density, space};
use crate::{Theme, rem};
use gpui::{
    App, AppContext, Bounds, Context, ElementId, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Pixels, Point, Render, RenderOnce,
    StatefulInteractiveElement, Styled, Window, canvas, div, px, relative,
};
use std::rc::Rc;

/// Track height and thumb size, in web pixels.
const TRACK: f32 = 4.0;
const THUMB: f32 = space::SM;

type ChangeHandler = Rc<dyn Fn(f32, &mut Window, &mut App)>;

struct SliderState {
    focus: FocusHandle,
    /// The track as last painted, to turn a press into a value.
    track: Bounds<Pixels>,
}

/// The drag payload of a slider, naming which one is dragged.
struct SliderDrag(ElementId);

/// Dragging shows no preview; the thumb itself follows the pointer.
struct NoPreview;

impl Render for NoPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    value: f32,
    max: f32,
    step: f32,
    on_change: ChangeHandler,
}

impl Slider {
    /// A slider from 0 to `max` showing `value`, moving in whole `step`s.
    pub fn new(
        id: impl Into<ElementId>,
        value: f32,
        max: f32,
        step: f32,
        on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            value,
            max,
            step,
            on_change: Rc::new(on_change),
        }
    }
}

/// The value at `x` along `track`, snapped to `step` and kept in range.
fn value_at(x: Pixels, track: Bounds<Pixels>, max: f32, step: f32) -> f32 {
    let width = f32::from(track.size.width);
    if width <= 0.0 || max <= 0.0 {
        return 0.0;
    }
    let fraction = (f32::from(x - track.left()) / width).clamp(0.0, 1.0);
    snap(fraction * max, max, step)
}

fn snap(value: f32, max: f32, step: f32) -> f32 {
    let snapped = if step > 0.0 {
        (value / step).round() * step
    } else {
        value
    };
    snapped.clamp(0.0, max.max(0.0))
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| SliderState {
            focus: cx.focus_handle(),
            track: Bounds::default(),
        });
        let focus = state.read(cx).focus.clone();
        let (value, max, step) = (self.value, self.max, self.step);
        let fraction = if max > 0.0 {
            (value / max).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let press_state = state.clone();
        let press = self.on_change.clone();
        let drag = self.on_change.clone();
        let keys = self.on_change.clone();
        let drag_id = self.id.clone();
        let track_state = state.clone();
        div()
            .id(self.id.clone())
            .track_focus(&crate::pressable::tab_stop(&focus))
            .tab_index(0)
            .h(rem(density::CONTROL_SM))
            .flex_1()
            .min_w(rem(space::XXL))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let (focus, track) = {
                    let state = press_state.read(cx);
                    (state.focus.clone(), state.track)
                };
                window.focus(&focus, cx);
                press(value_at(event.position.x, track, max, step), window, cx);
            })
            .on_drag(SliderDrag(self.id), |_, _: Point<Pixels>, _, cx| {
                cx.new(|_| NoPreview)
            })
            .on_drag_move::<SliderDrag>(move |event, window, cx| {
                if event.drag(cx).0 != drag_id {
                    return;
                }
                let track = track_state.read(cx).track;
                drag(
                    value_at(event.event.position.x, track, max, step),
                    window,
                    cx,
                );
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let next = match event.keystroke.key.as_str() {
                    "right" => value + step,
                    "left" => value - step,
                    "home" => 0.0,
                    "end" => max,
                    _ => return,
                };
                cx.stop_propagation();
                keys(snap(next, max, step), window, cx);
            })
            .focus_visible(|style| {
                style
                    .rounded(rem(space::XXS))
                    .ring(px(2.0))
                    .ring_color(palette.accent_text.to_gpui())
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(rem(TRACK))
                    .rounded_full()
                    .bg(palette.line.to_gpui())
                    .child({
                        let state = state.clone();
                        canvas(
                            move |bounds, _, cx| {
                                state.update(cx, |state, _| state.track = bounds);
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .inset_0()
                    })
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(relative(fraction))
                            .rounded_full()
                            .bg(palette.accent.to_gpui()),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(relative(fraction))
                            .top(rem((TRACK - THUMB) / 2.0))
                            .ml(rem(-THUMB / 2.0))
                            .size(rem(THUMB))
                            .rounded_full()
                            .border_1()
                            .border_color(palette.line_strong.to_gpui())
                            .bg(palette.raise.to_gpui()),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{snap, value_at};
    use gpui::{Bounds, point, px, size};

    #[test]
    fn a_press_maps_across_the_track_in_whole_steps() {
        let track = Bounds::new(point(px(100.0), px(0.0)), size(px(200.0), px(4.0)));
        assert_eq!(value_at(px(100.0), track, 60.0, 1.0), 0.0);
        assert_eq!(value_at(px(200.0), track, 60.0, 1.0), 30.0);
        assert_eq!(value_at(px(203.0), track, 60.0, 1.0), 31.0);
        assert_eq!(value_at(px(50.0), track, 60.0, 1.0), 0.0);
        assert_eq!(value_at(px(400.0), track, 60.0, 1.0), 60.0);
        assert_eq!(value_at(px(150.0), Bounds::default(), 60.0, 1.0), 0.0);
    }

    #[test]
    fn keys_stay_in_range() {
        assert_eq!(snap(-1.0, 60.0, 1.0), 0.0);
        assert_eq!(snap(61.0, 60.0, 1.0), 60.0);
        assert_eq!(snap(12.4, 60.0, 1.0), 12.0);
    }
}
