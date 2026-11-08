//! Editing a texture in the player's image editor: export it to a PNG, open
//! that there, and import it again each time it's saved. A save is an
//! ordinary import, with its fidelity notice and an undo step; undo and redo
//! rewrite the PNG so it keeps matching the document. The watching itself is
//! tgg-app's (`tgg_app::external`).

use super::{Document, files};
use crate::state::App;
use rune::Tx;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tgg_app::external::{self, Watched};

/// How often watched PNGs are checked for saves.
const CHECK: Duration = Duration::from_millis(500);

/// A document's textures being edited elsewhere.
#[derive(Debug, Default)]
pub struct External {
    /// By texture.
    pub watched: BTreeMap<usize, Watched>,
    /// When saves were last checked for.
    checked: Option<Instant>,
}

/// Open the selected texture in the player's image editor.
pub fn edit(app: &mut App) {
    let Some(document) = &app.editor else { return };
    let Some(texture) = document.selected else {
        return app.tell("Select a texture to edit in your image editor.", true);
    };
    let name = document.listing.name(texture);
    // Opened again, it keeps its file: the editor may still have it open.
    let path = document.external.watched.get(&texture).map_or_else(|| external::new_png_path(&format!("{name}.png")), |watched| watched.path.clone());
    let written = files::texture_png(document, texture).and_then(|png| {
        std::fs::create_dir_all(path.parent().expect("in a folder")).map_err(|error| error.to_string())?;
        std::fs::write(&path, png).map_err(|error| error.to_string())
    });
    if let Err(error) = written {
        return app.tell(format!("Couldn't export {name} for editing: {error}"), true);
    }
    if let Err(error) = external::open_in_editor(app.settings.external_editor.as_deref(), &path) {
        return app.tell(format!("Couldn't open your image editor: {error}"), true);
    }
    if let Some(document) = &mut app.editor {
        document.external.watched.insert(texture, Watched::new(path));
    }
    app.tell(format!("Editing {name} in your image editor; each save there updates it here."), false);
}

/// Import every watched PNG saved since it was last read, checking at most
/// every half second. While any is watched, it asks to be woken for the
/// next check. Returns whether anything changed.
pub fn check(app: &mut App, tx: &mut Tx) -> bool {
    let Some(document) = &mut app.editor else { return false };
    if document.external.watched.is_empty() {
        return false;
    }
    tx.wake_after(CHECK);
    let now = Instant::now();
    if document.external.checked.is_some_and(|checked| now.duration_since(checked) < CHECK) {
        return false;
    }
    document.external.checked = Some(now);
    let saved: Vec<(usize, std::time::SystemTime)> =
        document.external.watched.iter().filter_map(|(&texture, watched)| Some((texture, watched.saved()?))).collect();
    let mut changed = false;
    for (texture, at) in saved {
        let Some(document) = &mut app.editor else { break };
        let Some(watched) = document.external.watched.get_mut(&texture) else { continue };
        let name = watched.path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        // A save still being written doesn't decode: try again next check,
        // unless it stays that way.
        let png = std::fs::read(&watched.path).ok().filter(|png| image::load_from_memory(png).is_ok());
        let Some(png) = png else {
            if watched.failed_to_decode(at) {
                app.tell(format!("Couldn't read {name} as a PNG; save it again to try again."), true);
                changed = true;
            }
            continue;
        };
        watched.read(at);
        files::import_png(app, texture, &name, &png);
        changed = true;
    }
    changed
}

/// Rewrite a watched texture's PNG after undo or redo changed it, so it
/// matches the document again, without importing it back.
pub fn resync(app: &mut App, texture: usize) {
    let Some(document) = &app.editor else { return };
    let Some(watched) = document.external.watched.get(&texture) else { return };
    let path = watched.path.clone();
    let written = files::texture_png(document, texture).and_then(|png| std::fs::write(&path, png).map_err(|error| error.to_string()));
    match written {
        Ok(()) => {
            if let Some(document) = &mut app.editor {
                document.external.watched.insert(texture, Watched::new(path));
            }
        }
        Err(error) => app.tell(format!("Couldn't update the PNG in your image editor: {error}"), true),
    }
}

/// Stop watching and delete the PNGs, for a document going away.
pub fn forget(document: &mut Document) {
    for watched in std::mem::take(&mut document.external.watched).into_values() {
        external::remove_png(&watched.path);
    }
}
