//! The stage's pieces: the plate the model shows on, and the timeline under
//! it. Whoever shows the stage lays them out.

use super::{FRAME_COUNT_SPACE, VIEWPORT, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{pause_glyph, plate as plate_frame, play_glyph};
use rune::{El, effect, el, row, text as label};

/// The timeline's height, its play button's side, its track's height (the
/// room to grab it in), and the playhead's, in points.
const TIMELINE: f32 = 44.0;
const PLAY: f32 = 26.0;
const TRACK: f32 = 20.0;
const PLAYHEAD: f32 = 16.0;

/// The plate with the model drawn on it. Dragging across it turns the
/// camera (with Shift, moves it), and the wheel moves it in and out. `matched` names it to the roster's plate it opens from.
pub fn plate(app: &App, matched: &str, p: &Palette) -> El {
    let caption = app
        .stage
        .shown
        .as_ref()
        .map(|shown| shown.slot.file_name())
        .unwrap_or_default();
    let dragging = app
        .stage
        .shown
        .as_ref()
        .is_some_and(|shown| shown.gesture.is_some());
    plate_frame(p)
        .key("stage/plate")
        .matched(matched)
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(
            effect(VIEWPORT)
                .key("stage/viewport")
                .w_full()
                .h_full()
                .cursor(if dragging {
                    rune::Cursor::Grabbing
                } else {
                    rune::Cursor::Grab
                })
                .on_drag(actions::orbit)
                .on_wheel(actions::zoom),
        )
        .child(
            label(caption)
                .font(text::MONO_SMALL)
                .color(p.faint)
                .absolute(space::SM, space::XS + 2.0),
        )
}

/// Play and pause, the move playing, the track to scrub, and the frame.
pub fn timeline(app: &App, p: &Palette) -> El {
    let Some(shown) = &app.stage.shown else {
        return el();
    };
    if !shown.model.is_animated() {
        return el();
    }
    let toggle = el()
        .key("stage/play")
        .size(PLAY, PLAY)
        .radius(radius::CONTROL)
        .fill(p.ink)
        .center()
        .role("button")
        .label(if shown.playing { "Pause" } else { "Play" })
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .child(if shown.playing {
            pause_glyph(p.on_ink)
        } else {
            play_glyph(p.on_ink)
        })
        .on_press(actions::toggle_playback);
    let move_label = shown.move_label().unwrap_or_default();
    // The track's line and the playhead sit centered in its height.
    let track = el()
        .key("stage/track")
        .grow(1.0)
        .h(TRACK)
        .cursor(rune::Cursor::ResizeColumn)
        .on_drag(actions::scrub)
        .child(
            el().w_full()
                .h(size::LINE_STRONG)
                .fill(p.line_strong)
                .absolute(0.0, (TRACK - size::LINE_STRONG) / 2.0),
        )
        .child(
            el().size(size::LINE_STRONG, PLAYHEAD)
                .fill(p.accent)
                .absolute(0.0, (TRACK - PLAYHEAD) / 2.0)
                .live_offset(&app.stage.playhead),
        );
    row()
        .key("stage/timeline")
        .h(TIMELINE)
        .px(space::SM)
        .gap(space::SM)
        .items_center()
        .fill(p.surface)
        .border(size::LINE, p.line_strong)
        .radius(radius::CONTROL)
        .child(toggle)
        .child(label(move_label).font(text::STRONG).color(p.ink))
        .child(track)
        .child(
            label(FRAME_COUNT_SPACE)
                .font(text::MONO_SMALL)
                .color(p.faint)
                .live_text(&app.stage.frame_count),
        )
}
