//! The shell's flows, driven through Rune's headless runtime: places, the
//! sidebar, and Welcome finding a game. No GPU and no game files; games are
//! disc images written by hand.

use crate::melee::Subject;
use crate::nav::{self, Place};
use crate::shell;
use crate::state::{App, Start};
use crate::welcome::Welcome;
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tgg_app::GameChoice;
use tgg_app::settings::Settings;

const WINDOW: (f32, f32) = (1280.0, 800.0);
const FRAME: f64 = 1.0 / 60.0;

fn runtime(start: Start) -> Runtime<App> {
    let app = App::new(start, Settings::default(), false);
    let mut rt = Runtime::new(app, shell::view, WINDOW, Env::new(true, 2.0));
    // As `main` wires the window.
    rt.on_key = Some(nav::actions::on_key);
    rt.frame(0.0);
    rt
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
    let mut rt = runtime(game_at(&iso));
    assert_eq!(rt.state.nav.place, Place::Melee);
    tap(&mut rt, Place::Library.key());
    assert_eq!(rt.state.nav.place, Place::Library);
    tap(&mut rt, Place::Mods.key());
    assert_eq!(rt.state.nav.place, Place::Mods);
}

#[test]
fn the_sidebar_hides_whole_and_its_toggle_stays_put() {
    let (_folder, iso) = melee_iso();
    let mut rt = runtime(game_at(&iso));
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
    let mut rt = runtime(game_at(&iso));
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
    let rt = runtime(Start::Welcome(Welcome::default()));
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
    let mut rt = runtime(Start::Welcome(Welcome::default()));
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
    let mut rt = runtime(Start::Welcome(Welcome::default()));
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
    let mut rt = runtime(Start::Welcome(welcome));
    tap(&mut rt, "welcome/game/0");
    assert!(rt.state.game.is_some());
}

#[test]
fn the_roster_shows_the_games_fighters_and_whats_changed_once_its_read() {
    let (_folder, iso) = melee_iso();
    let mut rt = runtime(game_at(&iso));
    assert!(rt.find("melee/fighter/Falco").is_some(), "the test disc holds a Falco costume");
    assert!(rt.state.melee.states.is_none(), "nothing is read until the window runs");
    // The disc's PlFcRe.dat isn't the game's own file: it reads as custom.
    let library = tempfile::tempdir().expect("temp dir");
    rt.state.jobs.read_states(&iso, library.path());
    let done = rt.state.jobs.wait(std::time::Duration::from_secs(10)).expect("the states arrive");
    let crate::jobs::Done::States { iso: read, states } = done;
    assert!(crate::melee::actions::states_read(&mut rt.state, &read, states));
    let falco = &rt.state.melee.fighters[0];
    assert_eq!(rt.state.melee.changed_count(falco), 1);
}

#[test]
fn states_read_for_another_game_are_set_aside() {
    let (_folder, iso) = melee_iso();
    let mut rt = runtime(game_at(&iso));
    let elsewhere = Path::new("/elsewhere/melee.iso");
    assert!(!crate::melee::actions::states_read(&mut rt.state, elsewhere, Default::default()));
    assert!(rt.state.melee.states.is_none());
}

#[test]
fn the_stages_tab_shows_the_stages() {
    let (_folder, iso) = melee_iso();
    let mut rt = runtime(game_at(&iso));
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
    let mut rt = runtime(game_at(&iso));
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
    let mut rt = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    tap(&mut rt, "melee/crumb");
    assert_eq!(rt.state.melee.subject, None);
}

#[test]
fn a_costume_that_wont_load_says_why_and_stays_on_the_roster() {
    let (_folder, iso) = disc_holding(b"not a costume");
    let mut rt = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    assert_eq!(rt.state.melee.subject, None);
    let problem = rt.state.melee.problem.clone().expect("it says why");
    assert!(problem.starts_with("Falco couldn't open"), "{problem}");
    assert!(rt.find("melee/problem").is_some());
}

#[test]
fn a_pane_folds_to_its_header() {
    let (_folder, iso) = melee_iso();
    let mut rt = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    assert!(rt.find("melee/costume/PlFcRe.dat").is_some());
    tap(&mut rt, "melee/pane/costumes");
    assert!(rt.find("melee/costume/PlFcRe.dat").is_none(), "folded, the costumes hide");
    assert!(rt.find("melee/pane/costumes").is_some(), "but the header stays");
}

#[test]
fn going_to_another_place_takes_the_fighter_off_the_stage() {
    let (_folder, iso) = melee_iso();
    let mut rt = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    tap(&mut rt, Place::Library.key());
    assert!(rt.state.stage.shown.is_none());
    assert_eq!(rt.state.melee.subject, None);
}
