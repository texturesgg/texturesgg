//! Opening the player's game onto the roster, and keeping it current.

use super::{Melee, Pane, Subject, Tab};
use crate::stage::actions as stage;
use crate::state::App;
use melee_dat::{Character, MeleeSlot, Stage};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tgg_app::References;
use tgg_app::install::SlotState;

/// Make `references`' game the player's: its roster shows, and what its
/// slots hold and their pictures are read in the background.
pub fn open_game(app: &mut App, references: References) {
    stage::clear(app);
    app.melee = Melee::of(references.game());
    app.renders.clear();
    app.game = Some(references);
    refresh(app);
    // The first look for tgg-melee's disc ran before there was a game (a
    // first run picks it on Welcome); this one may be it.
    if matches!(
        app.mods.disc,
        crate::mods::Disc::Missing | crate::mods::Disc::Looking
    ) {
        crate::mods::actions::find_disc(app);
    }
}

/// Read what every slot holds, and ask for the roster's pictures.
pub fn refresh(app: &mut App) {
    let Some(references) = &app.game else { return };
    let iso = references.game().path().to_owned();
    app.jobs
        .read_states(&iso, &app.data.library, &app.data.history);
    let fighters = app.melee.fighters.iter().filter_map(Melee::shown_slot);
    let stages = app
        .melee
        .stages
        .iter()
        .map(|stage| MeleeSlot::Stage(*stage));
    for slot in fighters.chain(stages) {
        app.renders.request(&iso, slot);
    }
}

/// What each slot of the game at `iso` holds, and which can be undone, have
/// been read; they count if that's still the player's game.
pub fn states_read(
    app: &mut App,
    iso: &Path,
    states: HashMap<MeleeSlot, SlotState>,
    undoable: HashSet<MeleeSlot>,
) -> bool {
    let current = app
        .game
        .as_ref()
        .is_some_and(|references| references.game().path() == iso);
    if current {
        app.melee.states = Some(states);
        app.melee.undoable = undoable;
    }
    current
}

pub fn show_tab(app: &mut App, tab: Tab) {
    app.melee.tab = tab;
    opened(app);
}

/// Skins shows: the player's skins get their pictures while they're on show.
pub fn opened(app: &mut App) {
    if app.melee.tab == Tab::Mine {
        crate::library::actions::request_renders(app);
        crate::library::port::ask_all(app);
    }
}

/// Open `character` onto the stage, on the costume last chosen for it.
pub fn open_fighter(app: &mut App, character: Character) {
    let Some(slot) = app
        .melee
        .fighters
        .iter()
        .find(|fighter| fighter.character == character)
        .and_then(|fighter| app.melee.opening_slot(fighter))
    else {
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
    let Some(character) = slot.character() else {
        return;
    };
    if stage::show(app, slot).is_ok() {
        app.melee.chosen.insert(character, slot);
    }
    // Whether each of the player's skins for it would play online.
    let skins = app
        .library
        .skins()
        .iter()
        .filter(|skin| skin.slot.is_some_and(|made_for| made_for.fits(slot)))
        .map(|skin| (skin.id, slot))
        .collect();
    crate::library::port::ask(app, skins);
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

/// Skins, on the player's own skins.
pub fn show_mine(app: &mut App) {
    crate::nav::actions::go(app, crate::nav::Place::Skins);
    show_tab(app, Tab::Mine);
}
