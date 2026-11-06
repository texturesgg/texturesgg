//! Opening the player's game onto the roster, and keeping it current.

use super::{Melee, Tab};
use crate::state::App;
use melee_dat::MeleeSlot;
use std::collections::HashMap;
use std::path::Path;
use tgg_app::References;
use tgg_app::install::SlotState;
use tgg_app::library::Library;

/// Make `references`' game the player's: its roster shows, and what its
/// slots hold and their pictures are read in the background.
pub fn open_game(app: &mut App, references: References) {
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
