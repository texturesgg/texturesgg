//! Putting a model on the stage and playing it: which slot shows, which move,
//! paused or playing, where the playhead is, and the camera.

use super::Shown;
use crate::state::App;
use hsd_render::{CameraView, Orbit};
use melee_dat::MeleeSlot;
use rune::{Drag, Phase, Tx};

/// How far a drag across the stage turns the camera, in radians per point.
const ORBIT_PER_POINT: f64 = 0.01;
/// How far the camera may look up or down, in radians.
const PITCH_LIMIT: f64 = 1.4;

/// Show what `slot` of the player's game holds. A costume of the fighter
/// already showing keeps the camera and the move; anything else starts
/// from the front, idling. Returns why not, in the player's words.
pub fn show(app: &mut App, slot: MeleeSlot) -> Result<(), String> {
    let Some(references) = &app.game else { return Err("There's no game open.".into()) };
    let bytes = references.game().read_slot(slot).map_err(|error| error.to_string())?;
    let loaded = tgg_app::load_model(&slot.file_name(), &bytes, Some(references)).map_err(|error| error.to_string())?;
    let mut model = loaded.model;
    let moves = model.fighter().map(tgg_app::moves::moves).unwrap_or_default();
    let previous = app.stage.shown.take().filter(|shown| shown.slot.character().is_some() && shown.slot.character() == slot.character());
    let (orbit, animation, playing) = match &previous {
        Some(previous) => (previous.orbit, previous.animation, previous.playing),
        None => (CameraView::Front.orbit(), None, true),
    };
    app.stage.shown_count += 1;
    let generation = app.stage.shown_count;
    // A move that won't play on this costume falls back to the idle.
    let animation = animation.filter(|&animation| model.fighter_mut().is_some_and(|playback| playback.play(animation).is_ok()));
    app.stage.shown = Some(Shown { slot, model, orbit, playing, animation, moves, generation, posed: true });
    Ok(())
}

/// Take the model off the stage.
pub fn clear(app: &mut App) {
    app.stage.shown = None;
}

/// Play `animation` from its start.
pub fn play(app: &mut App, animation: usize) {
    let Some(shown) = &mut app.stage.shown else { return };
    let Some(playback) = shown.model.fighter_mut() else { return };
    if playback.play(animation).is_ok() {
        shown.animation = Some(animation);
        shown.playing = true;
        shown.posed = true;
    }
}

/// Pause, or play on from where it paused.
pub fn toggle_playback(app: &mut App) {
    if let Some(shown) = &mut app.stage.shown {
        shown.playing = !shown.playing;
    }
}

/// Show the frame `fraction` of the way through the move, and pause there.
pub fn seek(app: &mut App, fraction: f32) {
    let Some(shown) = &mut app.stage.shown else { return };
    let end = shown.model.end_frame();
    if shown.model.seek((fraction.clamp(0.0, 1.0) * end).floor()).is_ok() {
        shown.playing = false;
        shown.posed = true;
    }
}

/// A drag along the timeline's track: seek to where it is.
pub fn scrub(app: &mut App, drag: &mut Drag, _: &mut Tx) {
    if drag.w > 0.0 {
        seek(app, drag.x / drag.w);
    }
}

/// A drag across the stage turns the camera around the model.
pub fn orbit(app: &mut App, drag: &mut Drag, _: &mut Tx) {
    let Some(shown) = &mut app.stage.shown else { return };
    if drag.phase == Phase::Move {
        shown.orbit = Orbit {
            yaw: shown.orbit.yaw - f64::from(drag.dx) * ORBIT_PER_POINT,
            pitch: (shown.orbit.pitch + f64::from(drag.dy) * ORBIT_PER_POINT).clamp(-PITCH_LIMIT, PITCH_LIMIT),
            ..shown.orbit
        };
    }
}
