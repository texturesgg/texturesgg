//! The stage's pieces: the plate the model shows on, and the timeline under
//! it. Whoever shows the stage lays them out.

use super::{FRAME_COUNT_SPACE, VIEWPORT, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, space, text};
use crate::ui::{pause_glyph, plate as plate_frame, play_glyph};
use rune::{El, effect, el, row, text as label};

/// The plate with the model drawn on it. Dragging across it turns the
/// camera. `matched` names it to the roster's plate it opens from.
pub fn plate(app: &App, matched: &str, p: &Palette) -> El {
    let caption = app.stage.shown.as_ref().map(|shown| shown.slot.file_name()).unwrap_or_default();
    plate_frame(p).key("stage/plate").matched(matched).grow(1.0).min_h(0.0).w_full()
        .child(effect(VIEWPORT).key("stage/viewport").w_full().h_full()
            .cursor(rune::Cursor::Grab).on_drag(actions::orbit))
        .child(label(caption).font(text::MONO_SMALL).color(p.faint).absolute(space::SM, space::XS + 2.0))
}

/// Play and pause, the move playing, the track to scrub, and the frame.
pub fn timeline(app: &App, p: &Palette) -> El {
    let Some(shown) = &app.stage.shown else { return el() };
    if !shown.model.is_animated() {
        return el();
    }
    let toggle = el().key("stage/play").size(26.0, 26.0).radius(radius::CONTROL).fill(p.ink).center()
        .role("button").label(if shown.playing { "Pause" } else { "Play" }).focusable(true)
        .child(if shown.playing { pause_glyph(p.on_ink) } else { play_glyph(p.on_ink) })
        .on_press(actions::toggle_playback);
    let move_label = shown.move_label().unwrap_or_default();
    let track = el().key("stage/track").grow(1.0).h(20.0).cursor(rune::Cursor::ResizeColumn)
        .on_drag(actions::scrub)
        .child(el().w_full().h(2.0).fill(p.line_strong).absolute(0.0, 9.0))
        .child(el().size(2.0, 16.0).fill(p.accent).absolute(0.0, 2.0).live_offset(&app.stage.playhead));
    row().key("stage/timeline").h(44.0).px(space::SM).gap(space::SM).items_center()
        .fill(p.surface).border(1.0, p.line_strong).radius(radius::CONTROL)
        .child(toggle)
        .child(label(move_label).font(text::STRONG).color(p.ink))
        .child(track)
        .child(label(FRAME_COUNT_SPACE).font(text::MONO_SMALL).color(p.faint).live_text(&app.stage.frame_count))
}

