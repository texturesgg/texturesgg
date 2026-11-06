//! The shell's flows, driven through Rune's headless runtime: places, the
//! sidebar, and Welcome finding a game. No GPU and no game files; games are
//! disc images written by hand.

use crate::melee::Subject;
use crate::nav::{self, Place};
use crate::shell;
use crate::state::{App, Data, Start};
use crate::welcome::Welcome;
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tgg_app::GameChoice;
use tgg_app::settings::Settings;

const WINDOW: (f32, f32) = (1280.0, 800.0);
const FRAME: f64 = 1.0 / 60.0;

/// The app at `start`, its library and install history in a folder of their
/// own that lasts as long as the returned guard.
fn runtime(start: Start) -> (Runtime<App>, TempDir) {
    let folder = tempfile::tempdir().expect("temp dir");
    let data = Data { library: folder.path().join("library"), history: folder.path().join("games") };
    let app = App::new(start, Settings::default(), data, false);
    let mut rt = Runtime::new(app, shell::view, WINDOW, Env::new(true, 2.0));
    // As `main` wires the window.
    rt.on_key = Some(nav::actions::on_key);
    rt.frame(0.0);
    (rt, folder)
}

/// Press and release on the element keyed `key`, then let a frame run.
fn tap(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is on screen"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(FRAME);
}

/// Let springs come to rest.
fn settle(rt: &mut Runtime<App>) {
    for _ in 0..90 {
        rt.frame(FRAME);
    }
}

/// A Melee NTSC 1.02 disc image in a folder of its own, its one costume
/// (`PlFcRe.dat`) a model written by hand.
fn melee_iso() -> (TempDir, PathBuf) {
    disc_holding(&tgg_app::test_dat::model())
}

/// A Melee NTSC 1.02 disc image whose `PlFcRe.dat` holds `costume`.
fn disc_holding(costume: &[u8]) -> (TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("temp dir");
    let path = folder.path().join("melee.iso");
    std::fs::write(&path, tgg_app::game::test_disc(b"GALE01", 2, costume)).expect("write disc");
    (folder, path)
}

fn game_at(path: &Path) -> Start {
    let game = tgg_app::Game::open(path).expect("the test disc opens");
    Start::Game(tgg_app::game_references(game).expect("references"))
}

#[test]
fn a_place_in_the_sidebar_shows_it() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    assert_eq!(rt.state.nav.place, Place::Melee);
    tap(&mut rt, Place::Library.key());
    assert_eq!(rt.state.nav.place, Place::Library);
    tap(&mut rt, Place::Mods.key());
    assert_eq!(rt.state.nav.place, Place::Mods);
}

#[test]
fn the_sidebar_hides_whole_and_its_toggle_stays_put() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    settle(&mut rt);
    let toggle = rt.presented("shell/toggle").expect("the toggle shows").0;
    tap(&mut rt, "shell/toggle");
    settle(&mut rt);
    assert!(!rt.state.nav.sidebar);
    assert_eq!(rt.state.settings.sidebar, Some(false), "and it's remembered");
    let width = rt.presented("shell/sidebar").map_or(0.0, |(rect, _)| rect[2]);
    assert!(width < 0.5, "the sidebar narrows to nothing, not {width} pt");
    assert_eq!(rt.presented("shell/toggle").expect("the toggle still shows").0, toggle);
    tap(&mut rt, "shell/toggle");
    assert!(rt.state.nav.sidebar);
}

#[test]
fn ctrl_backslash_shows_and_hides_the_sidebar() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let command = KeyEvent { key: Key::Command('\\'), mods: Mods { cmd: true, ..Default::default() } };
    rt.key(command.clone());
    rt.frame(FRAME);
    assert!(!rt.state.nav.sidebar);
    rt.key(command);
    rt.frame(FRAME);
    assert!(rt.state.nav.sidebar);
}

#[test]
fn without_a_game_welcome_asks_for_one() {
    let (rt, _data) = runtime(Start::Welcome(Welcome::default()));
    assert!(rt.find("welcome/choose").is_some());
    assert!(rt.find(Place::Melee.key()).is_none(), "no places until there's a game");
    if cfg!(not(target_os = "macos")) {
        assert!(rt.find("window/close").is_some(), "a borderless window can still be closed");
    }
}

#[test]
fn an_iso_that_isnt_melee_is_refused_in_the_players_words() {
    let folder = tempfile::tempdir().expect("temp dir");
    let other = folder.path().join("other.iso");
    std::fs::write(&other, tgg_app::game::test_disc(b"GALE01", 0, b"file")).expect("write disc");
    let (mut rt, _data) = runtime(Start::Welcome(Welcome::default()));
    tap(&mut rt, "welcome/choose");
    rt.resolve_file_dialog(Some(other)).expect("a dialog was open");
    rt.frame(FRAME);
    assert!(rt.state.game.is_none());
    let problem = rt.state.welcome.problem.clone().expect("Welcome says why");
    assert!(problem.contains("1.02"), "it names the version Melee must be: {problem}");
    assert!(rt.find("welcome/problem").is_some());
}

#[test]
fn choosing_a_melee_iso_opens_the_game_and_remembers_it() {
    let (folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(Start::Welcome(Welcome::default()));
    tap(&mut rt, "welcome/choose");
    rt.resolve_file_dialog(Some(iso.clone())).expect("a dialog was open");
    rt.frame(FRAME);
    assert!(rt.state.game.is_some());
    assert_eq!(rt.state.nav.place, Place::Melee);
    assert_eq!(rt.state.settings.iso_path.as_deref(), Some(iso.as_path()));
    assert_eq!(rt.state.settings.game_folders, vec![folder.path().to_path_buf()]);
    assert!(rt.find(Place::Melee.key()).is_some(), "the places show");
}

#[test]
fn a_game_found_in_the_players_folders_opens_with_one_press() {
    let (_folder, iso) = melee_iso();
    let welcome = Welcome {
        games: vec![GameChoice { path: iso, slippi_plays: true, changed: 0 }],
        problem: None,
    };
    let (mut rt, _data) = runtime(Start::Welcome(welcome));
    tap(&mut rt, "welcome/game/0");
    assert!(rt.state.game.is_some());
}

#[test]
fn the_roster_shows_the_games_fighters_and_whats_changed_once_its_read() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    assert!(rt.find("melee/fighter/Falco").is_some(), "the test disc holds a Falco costume");
    assert!(rt.state.melee.states.is_none(), "nothing is read until the window runs");
    // The disc's PlFcRe.dat isn't the game's own file: it reads as custom.
    let data = tempfile::tempdir().expect("temp dir");
    rt.state.jobs.read_states(&iso, &data.path().join("library"), &data.path().join("games"));
    let done = rt.state.jobs.wait(std::time::Duration::from_secs(10)).expect("the states arrive");
    let crate::jobs::Done::States { iso: read, states, undoable } = done;
    assert!(undoable.is_empty(), "nothing's been installed");
    assert!(crate::melee::actions::states_read(&mut rt.state, &read, states, undoable));
    let falco = &rt.state.melee.fighters[0];
    assert_eq!(rt.state.melee.changed_count(falco), 1);
}

#[test]
fn states_read_for_another_game_are_set_aside() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let elsewhere = Path::new("/elsewhere/melee.iso");
    assert!(!crate::melee::actions::states_read(&mut rt.state, elsewhere, Default::default(), Default::default()));
    assert!(rt.state.melee.states.is_none());
}

#[test]
fn the_stages_tab_shows_the_stages() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/tab/stages");
    assert_eq!(rt.state.melee.tab, crate::melee::Tab::Stages);
    assert!(rt.find("melee/fighter/Falco").is_none());
}

fn press_escape(rt: &mut Runtime<App>) {
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(FRAME);
}

#[test]
fn a_fighter_opens_onto_the_stage_and_escape_closes_it_into_its_plate() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    assert_eq!(rt.state.melee.subject, Some(Subject::Fighter(melee_dat::Character::from_code("Fc").expect("Falco"))));
    let shown = rt.state.stage.shown.as_ref().expect("Falco is on the stage");
    assert_eq!(shown.slot.file_name(), "PlFcRe.dat");
    assert!(rt.find("stage/viewport").is_some());
    assert!(rt.find("melee/crumb").is_some(), "the title row says where the player is");
    assert!(rt.find("melee/fighter/Falco").is_none(), "the roster gives way to the stage");
    press_escape(&mut rt);
    assert_eq!(rt.state.melee.subject, None);
    assert!(rt.state.stage.shown.is_none(), "the stage empties");
    assert!(rt.find("melee/fighter/Falco").is_some());
}

#[test]
fn the_breadcrumb_goes_back_to_the_roster() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    tap(&mut rt, "melee/crumb");
    assert_eq!(rt.state.melee.subject, None);
}

#[test]
fn a_costume_that_wont_load_says_why_and_stays_on_the_roster() {
    let (_folder, iso) = disc_holding(b"not a costume");
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    assert_eq!(rt.state.melee.subject, None);
    let problem = rt.state.melee.problem.clone().expect("it says why");
    assert!(problem.starts_with("Falco couldn't open"), "{problem}");
    assert!(rt.find("melee/problem").is_some());
}

#[test]
fn a_pane_folds_to_its_header() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    assert!(rt.find("melee/costume/PlFcRe.dat").is_some());
    tap(&mut rt, "melee/pane/costumes");
    assert!(rt.find("melee/costume/PlFcRe.dat").is_none(), "folded, the costumes hide");
    assert!(rt.find("melee/pane/costumes").is_some(), "but the header stays");
}

#[test]
fn going_to_another_place_takes_the_fighter_off_the_stage() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    tap(&mut rt, Place::Library.key());
    assert!(rt.state.stage.shown.is_none());
    assert_eq!(rt.state.melee.subject, None);
}

/// A Falco Red skin on disk, as a player would add it.
fn falco_skin(folder: &Path) -> (PathBuf, Vec<u8>) {
    let bytes = tgg_app::test_dat::model_named("PlyFalco5KRe_Share_joint");
    let path = folder.join("Waffle Falco.dat");
    std::fs::write(&path, &bytes).expect("write skin");
    (path, bytes)
}

/// Add `file` through the Library's file dialog, into Review.
fn add_through_review(rt: &mut Runtime<App>, file: PathBuf) {
    tap(rt, Place::Library.key());
    tap(rt, "library/add-file");
    rt.resolve_file_dialog(Some(file)).expect("a dialog was open");
    rt.frame(FRAME);
    settle(rt);
}

/// Wait for what every slot holds to be read again, and take it.
fn reread(rt: &mut Runtime<App>) {
    let done = rt.state.jobs.wait(std::time::Duration::from_secs(10)).expect("the states arrive");
    let crate::jobs::Done::States { iso, states, undoable } = done;
    rt.update(|app, _| {
        crate::melee::actions::states_read(app, &iso, states, undoable);
    });
}

#[test]
fn a_skin_added_from_a_file_is_reviewed_then_kept() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    let review = rt.state.library_page.review.as_ref().expect("Review opens first");
    assert_eq!(review.items.len(), 1);
    assert_eq!(review.items[0].slot.map(|slot| slot.file_name()).as_deref(), Some("PlFcRe.dat"));
    assert!(rt.state.library.skins().is_empty(), "nothing joins until the player says");
    tap(&mut rt, "review/add");
    assert!(rt.state.library_page.review.is_none());
    assert_eq!(rt.state.library.skins().len(), 1);
    let notice = rt.state.notice.clone().expect("it says what happened");
    assert!(notice.text.starts_with("Added"), "{}", notice.text);
}

#[test]
fn add_and_install_writes_the_skin_into_the_game() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, bytes) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/install");
    let slot: melee_dat::MeleeSlot = "PlFcRe.dat".parse().expect("a slot");
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(game.read_slot(slot).expect("read back"), bytes, "the disc holds the skin now");
    assert!(rt.state.notice.as_ref().is_some_and(|notice| notice.text.starts_with("Installed")));
}

#[test]
fn an_install_undoes_from_the_fighters_costume() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let original = tgg_app::test_dat::model();
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/install");
    reread(&mut rt);
    let slot: melee_dat::MeleeSlot = "PlFcRe.dat".parse().expect("a slot");
    assert!(rt.state.melee.undoable.contains(&slot), "the install can be undone");
    tap(&mut rt, Place::Melee.key());
    tap(&mut rt, "melee/fighter/Falco");
    settle(&mut rt);
    tap(&mut rt, "melee/undo");
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(game.read_slot(slot).expect("read back"), original, "the costume is back as it was");
}

#[test]
fn a_skin_comes_out_of_the_library() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/add");
    let id = rt.state.library.skins()[0].id;
    settle(&mut rt);
    tap(&mut rt, &format!("library/remove/{id}"));
    assert!(rt.state.library.skins().is_empty());
}

#[test]
fn a_file_that_isnt_a_skin_says_why_and_adds_nothing() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let junk = files.path().join("notes.dat");
    std::fs::write(&junk, b"not a model").expect("write");
    add_through_review(&mut rt, junk);
    let review = rt.state.library_page.review.as_ref().expect("Review says why");
    assert!(review.items.is_empty());
    assert_eq!(review.rejected.len(), 1);
    assert!(rt.find("review/rejected/0").is_some());
    assert!(rt.find("review/add").is_none(), "there's nothing to add");
}

#[test]
fn escape_closes_review_without_adding() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    press_escape(&mut rt);
    assert!(rt.state.library_page.review.is_none());
    assert!(rt.state.library.skins().is_empty());
}
