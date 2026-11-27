//! The shell's flows, driven through Rune's headless runtime: places, the
//! sidebar, and first run finding a game. No GPU and no game files; games
//! are disc images written by hand.

use crate::first_run::FirstRun;
use crate::nav::Place;
use crate::shell;
use crate::state::{App, Data, Start};
use rune::{Env, Key, KeyEvent, Mods, Runtime};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tgg_app::settings::Settings;

const WINDOW: (f32, f32) = (1280.0, 800.0);
const FRAME: f64 = 1.0 / 60.0;

/// The app at `start`, its library and install history in a folder of their
/// own that lasts as long as the returned guard.
fn runtime(start: Start) -> (Runtime<App>, TempDir) {
    let folder = tempfile::tempdir().expect("temp dir");
    let data = Data::in_folder(folder.path());
    let app = App::new(start, Settings::default(), data, false);
    let mut rt = Runtime::new(app, shell::view, WINDOW, Env::new(true, 2.0));
    // As `main` wires the window.
    rt.on_key = Some(crate::keys::on_key);
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

/// Go to `place` from the sidebar, and let the page come to rest.
fn open_place(rt: &mut Runtime<App>, place: Place) {
    tap(rt, place.key());
    settle(rt);
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
    assert_eq!(rt.state.nav.place, Place::Play, "it opens on Play");
    tap(&mut rt, Place::Skins.key());
    assert_eq!(rt.state.nav.place, Place::Skins);
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
    assert_eq!(
        rt.state.settings.sidebar,
        Some(false),
        "and it's remembered"
    );
    let width = rt
        .presented("shell/sidebar")
        .map_or(0.0, |(rect, _)| rect[2]);
    assert!(
        width < 0.5,
        "the sidebar narrows to nothing, not {width} pt"
    );
    assert_eq!(
        rt.presented("shell/toggle")
            .expect("the toggle still shows")
            .0,
        toggle
    );
    tap(&mut rt, "shell/toggle");
    assert!(rt.state.nav.sidebar);
}

#[test]
fn the_sidebar_toggle_clears_titlebar_inset_and_collapses_in_fullscreen() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let chrome = rune::Chrome {
        full_size_content: true,
        traffic_lights: Some((
            crate::TRAFFIC_LIGHT_X,
            ((crate::ui::tokens::size::TOP_BAR - rune::Chrome::TRAFFIC_LIGHT) / 2.0).round(),
        )),
        drag_regions: true,
    };
    let inset = chrome.titlebar_inset();
    rt.set_titlebar_inset(inset);
    settle(&mut rt);
    let windowed = rt.presented("shell/toggle").expect("the toggle shows").0;
    assert!(
        (windowed[0] - (inset + crate::ui::tokens::space::XS)).abs() < 0.5,
        "toggle x {} should clear traffic lights inset {inset}",
        windowed[0]
    );
    let light_midline = chrome.traffic_light_rects().expect("traffic lights")[0][1]
        + rune::Chrome::TRAFFIC_LIGHT / 2.0;
    let toggle_midline = windowed[1] + windowed[3] / 2.0;
    assert!(
        (toggle_midline - light_midline).abs() < 0.5,
        "toggle midline {toggle_midline} should match traffic lights {light_midline}"
    );
    // Entering full screen drops the titlebar inset to 0.
    rt.set_titlebar_inset(0.0);
    settle(&mut rt);
    let fullscreen = rt.presented("shell/toggle").expect("the toggle shows").0;
    assert!(
        (fullscreen[0] - crate::ui::tokens::space::XS).abs() < 0.5,
        "toggle x {} should collapse to leading edge in full screen",
        fullscreen[0]
    );
}

#[test]
fn ctrl_backslash_shows_and_hides_the_sidebar() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let command = KeyEvent {
        key: Key::Command('\\'),
        mods: Mods {
            cmd: true,
            ..Default::default()
        },
    };
    rt.key(command.clone());
    rt.frame(FRAME);
    assert!(!rt.state.nav.sidebar);
    rt.key(command);
    rt.frame(FRAME);
    assert!(rt.state.nav.sidebar);
}

#[test]
fn first_run_asks_for_a_disc_and_the_game() {
    let (rt, _data) = runtime(Start::FirstRun {
        game: None,
        state: FirstRun::default(),
    });
    assert!(rt.find("first-run/choose-disc").is_some());
    assert!(rt.find("first-run/download").is_some());
    assert!(
        rt.find(Place::Play.key()).is_none(),
        "no places until there's a game and tgg-melee"
    );
}

#[test]
fn an_iso_that_isnt_melee_is_refused_in_the_players_words() {
    let folder = tempfile::tempdir().expect("temp dir");
    let other = folder.path().join("other.iso");
    std::fs::write(&other, tgg_app::game::test_disc(b"GALE01", 0, b"file")).expect("write disc");
    let (mut rt, _data) = runtime(Start::FirstRun {
        game: None,
        state: FirstRun::default(),
    });
    tap(&mut rt, "first-run/choose-disc");
    rt.resolve_file_dialog(Some(other))
        .expect("a dialog was open");
    rt.frame(FRAME);
    assert!(rt.state.game.is_none());
    let problem = rt
        .state
        .first_run
        .as_ref()
        .and_then(|first| first.problem.clone())
        .expect("first run says why");
    assert!(
        problem.contains("1.02"),
        "it names the version Melee must be: {problem}"
    );
    assert!(rt.find("first-run/problem").is_some());
}

#[test]
fn choosing_a_melee_iso_opens_the_game_and_remembers_it() {
    let (folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(Start::FirstRun {
        game: None,
        state: FirstRun::default(),
    });
    tap(&mut rt, "first-run/choose-disc");
    rt.resolve_file_dialog(Some(iso.clone()))
        .expect("a dialog was open");
    rt.frame(FRAME);
    assert!(rt.state.game.is_some());
    assert!(
        rt.state.first_run.is_some(),
        "first run stays until tgg-melee is installed"
    );
    assert_eq!(rt.state.settings.iso_path.as_deref(), Some(iso.as_path()));
    assert_eq!(
        rt.state.settings.game_folders,
        vec![folder.path().to_path_buf()]
    );
    assert_eq!(rt.state.mods.disc, crate::mods::Disc::Found(iso));
    assert!(rt.find(Place::Play.key()).is_none(), "no places yet");
}

#[test]
fn a_game_found_starts_first_run_with_that_game() {
    let (_folder, iso) = melee_iso();
    let game = tgg_app::Game::open(&iso).expect("the test disc opens");
    let references = tgg_app::game_references(game).expect("references");
    let (rt, _data) = runtime(Start::FirstRun {
        game: Some(references),
        state: FirstRun::default(),
    });
    assert!(rt.state.game.is_some());
    assert!(rt.state.first_run.is_some());
    assert!(rt.find("first-run/download").is_some());
    assert!(rt.find(Place::Play.key()).is_none());
}

#[test]
fn first_run_installs_the_game_and_makes_the_picked_set() {
    let (_folder, iso) = melee_iso();
    let game = tgg_app::Game::open(&iso).expect("the test disc opens");
    let references = tgg_app::game_references(game).expect("references");
    let (mut rt, _data) = runtime(Start::FirstRun {
        game: Some(references),
        state: FirstRun::default(),
    });
    rt.state.mods.net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    crate::first_run::actions::opened(&mut rt.state);
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered && !app.mods.releases.is_empty()
    });
    crate::mods::actions::use_disc(&mut rt.state, iso.clone());
    rt.frame(FRAME);
    tap(&mut rt, "first-run/download");
    assert_eq!(
        rt.state.first_run.as_ref().map(|first| first.stage),
        Some(crate::first_run::Stage::Downloading)
    );
    jobs_until(&mut rt, |app| {
        app.first_run
            .as_ref()
            .is_some_and(|first| first.stage == crate::first_run::Stage::Done)
            && app.sets.list.iter().any(|set| set.name == "Competitive")
    });
    assert!(rt.state.mods.port().is_some(), "tgg-melee is installed");
    let competitive = rt
        .state
        .sets
        .list
        .iter()
        .find(|set| set.name == "Competitive")
        .expect("the picked set was added");
    assert!(competitive.locked);
    assert_eq!(
        rt.state.sets.active, competitive.key,
        "and it's the one Play starts"
    );
}

#[test]
fn the_picker_shows_the_games_fighters_and_whats_changed_once_its_read() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    open_place(&mut rt, Place::Skins);
    tap(&mut rt, "skins/make");
    assert!(
        rt.find("melee/fighter/Falco").is_some(),
        "the test disc holds a Falco costume"
    );
    assert!(
        rt.state.melee.states.is_none(),
        "nothing is read until the window runs"
    );
    // The disc's PlFcRe.dat isn't the game's own file: it reads as custom.
    let data = tempfile::tempdir().expect("temp dir");
    rt.state.jobs.read_states(
        &iso,
        &data.path().join("library"),
        &data.path().join("games"),
    );
    let done = rt
        .state
        .jobs
        .wait(std::time::Duration::from_secs(10))
        .expect("the states arrive");
    let crate::jobs::Done::States {
        iso: read,
        states,
        undoable,
    } = done
    else {
        panic!("the slot states arrive first")
    };
    assert!(undoable.is_empty(), "nothing's been installed");
    assert!(crate::melee::actions::states_read(
        &mut rt.state,
        &read,
        states,
        undoable
    ));
    let falco = &rt.state.melee.fighters[0];
    assert_eq!(rt.state.melee.changed_count(falco), 1);
}

#[test]
fn states_read_for_another_game_are_set_aside() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let elsewhere = Path::new("/elsewhere/melee.iso");
    assert!(!crate::melee::actions::states_read(
        &mut rt.state,
        elsewhere,
        Default::default(),
        Default::default()
    ));
    assert!(rt.state.melee.states.is_none());
}

#[test]
fn the_picker_shows_the_stages_too() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    open_place(&mut rt, Place::Skins);
    tap(&mut rt, "skins/make");
    settle(&mut rt);
    tap(&mut rt, "skins/picker/stages");
    assert_eq!(rt.state.melee.picker, crate::melee::Roster::Stages);
    assert!(rt.find("melee/fighter/Falco").is_none());
}

fn press_escape(rt: &mut Runtime<App>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(FRAME);
}

#[test]
fn picking_a_fighter_opens_the_editor_and_escape_goes_back() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    make_a_skin(&mut rt, "Falco");
    assert!(rt.state.editor.is_some(), "Falco's costume is open");
    let shown = rt
        .state
        .stage
        .shown
        .as_ref()
        .expect("Falco is on the stage");
    assert_eq!(shown.slot.file_name(), "PlFcRe.dat");
    assert!(rt.find("stage/viewport").is_some());
    assert!(
        rt.find("melee/fighter/Falco").is_none(),
        "the picker gives way to the editor"
    );
    press_escape(&mut rt);
    settle(&mut rt);
    assert!(rt.state.editor.is_none(), "nothing was unsaved");
    assert!(rt.state.stage.shown.is_none(), "the stage empties");
    assert_eq!(rt.state.melee.subject, None);
    assert!(rt.find("skins/make").is_some(), "back on Skins");
}

#[test]
fn a_costume_that_wont_load_says_why_and_stays_on_the_page() {
    let (_folder, iso) = disc_holding(b"not a costume");
    let (mut rt, _data) = runtime(game_at(&iso));
    open_place(&mut rt, Place::Skins);
    tap(&mut rt, "skins/make");
    settle(&mut rt);
    tap(&mut rt, "melee/fighter/Falco");
    assert!(rt.state.editor.is_none());
    assert_eq!(rt.state.melee.subject, None);
    let problem = rt.state.melee.problem.clone().expect("it says why");
    assert!(problem.starts_with("Falco couldn't open"), "{problem}");
    assert!(rt.find("melee/problem").is_some());
}

#[test]
fn dragging_across_the_stage_turns_the_camera_by_how_far_it_went() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    make_a_skin(&mut rt, "Falco");
    let before = rt
        .state
        .stage
        .shown
        .as_ref()
        .expect("Falco is on the stage")
        .orbit;
    let (x, y) = rt.find("stage/viewport").expect("the stage shows");
    rt.pointer_down(x, y);
    rt.frame(FRAME);
    assert_eq!(rt.cursor(), rune::Cursor::Grabbing, "a held drag grabs");
    // Each move turns it by that move: four of 25 points turn it as one of 100,
    // and a wobble down and back up leaves the pitch where it was.
    for (step, wobble) in [(25.0, 0.0), (50.0, 30.0), (75.0, 0.0), (100.0, 0.0)] {
        rt.pointer_move(x + step, y + wobble);
        rt.frame(FRAME);
    }
    rt.pointer_up(x + 100.0, y);
    rt.frame(FRAME);
    let after = rt
        .state
        .stage
        .shown
        .as_ref()
        .expect("still on the stage")
        .orbit;
    assert!(
        (after.yaw - (before.yaw - 1.0)).abs() < 1e-6,
        "100 points turn it a radian: {} → {}",
        before.yaw,
        after.yaw
    );
    assert_eq!(after.pitch, before.pitch);
    assert_eq!(
        rt.cursor(),
        rune::Cursor::Grab,
        "let go, it's ready to grab again"
    );

    // Down turns it to look from above.
    rt.pointer_down(x, y);
    rt.frame(FRAME);
    for step in [10.0, 20.0, 30.0] {
        rt.pointer_move(x, y + step);
        rt.frame(FRAME);
    }
    rt.pointer_up(x, y + 30.0);
    rt.frame(FRAME);
    let above = rt
        .state
        .stage
        .shown
        .as_ref()
        .expect("still on the stage")
        .orbit;
    assert!(
        (above.pitch - (after.pitch + 0.3)).abs() < 1e-6,
        "30 points down tip it 0.3: {} → {}",
        after.pitch,
        above.pitch
    );
    assert_eq!(above.yaw, after.yaw);
    let after = above;

    let mut wheel = rune::WheelEvent::default();
    // One notch of a mouse wheel (Rune's 40-point line) moves it 4%.
    (wheel.x, wheel.y, wheel.dy) = (x, y, 40.0);
    rt.wheel_event(wheel);
    rt.frame(FRAME);
    let zoomed = rt
        .state
        .stage
        .shown
        .as_ref()
        .expect("still on the stage")
        .orbit;
    assert!(
        (zoomed.zoom - after.zoom * (-0.04f64).exp()).abs() < 1e-9,
        "one notch brings it in 4%: {} → {}",
        after.zoom,
        zoomed.zoom
    );
}

#[test]
fn going_to_another_place_takes_the_fighter_off_the_stage() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    make_a_skin(&mut rt, "Falco");
    tap(&mut rt, Place::Mods.key());
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

/// Add `file` through the Skin's file dialog, into Review.
fn add_through_review(rt: &mut Runtime<App>, file: PathBuf) {
    open_place(rt, Place::Skins);
    tap(rt, "skins/tab/using");
    tap(rt, "skins/add");
    rt.resolve_file_dialog(Some(file))
        .expect("a dialog was open");
    rt.frame(FRAME);
    settle(rt);
}

/// Make a skin on Skins: the picker, then the fighter, which opens the
/// editor.
fn make_a_skin(rt: &mut Runtime<App>, fighter: &str) {
    open_place(rt, Place::Skins);
    tap(rt, "skins/make");
    // The picker fades and rises in; let it land where the plate is drawn.
    settle(rt);
    tap(rt, &format!("melee/fighter/{fighter}"));
    settle(rt);
}

/// Open the player's skin `id` from its plate, and let its sheet land.
fn open_skin_sheet(rt: &mut Runtime<App>, id: tgg_app::ids::SkinId) {
    tap(rt, &format!("skins/skin/{id}"));
    settle(rt);
}

/// Wait for what every slot holds to be read again, and take it.
fn reread(rt: &mut Runtime<App>) {
    let done = rt
        .state
        .jobs
        .wait(std::time::Duration::from_secs(10))
        .expect("the states arrive");
    let crate::jobs::Done::States {
        iso,
        states,
        undoable,
    } = done
    else {
        panic!("the slot states arrive first")
    };
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
    let review = rt
        .state
        .library_page
        .review
        .as_ref()
        .expect("Review opens first");
    assert_eq!(review.items.len(), 1);
    assert_eq!(
        review.items[0].slot.map(|slot| slot.file_name()).as_deref(),
        Some("PlFcRe.dat")
    );
    assert!(
        rt.state.library.skins().is_empty(),
        "nothing joins until the player says"
    );
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
    assert_eq!(
        game.read_slot(slot).expect("read back"),
        bytes,
        "the disc holds the skin now"
    );
    assert!(
        rt.state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.starts_with("Installed"))
    );
}

#[test]
fn an_installs_toast_undoes_it_and_dismisses() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let original = tgg_app::test_dat::model();
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/install");
    settle(&mut rt);
    tap(&mut rt, "shell/notice/undo");
    let slot: melee_dat::MeleeSlot = "PlFcRe.dat".parse().expect("a slot");
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(
        game.read_slot(slot).expect("read back"),
        original,
        "Undo put the costume back"
    );
    assert!(
        rt.find("shell/notice/undo").is_none(),
        "the undo's own toast has nothing to undo"
    );
    tap(&mut rt, "shell/notice/dismiss");
    assert!(rt.state.notice.is_none());
}

#[test]
fn an_install_undoes_from_its_skins_sheet() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let original = tgg_app::test_dat::model();
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/install");
    reread(&mut rt);
    let slot: melee_dat::MeleeSlot = "PlFcRe.dat".parse().expect("a slot");
    assert!(
        rt.state.melee.undoable.contains(&slot),
        "the install can be undone"
    );
    let id = rt.state.library.skins()[0].id;
    tap(&mut rt, "skins/tab/using");
    open_skin_sheet(&mut rt, id);
    tap(&mut rt, "skins/sheet/slippi");
    reread(&mut rt);
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(
        game.read_slot(slot).expect("read back"),
        original,
        "the costume is back as it was"
    );
}

#[test]
fn a_skin_comes_out_of_the_players_skins() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/add");
    let id = rt.state.library.skins()[0].id;
    settle(&mut rt);
    tap(&mut rt, "skins/tab/using");
    open_skin_sheet(&mut rt, id);
    tap(&mut rt, "skins/sheet/remove");
    assert!(rt.state.library.skins().is_empty());
}

#[test]
fn a_skin_goes_into_tgg_melee_and_comes_back_out() {
    let (_folder, iso) = melee_iso();
    let (mut rt, data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let (skin, _) = falco_skin(files.path());
    add_through_review(&mut rt, skin);
    tap(&mut rt, "review/add");
    let id = rt.state.library.skins()[0].id;
    settle(&mut rt);
    let skins_file = data.path().join("skins.json");
    let plays = |file: &Path| {
        tgg_mod::skins(file)
            .expect("skins.json reads")
            .slots
            .get("PlFcRe.dat")
            .map(|skin| skin.sha256.clone())
    };
    tap(&mut rt, "skins/tab/using");
    open_skin_sheet(&mut rt, id);
    tap(&mut rt, "skins/sheet/use");
    assert_eq!(plays(&skins_file), Some(id.to_string()));
    let notice = rt.state.notice.as_ref().expect("it says so");
    assert!(notice.text.contains("tgg-melee"), "{}", notice.text);
    assert!(
        crate::library::view::in_game(&rt.state, &rt.state.library.skins()[0]),
        "the switch shows it on"
    );
    let skin = &rt.state.library.skins()[0];
    assert!(
        !crate::library::view::in_iso(&rt.state, skin),
        "not in Slippi"
    );
    tap(&mut rt, "shell/notice/undo");
    assert_eq!(plays(&skins_file), None, "Undo takes it back out");
    tap(&mut rt, "skins/sheet/use");
    let slot = rt.state.library.skins()[0]
        .slot
        .expect("made for Falco Red");
    rt.update(move |app, _| crate::library::port::remove(app, slot));
    assert_eq!(plays(&skins_file), None, "the original plays again");
    tap(&mut rt, "shell/notice/undo");
    assert_eq!(
        plays(&skins_file),
        Some(id.to_string()),
        "Undo puts it back"
    );
}

#[test]
fn a_file_that_isnt_a_skin_says_why_and_adds_nothing() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let files = tempfile::tempdir().expect("temp dir");
    let junk = files.path().join("notes.dat");
    std::fs::write(&junk, b"not a model").expect("write");
    add_through_review(&mut rt, junk);
    let review = rt
        .state
        .library_page
        .review
        .as_ref()
        .expect("Review says why");
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

/// Falco open in the editor, on the test disc's costume.
fn editing() -> (Runtime<App>, TempDir, TempDir) {
    let (folder, iso) = melee_iso();
    let (mut rt, data) = runtime(game_at(&iso));
    make_a_skin(&mut rt, "Falco");
    (rt, folder, data)
}

/// A one-color PNG `size` pixels square, on disk.
fn png(folder: &Path, size: u32, rgba: [u8; 4]) -> PathBuf {
    let path = folder.join(format!("paint-{size}.png"));
    image::RgbaImage::from_pixel(size, size, image::Rgba(rgba))
        .save(&path)
        .expect("write png");
    path
}

fn command(rt: &mut Runtime<App>, character: char, shift: bool) {
    rt.key(KeyEvent {
        key: Key::Command(character),
        mods: Mods {
            cmd: true,
            shift,
            ..Default::default()
        },
    });
    rt.frame(FRAME);
}

#[test]
fn edit_textures_opens_the_costume_showing_with_its_first_texture() {
    let (rt, _disc, _data) = editing();
    let document = rt.state.editor.as_ref().expect("the editor opens");
    assert_eq!(document.slot.file_name(), "PlFcRe.dat");
    assert_eq!(document.texture_count(), 1);
    assert_eq!(document.selected, Some(0));
    assert!(rt.find("editor/texture/0").is_some());
    let shown = rt.state.stage.shown.as_ref().expect("on the same stage");
    assert!(
        !shown.highlight.is_empty(),
        "the selected texture's surfaces tint"
    );
}

#[test]
fn done_without_edits_goes_straight_back() {
    let (mut rt, _disc, _data) = editing();
    tap(&mut rt, "editor/done");
    settle(&mut rt);
    assert!(rt.state.editor.is_none());
    assert!(rt.state.stage.shown.is_none(), "the stage empties");
    assert!(rt.find("skins/make").is_some(), "back on Skins");
}

#[test]
fn an_imported_png_shows_on_the_model_and_undoes() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    rt.frame(FRAME);
    let document = rt.state.editor.as_ref().expect("still editing");
    assert!(document.modified());
    assert!(
        rt.state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("every texel exact")),
        "{:?}",
        rt.state.notice
    );
    assert!(
        !rt.state
            .stage
            .shown
            .as_ref()
            .expect("on stage")
            .edited
            .is_empty(),
        "the model shows it"
    );
    assert!(
        rt.find("editor/edited").is_some(),
        "the title row says it's edited"
    );
    command(&mut rt, 'z', false);
    assert!(
        !rt.state.editor.as_ref().expect("still editing").modified(),
        "undone back to as it opened"
    );
    command(&mut rt, 'z', true);
    assert!(
        rt.state.editor.as_ref().expect("still editing").modified(),
        "and redone"
    );
}

#[test]
fn a_png_of_the_wrong_size_is_turned_away() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let small = png(files.path(), 4, [0, 0, 255, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &small));
    let notice = rt.state.notice.clone().expect("it says why");
    assert!(
        notice.error && notice.text.contains("is 4×4; this texture is 8×8"),
        "{}",
        notice.text
    );
    assert!(!rt.state.editor.as_ref().expect("still editing").modified());
}

#[test]
fn leaving_with_unsaved_edits_asks_and_discarding_puts_the_costume_back() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    press_escape(&mut rt);
    assert_eq!(
        rt.state
            .editor
            .as_ref()
            .and_then(|document| document.pending),
        Some(crate::editor::Pending::Leave)
    );
    settle(&mut rt);
    tap(&mut rt, "prompt/discard");
    assert!(rt.state.editor.is_none());
    assert!(
        rt.state.stage.shown.is_none(),
        "the stage lets the edited costume go"
    );
    let slot: melee_dat::MeleeSlot = "PlFcRe.dat".parse().expect("a slot");
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(
        game.read_slot(slot).expect("read back"),
        tgg_app::test_dat::model(),
        "the costume is as it was"
    );
}

/// The app again on `iso` with the data in `data`, as the next launch
/// finds it, offering what the last left.
fn relaunch(iso: &Path, data: &Path) -> Runtime<App> {
    let app = App::new(
        game_at(iso),
        Settings::default(),
        Data::in_folder(data),
        false,
    );
    let mut rt = Runtime::new(app, shell::view, WINDOW, Env::new(true, 2.0));
    rt.on_key = Some(crate::keys::on_key);
    rt.update(|app, _| crate::editor::draft::offer(app));
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

#[test]
fn unsaved_edits_left_by_a_closed_window_reopen_next_launch() {
    let (folder, iso) = melee_iso();
    let (mut rt, data) = runtime(game_at(&iso));
    make_a_skin(&mut rt, "Falco");
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    let draft = data.path().join("draft").join("PlFcRe.dat");
    let edited = std::fs::read(&draft).expect("the edit is kept as it's made");
    // Undone back to as it opened, nothing's unsaved: no draft.
    command(&mut rt, 'z', false);
    assert!(!draft.exists());
    command(&mut rt, 'z', true);
    assert!(draft.exists());
    // The window closes without asking.
    drop(rt);

    let mut rt = relaunch(&iso, data.path());
    assert!(rt.find("shell/notice/reopen").is_some(), "it's offered");
    tap(&mut rt, "shell/notice/reopen");
    let document = rt.state.editor.as_ref().expect("Falco's costume opens");
    assert_eq!(document.slot.file_name(), "PlFcRe.dat");
    assert!(document.modified(), "its edits are still unsaved");
    let open = document.listing.document.as_ref().expect("open");
    assert_eq!(open.bytes(), edited.as_slice());
    assert_eq!(rt.state.nav.place, Place::Skins);
    assert!(rt.state.notice.is_none());

    // Leaving and discarding them throws the draft away.
    press_escape(&mut rt);
    settle(&mut rt);
    tap(&mut rt, "prompt/discard");
    assert!(rt.state.editor.is_none());
    assert!(!draft.exists());
    drop(folder);
}

#[test]
fn a_draft_can_be_discarded_without_opening_it() {
    let (mut rt, _disc, data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    let iso = rt
        .state
        .game
        .as_ref()
        .expect("a game")
        .game()
        .path()
        .to_owned();
    drop(rt);
    let mut rt = relaunch(&iso, data.path());
    tap(&mut rt, "shell/notice/discard");
    assert!(rt.state.notice.is_none());
    assert!(rt.state.editor.is_none());
    assert!(!data.path().join("draft").exists());
}

#[test]
fn saving_clears_the_draft() {
    let (mut rt, _disc, data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    assert!(data.path().join("draft").join("PlFcRe.dat").exists());
    settle(&mut rt);
    tap(&mut rt, "editor/install");
    assert!(!data.path().join("draft").exists());
}

#[test]
fn save_and_install_writes_the_edited_costume_into_the_game() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    settle(&mut rt);
    tap(&mut rt, "editor/install");
    let document = rt.state.editor.as_ref().expect("still editing");
    assert!(!document.modified(), "saved");
    let edited = document
        .listing
        .document
        .as_ref()
        .expect("open")
        .bytes()
        .to_vec();
    let slot = document.slot;
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(
        game.read_slot(slot).expect("read back"),
        edited,
        "the disc holds the edit"
    );
    let skins = rt.state.library.skins();
    assert_eq!(skins.len(), 1);
    assert_eq!(skins[0].name, "Falco · Red (edited)");
}

#[test]
fn the_prompt_can_save_then_go_where_the_player_was_going() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    tap(&mut rt, Place::Mods.key());
    assert_eq!(rt.state.nav.place, Place::Skins, "it asks before going");
    settle(&mut rt);
    tap(&mut rt, "prompt/save");
    assert_eq!(rt.state.nav.place, Place::Mods);
    assert!(rt.state.editor.is_none());
    assert_eq!(rt.state.library.skins().len(), 1);
}

#[test]
fn a_texture_exports_as_the_game_draws_it() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let out = files.path().join("out.png");
    rt.update(|app, _| crate::editor::files::export_to(app, 0, &out));
    let image = image::open(&out).expect("a PNG").to_rgba8();
    assert_eq!(image.dimensions(), (8, 8));
}

/// The scene's ids for the open costume's texture `index`, as a pick reports
/// a surface drawing it.
fn drawn(rt: &Runtime<App>, index: usize) -> Vec<dat_parser::hsd::scene::HsdTextureSourceId> {
    let document = rt.state.editor.as_ref().expect("editing");
    document
        .listing
        .document
        .as_ref()
        .expect("readable")
        .textures()[index]
        .uses
        .clone()
}

#[test]
fn a_click_on_the_model_asks_to_pick_and_a_drag_doesnt() {
    let (mut rt, _iso, _data) = editing();
    let (x, y) = rt.find("stage/viewport").expect("the stage shows");
    rt.pointer_down(x, y);
    rt.pointer_move(x + 2.0, y + 1.0);
    rt.pointer_up(x + 2.0, y + 1.0);
    rt.frame(FRAME);
    let pick = rt
        .state
        .stage
        .shown
        .as_ref()
        .expect("on the stage")
        .pick
        .expect("a click picks");
    assert!(
        pick.iter().all(|fraction| (0.0..=1.0).contains(fraction)),
        "{pick:?} is on the plate"
    );

    rt.state.stage.shown.as_mut().expect("on the stage").pick = None;
    rt.pointer_down(x, y);
    rt.pointer_move(x + 40.0, y);
    rt.pointer_up(x + 40.0, y);
    rt.frame(FRAME);
    assert!(
        rt.state
            .stage
            .shown
            .as_ref()
            .expect("on the stage")
            .pick
            .is_none(),
        "a drag turns the camera instead"
    );
}

#[test]
fn picking_a_surface_selects_its_texture_and_picking_it_again_clears_it() {
    let (mut rt, _iso, _data) = editing();
    let texture = drawn(&rt, 0);
    rt.update(|app, _| crate::editor::actions::select(app, None));
    // A surface blending the texture twice offers it once.
    rt.state.stage.picked = Some(vec![texture.clone(), texture.clone()]);
    poll(&mut rt);
    assert_eq!(rt.state.editor.as_ref().expect("editing").selected, Some(0));
    assert!(
        !rt.state
            .stage
            .shown
            .as_ref()
            .expect("on the stage")
            .highlight
            .is_empty(),
        "the model shows where it draws"
    );

    rt.state.stage.picked = Some(vec![texture]);
    poll(&mut rt);
    assert_eq!(
        rt.state.editor.as_ref().expect("editing").selected,
        None,
        "the selected one again clears it"
    );
    assert_eq!(
        rt.state.notice.as_ref().map(|notice| notice.text.as_str()),
        Some("Selection cleared")
    );
}

#[test]
fn a_click_that_misses_the_model_changes_nothing() {
    let (mut rt, _iso, _data) = editing();
    let before = rt.state.editor.as_ref().expect("editing").selected;
    rt.state.stage.picked = Some(Vec::new());
    poll(&mut rt);
    assert_eq!(rt.state.editor.as_ref().expect("editing").selected, before);
}

/// Run the app's poll as the window does between frames: it hands on what
/// the viewport read back.
fn poll(rt: &mut Runtime<App>) {
    rt.update(|app, tx| {
        crate::jobs::poll(app, FRAME, tx);
    });
    rt.frame(FRAME);
}

#[test]
fn a_texture_edited_in_the_image_editor_comes_back_with_each_save() {
    let (mut rt, _disc, _data) = editing();
    // A command that starts and exits, standing in for the player's editor.
    rt.state.settings.external_editor = Some("true".into());
    tap(&mut rt, "editor/external");
    let path = {
        let document = rt.state.editor.as_ref().expect("editing");
        document
            .external
            .watched
            .get(&0)
            .expect("the selected texture is watched")
            .path
            .clone()
    };
    let exported = std::fs::read(&path).expect("its PNG is written");
    assert!(
        !rt.state.editor.as_ref().expect("editing").modified(),
        "opening it there changes nothing yet"
    );

    // The player paints it red and saves.
    let red = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
    red.save(&path).expect("save");
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(later)
        .expect("touch");
    poll(&mut rt);
    assert!(
        rt.state.editor.as_ref().expect("editing").modified(),
        "the save came back as an edit"
    );
    assert!(
        !rt.state
            .stage
            .shown
            .as_ref()
            .expect("on stage")
            .edited
            .is_empty(),
        "and shows on the model"
    );

    // Undo puts the PNG back as the document has it, without importing it again.
    command(&mut rt, 'z', false);
    assert!(!rt.state.editor.as_ref().expect("editing").modified());
    assert_eq!(
        std::fs::read(&path).expect("still there"),
        exported,
        "the PNG matches the document again"
    );
    assert!(
        rt.state.editor.as_ref().expect("editing").external.watched[&0]
            .saved()
            .is_none(),
        "the rewrite isn't a save"
    );

    tap(&mut rt, "editor/done");
    assert!(!path.exists(), "leaving the costume deletes its PNG");
}

#[test]
fn ctrl_shift_e_opens_the_selected_texture_in_the_image_editor() {
    let (mut rt, _disc, _data) = editing();
    rt.state.settings.external_editor = Some("true".into());
    command(&mut rt, 'e', true);
    assert!(
        rt.state
            .editor
            .as_ref()
            .expect("editing")
            .external
            .watched
            .contains_key(&0)
    );
    tap(&mut rt, "editor/done");
}

fn first_color(rt: &Runtime<App>) -> [u8; 4] {
    rt.state.editor.as_ref().expect("editing").colors[0].swatches[0].rgba
}

#[test]
fn a_color_changed_in_the_colors_pane_shows_on_the_model_and_undoes() {
    let (mut rt, _iso, _data) = editing();
    assert_eq!(
        first_color(&rt),
        [96, 224, 96, 255],
        "the costume's green vertices"
    );
    let generation = rt.state.stage.shown.as_ref().expect("on stage").generation;
    tap(&mut rt, "editor/color/0/0");
    settle(&mut rt);
    assert!(rt.find("editor/pick").is_some(), "its sliders open");
    // Drag red to its far end.
    let (x, y) = rt.find("editor/pick/R").expect("the red slider");
    rt.pointer_down(x, y);
    rt.pointer_move(x + 400.0, y);
    rt.pointer_up(x + 400.0, y);
    rt.frame(FRAME);
    tap(&mut rt, "editor/pick/apply");
    // Its vertices keep 5 bits of red, so full red reads back as 248.
    assert_eq!(
        first_color(&rt),
        [248, 224, 96, 255],
        "every vertex of that color changed, as the game reads it"
    );
    assert!(rt.state.editor.as_ref().expect("editing").modified());
    assert!(
        rt.state
            .editor
            .as_ref()
            .expect("editing")
            .color_pick
            .is_none(),
        "the sliders close"
    );
    assert_ne!(
        rt.state.stage.shown.as_ref().expect("on stage").generation,
        generation,
        "the model is drawn afresh"
    );
    assert!(
        rt.state
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("#60E060 to #FFE060")),
        "{:?}",
        rt.state.notice
    );

    command(&mut rt, 'z', false);
    assert_eq!(
        first_color(&rt),
        [96, 224, 96, 255],
        "undone, the list reads the green again"
    );
    assert!(!rt.state.editor.as_ref().expect("editing").modified());
}

#[test]
fn shifting_a_groups_hue_turns_all_its_colors() {
    let (mut rt, _iso, _data) = editing();
    let before: Vec<[u8; 4]> = rt.state.editor.as_ref().expect("editing").colors[0]
        .swatches
        .iter()
        .map(|swatch| swatch.rgba)
        .collect();
    tap(&mut rt, "editor/hue/0");
    settle(&mut rt);
    rt.update(|app, _| crate::editor::colors::set_hue(app, 120.0));
    rt.frame(FRAME);
    tap(&mut rt, "editor/pick/apply");
    let after: Vec<[u8; 4]> = rt.state.editor.as_ref().expect("editing").colors[0]
        .swatches
        .iter()
        .map(|swatch| swatch.rgba)
        .collect();
    let mut expected: Vec<[u8; 4]> = before
        .iter()
        .map(|&rgba| tgg_app::colors::turn_hue(rgba, 120.0))
        .collect();
    let mut sorted = after.clone();
    expected.sort_unstable();
    sorted.sort_unstable();
    // Within what the vertices' 5- and 6-bit channels keep.
    let close = sorted
        .iter()
        .zip(&expected)
        .all(|(got, want)| got.iter().zip(want).all(|(a, b)| a.abs_diff(*b) <= 8));
    assert!(
        close,
        "each color turned a third of the way round: {sorted:?}, wanted about {expected:?}"
    );
}

#[test]
fn cancelling_a_color_change_leaves_the_costume_alone() {
    let (mut rt, _iso, _data) = editing();
    tap(&mut rt, "editor/color/0/0");
    settle(&mut rt);
    rt.update(|app, _| crate::editor::colors::set_channel(app, 0, 0.0));
    rt.frame(FRAME);
    tap(&mut rt, "editor/pick/cancel");
    assert!(rt.find("editor/pick").is_none());
    assert_eq!(first_color(&rt), [96, 224, 96, 255]);
    assert!(!rt.state.editor.as_ref().expect("editing").modified());
}

/// The app on Mods, against a fake textures.gg, once its first answers are
/// in. The fake is handed back to publish to as a test goes.
fn on_mods(net: std::sync::Arc<tgg_mod::FakeNet>) -> (Runtime<App>, TempDir, TempDir) {
    let (disc, iso) = melee_iso();
    let (mut rt, data) = runtime(game_at(&iso));
    rt.state.mods.net = net;
    tap(&mut rt, Place::Mods.key());
    jobs_until(&mut rt, |app| {
        app.mods.registry != crate::mods::Registry::Asking
            && app.mods.disc != crate::mods::Disc::Looking
    });
    (rt, disc, data)
}

/// Hand finished jobs to the app until `done` holds, for up to 20 seconds.
fn jobs_until(rt: &mut Runtime<App>, done: impl Fn(&App) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !done(&rt.state) {
        assert!(
            std::time::Instant::now() < deadline,
            "the jobs finished in time"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        poll(rt);
    }
    settle(rt);
}

/// On Mods with tgg-melee installed from the fake.
fn with_port(net: std::sync::Arc<tgg_mod::FakeNet>) -> (Runtime<App>, TempDir, TempDir) {
    let (mut rt, disc, data) = on_mods(net);
    tap(&mut rt, "mods/install-port");
    jobs_until(&mut rt, |app| {
        app.mods.port().is_some() && app.mods.registry == crate::mods::Registry::Answered
    });
    (rt, disc, data)
}

/// Scroll the element keyed `key` into view, then tap it.
fn tap_shown(rt: &mut Runtime<App>, key: &str) {
    let owned = key.to_owned();
    rt.update(move |_, tx| tx.scroll_into_view(owned.clone()));
    settle(rt);
    tap(rt, key);
}

#[test]
fn a_link_from_textures_gg_opens_its_set_once_tgg_melee_is_there_then_its_mod() {
    let (mut rt, _disc, _data) = on_mods(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    rt.update(|app, _| crate::links::open(app, "textures-gg://sets/competitive/"));
    poll(&mut rt);
    assert!(rt.state.link.is_some(), "it waits for tgg-melee");
    tap(&mut rt, "mods/install-port");
    jobs_until(&mut rt, |app| {
        app.link.is_none() && app.sets.get("competitive").is_some()
    });
    assert_eq!(rt.state.mods.chosen_set.as_deref(), Some("competitive"));
    assert!(
        rt.state
            .sets
            .get("competitive")
            .is_some_and(|set| set.locked),
        "the set arrives locked, as textures.gg keeps it"
    );
    assert!(rt.find("mods/set/make-own").is_some());
    rt.update(|app, _| crate::links::open(app, "textures-gg://mods/tgg.legal-stages"));
    settle(&mut rt);
    assert_eq!(
        rt.state
            .mods
            .selected
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("tgg.legal-stages")
    );
    assert_eq!(rt.state.mods.filter, crate::mods::Filter::All);
}

#[test]
fn a_new_mod_starts_from_the_sdk_in_a_folder_of_its_own() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    let mods = tempfile::tempdir().expect("temp");
    tap(&mut rt, "mods/new-mod");
    jobs_until(&mut rt, |app| app.new_mod.sdk.is_some());
    let parent = mods.path().to_owned();
    rt.update(move |app, _| {
        app.new_mod.name.text = "Neutral spawns".into();
        app.new_mod.parent = parent.clone();
        // The fake SDK ships the C template only.
        app.new_mod.kind = crate::newmod::Kind::Native;
    });
    settle(&mut rt);
    assert_eq!(
        rt.state.new_mod.id_text(),
        "neutral-spawns",
        "the id comes from the name"
    );
    tap(&mut rt, "newmod/create");
    jobs_until(&mut rt, |app| {
        !matches!(app.new_mod.stage, crate::newmod::Stage::Creating)
    });
    let crate::newmod::Stage::Created(created) = &rt.state.new_mod.stage else {
        panic!("it started");
    };
    let folder = mods.path().join("neutral-spawns");
    assert_eq!(created.dir, folder);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).expect("its manifest"))
            .expect("json");
    assert_eq!(manifest["id"], "neutral-spawns");
    assert_eq!(manifest["name"], "Neutral spawns");
    assert_eq!(created.example, "template", "the C template, as picked");
    assert!(folder.join("src").join("mod.c").is_file());
    assert!(
        rt.find("newmod/dev").is_some() && rt.find("newmod/open").is_some(),
        "what's next shows"
    );
    tap(&mut rt, "newmod/done");
    assert!(!rt.state.new_mod.open);
}

#[test]
fn new_mod_waits_for_tgg_melee() {
    let (rt, _disc, _data) = on_mods(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    assert!(
        rt.find("mods/new-mod").is_none(),
        "a mod starts from the installed tgg-melee's SDK"
    );
}

/// Put the textures.gg set `name`'s mods in the mods folder, as installing
/// them would, and read what's installed again.
fn install_mods(rt: &mut Runtime<App>, name: &str) {
    rt.update(|app, _| {
        let Some(catalog) = &app.mods.catalog else {
            return;
        };
        let Some(set) = app.mods.sets.iter().find(|set| set.name == name).cloned() else {
            return;
        };
        let plan = tgg_mod::plan_set(&set, catalog, &app.mods.local);
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let _ = tgg_mod::apply_cached(
            &*app.mods.net.clone(),
            &plan,
            &app.data.mods,
            None,
            &tgg_mod::no_progress,
            &cancel,
        );
        crate::mods::actions::read_disk(app);
    });
    jobs_until(rt, |app| {
        !app.mods.local.mods.is_empty()
            && app
                .sets
                .get(tgg_mod::sets::INSTALLED)
                .is_some_and(|set| !set.lines.is_empty())
    });
}

/// Make a set of the player's named `name` that builds on `links`, and show
/// it.
fn make_set(rt: &mut Runtime<App>, name: &str, links: &[&str]) {
    let wanted = name.to_owned();
    let for_job = wanted.clone();
    let links: Vec<String> = links.iter().map(|link| (*link).to_owned()).collect();
    rt.update(move |app, _| {
        let (data, local) = (app.data.tgg.clone(), app.mods.local.clone());
        let net = app.mods.net.clone();
        let Ok(mut set) = tgg_mod::sets::new(&data, &for_job) else {
            return;
        };
        for link in &links {
            match tgg_mod::sets::build_on(&*net, &data, &local, &set.key, link) {
                Ok(built) => set = built,
                Err(_) => return,
            }
        }
        let _ = tgg_mod::sets::save(&data, &set);
        crate::sets::actions::read(app);
    });
    jobs_until(rt, |app| app.sets.list.iter().any(|set| set.name == wanted));
    rt.update(|app, _| {
        if let Some(key) = app
            .sets
            .list
            .iter()
            .find(|set| set.name == wanted)
            .map(|set| set.key.clone())
        {
            crate::mods::actions::choose_set(app, &key);
        }
    });
    settle(rt);
}

#[test]
fn mods_lists_every_mod_and_the_filters_keep_their_tags() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    assert!(rt.find("mods/new-set").is_some(), "sets start with New set");
    assert!(
        rt.find("mods/set/none").is_some(),
        "no set yet, so the panel says so"
    );
    assert!(
        rt.find("mods/catalog/tgg.netplay").is_some()
            && rt.find("mods/catalog/tgg.replay-recorder").is_some(),
        "every mod textures.gg offers"
    );
    tap(&mut rt, "mods/filter/Rules");
    settle(&mut rt);
    assert_eq!(rt.state.mods.filter, crate::mods::Filter::Rules);
    assert!(
        rt.find("mods/catalog/tgg.replay-recorder").is_none(),
        "the replay mods aren't rules"
    );
    assert!(rt.find("mods/catalog/tgg.netplay").is_some());
}

#[test]
fn sets_tab_lists_official_and_local_sets_and_adds_one() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    tap(&mut rt, "mods/tab/sets");
    settle(&mut rt);
    assert_eq!(rt.state.mods.tab, crate::mods::Tab::Sets);
    assert!(
        rt.find("mods/sets/official/competitive/add").is_some()
            && rt.find("mods/sets/official/replays/add").is_some(),
        "official sets from textures.gg are browsable and addable"
    );
    tap(&mut rt, "mods/sets/official/competitive/add");
    jobs_until(&mut rt, |app| {
        app.sets.list.iter().any(|set| set.key == "competitive")
    });
    settle(&mut rt);
    assert!(
        rt.find("mods/sets/mine/competitive").is_some(),
        "the added set appears under Your sets"
    );
    assert!(
        rt.find("mods/sets/official/competitive/added").is_some(),
        "the official row switches from Add set to Added"
    );
    assert_eq!(rt.state.mods.chosen_set.as_deref(), Some("competitive"));
    tap(&mut rt, "mods/sets/mine/competitive/use");
    jobs_until(&mut rt, |app| app.sets.active == "competitive");
    settle(&mut rt);
    assert!(
        rt.find("mods/sets/mine/competitive/using").is_some(),
        "activating a set marks it Using"
    );
    tap(&mut rt, "mods/tab/mods");
    settle(&mut rt);
    assert_eq!(rt.state.mods.tab, crate::mods::Tab::Mods);
    assert!(rt.find("mods/catalog/tgg.netplay").is_some());
}

#[test]
fn a_textures_gg_set_arrives_locked_and_makes_your_own() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    rt.update(|app, _| crate::mods::actions::add_set(app, "competitive"));
    jobs_until(&mut rt, |app| {
        app.sets.list.iter().any(|set| set.name == "Competitive")
    });
    assert_eq!(
        rt.state
            .sets
            .get("competitive")
            .map(|set| (set.locked, set.name.clone())),
        Some((true, "Competitive".into()))
    );
    assert_eq!(rt.state.mods.chosen_set.as_deref(), Some("competitive"));
    assert!(rt.find("mods/set/make-own").is_some());
    tap(&mut rt, "mods/set/make-own");
    jobs_until(&mut rt, |app| {
        app.sets.list.iter().any(|set| set.name == "My Competitive")
    });
    let mine = rt
        .state
        .sets
        .list
        .iter()
        .find(|set| set.name == "My Competitive")
        .expect("made");
    assert!(!mine.locked && mine.builds_on().any(|link| link == "competitive"));
    assert_eq!(rt.state.mods.chosen_set.as_deref(), Some(mine.key.as_str()));
    tap(&mut rt, "mods/set/more");
    settle(&mut rt);
    assert!(
        rt.find("mods/set/rename").is_some() && rt.find("mods/set/delete").is_some(),
        "a set of the player's can be renamed and deleted"
    );
}

#[test]
fn add_on_a_locked_set_makes_your_own_first_then_adds() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    rt.update(|app, _| crate::mods::actions::add_set(app, "competitive"));
    jobs_until(&mut rt, |app| {
        app.sets.list.iter().any(|set| set.name == "Competitive")
    });
    settle(&mut rt);
    tap_shown(&mut rt, "mods/catalog/tgg.legal-stages/add");
    jobs_until(&mut rt, |app| {
        app.sets.list.iter().any(|set| set.name == "My Competitive")
    });
    let mine = rt
        .state
        .sets
        .list
        .iter()
        .find(|set| set.name == "My Competitive")
        .expect("made");
    let id = "tgg.legal-stages".parse::<tgg_mod::ModId>().expect("an id");
    assert!(
        mine.mod_line(&id).is_some_and(|line| !line.left_out),
        "the mod is its own line in the new set"
    );
    assert!(
        rt.state.notice.as_ref().is_some_and(|notice| notice
            .text
            .starts_with("Made My Competitive from Competitive")),
        "{:?}",
        rt.state.notice
    );
}

#[test]
fn a_players_set_leaves_a_mod_out_and_puts_it_back() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    make_set(&mut rt, "Friday night", &["competitive"]);
    let id = "tgg.quick-start".parse::<tgg_mod::ModId>().expect("an id");
    jobs_until(&mut rt, |app| {
        app.sets.plan("friday-night").is_some_and(|plan| {
            plan.mods.iter().any(|m| {
                m.group
                    == tgg_mod::sets::Group::FromSet {
                        set: "competitive".into(),
                        title: "Competitive".into(),
                    }
            })
        })
    });
    tap_shown(&mut rt, "mods/set/tgg.quick-start/leave-out");
    jobs_until(&mut rt, |app| {
        app.sets
            .get("friday-night")
            .and_then(|set| set.mod_line(&id))
            .is_some_and(|line| line.left_out)
    });
    jobs_until(&mut rt, |app| {
        app.sets.plan("friday-night").is_some_and(|plan| {
            plan.mods
                .iter()
                .any(|m| m.id == id && m.group == tgg_mod::sets::Group::LeftOut)
        })
    });
    tap_shown(&mut rt, "mods/set/tgg.quick-start/put-back");
    jobs_until(&mut rt, |app| {
        app.sets
            .get("friday-night")
            .and_then(|set| set.mod_line(&id))
            .is_some_and(|line| !line.left_out)
    });
}

#[test]
fn added_in_the_list_takes_a_mod_out_as_the_set_has_it() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    make_set(&mut rt, "Friday night", &["competitive"]);
    let from_set = "tgg.quick-start".parse::<tgg_mod::ModId>().expect("an id");
    let own = "tgg.replay-recorder"
        .parse::<tgg_mod::ModId>()
        .expect("an id");
    let line = |app: &App, id: &tgg_mod::ModId| {
        app.sets
            .get("friday-night")
            .and_then(|set| set.mod_line(id))
            .map(|line| line.left_out)
    };
    jobs_until(&mut rt, |app| app.sets.plan("friday-night").is_some());
    tap_shown(&mut rt, "mods/catalog/tgg.replay-recorder/add");
    jobs_until(&mut rt, |app| line(app, &own) == Some(false));
    jobs_until(&mut rt, |app| {
        app.sets
            .plan("friday-night")
            .is_some_and(|plan| plan.mods.iter().any(|m| m.id == own))
    });
    // Its own line goes; one of the set it builds on is left out.
    tap_shown(&mut rt, "mods/catalog/tgg.replay-recorder/remove");
    jobs_until(&mut rt, |app| {
        line(app, &own).is_none()
            && app
                .sets
                .plan("friday-night")
                .is_some_and(|plan| plan.mods.iter().all(|m| m.id != own))
    });
    tap_shown(&mut rt, "mods/catalog/tgg.quick-start/remove");
    jobs_until(&mut rt, |app| line(app, &from_set) == Some(true));
}

#[test]
fn installed_mods_switch_a_mod_off_in_the_folder() {
    let (mut rt, _disc, data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    install_mods(&mut rt, "competitive");
    rt.update(|app, _| {
        crate::mods::actions::choose_set(app, tgg_mod::sets::INSTALLED);
    });
    settle(&mut rt);
    assert!(
        rt.find("mods/set/tgg.quick-start/switch").is_some(),
        "the mods folder's mods carry switches"
    );
    tap_shown(&mut rt, "mods/set/tgg.quick-start/switch");
    settle(&mut rt);
    assert!(
        data.path().join("mods/.tgg.quick-start").is_dir(),
        "a turned-off mod's folder is hidden from the game"
    );
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn find_a_match_starts_the_set_and_cancel_ends_it() {
    let (mut rt, _disc, data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    jobs_until(&mut rt, |app| app.sets.active().is_some());
    rt.update(|app, _| app.signin.who = Some(crate::signed_in("ciao")));
    open_place(&mut rt, Place::Play);
    tap(&mut rt, "play/find");
    jobs_until(&mut rt, |app| app.mods.playing.is_some());
    assert!(
        tgg_mod::game_running(&data.path().join("mods")),
        "other tgg's see the game running"
    );
    assert!(rt.state.sets.session.is_some(), "Play knows how it started");
    assert!(
        rt.find("play/session").is_some() && rt.find("play/find").is_none(),
        "the button becomes the search, with Cancel"
    );
    tap(&mut rt, "play/session");
    jobs_until(&mut rt, |app| app.mods.playing.is_none());
    assert!(rt.state.sets.session.is_none(), "the session goes with it");
    assert!(rt.find("play/find").is_some(), "Find a match is back");
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn play_lists_the_meeting_points_queues_and_join_searches_with_its_mods() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    jobs_until(&mut rt, |app| app.sets.active().is_some());
    rt.update(|app, _| app.signin.who = Some(crate::signed_in("ciao")));
    open_place(&mut rt, Place::Play);
    jobs_until(&mut rt, |app| app.play.queues.is_some());
    assert!(
        rt.find("play/queue/competitive").is_some(),
        "the standing queue is listed: {:?}",
        rt.state.play.queues
    );

    tap(&mut rt, "play/queue/competitive/join");
    jobs_until(&mut rt, |app| app.mods.playing.is_some());
    assert_eq!(
        rt.state
            .sets
            .session
            .as_ref()
            .map(|session| session.name.as_str()),
        Some("Competitive"),
        "the session is the queue's"
    );
    let port = rt.state.mods.port().expect("tgg-melee").executable();
    let written = port.with_file_name(format!(
        "{}.env",
        port.file_name().expect("a file name").to_string_lossy()
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !written.is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "the game wrote its environment"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let env = std::fs::read_to_string(&written).expect("the game's environment");
    assert!(
        env.lines().any(|line| line == "TGG_DIRECT_CONNECT=search"),
        "the game searches: {env}"
    );
    let mods_dir = env
        .lines()
        .find_map(|line| line.strip_prefix("TGG_MODS_DIR="))
        .expect("the game is given its mods");
    let loaded = tgg_mod::local(std::path::Path::new(mods_dir), None).expect("its mods read");
    let legal = "tgg.legal-stages".parse().expect("an id");
    assert_eq!(
        loaded.get(&legal).map(|m| m.manifest.version.to_string()),
        Some("0.1.1".to_owned()),
        "it loads the queue's mods at the queue's versions"
    );
    tap(&mut rt, "play/session");
    jobs_until(&mut rt, |app| app.mods.playing.is_none());
}

#[test]
fn offline_mods_says_so_and_keeps_what_is_installed() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    net.set_offline(true);
    let (rt, _disc, _data) = on_mods(net);
    assert_eq!(rt.state.mods.registry, crate::mods::Registry::Offline);
    assert!(rt.find("mods/registry").is_some());
}

#[test]
fn every_mod_can_be_browsed_and_searched_before_tgg_melee_is_installed() {
    let (mut rt, _disc, _data) = on_mods(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    assert_eq!(
        rt.state.mods.offered().len(),
        10,
        "the newest release of each, from the newest tgg-melee's catalog"
    );
    assert!(rt.find("mods/catalog/tgg.netplay").is_some());
    assert!(
        rt.find("mods/catalog/tgg.netplay/add").is_none(),
        "installing waits for tgg-melee"
    );
    rt.update(|app, _| app.mods.search.text = "replay".into());
    rt.frame(FRAME);
    let found: Vec<String> = rt
        .state
        .mods
        .found()
        .iter()
        .map(|entry| entry.manifest.id.to_string())
        .collect();
    assert_eq!(found.len(), 3, "the three replay mods: {found:?}");
    assert!(found.iter().all(|id| id.starts_with("tgg.replay-")));
    assert!(
        rt.find("mods/catalog/tgg.netplay").is_none(),
        "the search hides the rest"
    );
    // A tag, and the publisher, find mods too.
    rt.update(|app, _| app.mods.search.text = "quality-of-life".into());
    rt.frame(FRAME);
    assert_eq!(rt.state.mods.found().len(), 3, "the replay mods' tag");
    rt.update(|app, _| app.mods.search.text = "textures.gg".into());
    rt.frame(FRAME);
    assert_eq!(rt.state.mods.found().len(), 10, "every mod's publisher");
    rt.update(|app, _| app.mods.search.text = "nothing like this".into());
    rt.frame(FRAME);
    assert!(rt.find("mods/all/empty").is_some());
}

/// Show Installed mods, where Add puts a mod in the mods folder.
fn on_installed(rt: &mut Runtime<App>) {
    rt.update(|app, _| {
        crate::mods::actions::choose_set(app, tgg_mod::sets::INSTALLED);
    });
    settle(rt);
}

#[test]
fn one_mod_installs_with_what_it_needs() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    // Native code is reviewed first.
    net.set_native("tgg.direct-connect");
    let (mut rt, _disc, _data) = with_port(net);
    on_installed(&mut rt);
    tap_shown(&mut rt, "mods/catalog/tgg.direct-connect/add");
    settle(&mut rt);
    let review = rt.state.mods.review.as_ref().expect("the plan shows first");
    let ids: Vec<String> = review
        .plan
        .steps
        .iter()
        .map(|step| step.id.to_string())
        .collect();
    assert!(
        ids.contains(&"tgg.direct-connect".to_owned()) && ids.contains(&"tgg.netplay".to_owned()),
        "and what it needs: {ids:?}"
    );
    tap(&mut rt, "mods/review/confirm");
    let direct: tgg_mod::ModId = "tgg.direct-connect".parse().expect("an id");
    jobs_until(&mut rt, |app| {
        app.mods.work.is_none()
            && !app.mods.local.mods.is_empty()
            && app
                .sets
                .get(tgg_mod::sets::INSTALLED)
                .is_some_and(|set| set.mod_line(&direct).is_some())
    });
    assert_eq!(rt.state.mods.local.mods.len(), 2);
    assert!(
        rt.find("mods/set/tgg.direct-connect/switch").is_some(),
        "installed, its row in the mods folder has its switch"
    );
    assert_eq!(
        rt.state.mods.selected, None,
        "pressing Add didn't open the details"
    );
}

#[test]
fn a_mod_of_only_files_installs_without_the_review() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    on_installed(&mut rt);
    tap_shown(&mut rt, "mods/catalog/tgg.direct-connect/add");
    settle(&mut rt);
    assert!(
        rt.state.mods.review.is_none(),
        "no code of its own, nothing to review"
    );
    let direct: tgg_mod::ModId = "tgg.direct-connect".parse().expect("an id");
    jobs_until(&mut rt, |app| {
        app.mods.work.is_none() && app.mods.local.get(&direct).is_some()
    });
    assert_eq!(
        rt.state.mods.local.mods.len(),
        2,
        "with what it needs: {:?}",
        rt.state.mods.local.mods
    );
}

#[test]
fn a_mods_details_slide_in_and_install_it() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    net.set_native("tgg.replay-browser");
    let (mut rt, _disc, _data) = with_port(net);
    on_installed(&mut rt);
    narrow(&mut rt);
    tap_shown(&mut rt, "mods/catalog/tgg.replay-browser");
    settle(&mut rt);
    assert_eq!(
        rt.state
            .mods
            .selected
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("tgg.replay-browser")
    );
    let width = rt
        .presented("mods/details")
        .map_or(0.0, |(rect, _)| rect[2]);
    assert!(
        width > 300.0,
        "the pane opens to its column, not {width} pt"
    );
    assert!(rt.find("mods/details/body").is_some());
    tap(&mut rt, "mods/details/install");
    settle(&mut rt);
    tap(&mut rt, "mods/review/confirm");
    jobs_until(&mut rt, |app| {
        app.mods.work.is_none() && !app.mods.local.mods.is_empty()
    });
    assert!(
        rt.find("mods/details/switch").is_some(),
        "installed, the pane shows its switch"
    );
    tap(&mut rt, "mods/details/close");
    settle(&mut rt);
    let width = rt
        .presented("mods/details")
        .map_or(0.0, |(rect, _)| rect[2]);
    assert!(
        width < 0.5,
        "closed, the pane narrows to nothing, not {width} pt"
    );
}

#[test]
fn rows_say_who_publishes_each_mod_and_the_details_bring_its_page() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    assert_eq!(
        rt.state.mods.listings.len(),
        10,
        "the registry's listing of every mod"
    );
    let netplay: tgg_mod::ModId = "tgg.netplay".parse().expect("an id");
    assert!(!rt.state.mods.listings[&netplay].author.is_empty());
    // Icons arrive on threads of their own.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while rt
        .state
        .mods
        .icon(&netplay)
        .is_none_or(|pixels| format!("{pixels:?}").contains("0x0"))
    {
        assert!(std::time::Instant::now() < deadline, "the icon arrives");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    tap_shown(&mut rt, "mods/catalog/tgg.netplay");
    jobs_until(&mut rt, |app| {
        app.mods.details.get(&netplay).is_some_and(Option::is_some)
    });
    assert!(
        rt.find("mods/details/body").is_some(),
        "its details, with who publishes it"
    );
    assert!(rt.find("mods/details/readme").is_some(), "its README");
    assert!(
        rt.find("mods/details/releases").is_some(),
        "and its releases"
    );
}

#[test]
fn a_mod_not_built_for_this_version_is_listed_and_says_so() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net);
    // The registry lists a mod with no build for this game yet.
    rt.update(|app, _| {
        let mut summary = app
            .mods
            .listings
            .values()
            .next()
            .expect("a listing")
            .clone();
        summary.id = "tgg.someday".parse().expect("an id");
        summary.name = "Someday".into();
        app.mods.listings.insert(summary.id.clone(), summary);
    });
    rt.frame(FRAME);
    assert!(rt.find("mods/all/unbuilt").is_some() && rt.find("mods/listed/tgg.someday").is_some());
    tap_shown(&mut rt, "mods/listed/tgg.someday");
    settle(&mut rt);
    assert!(
        rt.find("mods/details/unbuilt").is_some(),
        "its details say it waits for a build"
    );
    assert!(rt.find("mods/details/install").is_none());
}

#[test]
fn a_long_readme_and_release_history_show_their_start_until_asked() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    let netplay: tgg_mod::ModId = "tgg.netplay".parse().expect("an id");
    tap_shown(&mut rt, "mods/catalog/tgg.netplay");
    jobs_until(&mut rt, |app| {
        app.mods.details.get(&netplay).is_some_and(Option::is_some)
    });
    // Say its README and history are long.
    rt.update(|app, _| {
        let Some(Some(page)) = app
            .mods
            .details
            .get_mut(&"tgg.netplay".parse().expect("an id"))
        else {
            return;
        };
        page.readme = Some((0..30).map(|n| format!("Paragraph {n}.\n\n")).collect());
        let release = page.releases[0].clone();
        page.releases = vec![release; 12];
    });
    settle(&mut rt);
    assert!(
        rt.find("mods/details/readme/fold").is_some()
            && rt.find("mods/details/releases/fold").is_some()
    );
    let readme = crate::mods::Expand::Readme;
    tap_shown(&mut rt, "mods/details/readme/fold");
    assert!(rt.state.mods.expanded.contains(&readme), "all of it shows");
    assert!(
        rt.find("mods/details/readme/fold").is_some(),
        "and can be cut short again"
    );
    tap_shown(&mut rt, "mods/details/readme/fold");
    assert!(!rt.state.mods.expanded.contains(&readme));
}

#[test]
fn official_mods_say_so_and_come_first() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    let netplay: tgg_mod::ModId = "tgg.netplay".parse().expect("an id");
    assert!(
        rt.state.mods.official(&netplay),
        "the fake's first-party mods are official"
    );
    // Their details say so, and they come first in the list.
    tap_shown(&mut rt, "mods/catalog/tgg.netplay");
    settle(&mut rt);
    assert!(rt.find("mods/details/body").is_some());
    // Say one isn't: it sorts after the official ones.
    rt.update(|app, _| {
        let id: tgg_mod::ModId = "tgg.competitive-rules".parse().expect("an id");
        if let Some(summary) = app.mods.listings.get_mut(&id) {
            summary.publisher.official = false;
        }
    });
    let ids: Vec<String> = rt
        .state
        .mods
        .offered()
        .iter()
        .map(|entry| entry.manifest.id.to_string())
        .collect();
    assert_eq!(
        ids.last().map(String::as_str),
        Some("tgg.competitive-rules"),
        "{ids:?}"
    );
}

#[test]
fn versions_install_switch_and_remove_and_an_installed_newer_one_is_still_an_update() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    net.publish_port("0.6.0");
    rt.update(|app, _| crate::mods::actions::fetch(app));
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    rt.update(|app, _| crate::mods::actions::show_versions(app, true));
    settle(&mut rt);
    assert!(
        rt.find("mods/versions/0.6.0/install").is_some(),
        "the newer release offers to install"
    );
    assert!(
        rt.find("mods/versions/0.5.0/use").is_none(),
        "the one in use has nothing to do"
    );
    tap(&mut rt, "mods/versions/0.6.0/install");
    jobs_until(&mut rt, |app| {
        app.mods.port().is_some_and(|port| port.version == "0.6.0")
            && app.mods.registry == crate::mods::Registry::Answered
    });
    tap(&mut rt, "mods/versions/0.5.0/use");
    settle(&mut rt);
    assert_eq!(
        rt.state.mods.port().map(|port| port.version.as_str()),
        Some("0.5.0")
    );
    tap(&mut rt, "mods/versions/done");
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    assert_eq!(
        rt.state.mods.port_update().map(|release| release.version),
        Some("0.6.0".to_owned()),
        "0.6.0, installed, is still newer"
    );
    // Updating to an installed version downloads nothing.
    rt.update(|app, _| crate::mods::actions::install_port(app));
    assert!(
        rt.state.mods.work.is_none(),
        "using an installed version downloads nothing"
    );
    assert_eq!(
        rt.state.mods.port().map(|port| port.version.as_str()),
        Some("0.6.0")
    );
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    rt.update(|app, _| crate::mods::actions::show_versions(app, true));
    settle(&mut rt);
    tap(&mut rt, "mods/versions/0.5.0/remove");
    assert_eq!(rt.state.mods.ports.len(), 1, "an older version can go");
}

#[test]
fn settings_versions_opens_inline_without_stacking_dialogs() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    net.publish_port("0.6.0");
    rt.update(|app, _| crate::mods::actions::fetch(app));
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    tap(&mut rt, "sidebar/settings");
    settle(&mut rt);
    assert!(rt.find("settings").is_some());
    assert!(rt.find("settings/versions").is_some());
    assert!(rt.find("mods/versions/scrim").is_none());

    tap(&mut rt, "settings/versions");
    settle(&mut rt);
    // Single Settings dialog swaps inline to Versions: no second modal scrim,
    // and Versions controls are directly interactive.
    assert!(rt.find("settings").is_some());
    assert!(rt.find("mods/versions/scrim").is_none());
    assert!(rt.find("settings/versions").is_none());
    assert!(rt.find("mods/versions/0.6.0/install").is_some());

    tap(&mut rt, "mods/versions/back");
    settle(&mut rt);
    assert!(rt.find("settings").is_some());
    assert!(rt.find("settings/versions").is_some());
    assert!(rt.find("mods/versions/0.6.0/install").is_none());

    tap(&mut rt, "settings/versions");
    settle(&mut rt);
    tap(&mut rt, "mods/versions/done");
    settle(&mut rt);
    assert!(!rt.state.preferences.open);
    assert!(!rt.state.mods.versions);
    assert!(rt.find("settings").is_none());
}

#[test]
fn a_newer_tgg_melee_is_offered_and_the_older_one_stays_to_go_back_to() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    install_mods(&mut rt, "replays");
    net.publish_port("0.6.0");
    rt.update(|app, _| crate::mods::actions::fetch(app));
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    assert_eq!(
        rt.state.mods.port_update().map(|release| release.version),
        Some("0.6.0".to_owned()),
        "0.6.0 is offered"
    );
    rt.update(|app, _| crate::mods::actions::install_version(app, "0.6.0"));
    jobs_until(&mut rt, |app| {
        app.mods.port().is_some_and(|port| port.version == "0.6.0")
            && app.mods.registry == crate::mods::Registry::Answered
    });
    assert_eq!(rt.state.mods.ports.len(), 2, "the older version stays");
    rt.update(|app, _| crate::mods::actions::use_port(app, "0.5.0"));
    assert_eq!(
        rt.state.mods.port().map(|port| port.version.as_str()),
        Some("0.5.0")
    );
}

#[test]
fn one_mod_updates_on_its_own_and_stays_in_its_set() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    install_mods(&mut rt, "replays");
    on_installed(&mut rt);
    net.publish_mod("tgg.replay-recorder", "Replay recorder", "0.1.1", &[]);
    net.publish_mod("tgg.replay-player", "Replay player", "0.2.1", &[]);
    rt.update(|app, _| crate::mods::actions::fetch(app));
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    tap_shown(&mut rt, "mods/set/tgg.replay-recorder/update");
    // Only that mod changes, so no review: it updates on its own.
    assert!(rt.state.mods.review.is_none());
    let target = rt
        .state
        .mods
        .work
        .as_ref()
        .and_then(|work| work.target.clone());
    assert_eq!(
        target.map(|id| id.to_string()).as_deref(),
        Some("tgg.replay-recorder")
    );
    rt.frame(FRAME);
    assert!(
        rt.find("mods/work").is_none(),
        "its progress isn't above the page"
    );
    jobs_until(&mut rt, |app| {
        app.mods.work.is_none()
            && app
                .mods
                .local
                .get(&"tgg.replay-recorder".parse().expect("an id"))
                .is_some_and(|m| m.update.is_none())
    });
    let local = &rt.state.mods.local;
    let recorder = local
        .get(&"tgg.replay-recorder".parse().expect("an id"))
        .expect("installed");
    assert_eq!(recorder.manifest.version.to_string(), "0.1.1");
    assert_eq!(recorder.sets(), ["replays"], "still part of Replays");
    assert!(
        !recorder.direct(),
        "not turned into one installed on its own"
    );
    let player = local
        .get(&"tgg.replay-player".parse().expect("an id"))
        .expect("installed");
    assert_eq!(
        player.manifest.version.to_string(),
        "0.2.0",
        "the other mod's update waits"
    );
    assert!(player.update.is_some());
}

#[test]
fn settings_keep_each_change_and_the_image_editor_when_closed() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    rt.key(KeyEvent {
        key: Key::Command(','),
        mods: Mods {
            cmd: true,
            ..Default::default()
        },
    });
    rt.frame(FRAME);
    assert!(rt.state.preferences.open, "Ctrl , opens Settings");
    settle(&mut rt);
    tap(&mut rt, "settings/updates/switch");
    assert_eq!(rt.state.settings.check_for_updates, Some(false));
    // Another app, by its command.
    tap(&mut rt, "settings/image-editor");
    settle(&mut rt);
    assert!(
        rt.find("settings/image-editor/menu").is_some(),
        "the select opens its menu"
    );
    tap(&mut rt, "settings/image-editor/other");
    assert!(rt.state.preferences.menu.is_none(), "choosing closes it");
    settle(&mut rt);
    assert!(
        rt.find("settings/image-editor/field").is_some(),
        "Other shows the field"
    );
    rt.update(|app, _| app.preferences.editor.text = "  my-editor --new  ".into());
    tap(&mut rt, "settings/done");
    assert!(!rt.state.preferences.open);
    assert_eq!(
        rt.state.settings.external_editor.as_deref(),
        Some("my-editor --new"),
        "kept as typed, trimmed"
    );
    // Reopened, it's still Other; the system's default is a press away.
    tap(&mut rt, "sidebar/settings");
    settle(&mut rt);
    assert!(rt.state.preferences.other);
    tap(&mut rt, "settings/image-editor");
    settle(&mut rt);
    tap(&mut rt, "settings/image-editor/default");
    assert_eq!(rt.state.settings.external_editor, None);
    assert!(rt.find("settings/image-editor/field").is_none());
}

#[test]
fn an_app_that_opens_pngs_is_one_press() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "sidebar/settings");
    // Whatever this computer has, an app found is offered by name.
    rt.update(|app, _| {
        app.preferences.apps = vec![tgg_app::image_apps::ImageApp {
            name: "Aseprite".into(),
            command: "aseprite".into(),
        }]
    });
    settle(&mut rt);
    tap(&mut rt, "settings/image-editor");
    settle(&mut rt);
    tap(&mut rt, "settings/image-editor/app/0");
    assert_eq!(
        rt.state.settings.external_editor.as_deref(),
        Some("aseprite")
    );
    assert!(!rt.state.preferences.other);
}

#[test]
fn a_report_shows_what_it_would_send_and_copies_it() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "sidebar/settings");
    settle(&mut rt);
    tap_shown(&mut rt, "settings/report/open");
    settle(&mut rt);
    assert!(
        !rt.state.preferences.open,
        "Settings gives way to the report"
    );
    let report = rt.state.report.as_ref().expect("the report shows");
    assert!(
        report.text.starts_with("tgg-studio "),
        "it says what's running: {}",
        report.text.lines().next().unwrap_or_default()
    );
    tap(&mut rt, "report/copy");
    assert_eq!(
        rt.state
            .report
            .as_ref()
            .map(|report| report.progress.clone()),
        Some(crate::report::Progress::Copied)
    );
    rt.update(|app, _| {
        crate::report::actions::sent(app, Ok("r-42".into()));
    });
    rt.frame(FRAME);
    assert!(
        rt.find("report/code").is_some() && rt.find("report/copy-code").is_some(),
        "once sent, the report code has its own copy button"
    );
    tap(&mut rt, "report/copy-code");
    assert_eq!(
        rt.state
            .report
            .as_ref()
            .map(|report| report.progress.clone()),
        Some(crate::report::Progress::Sent {
            id: "r-42".into(),
            copied: true,
        })
    );
    tap(&mut rt, "report/send");
    assert!(
        rt.state.report.is_none(),
        "once it's sent, the button closes it"
    );
}

#[test]
fn signing_in_shows_the_code_then_who_is_signed_in() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    assert!(rt.state.signin.who.is_none());
    // Signed out, the account row says so; pressing it starts signing in.
    rt.update(|app, _| {
        app.signin.login = Some(crate::signin::Login {
            code: Some("ABCD-EFGH".into()),
            link: Some("https://textures.gg/device?code=ABCDEFGH".into()),
            expires: None,
            interval: std::time::Duration::from_secs(5),
            next: None,
            polling: true,
            problem: None,
            device: None,
        });
    });
    settle(&mut rt);
    assert!(
        rt.find("signin/code").is_some() && rt.find("signin/open").is_some(),
        "the code, and the page to confirm it on"
    );
    // Not yet confirmed: it waits and asks again.
    rt.update(|app, tx| crate::signin::actions::polled(app, Ok(tgg_account::Poll::Pending), tx));
    assert!(
        rt.state
            .signin
            .login
            .as_ref()
            .is_some_and(|login| login.next.is_some() && !login.polling)
    );
    // Confirmed: signed in, the dialog closes, and the foot says who.
    let account = tgg_account::Account {
        handle: "ciao".into(),
        display_name: None,
        id: Some("1".into()),
    };
    rt.update(move |app, tx| {
        crate::signin::actions::polled(app, Ok(tgg_account::Poll::Done(account.clone())), tx)
    });
    settle(&mut rt);
    assert!(rt.state.signin.login.is_none());
    assert_eq!(
        rt.state
            .signin
            .who
            .as_ref()
            .map(|who| who.account.handle.as_str()),
        Some("ciao")
    );
    assert_eq!(
        rt.state.notice.as_ref().map(|notice| notice.text.as_str()),
        Some("Signed in as ciao.")
    );
    // The account row opens You: who's signed in, and signing out.
    tap(&mut rt, "sidebar/account");
    settle(&mut rt);
    assert_eq!(rt.state.nav.place, Place::Profile);
    assert!(rt.find("profile/sign-out").is_some());
    // Signed out, You goes, back to Play.
    rt.update(|app, tx| {
        crate::signin::actions::done(app, crate::signin::Done::SignedOut(Ok(())), tx);
    });
    settle(&mut rt);
    assert_eq!(rt.state.nav.place, Place::Play);
    assert!(rt.state.signin.who.is_none());
}

#[test]
fn an_expired_code_says_so_and_starts_again() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    rt.update(|app, tx| {
        app.signin.login = Some(crate::signin::Login::asking());
        crate::signin::actions::polled(app, Ok(tgg_account::Poll::Expired), tx);
    });
    settle(&mut rt);
    assert!(
        rt.find("signin/again").is_some(),
        "Start again, for a new code"
    );
    assert!(
        rt.state
            .signin
            .login
            .as_ref()
            .is_some_and(|login| login.problem.is_some())
    );
    tap(&mut rt, "signin/cancel");
    assert!(rt.state.signin.login.is_none());
}

/// A tgg-melee replay in `folder` whose game had Legal stages and
/// Competitive rules at 0.1.0, played by ciao (Peach) and Krowten (Fox).
fn tgg_replay_in(folder: &Path) -> PathBuf {
    use tgg_mod::slp::write;
    let profile = serde_json::json!({
        "format": 1,
        "game": {"name": "tgg-melee", "version": tgg_mod::FakeNet::PORT_VERSION, "layout": tgg_mod::FakeNet::LAYOUT},
        "mods": [
            {"id": "tgg.legal-stages", "version": "0.1.0"},
            {"id": "tgg.competitive-rules", "version": "0.1.0"}
        ]
    });
    let metadata = serde_json::json!({
        "playedOn": "tgg-melee",
        "players": {"0": {"names": {"netplay": "ciao"}}, "1": {"names": {"netplay": "Krowten"}}},
        "tgg": {"game": format!("tgg-melee {} {}", tgg_mod::FakeNet::PORT_VERSION, tgg_mod::FakeNet::LAYOUT), "profile": profile}
    });
    let events = write::events([3, 18, 0], 3, false, 1, &[]);
    let path = folder.join("Game_20260815T011035.slp");
    std::fs::write(&path, write::replay(&events, Some(&metadata))).expect("write a replay");
    path
}

/// On Replays, with tgg-melee from the fake and `folder` as its one
/// replay folder, once the replays are read and resolved.
fn on_replays(folder: &Path) -> (Runtime<App>, TempDir, TempDir) {
    let (mut rt, disc, data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    rt.update(|app, _| app.replays.folders = vec![("tgg-melee".into(), folder.to_owned())]);
    tap(&mut rt, crate::nav::Place::Replays.key());
    jobs_until(&mut rt, |app| {
        app.replays.scanned && !app.replays.refreshing && !app.replays.scanning
    });
    (rt, disc, data)
}

#[test]
fn replays_list_who_played_and_what_each_needs() {
    let replays = tempfile::tempdir().expect("temp");
    let path = tgg_replay_in(replays.path());
    let (rt, _disc, _data) = on_replays(replays.path());
    let item = rt
        .state
        .replays
        .items
        .first()
        .expect("the replay is listed");
    let names: Vec<String> = item
        .info
        .as_ref()
        .expect("read")
        .players
        .iter()
        .filter_map(|player| player.name.clone())
        .collect();
    assert_eq!(names, ["ciao", "Krowten"]);
    assert_eq!(
        item.status,
        crate::replays::Status::Needs {
            mods: 3,
            size: item_size(&rt),
            game: None
        },
        "its two mods and the replay player aren't installed"
    );
    assert!(
        rt.find(&format!("replays/{}/play", path.display()))
            .is_some()
    );
}

#[test]
fn installing_mods_plans_the_replays_again() {
    let replays = tempfile::tempdir().expect("temp");
    tgg_replay_in(replays.path());
    let (mut rt, _disc, _data) = on_replays(replays.path());
    tap(&mut rt, crate::nav::Place::Mods.key());
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    install_mods(&mut rt, "competitive");
    assert!(!rt.state.replays.planned, "what they need is out of date");
    tap(&mut rt, crate::nav::Place::Replays.key());
    jobs_until(&mut rt, |app| app.replays.planned);
}

/// The bytes the listed replay's mods download.
fn item_size(rt: &Runtime<App>) -> u64 {
    match &rt.state.replays.items[0].status {
        crate::replays::Status::Needs { size, .. } => *size,
        _ => 0,
    }
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn a_replay_that_needs_mods_is_reviewed_then_plays_in_its_own_setup() {
    let replays = tempfile::tempdir().expect("temp");
    let path = tgg_replay_in(replays.path());
    let (mut rt, _disc, data) = on_replays(replays.path());
    let iso = rt
        .state
        .game
        .as_ref()
        .expect("a game")
        .game()
        .path()
        .to_owned();
    rt.update(|app, _| crate::mods::actions::use_disc(app, iso.clone()));
    tap_shown(&mut rt, &format!("replays/{}/play", path.display()));
    // A replay that plays from the installed mods is checked for
    // withdrawals off the window's thread before its review shows.
    jobs_until(&mut rt, |app| app.replays.review.is_some());
    let review = rt
        .state
        .replays
        .review
        .as_ref()
        .expect("what it needs shows first");
    // Its two mods, and the replay player that plays it.
    assert_eq!(review.plan.mods.len(), 3);
    assert!(rt.find("replays/review/mod/tgg.replay-player").is_some());
    assert!(rt.find("replays/review/mod/tgg.legal-stages").is_some());
    tap(&mut rt, "replays/review/play");
    jobs_until(&mut rt, |app| {
        app.replays.preparing.is_none() && app.mods.playing.is_some()
    });
    assert!(
        rt.state.mods.local.mods.is_empty(),
        "the player's own mods are untouched"
    );
    assert_eq!(
        tgg_mod::profile_games(&data.path().join("tgg-data")).len(),
        1,
        "a profile game runs"
    );
    open_place(&mut rt, Place::Play);
    tap(&mut rt, "play/session");
    jobs_until(&mut rt, |app| app.mods.playing.is_none());
}

#[test]
fn a_sign_out_elsewhere_shows_when_the_window_is_back_and_the_link_shows_to_copy() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    // Signed in, as last checked; then signed out with `tgg` (no saved sign-in).
    rt.update(|app, _| {
        app.signin.who = Some(tgg_account::SignedIn {
            account: tgg_account::Account {
                handle: "ciao".into(),
                display_name: None,
                id: None,
            },
            verified: true,
        });
    });
    rt.update(|app, tx| crate::signin::actions::focused(app, true, tx));
    assert!(
        rt.state.signin.who.is_none(),
        "coming back to the window, the sign-out shows"
    );
    // Signing in shows the page's link, to copy when the browser doesn't open.
    rt.update(|app, _| {
        let mut login = crate::signin::Login::asking();
        login.code = Some("ABCD-EFGH".into());
        login.link = Some("https://textures.gg/device?code=ABCDEFGH".into());
        app.signin.login = Some(login);
    });
    settle(&mut rt);
    assert!(rt.find("signin/link").is_some() && rt.find("signin/copy-link").is_some());
}

#[test]
fn a_replays_details_slide_in_with_its_players_and_stats() {
    let replays = tempfile::tempdir().expect("temp");
    let path = tgg_replay_in(replays.path());
    let (mut rt, _disc, _data) = on_replays(replays.path());
    narrow(&mut rt);
    tap_shown(&mut rt, &format!("replays/{}", path.display()));
    jobs_until(&mut rt, |app| {
        app.replays.stats.get(&path).is_some_and(Option::is_some)
    });
    assert!(
        rt.find("replays/details/body").is_some(),
        "its details show"
    );
    assert!(
        rt.find("replays/details/player/0").is_some()
            && rt.find("replays/details/player/1").is_some()
    );
    assert!(
        rt.find("replays/details/stats").is_some(),
        "its stats, or why they couldn't be read"
    );
    tap(&mut rt, "replays/details/close");
    assert!(rt.state.replays.selected.is_none());
    // Escape closes it too.
    tap_shown(&mut rt, &format!("replays/{}", path.display()));
    assert!(rt.state.replays.selected.is_some());
    press_escape(&mut rt);
    assert!(rt.state.replays.selected.is_none());
}

#[test]
fn replays_filter_by_a_search_and_by_where_they_came_from() {
    let replays = tempfile::tempdir().expect("temp");
    tgg_replay_in(replays.path());
    let (mut rt, _disc, _data) = on_replays(replays.path());
    assert_eq!(rt.state.replays.shown().len(), 1);
    rt.update(|app, _| app.replays.search.text = "krowten".into());
    assert_eq!(
        rt.state.replays.shown().len(),
        1,
        "a player's name finds it"
    );
    rt.update(|app, _| app.replays.search.text = "pokémon stadium".into());
    assert_eq!(rt.state.replays.shown().len(), 1, "so does its stage");
    rt.update(|app, _| app.replays.search.text = "zelda".into());
    assert!(rt.state.replays.shown().is_empty());
    rt.update(|app, _| app.replays.search.text.clear());
    rt.frame(FRAME);
    tap(&mut rt, "replays/source/all");
    assert_eq!(rt.state.replays.shown().len(), 1);
    rt.update(|app, _| app.replays.source = Some("Slippi".into()));
    assert!(
        rt.state.replays.shown().is_empty(),
        "it came from tgg-melee's folder"
    );
}

/// Narrow the window below where lists and details sit side by side, so
/// details slide in over the list.
fn narrow(rt: &mut Runtime<App>) {
    rt.set_viewport((960.0, WINDOW.1), 2.0);
    settle(rt);
    assert!(
        !rt.state.side_by_side(),
        "narrow enough for details to slide in"
    );
}

#[test]
fn wide_a_list_and_its_details_sit_side_by_side() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    assert!(rt.state.side_by_side(), "the test window is wide");
    assert!(
        rt.find("mods/set/none").is_some(),
        "with no set on disk the panel says to choose one"
    );
    tap_shown(&mut rt, "mods/catalog/tgg.netplay");
    settle(&mut rt);
    assert!(rt.find("mods/details/column").is_some() && rt.find("mods/details/body").is_some());
    let pane = rt
        .presented("mods/details")
        .map_or(0.0, |(rect, _)| rect[2]);
    assert!(pane < 0.5, "no pane slides in over the list, not {pane} pt");
    // The details close back to the set panel.
    tap(&mut rt, "mods/details/column/close");
    settle(&mut rt);
    assert!(rt.find("mods/details/column").is_none());
}

#[test]
fn play_says_what_to_set_up_before_it_can_play() {
    let (_folder, iso) = melee_iso();
    let (rt, _data) = runtime(game_at(&iso));
    assert!(
        rt.find("play/setup").is_some(),
        "no tgg-melee yet, so Play says so"
    );
    assert!(rt.find("play/card").is_none(), "and there's no card");
}

#[test]
fn a_toast_goes_by_itself_but_a_failure_stays() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    rt.update(|app, _| app.tell("Something happened.", false));
    rt.frame(FRAME);
    assert!(rt.find("shell/notice").is_some(), "it shows");
    jobs_until(&mut rt, |app| app.notice.is_none());
    assert!(rt.find("shell/notice").is_none(), "and goes");
    rt.update(|app, _| app.tell("Something failed.", true));
    std::thread::sleep(std::time::Duration::from_secs(6));
    poll(&mut rt);
    assert!(rt.state.notice.is_some(), "a failure waits to be dismissed");
}

#[test]
fn made_by_you_shows_just_the_skins_the_player_made() {
    let (mut rt, _disc, _data) = editing();
    tap(&mut rt, "editor/save");
    settle(&mut rt);
    let edited = rt.state.library.skins().len();
    assert!(edited > 0, "saving keeps the costume as a skin");
    rt.update(crate::editor::actions::leave);
    settle(&mut rt);
    tap(&mut rt, "skins/tab/made");
    settle(&mut rt);
    let skin = &rt.state.library.skins()[0];
    assert!(rt.find(&format!("skins/skin/{}", skin.id)).is_some());
}

/// Signed in to textures.gg as ciao, as the site last said.
fn sign_in_as_ciao(rt: &mut Runtime<App>) {
    rt.update(|app, _| {
        app.signin.who = Some(tgg_account::SignedIn {
            account: tgg_account::Account {
                handle: "ciao".into(),
                display_name: None,
                id: None,
            },
            verified: true,
        });
    });
}

#[test]
fn you_shows_the_skins_the_player_made_and_goes_to_them() {
    let (mut rt, _disc, _data) = editing();
    tap(&mut rt, "editor/save");
    settle(&mut rt);
    rt.update(crate::editor::actions::leave);
    sign_in_as_ciao(&mut rt);
    settle(&mut rt);
    tap(&mut rt, "sidebar/account");
    settle(&mut rt);
    assert_eq!(rt.state.nav.place, Place::Profile);
    assert!(
        rt.find("sidebar/marker").is_none(),
        "no place above is marked"
    );
    let skin = rt.state.library.skins()[0].id;
    assert!(rt.find(&format!("profile/skin/{skin}")).is_some());
    // Nothing being made on Mods yet: it says where to start one.
    assert!(rt.find("profile/mods/empty").is_some());
    tap(&mut rt, "profile/skins/all");
    settle(&mut rt);
    assert_eq!(rt.state.nav.place, Place::Skins);
    assert_eq!(rt.state.melee.tab, crate::melee::Tab::Made);
}

#[test]
fn you_with_nothing_made_says_where_to_make_it() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    sign_in_as_ciao(&mut rt);
    rt.frame(FRAME);
    tap(&mut rt, "sidebar/account");
    settle(&mut rt);
    assert!(rt.find("profile/head").is_some(), "who's signed in shows");
    assert!(
        rt.find("profile/skins/empty").is_some(),
        "it says where skins you make show"
    );
    tap(&mut rt, "profile/skins/make");
    settle(&mut rt);
    assert_eq!(rt.state.nav.place, Place::Skins);
    assert!(rt.state.melee.making, "the picker opens");
}

#[test]
fn signed_out_the_account_row_signs_in_instead() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "sidebar/account");
    assert!(rt.state.signin.login.is_some(), "the sign-in dialog opens");
    assert_eq!(rt.state.nav.place, Place::Play);
}

struct BrowseNet {
    requests: std::sync::Mutex<Vec<String>>,
}

impl BrowseNet {
    fn new() -> Self {
        Self {
            requests: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn urls(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl tgg_mod::Net for BrowseNet {
    fn get(&self, url: &str, _limit: u64) -> Result<Vec<u8>, tgg_mod::NetError> {
        self.requests.lock().unwrap().push(url.to_owned());
        if url.ends_with("/api/games/melee/targets") {
            return Ok(br#"[
                {"name":"Fox","slug":"fox","category":"character","packCount":42},
                {"name":"Falco","slug":"falco","category":"character","packCount":30},
                {"name":"Battlefield","slug":"battlefield","category":"stage","packCount":18}
            ]"#
            .to_vec());
        }
        if url.contains("search=nothing-here") {
            return Ok(br#"{"items":[],"page":1,"pageSize":24,"total":0,"totalPages":0}"#.to_vec());
        }
        if url.contains("target=fox") {
            return Ok(br#"{
                "items":[
                    {"id":"fox-1","title":"Neon Fox","slug":"neon-fox","targetName":"Fox","targetSlug":"fox","targetCategory":"character","creatorName":"jayson"}
                ],
                "page":1,"pageSize":24,"total":1,"totalPages":1
            }"#
            .to_vec());
        }
        if url.contains("category=stage") {
            return Ok(br#"{
                "items":[
                    {"id":"bf-1","title":"Night Battlefield","slug":"night-bf","targetName":"Battlefield","targetSlug":"battlefield","targetCategory":"stage","creatorName":"saavy"}
                ],
                "page":1,"pageSize":24,"total":1,"totalPages":1
            }"#
            .to_vec());
        }
        if url.contains("page=2") {
            return Ok(br#"{
                "items":[
                    {"id":"p3","title":"Third Skin","slug":"third-skin","targetName":"Falco","targetSlug":"falco","targetCategory":"character","creatorName":"alice"}
                ],
                "page":2,"pageSize":24,"total":3,"totalPages":2
            }"#
            .to_vec());
        }
        Ok(br#"{
            "items":[
                {"id":"p1","title":"First Skin","slug":"first-skin","targetName":"Fox","targetSlug":"fox","targetCategory":"character","creatorName":"alice"},
                {"id":"p2","title":"Second Skin","slug":"second-skin","targetName":"Battlefield","targetSlug":"battlefield","targetCategory":"stage","creatorName":"bob"}
            ],
            "page":1,"pageSize":24,"total":3,"totalPages":2
        }"#
        .to_vec())
    }

    fn api_url(&self) -> &str {
        "https://api.test"
    }

    fn releases_url(&self) -> &str {
        "https://releases.test"
    }

    fn meet_url(&self) -> &str {
        "https://meet.test"
    }
}

#[test]
fn skins_browse_queries_server_for_pagination_sort_filters_and_search() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let net = std::sync::Arc::new(BrowseNet::new());
    rt.state.mods.net = net.clone();
    rt.state.jobs.inline = true;

    tap(&mut rt, Place::Skins.key());
    rt.update(|app, _| crate::library::site::load(app));
    poll(&mut rt);
    settle(&mut rt);

    // Page 1 loaded with targets, showing 2 of 3 skins and a Load more button.
    assert!(rt.find("skins/site/p1").is_some());
    assert!(rt.find("skins/site/p2").is_some());
    assert!(rt.find("skins/site/p3").is_none());
    assert!(rt.find("skins/browse/more").is_some());
    assert!(rt.find("skins/target").is_some());
    assert!(rt.find("skins/sort").is_some());
    assert!(rt.find("skins/browse/site").is_some());

    // Load more fetches page 2 and appends without dropping page 1.
    tap(&mut rt, "skins/browse/more");
    poll(&mut rt);
    settle(&mut rt);
    assert!(rt.find("skins/site/p1").is_some());
    assert!(rt.find("skins/site/p2").is_some());
    assert!(rt.find("skins/site/p3").is_some());
    assert!(
        rt.find("skins/browse/more").is_none(),
        "Load more hides once the last page is loaded"
    );

    // Sort picker triggers a fresh page-1 query with sortBy=newest.
    tap(&mut rt, "skins/sort");
    rt.frame(FRAME);
    tap(&mut rt, "skins/sort/newest");
    poll(&mut rt);
    settle(&mut rt);
    assert!(
        net.urls()
            .iter()
            .any(|u| u.contains("sortBy=newest&page=1")),
        "expected sortBy=newest query in {:?}",
        net.urls()
    );

    // Target picker filters server-side by fighter slug.
    tap(&mut rt, "skins/target");
    rt.frame(FRAME);
    tap(&mut rt, "skins/target/fox");
    poll(&mut rt);
    settle(&mut rt);
    assert!(rt.find("skins/site/fox-1").is_some());
    assert!(
        net.urls().iter().any(|u| u.contains("target=fox")),
        "expected target=fox query"
    );

    // Switching kind to Stages clears the incompatible Fox target and queries category=stage.
    tap(&mut rt, "skins/kind/Stages");
    poll(&mut rt);
    settle(&mut rt);
    assert_eq!(rt.state.melee.target, None);
    assert!(rt.find("skins/site/bf-1").is_some());
    assert!(
        net.urls()
            .iter()
            .any(|u| u.contains("category=stage") && !u.contains("target=fox")),
        "switching kind should drop incompatible target"
    );

    // Server-side search with no matches offers Clear filters, which resets and reloads.
    rt.update(|app, _| {
        app.melee.search.text = "nothing-here".to_owned();
        crate::melee::actions::submit_search(app);
    });
    poll(&mut rt);
    settle(&mut rt);
    assert!(rt.find("skins/browse/empty").is_some());
    assert!(rt.find("skins/browse/clear").is_some());

    tap(&mut rt, "skins/browse/clear");
    poll(&mut rt);
    settle(&mut rt);
    assert_eq!(rt.state.melee.kind, None);
    assert_eq!(rt.state.melee.target, None);
    assert!(rt.state.melee.search.text.is_empty());
    assert!(rt.find("skins/site/p1").is_some());
}

#[test]
fn skins_browse_discards_stale_responses_from_older_queries() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let net = std::sync::Arc::new(BrowseNet::new());
    rt.state.mods.net = net;
    rt.state.jobs.inline = true;

    tap(&mut rt, Place::Skins.key());
    rt.update(|app, _| crate::library::site::load(app));
    poll(&mut rt);
    settle(&mut rt);
    let current_gen = rt.state.library_page.browse.generation;
    assert!(current_gen >= 1);

    let stale = crate::library::site::Done::Listed {
        generation: current_gen - 1,
        append: false,
        result: Ok(crate::library::site::ListedPage {
            items: Vec::new(),
            page: 1,
            total: 0,
            total_pages: 0,
        }),
    };
    let changed = crate::library::site::done(&mut rt.state, stale);
    assert!(!changed, "stale response must be ignored");
    assert!(
        rt.state
            .library_page
            .browse
            .items
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_some_and(|items| items.len() == 2),
        "existing page stays intact"
    );
}

#[test]
fn first_run_unchecking_default_set_activates_vanilla_and_marks_onboarded() {
    let (_folder, iso) = melee_iso();
    let game = tgg_app::Game::open(&iso).expect("the test disc opens");
    let references = tgg_app::game_references(game).expect("references");
    let (mut rt, _data) = runtime(Start::FirstRun {
        game: Some(references),
        state: FirstRun::default(),
    });
    rt.state.mods.net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    crate::first_run::actions::opened(&mut rt.state);
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered && !app.mods.releases.is_empty()
    });
    crate::mods::actions::use_disc(&mut rt.state, iso);
    settle(&mut rt);

    // Default set starts checked; turning it off sets up plain Melee (Vanilla).
    assert!(
        rt.state
            .first_run
            .as_ref()
            .is_some_and(|first| first.use_default_set)
    );
    tap(&mut rt, "first-run/pick/competitive");
    assert!(
        rt.state
            .first_run
            .as_ref()
            .is_some_and(|first| !first.use_default_set)
    );
    tap(&mut rt, "first-run/download");
    jobs_until(&mut rt, |app| {
        app.first_run
            .as_ref()
            .is_some_and(|first| first.stage == crate::first_run::Stage::Done)
            && app.sets.active == tgg_mod::sets::VANILLA
    });
    assert!(
        rt.state.settings.onboarded,
        "completing first run marks onboarded"
    );

    tap(&mut rt, "first-run/play");
    settle(&mut rt);
    assert!(rt.state.first_run.is_none());
    assert_eq!(rt.state.nav.place, Place::Play);
    assert!(rt.state.sets.is_vanilla());
    assert!(rt.find("play/offline").is_some());
    assert!(rt.find("play/find").is_none());
}

#[test]
fn start_runs_first_run_until_onboarded_even_with_existing_port_and_iso() {
    let (_folder, iso) = melee_iso();
    let temp = tempfile::tempdir().expect("temp dir");
    let data = Data::in_folder(temp.path());
    let net = tgg_mod::FakeNet::new();
    let release = tgg_mod::port_releases(&net)
        .expect("releases")
        .into_iter()
        .next()
        .expect("a port release");
    let never = std::sync::atomic::AtomicBool::new(false);
    tgg_mod::install_port(&net, &release, &data.ports, &|_| {}, &never).expect("install port");

    let fresh = Settings::default();
    assert!(
        matches!(
            crate::start(Some(&iso), &data, &fresh),
            Start::FirstRun { game: Some(_), .. }
        ),
        "fresh studio install runs FirstRun even when tgg-melee + ISO already exist"
    );

    let mut onboarded = Settings::default();
    onboarded.onboarded = true;
    assert!(
        matches!(crate::start(Some(&iso), &data, &onboarded), Start::Game(_)),
        "once onboarded, launches straight into the app"
    );
}

#[test]
fn first_run_handles_already_installed_and_updatable_tgg_melee() {
    // Part A: tgg-melee already installed and up to date.
    let (_folder, iso) = melee_iso();
    let game = tgg_app::Game::open(&iso).expect("disc");
    let references = tgg_app::game_references(game).expect("refs");
    let (mut rt, _data) = runtime(Start::FirstRun {
        game: Some(references),
        state: FirstRun::default(),
    });
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let release = tgg_mod::port_releases(&*net)
        .expect("releases")
        .into_iter()
        .next()
        .expect("release");
    let never = std::sync::atomic::AtomicBool::new(false);
    tgg_mod::install_port(&*net, &release, &rt.state.data.ports, &|_| {}, &never)
        .expect("preinstall port");
    rt.state.mods.net = net.clone();
    crate::first_run::actions::opened(&mut rt.state);
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered && app.mods.port().is_some()
    });
    assert!(rt.find("first-run/port-installed").is_some());

    // Part B: newer tgg-melee 0.6.0 is available -> offers updating right in FirstRun.
    net.publish_port("0.6.0");
    rt.update(|app, _| crate::mods::actions::fetch(app));
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered && app.mods.port_update().is_some()
    });
    assert!(rt.find("first-run/port-update").is_some());

    tap(&mut rt, "first-run/download");
    assert_eq!(
        rt.state.first_run.as_ref().map(|first| first.stage),
        Some(crate::first_run::Stage::Downloading)
    );
    jobs_until(&mut rt, |app| {
        app.first_run
            .as_ref()
            .is_some_and(|first| first.stage == crate::first_run::Stage::Done)
            && app.mods.port().is_some_and(|port| port.version == "0.6.0")
            && app.sets.list.iter().any(|set| set.name == "Competitive")
    });
    assert!(rt.state.settings.onboarded);
}

#[test]
fn sets_tab_hides_online_play_and_new_set_works_without_it() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net);

    // Inject an `online-play` set into the fetched list to confirm Sets hides it.
    rt.update(|app, _| {
        if let Some(mut online) = app.mods.sets.first().cloned() {
            online.name = tgg_mod::sets::ONLINE_PLAY.into();
            online.title = "Online play".into();
            app.mods.sets.push(online);
        }
    });
    tap(&mut rt, "mods/tab/sets");
    settle(&mut rt);
    assert!(
        rt.find("mods/sets/official/online-play").is_none(),
        "online-play is hidden from the From textures.gg browse list"
    );
    assert!(rt.find("mods/sets/official/competitive").is_some());

    // Without online-play in the registry, New set pre-selects Competitive and
    // lets the player uncheck it to create an empty custom set with no warning.
    rt.update(|app, _| {
        app.notice = None;
        app.mods
            .sets
            .retain(|set| set.name != tgg_mod::sets::ONLINE_PLAY);
    });
    tap(&mut rt, "mods/new-set");
    settle(&mut rt);
    assert!(rt.find("mods/new-set/no-online").is_none());
    assert_eq!(
        rt.state
            .mods
            .new_set
            .as_ref()
            .map(|sheet| sheet.builds_on.clone()),
        Some(vec![crate::first_run::DEFAULT_SET.to_owned()])
    );
    tap(&mut rt, "mods/new-set/competitive");
    rt.update(|app, _| {
        if let Some(sheet) = &mut app.mods.new_set {
            sheet.name.text = "Blank custom".into();
        }
    });
    settle(&mut rt);
    tap(&mut rt, "mods/new-set/create");
    jobs_until(&mut rt, |app| {
        app.sets.list.iter().any(|set| set.name == "Blank custom")
    });
    assert!(
        rt.state.notice.is_none(),
        "no obsolete online-play warning toast"
    );
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn play_vanilla_mode_launches_offline_with_empty_mods_dir() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    jobs_until(&mut rt, |app| app.sets.active().is_some());
    open_place(&mut rt, Place::Play);

    tap(&mut rt, "play/set");
    settle(&mut rt);
    assert!(rt.find("play/set/vanilla").is_some());
    tap(&mut rt, "play/set/vanilla");
    settle(&mut rt);

    assert!(rt.state.sets.is_vanilla());
    assert!(rt.find("play/find").is_none());
    assert!(rt.find("play/friend").is_none());
    assert!(rt.find("play/offline").is_some());

    tap(&mut rt, "play/offline");
    jobs_until(&mut rt, |app| app.mods.playing.is_some());
    assert_eq!(
        rt.state
            .sets
            .session
            .as_ref()
            .map(|session| session.name.as_str()),
        Some(tgg_mod::sets::VANILLA_NAME)
    );
    let vanilla_dir = tgg_mod::sets::vanilla_mods_dir(&rt.state.data.tgg);
    assert!(
        vanilla_dir.is_dir(),
        "empty vanilla mods folder was prepared"
    );
    tap(&mut rt, "play/session");
    jobs_until(&mut rt, |app| app.mods.playing.is_none());
}

#[test]
fn play_and_sidebar_surface_tgg_melee_update_and_install_it() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    open_place(&mut rt, Place::Play);
    assert!(rt.find("play/port-update").is_none());
    assert!(rt.find("sidebar/port-update").is_none());

    net.publish_port("0.6.0");
    rt.update(|app, _| crate::mods::actions::fetch(app));
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered && app.mods.port_update().is_some()
    });
    assert!(rt.find("play/port-update").is_some());
    assert!(rt.find("play/port-update/install").is_some());
    assert!(rt.find("sidebar/port-update").is_some());

    tap(&mut rt, "play/port-update/install");
    jobs_until(&mut rt, |app| {
        app.mods.port().is_some_and(|port| port.version == "0.6.0")
            && app.mods.registry == crate::mods::Registry::Answered
    });
    assert!(rt.find("play/port-update").is_none());
    assert!(rt.find("sidebar/port-update").is_none());
}

#[test]
fn top_bar_clears_windows_trailing_caption_insets() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    open_place(&mut rt, Place::Replays);
    settle(&mut rt);

    let without_inset = rt
        .presented("replays/open")
        .expect("top bar action shows")
        .0;
    let trailing = 138.0;
    let mut insets = rune::ChromeInsets::default();
    insets.trailing = trailing;
    insets.height = crate::ui::tokens::size::TOP_BAR;
    rt.set_chrome_insets(insets);
    settle(&mut rt);
    let with_inset = rt
        .presented("replays/open")
        .expect("top bar action still shows")
        .0;
    assert!(
        ((without_inset[0] - with_inset[0]) - trailing).abs() < 0.5,
        "top bar action should shift left by trailing inset {trailing}: before={} after={}",
        without_inset[0],
        with_inset[0]
    );
    assert!(
        with_inset[0] + with_inset[2] <= WINDOW.0 - trailing,
        "top bar action must not overlap Windows caption buttons"
    );
}

#[test]
fn modified_disc_shows_warning_in_first_run_and_play() {
    let folder = tempfile::tempdir().expect("temp dir");
    let iso = folder.path().join("modded.iso");
    std::fs::write(
        &iso,
        tgg_app::game::test_disc_with_file(b"GALE01", 2, b"PlFc.dat", b"modified"),
    )
    .expect("write modded disc");

    let game = tgg_app::Game::open(&iso).expect("open disc");
    let references = tgg_app::game_references(game).expect("refs");
    let (mut rt, data) = runtime(Start::FirstRun {
        game: Some(references),
        state: FirstRun::default(),
    });
    rt.state.mods.net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    crate::first_run::actions::opened(&mut rt.state);
    jobs_until(&mut rt, |app| {
        matches!(app.mods.disc, crate::mods::Disc::Found(_))
            && !app.mods.disc_modified.is_empty()
            && app.mods.registry == crate::mods::Registry::Answered
            && !app.mods.releases.is_empty()
    });
    assert_eq!(rt.state.mods.disc_modified, vec!["PlFc.dat"]);
    assert!(rt.find("first-run/disc-warning").is_some());

    // Complete setup and check Play also surfaces the disc warning with Choose another.
    tap(&mut rt, "first-run/download");
    jobs_until(&mut rt, |app| {
        app.first_run
            .as_ref()
            .is_some_and(|first| first.stage == crate::first_run::Stage::Done)
    });
    tap(&mut rt, "first-run/play");
    settle(&mut rt);
    assert!(rt.find("play/disc-warning").is_some());
    assert!(rt.find("play/disc-warning/choose").is_some());

    // Once a cached clean copy exists under `<data>/clean/<sha256>`, re-checking clears the warning.
    let vanilla = melee_dat::vanilla::vanilla_files()
        .find(|f| f.name == "PlFc.dat")
        .expect("PlFc.dat");
    let clean = data.path().join("clean");
    std::fs::create_dir_all(&clean).expect("clean dir");
    std::fs::write(clean.join(&vanilla.sha256), vec![0u8; vanilla.byte_length])
        .expect("clean copy");
    rt.update(|app, _| crate::mods::actions::use_disc(app, iso.clone()));
    jobs_until(&mut rt, |app| app.mods.disc_modified.is_empty());
    assert!(rt.find("play/disc-warning").is_none());
}

#[test]
fn settings_shows_port_and_app_updates_and_opens_in_app_update_dialog() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let current = tgg_mod::port_releases(&*net)
        .expect("releases")
        .into_iter()
        .next()
        .expect("current port");
    let never = std::sync::atomic::AtomicBool::new(false);
    tgg_mod::install_port(&*net, &current, &rt.state.data.ports, &|_| {}, &never).expect("install");
    net.publish_port("0.6.0");
    rt.update(move |app, _| {
        app.mods.net = net;
        crate::mods::actions::read_disk(app);
    });

    // Open Settings and click Check for updates on tgg-melee -> Update button appears.
    tap(&mut rt, "sidebar/settings");
    settle(&mut rt);
    assert!(rt.find("settings/update-port").is_none());
    tap(&mut rt, "settings/fetch");
    jobs_until(&mut rt, |app| {
        app.mods.registry == crate::mods::Registry::Answered
    });
    settle(&mut rt);
    assert!(
        rt.find("settings/update-port").is_some(),
        "Settings surfaces an inline Update button when a newer tgg-melee exists"
    );

    // An available app update shows both in Settings and the top bar, and opens the update dialog.
    let release = tgg_app::update::Release {
        version: "9.9.9".into(),
        released: Some("2026-11-28".into()),
        notes_url: Some("https://github.com/texturesgg/texturesgg/releases/tag/app-v9.9.9".into()),
        assets: tgg_app::update::Assets {
            windows: vec![tgg_app::update::Asset {
                kind: "Installer".into(),
                url: "https://example.com/setup.exe".into(),
                size: 12_000_000,
                sha256: "abc".into(),
                min_os: None,
            }],
            macos: vec![tgg_app::update::Asset {
                kind: "dmg".into(),
                url: "https://example.com/app.dmg".into(),
                size: 17_000_000,
                sha256: "def".into(),
                min_os: None,
            }],
            linux: vec![tgg_app::update::Asset {
                kind: "tar.gz".into(),
                url: "https://example.com/app.tar.gz".into(),
                size: 20_000_000,
                sha256: "ghi".into(),
                min_os: None,
            }],
        },
    };
    rt.update(move |app, _| {
        app.update.available = Some(release);
    });
    settle(&mut rt);
    assert!(
        rt.find("settings/update-app").is_some(),
        "Settings shows an inline Update button when an app update is available"
    );
    tap(&mut rt, "settings/update-app");
    settle(&mut rt);
    assert!(
        !rt.state.preferences.open,
        "Settings closes for update dialog"
    );
    assert!(rt.find("update").is_some(), "In-app update dialog opens");
    tap(&mut rt, "update/close");
    settle(&mut rt);
    assert!(rt.state.update.dialog.is_none());

    // Clicking the top-bar "textures.gg 9.9.9 is out" action also opens the update dialog.
    assert!(rt.find("shell/update").is_some());
    tap(&mut rt, "shell/update");
    settle(&mut rt);
    assert!(rt.find("update").is_some());
}
