//! Putting a model on the stage and playing it: which slot shows, which move,
//! paused or playing, where the playhead is, and the camera.

use super::Shown;
use crate::state::App;
use hsd_render::{CameraView, Orbit};
use dat_parser::hsd::scene::{HsdTextureIndex, HsdTextureSourceId};
use melee_dat::MeleeSlot;
use rune::{Drag, Phase, Tx, WheelEvent};

/// How far a drag across the stage turns the camera, in radians per point.
/// tgg-editor's camera, as are the rates below; the renderer keeps it within
/// what the model allows (`viewport` writes that back).
const ORBIT_PER_POINT: f64 = 0.01;
/// How far the wheel moves the camera, per point scrolled.
const ZOOM_PER_POINT: f64 = 0.002;
/// A notched wheel's step, in the points tgg-editor's rate is per: gpui
/// counts a line as 20, where Rune hands it over as 40.
const LINE_POINTS: f32 = 20.0;
const RUNE_LINE_POINTS: f32 = 40.0;

/// Show what `slot` of the player's game holds. A costume of the fighter
/// already showing keeps the camera and the move; anything else starts
/// from the front, idling. Returns why not, in the player's words.
pub fn show(app: &mut App, slot: MeleeSlot) -> Result<(), String> {
    let Some(references) = &app.game else { return Err("There's no game open.".into()) };
    let bytes = references.game().read_slot(slot).map_err(|error| error.to_string())?;
    let loaded = tgg_app::load_model(&slot.file_name(), &bytes, Some(references)).map_err(|error| error.to_string())?;
    let (mut model, shared, store) = (loaded.model, loaded.shared, loaded.store);
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
    app.stage.shown = Some(Shown {
        slot,
        model,
        orbit,
        gesture: None,
        playing,
        animation,
        moves,
        shared,
        generation,
        posed: true,
        restart: true,
        store,
        edited: Default::default(),
        pending: Vec::new(),
        highlight: Vec::new(),
        highlight_changed: false,
    });
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
        shown.restart = true;
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
        shown.restart = true;
    }
}

/// A drag along the timeline's track: seek to where it is.
pub fn scrub(app: &mut App, drag: &mut Drag, _: &mut Tx) {
    if drag.w > 0.0 {
        seek(app, drag.x / drag.w);
    }
}

/// A drag across the stage turns the camera around the model, or with Shift
/// held moves it across. Each move turns it by that move alone: Rune reports
/// travel since the press, so the stage keeps the travel it last heard.
pub fn orbit(app: &mut App, drag: &mut Drag, tx: &mut Tx) {
    let Some(shown) = &mut app.stage.shown else { return };
    match drag.phase {
        Phase::Down => shown.gesture = Some([0.0, 0.0]),
        Phase::Move => {
            let Some([heard_x, heard_y]) = shown.gesture else { return };
            shown.gesture = Some([drag.dx, drag.dy]);
            let (dx, dy) = (f64::from(drag.dx - heard_x), f64::from(drag.dy - heard_y));
            let orbit = shown.orbit;
            shown.orbit = if tx.mods().shift {
                orbit.panned(dx, dy, drag.w.max(1.0) as u32, drag.h.max(1.0) as u32)
            } else {
                Orbit { yaw: orbit.yaw - dx * ORBIT_PER_POINT, pitch: orbit.pitch + dy * ORBIT_PER_POINT, ..orbit }
            };
        }
        Phase::Up => shown.gesture = None,
    }
}

/// The wheel over the stage moves the camera in and out.
pub fn zoom(app: &mut App, wheel: WheelEvent, _: &mut Tx) -> bool {
    let Some(shown) = &mut app.stage.shown else { return false };
    let points = if wheel.precise { wheel.dy } else { wheel.dy / RUNE_LINE_POINTS * LINE_POINTS };
    shown.orbit.zoom *= (-f64::from(points) * ZOOM_PER_POINT).exp();
    true
}

/// Play the move for `action` (`SpecialLwLoop`), as a shared model's row
/// does to show the model: the shine through the down special.
pub fn play_action(app: &mut App, action: &str) {
    let animation = app.stage.shown.as_ref().and_then(|shown| shown.animation_for(action));
    match animation {
        Some(animation) => play(app, animation),
        None => tgg_app::log(&format!("no move plays {action}")),
    }
}

/// Show edited pixels: each decoded use of an edited texture, with its
/// (width, height), matched to its scene texture by image and palette
/// descriptors.
pub fn update_textures<'a>(app: &mut App, decoded: impl IntoIterator<Item = (HsdTextureSourceId, (u32, u32), &'a [u8])>) {
    let Some(shown) = &mut app.stage.shown else { return };
    for (id, size, pixels) in decoded {
        if let Some(index) = shown.model.scene().textures.iter().position(|texture| texture.id == id) {
            shown.edited.insert(index, (size, pixels.to_vec()));
            shown.pending.push(index);
        }
    }
}

/// Tint the surfaces that draw any of `uses`; none stops.
pub fn set_highlight(app: &mut App, uses: &[HsdTextureSourceId]) {
    let Some(shown) = &mut app.stage.shown else { return };
    let tinted: Vec<HsdTextureIndex> = shown
        .model
        .scene()
        .textures
        .iter()
        .enumerate()
        .filter(|(_, texture)| uses.contains(&texture.id))
        .map(|(index, _)| HsdTextureIndex(index))
        .collect();
    if tinted != shown.highlight {
        shown.highlight = tinted;
        shown.highlight_changed = true;
    }
}

/// Show `bytes` in place of the model showing, from the same camera and at
/// the same point of the same move: the file changed in a way the renderer
/// can't patch, such as its colors. Edits shown so far are in the bytes.
pub fn replace(app: &mut App, bytes: &[u8]) -> Result<(), String> {
    let Some(references) = &app.game else { return Err("There's no game open.".into()) };
    let Some(shown) = &app.stage.shown else { return Ok(()) };
    let loaded = tgg_app::load_model(&shown.slot.file_name(), bytes, Some(references)).map_err(|error| error.to_string())?;
    let mut model = loaded.model;
    let (animation, frame, playing) = (shown.animation, shown.model.frame(), shown.playing);
    let animation = animation.filter(|&animation| model.fighter_mut().is_some_and(|playback| playback.play(animation).is_ok()));
    let _ = model.seek(frame);
    app.stage.shown_count += 1;
    let generation = app.stage.shown_count;
    let Some(shown) = &mut app.stage.shown else { return Ok(()) };
    shown.model = model;
    shown.animation = animation;
    shown.playing = playing;
    shown.generation = generation;
    shown.edited.clear();
    shown.pending.clear();
    shown.posed = true;
    shown.restart = true;
    Ok(())
}
