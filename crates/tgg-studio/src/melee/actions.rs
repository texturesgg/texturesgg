//! Opening the player's game onto the picker, showing a tab or a kind,
//! opening a skin's sheet, and turning a skin on in tgg-melee or writing it
//! into the player's Melee disc.

use super::{Kind, Melee, Roster, Sheet, Subject, Tab};
use crate::library::site::{self, Pack, Then};
use crate::stage::actions as stage;
use crate::state::App;
use melee_dat::{Character, MeleeSlot, Stage};
use rune::Tx;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tgg_app::References;

use tgg_app::ids::SkinId;
use tgg_app::install::SlotState;

/// Make `references`' game the player's: its roster shows, and what its
/// slots hold and their pictures are read in the background.
pub fn open_game(app: &mut App, references: References) {
    stage::clear(app);
    app.play.fighter = None;
    app.melee = Melee::of(references.game());
    app.renders.clear();
    app.game = Some(references);
    refresh(app);
    crate::play::actions::stand(app);
    // The first look for tgg-melee's disc ran before there was a game (a
    // first run picks it); this one may be it.
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

/// Skins shows: the skins' pictures, the ones the site lists, and whether
/// tgg-melee would play each of the player's own online.
pub fn opened(app: &mut App) {
    crate::library::actions::request_renders(app);
    crate::library::port::ask_all(app);
    if app.melee.tab == Tab::Browse && app.persist {
        site::load(app);
    }
}

pub fn show_tab(app: &mut App, tab: Tab) {
    app.melee.tab = tab;
    app.melee.sheet = None;
    opened(app);
}

/// Show only skins of `kind`, or every kind when it's none.
pub fn show_kind(app: &mut App, kind: Option<Kind>) {
    app.melee.kind = kind;
}

/// Open the sheet of the player's skin `id`.
pub fn open_skin(app: &mut App, id: SkinId) {
    app.melee.sheet = Some(Sheet::Skin(id));
}

/// Open the sheet of the site's skin `pack`.
pub fn open_site_skin(app: &mut App, pack: Pack) {
    app.melee.sheet = Some(Sheet::Site(pack));
}

pub fn close_sheet(app: &mut App) {
    app.melee.sheet = None;
}

/// Make a skin: the game's fighters and stages, to pick one. You's page and
/// a skin's page both come here, so it opens Skins first.
pub fn make(app: &mut App) {
    crate::nav::actions::go(app, crate::nav::Place::Skins);
    app.melee.sheet = None;
    app.melee.making = true;
    app.melee.picker = Roster::Fighters;
}

pub fn close_making(app: &mut App) {
    app.melee.making = false;
}

pub fn pick_roster(app: &mut App, roster: Roster) {
    app.melee.picker = roster;
}

/// Pick `character`: open its costume on the stage, and the editor on it.
pub fn pick_fighter(app: &mut App, character: Character) {
    app.melee.making = false;
    open_fighter(app, character);
    if app.stage.shown.is_some() {
        crate::editor::actions::open(app);
    }
}

/// Pick `stage`: open it on the stage, and the editor on it.
pub fn pick_stage(app: &mut App, stage: Stage) {
    app.melee.making = false;
    open_stage(app, stage);
    if app.stage.shown.is_some() {
        crate::editor::actions::open(app);
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

/// Back to the skins: a sheet or the picker closes, then the stage. Whether
/// there was a level to go up.
pub fn back(app: &mut App) -> bool {
    if app.melee.sheet.take().is_some() || std::mem::take(&mut app.melee.making) {
        return true;
    }
    if app.melee.subject.take().is_none() {
        return false;
    }
    stage::clear(app);
    true
}

/// Skins, on the ones the player made.
pub fn show_mine(app: &mut App) {
    crate::nav::actions::go(app, crate::nav::Place::Skins);
    show_tab(app, Tab::Made);
}

/// The player's skin the sheet shows, while it's one of theirs.
pub fn sheet_skin(app: &App) -> Option<&tgg_app::library::Skin> {
    let Some(Sheet::Skin(id)) = &app.melee.sheet else {
        return None;
    };
    app.library.skins().iter().find(|skin| skin.id == *id)
}

/// The site's skin the sheet shows, while it's one of the site's.
pub fn sheet_pack(app: &App) -> Option<&Pack> {
    let Some(Sheet::Site(pack)) = &app.melee.sheet else {
        return None;
    };
    Some(pack)
}

/// Whether the sheet's skin is on in tgg-melee now.
pub fn sheet_using(app: &App) -> bool {
    if let Some(skin) = sheet_skin(app) {
        return skin
            .slot
            .is_some_and(|slot| crate::library::port::plays(app, skin.id, slot));
    }
    if let Some(pack) = sheet_pack(app) {
        return site::kept(app, &pack.id)
            .iter()
            .filter_map(|id| app.library.skins().iter().find(|skin| skin.id == *id))
            .any(|skin| crate::library::view::in_game(app, skin));
    }
    false
}

/// Whether the sheet's skin is in the player's Melee disc now.
pub fn sheet_in_slippi(app: &App) -> bool {
    if let Some(skin) = sheet_skin(app) {
        return crate::library::view::in_iso(app, skin);
    }
    if let Some(pack) = sheet_pack(app) {
        return site::kept(app, &pack.id)
            .iter()
            .filter_map(|id| app.library.skins().iter().find(|skin| skin.id == *id))
            .any(|skin| crate::library::view::in_iso(app, skin));
    }
    false
}

/// Turn the sheet's skin on in tgg-melee, or off.
pub fn turn_use(app: &mut App, on: bool) {
    if let Some(skin) = sheet_skin(app) {
        let ids = [skin.id];
        return crate::library::actions::turn_on(app, &ids, on);
    }
    let Some(pack) = sheet_pack(app).cloned() else {
        return;
    };
    if on {
        let kept = site::kept(app, &pack.id);
        if kept.is_empty() {
            site::fetch(app, &pack, Then::Use);
        } else {
            let ids = kept.to_vec();
            crate::library::actions::turn_on(app, &ids, true);
        }
    } else {
        let ids = site::kept(app, &pack.id).to_vec();
        crate::library::actions::turn_on(app, &ids, false);
    }
}

/// Write the sheet's skin into the player's Melee disc, or take it back out.
pub fn turn_slippi(app: &mut App, on: bool) {
    if let Some(skin) = sheet_skin(app) {
        let ids = [skin.id];
        return crate::library::actions::write_into_slippi(app, &ids, on);
    }
    let Some(pack) = sheet_pack(app).cloned() else {
        return;
    };
    if on {
        let kept = site::kept(app, &pack.id);
        if kept.is_empty() {
            site::fetch(app, &pack, Then::Slippi);
        } else {
            let ids = kept.to_vec();
            crate::library::actions::write_into_slippi(app, &ids, true);
        }
    } else {
        let ids = site::kept(app, &pack.id).to_vec();
        crate::library::actions::write_into_slippi(app, &ids, false);
    }
}

/// Take the sheet's skin out of the player's own.
pub fn remove_skin(app: &mut App) {
    let Some(skin) = sheet_skin(app) else { return };
    let id = skin.id;
    crate::library::actions::remove(app, id);
    app.melee.sheet = None;
}

/// Open the site skin's page in the browser.
pub fn open_page(app: &mut App, tx: &mut Tx) {
    let Some(pack) = sheet_pack(app) else { return };
    let url = format!("{}{}", site::SITE, pack.path());
    if let Err(error) = tx.open_url(&url) {
        app.tell(format!("Couldn't open textures.gg: {error}"), true);
    }
}
