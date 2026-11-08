//! The shell's flows, driven through Rune's headless runtime: places, the
//! sidebar, and Welcome finding a game. No GPU and no game files; games are
//! disc images written by hand.

use crate::melee::Subject;
use crate::nav::Place;
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
    let crate::jobs::Done::States { iso: read, states, undoable } = done else { panic!("the slot states arrive first") };
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
fn dragging_across_the_stage_turns_the_camera_by_how_far_it_went() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    settle(&mut rt);
    let before = rt.state.stage.shown.as_ref().expect("Falco is on the stage").orbit;
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
    let after = rt.state.stage.shown.as_ref().expect("still on the stage").orbit;
    assert!((after.yaw - (before.yaw - 1.0)).abs() < 1e-6, "100 points turn it a radian: {} → {}", before.yaw, after.yaw);
    assert_eq!(after.pitch, before.pitch);
    assert_eq!(rt.cursor(), rune::Cursor::Grab, "let go, it's ready to grab again");

    // Down turns it to look from above.
    rt.pointer_down(x, y);
    rt.frame(FRAME);
    for step in [10.0, 20.0, 30.0] {
        rt.pointer_move(x, y + step);
        rt.frame(FRAME);
    }
    rt.pointer_up(x, y + 30.0);
    rt.frame(FRAME);
    let above = rt.state.stage.shown.as_ref().expect("still on the stage").orbit;
    assert!((above.pitch - (after.pitch + 0.3)).abs() < 1e-6, "30 points down tip it 0.3: {} → {}", after.pitch, above.pitch);
    assert_eq!(above.yaw, after.yaw);
    let after = above;

    let mut wheel = rune::WheelEvent::default();
    // One notch of a mouse wheel (Rune's 40-point line) moves it as one does
    // in tgg-editor: 4%.
    (wheel.x, wheel.y, wheel.dy) = (x, y, 40.0);
    rt.wheel_event(wheel);
    rt.frame(FRAME);
    let zoomed = rt.state.stage.shown.as_ref().expect("still on the stage").orbit;
    assert!((zoomed.zoom - after.zoom * (-0.04f64).exp()).abs() < 1e-9, "one notch brings it in 4%: {} → {}", after.zoom, zoomed.zoom);
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
    let crate::jobs::Done::States { iso, states, undoable } = done else { panic!("the slot states arrive first") };
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

/// Falco open in the editor, on the test disc's costume.
fn editing() -> (Runtime<App>, TempDir, TempDir) {
    let (folder, iso) = melee_iso();
    let (mut rt, data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    settle(&mut rt);
    tap(&mut rt, "melee/edit");
    settle(&mut rt);
    (rt, folder, data)
}

/// A one-color PNG `size` pixels square, on disk.
fn png(folder: &Path, size: u32, rgba: [u8; 4]) -> PathBuf {
    let path = folder.join(format!("paint-{size}.png"));
    image::RgbaImage::from_pixel(size, size, image::Rgba(rgba)).save(&path).expect("write png");
    path
}

fn command(rt: &mut Runtime<App>, character: char, shift: bool) {
    rt.key(KeyEvent { key: Key::Command(character), mods: Mods { cmd: true, shift, ..Default::default() } });
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
    assert!(!shown.highlight.is_empty(), "the selected texture's surfaces tint");
}

#[test]
fn done_without_edits_goes_straight_back() {
    let (mut rt, _disc, _data) = editing();
    tap(&mut rt, "editor/done");
    assert!(rt.state.editor.is_none());
    assert!(rt.state.melee.subject.is_some(), "back to Falco on the stage");
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
    assert!(rt.state.notice.as_ref().is_some_and(|notice| notice.text.contains("every texel exact")), "{:?}", rt.state.notice);
    assert!(!rt.state.stage.shown.as_ref().expect("on stage").edited.is_empty(), "the model shows it");
    assert!(rt.find("editor/edited").is_some(), "the title row says it's edited");
    command(&mut rt, 'z', false);
    assert!(!rt.state.editor.as_ref().expect("still editing").modified(), "undone back to as it opened");
    command(&mut rt, 'z', true);
    assert!(rt.state.editor.as_ref().expect("still editing").modified(), "and redone");
}

#[test]
fn a_png_of_the_wrong_size_is_turned_away() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let small = png(files.path(), 4, [0, 0, 255, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &small));
    let notice = rt.state.notice.clone().expect("it says why");
    assert!(notice.error && notice.text.contains("is 4×4; this texture is 8×8"), "{}", notice.text);
    assert!(!rt.state.editor.as_ref().expect("still editing").modified());
}

#[test]
fn leaving_with_unsaved_edits_asks_and_discarding_puts_the_costume_back() {
    let (mut rt, _disc, _data) = editing();
    let files = tempfile::tempdir().expect("temp dir");
    let red = png(files.path(), 8, [255, 0, 0, 255]);
    rt.update(|app, _| crate::editor::files::import_file(app, 0, &red));
    press_escape(&mut rt);
    assert_eq!(rt.state.editor.as_ref().and_then(|document| document.pending), Some(crate::editor::Pending::Leave));
    settle(&mut rt);
    tap(&mut rt, "prompt/discard");
    assert!(rt.state.editor.is_none());
    assert!(rt.state.stage.shown.as_ref().expect("Falco on the stage").edited.is_empty(), "the edit is gone");
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
    let edited = document.listing.document.as_ref().expect("open").bytes().to_vec();
    let slot = document.slot;
    let game = rt.state.game.as_ref().expect("a game").game();
    assert_eq!(game.read_slot(slot).expect("read back"), edited, "the disc holds the edit");
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
    tap(&mut rt, Place::Library.key());
    assert_eq!(rt.state.nav.place, Place::Melee, "it asks before going");
    settle(&mut rt);
    tap(&mut rt, "prompt/save");
    assert_eq!(rt.state.nav.place, Place::Library);
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
    document.listing.document.as_ref().expect("readable").textures()[index].uses.clone()
}

#[test]
fn a_click_on_the_model_asks_to_pick_and_a_drag_doesnt() {
    let (mut rt, _iso, _data) = editing();
    let (x, y) = rt.find("stage/viewport").expect("the stage shows");
    rt.pointer_down(x, y);
    rt.pointer_move(x + 2.0, y + 1.0);
    rt.pointer_up(x + 2.0, y + 1.0);
    rt.frame(FRAME);
    let pick = rt.state.stage.shown.as_ref().expect("on the stage").pick.expect("a click picks");
    assert!(pick.iter().all(|fraction| (0.0..=1.0).contains(fraction)), "{pick:?} is on the plate");

    rt.state.stage.shown.as_mut().expect("on the stage").pick = None;
    rt.pointer_down(x, y);
    rt.pointer_move(x + 40.0, y);
    rt.pointer_up(x + 40.0, y);
    rt.frame(FRAME);
    assert!(rt.state.stage.shown.as_ref().expect("on the stage").pick.is_none(), "a drag turns the camera instead");
}

#[test]
fn off_the_editor_a_click_on_the_model_picks_nothing() {
    let (_folder, iso) = melee_iso();
    let (mut rt, _data) = runtime(game_at(&iso));
    tap(&mut rt, "melee/fighter/Falco");
    settle(&mut rt);
    tap(&mut rt, "stage/viewport");
    assert!(rt.state.stage.shown.as_ref().expect("on the stage").pick.is_none());
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
    assert!(!rt.state.stage.shown.as_ref().expect("on the stage").highlight.is_empty(), "the model shows where it draws");

    rt.state.stage.picked = Some(vec![texture]);
    poll(&mut rt);
    assert_eq!(rt.state.editor.as_ref().expect("editing").selected, None, "the selected one again clears it");
    assert_eq!(rt.state.notice.as_ref().map(|notice| notice.text.as_str()), Some("Selection cleared"));
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
        document.external.watched.get(&0).expect("the selected texture is watched").path.clone()
    };
    let exported = std::fs::read(&path).expect("its PNG is written");
    assert!(!rt.state.editor.as_ref().expect("editing").modified(), "opening it there changes nothing yet");

    // The player paints it red and saves.
    let red = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
    red.save(&path).expect("save");
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options().write(true).open(&path).expect("open").set_modified(later).expect("touch");
    poll(&mut rt);
    assert!(rt.state.editor.as_ref().expect("editing").modified(), "the save came back as an edit");
    assert!(!rt.state.stage.shown.as_ref().expect("on stage").edited.is_empty(), "and shows on the model");

    // Undo puts the PNG back as the document has it, without importing it again.
    command(&mut rt, 'z', false);
    assert!(!rt.state.editor.as_ref().expect("editing").modified());
    assert_eq!(std::fs::read(&path).expect("still there"), exported, "the PNG matches the document again");
    assert!(rt.state.editor.as_ref().expect("editing").external.watched[&0].saved().is_none(), "the rewrite isn't a save");

    tap(&mut rt, "editor/done");
    assert!(!path.exists(), "leaving the costume deletes its PNG");
}

#[test]
fn ctrl_shift_e_opens_the_selected_texture_in_the_image_editor() {
    let (mut rt, _disc, _data) = editing();
    rt.state.settings.external_editor = Some("true".into());
    command(&mut rt, 'e', true);
    assert!(rt.state.editor.as_ref().expect("editing").external.watched.contains_key(&0));
    tap(&mut rt, "editor/done");
}

fn first_color(rt: &Runtime<App>) -> [u8; 4] {
    rt.state.editor.as_ref().expect("editing").colors[0].swatches[0].rgba
}

#[test]
fn a_color_changed_in_the_colors_pane_shows_on_the_model_and_undoes() {
    let (mut rt, _iso, _data) = editing();
    assert_eq!(first_color(&rt), [96, 224, 96, 255], "the costume's green vertices");
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
    assert_eq!(first_color(&rt), [248, 224, 96, 255], "every vertex of that color changed, as the game reads it");
    assert!(rt.state.editor.as_ref().expect("editing").modified());
    assert!(rt.state.editor.as_ref().expect("editing").color_pick.is_none(), "the sliders close");
    assert_ne!(rt.state.stage.shown.as_ref().expect("on stage").generation, generation, "the model is drawn afresh");
    assert!(rt.state.notice.as_ref().is_some_and(|notice| notice.text.contains("#60E060 to #FFE060")), "{:?}", rt.state.notice);

    command(&mut rt, 'z', false);
    assert_eq!(first_color(&rt), [96, 224, 96, 255], "undone, the list reads the green again");
    assert!(!rt.state.editor.as_ref().expect("editing").modified());
}

#[test]
fn shifting_a_groups_hue_turns_all_its_colors() {
    let (mut rt, _iso, _data) = editing();
    let before: Vec<[u8; 4]> = rt.state.editor.as_ref().expect("editing").colors[0].swatches.iter().map(|swatch| swatch.rgba).collect();
    tap(&mut rt, "editor/hue/0");
    settle(&mut rt);
    rt.update(|app, _| crate::editor::colors::set_hue(app, 120.0));
    rt.frame(FRAME);
    tap(&mut rt, "editor/pick/apply");
    let after: Vec<[u8; 4]> = rt.state.editor.as_ref().expect("editing").colors[0].swatches.iter().map(|swatch| swatch.rgba).collect();
    let mut expected: Vec<[u8; 4]> = before.iter().map(|&rgba| tgg_app::colors::turn_hue(rgba, 120.0)).collect();
    let mut sorted = after.clone();
    expected.sort_unstable();
    sorted.sort_unstable();
    // Within what the vertices' 5- and 6-bit channels keep.
    let close = sorted.iter().zip(&expected).all(|(got, want)| got.iter().zip(want).all(|(a, b)| a.abs_diff(*b) <= 8));
    assert!(close, "each color turned a third of the way round: {sorted:?}, wanted about {expected:?}");
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
    jobs_until(&mut rt, |app| app.mods.registry != crate::mods::Registry::Asking && app.mods.disc != crate::mods::Disc::Looking);
    (rt, disc, data)
}

/// Hand finished jobs to the app until `done` holds, for up to 20 seconds.
fn jobs_until(rt: &mut Runtime<App>, done: impl Fn(&App) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !done(&rt.state) {
        assert!(std::time::Instant::now() < deadline, "the jobs finished in time");
        std::thread::sleep(std::time::Duration::from_millis(10));
        poll(rt);
    }
    settle(rt);
}

/// On Mods with tgg-melee installed from the fake.
fn with_port(net: std::sync::Arc<tgg_mod::FakeNet>) -> (Runtime<App>, TempDir, TempDir) {
    let (mut rt, disc, data) = on_mods(net);
    tap(&mut rt, "mods/install-port");
    jobs_until(&mut rt, |app| app.mods.port().is_some() && app.mods.registry == crate::mods::Registry::Answered);
    (rt, disc, data)
}

/// Scroll the element keyed `key` into view, then tap it.
fn tap_shown(rt: &mut Runtime<App>, key: &str) {
    let owned = key.to_owned();
    rt.update(move |_, tx| tx.scroll_into_view(owned.clone()));
    settle(rt);
    tap(rt, key);
}

/// Install the set `name` through its review.
fn install_set(rt: &mut Runtime<App>, name: &str) {
    tap_shown(rt, &format!("mods/set/{name}/install"));
    settle(rt);
    tap(rt, "mods/review/confirm");
    jobs_until(rt, |app| app.mods.work.is_none() && app.mods.local.set(name).is_some());
}

#[test]
fn mods_offers_tgg_melee_then_its_sets() {
    let (mut rt, _disc, _data) = on_mods(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    assert_eq!(rt.state.mods.registry, crate::mods::Registry::Answered);
    assert!(rt.find("mods/install-port").is_some(), "tgg-melee offers to install");
    assert!(rt.find("mods/set/competitive").is_some() && rt.find("mods/set/replays").is_some(), "the sets show before it's installed");
    assert!(rt.find("mods/set/competitive/install").is_none(), "but install once tgg-melee is there");
    assert!(rt.find("mods/note").is_some());
    tap(&mut rt, "mods/install-port");
    assert!(rt.find("mods/work").is_some(), "the install shows its progress");
    jobs_until(&mut rt, |app| app.mods.port().is_some() && app.mods.registry == crate::mods::Registry::Answered);
    assert_eq!(rt.state.mods.port().map(|port| port.version.as_str()), Some(tgg_mod::FakeNet::PORT_VERSION));
    assert!(rt.find("mods/install-port").is_none() && rt.find("mods/work").is_none());
    assert!(rt.find("mods/set/competitive/install").is_some(), "a set installs now");
    assert!(rt.find("mods/play").is_none(), "the test disc isn't unmodified, so nothing to play yet");
    assert_eq!(rt.state.mods.disc, crate::mods::Disc::Missing);
}

#[test]
fn a_set_is_reviewed_then_installed() {
    let (mut rt, _disc, data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    tap(&mut rt, "mods/set/competitive/install");
    settle(&mut rt);
    let review = rt.state.mods.review.as_ref().expect("the plan shows first");
    assert_eq!(review.plan.steps.len(), 7);
    assert!(rt.find("mods/review/tgg.direct-connect").is_some());
    assert!(rt.state.mods.local.mods.is_empty(), "nothing installs until the player says");
    tap(&mut rt, "mods/review/confirm");
    jobs_until(&mut rt, |app| app.mods.work.is_none() && !app.mods.local.mods.is_empty());
    assert_eq!(rt.state.mods.local.mods.len(), 7);
    assert_eq!(rt.state.mods.local.set("competitive").map(|set| set.status), Some(tgg_mod::SetStatus::Complete));
    assert!(data.path().join("mods/tgg.netplay").is_dir(), "into the mods folder");
    assert!(rt.find("mods/set/competitive/install").is_none(), "an installed set doesn't offer to install");
    tap(&mut rt, "mods/tab/installed");
    assert!(rt.find("mods/installed").is_some() && rt.find("mods/mod/tgg.netplay").is_some());
    assert_eq!(rt.state.notice.as_ref().map(|notice| notice.text.as_str()), Some("Installed 7 mods."));
}

#[test]
fn a_mod_switched_off_customizes_its_set_and_back() {
    let (mut rt, _disc, data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    install_set(&mut rt, "competitive");
    tap(&mut rt, "mods/tab/installed");
    tap_shown(&mut rt, "mods/mod/tgg.quick-start/switch");
    assert!(data.path().join("mods/.tgg.quick-start").is_dir(), "a turned-off mod's folder is hidden from the game");
    assert_eq!(rt.state.mods.local.set("competitive").map(|set| set.status), Some(tgg_mod::SetStatus::Customized));
    tap_shown(&mut rt, "mods/mod/tgg.quick-start/switch");
    assert_eq!(rt.state.mods.local.set("competitive").map(|set| set.status), Some(tgg_mod::SetStatus::Complete));
}

#[test]
fn update_all_installs_what_textures_gg_has_newer() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    install_set(&mut rt, "replays");
    assert!(rt.find("mods/update-all").is_none(), "nothing's newer yet");
    net.publish_mod("tgg.replay-recorder", "Replay recorder", "0.1.1", &[]);
    tap(&mut rt, "mods/refresh");
    jobs_until(&mut rt, |app| app.mods.registry == crate::mods::Registry::Answered);
    assert!(rt.state.mods.has_updates());
    tap(&mut rt, "mods/tab/installed");
    tap_shown(&mut rt, "mods/update-all");
    settle(&mut rt);
    tap(&mut rt, "mods/review/confirm");
    jobs_until(&mut rt, |app| app.mods.work.is_none() && !app.mods.has_updates());
    let recorder = rt.state.mods.local.mods.iter().find(|m| m.manifest.id.as_str() == "tgg.replay-recorder").expect("still installed");
    assert_eq!(recorder.manifest.version.to_string(), "0.1.1");
    assert_eq!(rt.state.notice.as_ref().map(|notice| notice.text.as_str()), Some("Updated 1 mod."));
}

#[test]
fn a_set_removed_takes_its_mods() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    install_set(&mut rt, "replays");
    tap(&mut rt, "mods/set/replays/remove");
    assert!(rt.state.mods.local.mods.is_empty());
    tap(&mut rt, "mods/tab/installed");
    assert!(rt.find("mods/installed/empty").is_some(), "the Installed tab says there's nothing");
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
fn play_starts_tgg_melee_and_stop_ends_it() {
    let (mut rt, _disc, data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    // The test disc has a costume of its own; choose it to play anyway.
    let iso = rt.state.game.as_ref().expect("a game").game().path().to_owned();
    rt.update(|app, _| crate::mods::actions::use_disc(app, iso.clone()));
    rt.frame(FRAME);
    tap(&mut rt, "mods/play");
    assert!(rt.state.mods.playing.is_some());
    assert!(tgg_mod::game_running(&data.path().join("mods")), "other tgg's see the game running");
    assert!(rt.find("mods/stop").is_some());
    tap(&mut rt, "mods/stop");
    jobs_until(&mut rt, |app| app.mods.playing.is_none());
    assert!(rt.find("mods/play").is_some());
}

#[test]
fn every_mod_can_be_browsed_and_searched_before_tgg_melee_is_installed() {
    let (mut rt, _disc, _data) = on_mods(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    tap(&mut rt, "mods/tab/all");
    assert_eq!(rt.state.mods.offered().len(), 10, "the newest release of each, from the newest tgg-melee's catalog");
    assert!(rt.find("mods/catalog/tgg.netplay").is_some());
    assert!(rt.find("mods/catalog/tgg.netplay/install").is_none(), "installing waits for tgg-melee");
    rt.update(|app, _| app.mods.search.text = "replay".into());
    rt.frame(FRAME);
    let found: Vec<String> = rt.state.mods.found().iter().map(|entry| entry.manifest.id.to_string()).collect();
    assert_eq!(found.len(), 3, "the three replay mods: {found:?}");
    assert!(found.iter().all(|id| id.starts_with("tgg.replay-")));
    assert!(rt.find("mods/catalog/tgg.netplay").is_none(), "the search hides the rest");
    rt.update(|app, _| app.mods.search.text = "nothing like this".into());
    rt.frame(FRAME);
    assert!(rt.find("mods/all/empty").is_some());
}

#[test]
fn one_mod_installs_with_what_it_needs() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    tap(&mut rt, "mods/tab/all");
    tap_shown(&mut rt, "mods/catalog/tgg.direct-connect/install");
    settle(&mut rt);
    let review = rt.state.mods.review.as_ref().expect("the plan shows first");
    let ids: Vec<String> = review.plan.steps.iter().map(|step| step.id.to_string()).collect();
    assert!(ids.contains(&"tgg.direct-connect".to_owned()) && ids.contains(&"tgg.netplay".to_owned()), "and what it needs: {ids:?}");
    tap(&mut rt, "mods/review/confirm");
    jobs_until(&mut rt, |app| app.mods.work.is_none() && !app.mods.local.mods.is_empty());
    assert_eq!(rt.state.mods.local.mods.len(), 2);
    assert!(rt.find("mods/catalog/tgg.direct-connect/switch").is_some(), "installed, it shows its switch");
    assert_eq!(rt.state.mods.selected, None, "pressing Install didn't open the details");
}

#[test]
fn a_mods_details_slide_in_and_install_it() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    tap(&mut rt, "mods/tab/all");
    tap_shown(&mut rt, "mods/catalog/tgg.replay-browser");
    settle(&mut rt);
    assert_eq!(rt.state.mods.selected.as_ref().map(ToString::to_string).as_deref(), Some("tgg.replay-browser"));
    let width = rt.presented("mods/details").map_or(0.0, |(rect, _)| rect[2]);
    assert!(width > 300.0, "the pane opens to its column, not {width} pt");
    assert!(rt.find("mods/details/body").is_some());
    tap(&mut rt, "mods/details/install");
    settle(&mut rt);
    tap(&mut rt, "mods/review/confirm");
    jobs_until(&mut rt, |app| app.mods.work.is_none() && !app.mods.local.mods.is_empty());
    assert!(rt.find("mods/details/switch").is_some(), "installed, the pane shows its switch");
    tap(&mut rt, "mods/details/close");
    settle(&mut rt);
    let width = rt.presented("mods/details").map_or(0.0, |(rect, _)| rect[2]);
    assert!(width < 0.5, "closed, the pane narrows to nothing, not {width} pt");
}

#[test]
fn a_newer_tgg_melee_says_which_mods_wait_and_the_older_one_stays_to_go_back_to() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    install_set(&mut rt, "replays");
    net.publish_port("0.6.0");
    tap(&mut rt, "mods/refresh");
    jobs_until(&mut rt, |app| app.mods.registry == crate::mods::Registry::Answered);
    assert!(rt.find("mods/update-port").is_some(), "0.6.0 is offered");
    // The fake's mods fit every game; say the newer one has only the player's recorder built.
    rt.update(|app, _| app.mods.next_built = Some(vec!["tgg.replay-recorder".parse().expect("an id")]));
    rt.frame(FRAME);
    assert!(rt.find("mods/update-waits").is_some(), "the mods that would wait are named before updating");
    assert_eq!(rt.state.mods.unbuilt_for_update().len(), 2);
    tap(&mut rt, "mods/update-port");
    jobs_until(&mut rt, |app| app.mods.port().is_some_and(|port| port.version == "0.6.0") && app.mods.registry == crate::mods::Registry::Answered);
    assert_eq!(rt.state.mods.ports.len(), 2, "the older version stays");
    assert_eq!(rt.state.mods.fallback, None, "every mod here fits 0.6.0");
    // Say some didn't: the way back is one press.
    rt.update(|app, _| app.mods.fallback = Some(crate::mods::Fallback { version: "0.5.0".into(), unfit: 2 }));
    settle(&mut rt);
    tap(&mut rt, "mods/use-fallback");
    assert_eq!(rt.state.mods.port().map(|port| port.version.as_str()), Some("0.5.0"));
    assert_eq!(rt.state.mods.fallback, None);
}

#[test]
fn the_details_pane_is_the_window_tall_and_takes_the_window_controls() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    tap(&mut rt, "mods/tab/all");
    let close = rt.presented("window/close").expect("the window's close shows").0;
    tap_shown(&mut rt, "mods/catalog/tgg.netplay");
    settle(&mut rt);
    let (pane, _) = rt.presented("mods/details").expect("the pane shows");
    assert!(pane[1] < 0.5, "it starts at the window's top, not {} pt down", pane[1]);
    let moved = rt.presented("window/close").expect("the window's close still shows").0;
    assert!((moved[0] + moved[2] - (close[0] + close[2])).abs() < 0.5 && (moved[1] - close[1]).abs() < 0.5,
        "and it stays where it was: {close:?} then {moved:?}");
    tap(&mut rt, "mods/details/close");
    settle(&mut rt);
    assert_eq!(rt.state.mods.selected, None);
}

#[test]
fn checking_for_updates_moves_nothing_on_the_page() {
    let (mut rt, _disc, _data) = with_port(std::sync::Arc::new(tgg_mod::FakeNet::new()));
    let before = rt.presented("mods/tab/sets").expect("the tabs show").0;
    tap(&mut rt, "mods/refresh");
    settle(&mut rt);
    assert!(rt.find("mods/checking").is_some(), "the action says it's checking, in place");
    assert_eq!(rt.presented("mods/tab/sets").expect("the tabs show").0, before, "while it asks");
    jobs_until(&mut rt, |app| app.mods.registry == crate::mods::Registry::Answered);
    assert_eq!(rt.presented("mods/tab/sets").expect("the tabs show").0, before, "and once it's answered");
}

#[test]
fn versions_install_switch_and_remove_and_an_installed_newer_one_is_still_an_update() {
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let (mut rt, _disc, _data) = with_port(net.clone());
    net.publish_port("0.6.0");
    tap(&mut rt, "mods/refresh");
    jobs_until(&mut rt, |app| app.mods.registry == crate::mods::Registry::Answered);
    tap(&mut rt, "mods/versions");
    settle(&mut rt);
    assert!(rt.find("mods/versions/0.6.0/install").is_some(), "the newer release offers to install");
    assert!(rt.find("mods/versions/0.5.0/use").is_none(), "the one in use has nothing to do");
    tap(&mut rt, "mods/versions/0.6.0/install");
    jobs_until(&mut rt, |app| app.mods.port().is_some_and(|port| port.version == "0.6.0") && app.mods.registry == crate::mods::Registry::Answered);
    tap(&mut rt, "mods/versions/0.5.0/use");
    settle(&mut rt);
    assert_eq!(rt.state.mods.port().map(|port| port.version.as_str()), Some("0.5.0"));
    tap(&mut rt, "mods/versions/done");
    jobs_until(&mut rt, |app| app.mods.registry == crate::mods::Registry::Answered);
    assert_eq!(rt.state.mods.port_update().map(|release| release.version), Some("0.6.0".to_owned()), "0.6.0, installed, is still newer");
    tap(&mut rt, "mods/update-port");
    assert!(rt.state.mods.work.is_none(), "using an installed version downloads nothing");
    assert_eq!(rt.state.mods.port().map(|port| port.version.as_str()), Some("0.6.0"));
    jobs_until(&mut rt, |app| app.mods.registry == crate::mods::Registry::Answered);
    tap(&mut rt, "mods/versions");
    settle(&mut rt);
    tap(&mut rt, "mods/versions/0.5.0/remove");
    assert_eq!(rt.state.mods.ports.len(), 1, "an older version can go");
}
