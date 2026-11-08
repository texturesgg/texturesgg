//! The Colors pane's actions: choose a swatch or a group's hue to change,
//! move its sliders, and apply. Applying writes the color everywhere the old
//! one was, as one undo step, and shows the model afresh from the document:
//! the renderer bakes vertex colors into its geometry, so it can't patch them
//! in place. The colors themselves are tgg-app's (`tgg_app::colors`).

use super::Document;
use crate::stage::actions as stage;
use crate::state::App;
use rune::Tx;
use tgg_app::colors::{self, Change};

/// The change being made, before it's applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorPick {
    pub group: usize,
    pub change: Change,
}

/// List the document's colors again, after it opened or its colors changed.
pub fn read(document: &mut Document) {
    document.colors = match &document.listing.document {
        Ok(open) => colors::groups(open, &document.listing.names),
        Err(_) => Vec::new(),
    };
    document.color_pick = None;
}

/// Start changing swatch `index` of `group`, from its color; its sliders
/// scroll into view.
pub fn pick_swatch(app: &mut App, group: usize, index: usize, tx: &mut Tx) {
    let Some(document) = &mut app.editor else { return };
    let Some(swatch) = document.colors.get(group).and_then(|group| group.swatches.get(index)) else { return };
    document.color_pick = Some(ColorPick { group, change: Change::Swatch { index, rgba: swatch.rgba } });
    tx.scroll_into_view("editor/pick");
}

/// Start turning every vertex color of `group` around the hue circle; its
/// slider scrolls into view.
pub fn pick_hue(app: &mut App, group: usize, tx: &mut Tx) {
    let Some(document) = &mut app.editor else { return };
    document.color_pick = Some(ColorPick { group, change: Change::Hue(0.0) });
    tx.scroll_into_view("editor/pick");
}

/// Set channel `channel` (red, green, blue, alpha) of the swatch being
/// changed to `value`, 0 to 255.
pub fn set_channel(app: &mut App, channel: usize, value: f32) {
    if let Some(ColorPick { change: Change::Swatch { rgba, .. }, .. }) = app.editor.as_mut().and_then(|document| document.color_pick.as_mut()) {
        rgba[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
}

/// Set how far the group being changed turns, in degrees.
pub fn set_hue(app: &mut App, degrees: f32) {
    if let Some(ColorPick { change: Change::Hue(hue), .. }) = app.editor.as_mut().and_then(|document| document.color_pick.as_mut()) {
        *hue = degrees.round().clamp(-180.0, 180.0);
    }
}

/// Put the change away unapplied.
pub fn cancel(app: &mut App) {
    if let Some(document) = &mut app.editor {
        document.color_pick = None;
    }
}

/// Write the change, show the model afresh, and say what changed.
pub fn apply(app: &mut App) {
    let Some(document) = &mut app.editor else { return };
    let Some(pick) = document.color_pick.take() else { return };
    let Some(group) = document.colors.get(pick.group).cloned() else { return };
    let Ok(open) = &mut document.listing.document else { return };
    match colors::apply(open, &group, pick.change) {
        Ok(what) => {
            let bytes = open.bytes().to_vec();
            read(document);
            if let Err(problem) = stage::replace(app, &bytes) {
                return app.tell(problem, true);
            }
            app.tell(format!("Recolored {}: {what}", group.title), false);
        }
        Err(error) => app.tell(format!("Couldn't recolor: {error}"), true),
    }
}
