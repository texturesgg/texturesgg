//! Opening the player's game onto the roster, and keeping it current.

use super::{Melee, Pane, Subject, Tab};
use crate::stage::actions as stage;
use crate::state::App;
use melee_dat::{Character, MeleeSlot, Stage};
use std::collections::HashMap;
use std::path::Path;
use tgg_app::References;
use tgg_app::install::SlotState;
use tgg_app::library::Library;

/// Make `references`' game the player's: its roster shows, and what its
/// slots hold and their pictures are read in the background.
pub fn open_game(app: &mut App, references: References) {
    stage::clear(app);
    app.melee = Melee::of(references.game());
    app.renders.clear();
    app.game = Some(references);
    refresh(app);
}

/// Read what every slot holds, and ask for the roster's pictures.
pub fn refresh(app: &mut App) {
    let Some(references) = &app.game else { return };
    let iso = references.game().path().to_owned();
    app.jobs.read_states(&iso, &Library::default_root());
    let fighters = app.melee.fighters.iter().filter_map(Melee::shown_slot);
    let stages = app.melee.stages.iter().map(|stage| MeleeSlot::Stage(*stage));
    for slot in fighters.chain(stages) {
        app.renders.request(&iso, slot);
    }
}

/// What each slot of the game at `iso` holds has been read; it counts if
/// that's still the player's game.
pub fn states_read(app: &mut App, iso: &Path, states: HashMap<MeleeSlot, SlotState>) -> bool {
    let current = app.game.as_ref().is_some_and(|references| references.game().path() == iso);
    if current {
        app.melee.states = Some(states);
    }
    current
}

pub fn show_tab(app: &mut App, tab: Tab) {
    app.melee.tab = tab;
}

/// Open `character` onto the stage, on the costume last chosen for it.
pub fn open_fighter(app: &mut App, character: Character) {
    let Some(slot) = app.melee.fighters.iter().find(|fighter| fighter.character == character).and_then(|fighter| app.melee.opening_slot(fighter)) else {
        return;
    };
    open(app, Subject::Fighter(character), slot);
}

/// Open `stage` onto the stage.
pub fn open_stage(app: &mut App, stage: Stage) {
    open(app, Subject::Stage(stage), MeleeSlot::Stage(stage));
}

fn open(app: &mut App, subject: Subject, slot: MeleeSlot) {
    match stage::show(app, slot) {
        Ok(()) => {
            app.melee.subject = Some(subject);
            app.melee.problem = None;
        }
        Err(problem) => {
            tgg_app::log(&format!("couldn't open {}: {problem}", slot.file_name()));
            app.melee.problem = Some(format!("{} couldn't open: {problem}", subject.name()));
        }
    }
}

/// Show the fighter opened in its costume `slot`, keeping the camera and move.
pub fn choose_costume(app: &mut App, slot: MeleeSlot) {
    let Some(character) = slot.character() else { return };
    if stage::show(app, slot).is_ok() {
        app.melee.chosen.insert(character, slot);
    }
}

/// Back to the roster: the stage closes into the plate it opened from.
/// Whether there was a subject to leave.
pub fn back(app: &mut App) -> bool {
    if app.melee.subject.take().is_none() {
        return false;
    }
    stage::clear(app);
    true
}

/// Fold a pane to its header, or open it again.
pub fn toggle_pane(app: &mut App, pane: Pane) {
    let panes = &mut app.melee.panes;
    match pane {
        Pane::Costumes => panes.costumes = !panes.costumes,
        Pane::Shared => panes.shared = !panes.shared,
        Pane::Moves => panes.moves = !panes.moves,
    }
}
