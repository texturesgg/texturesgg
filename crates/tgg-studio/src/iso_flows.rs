//! Flows only a real fighter shows: the shared models it holds, and the
//! camera kept across its costumes. They read a clean Melee NTSC 1.02 disc
//! image named by `TGG_MELEE_ISO`:
//!
//! ```text
//! TGG_MELEE_ISO=melee.iso cargo test -p tgg-studio --features melee-iso
//! ```

use crate::melee::actions as melee;
use crate::shell;
use crate::state::{App, Data, Start};
use melee_dat::Character;
use rune::{Env, Runtime};
use std::path::PathBuf;
use tgg_app::settings::Settings;

fn runtime() -> (Runtime<App>, tempfile::TempDir) {
    let iso = PathBuf::from(
        std::env::var_os("TGG_MELEE_ISO").expect("TGG_MELEE_ISO names a clean Melee disc image"),
    );
    let game = tgg_app::Game::open(&iso).expect("a clean Melee NTSC 1.02 disc");
    let start = Start::Game(tgg_app::game_references(game).expect("references"));
    let folder = tempfile::tempdir().expect("temp dir");
    let data = Data::in_folder(folder.path());
    let mut rt = Runtime::new(
        App::new(start, Settings::default(), data, false),
        shell::view,
        (1280.0, 800.0),
        Env::new(true, 2.0),
    );
    rt.frame(0.0);
    (rt, folder)
}

/// Let the opening's motion come to rest: presses land where things are drawn.
fn settle(rt: &mut Runtime<App>) {
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn fox_opens_with_the_models_his_moves_draw() {
    let (mut rt, _data) = runtime();
    let fox = Character::from_code("Fx").expect("Fox");
    rt.update(|app, _| melee::open_fighter(app, fox));
    settle(&mut rt);
    let shown = rt.state.stage.shown.as_ref().expect("Fox is on the stage");
    assert!(
        !shown.shared.is_empty(),
        "a stock Fox plays with his shared models"
    );
}

#[test]
fn a_new_costume_keeps_the_camera() {
    let (mut rt, _data) = runtime();
    let fox = Character::from_code("Fx").expect("Fox");
    rt.update(|app, _| melee::open_fighter(app, fox));
    rt.update(|app, _| app.stage.shown.as_mut().expect("on the stage").orbit.yaw = 1.0);
    settle(&mut rt);
    let orange: melee_dat::MeleeSlot = "PlFxOr.dat".parse().expect("a slot");
    rt.update(|app, _| melee::choose_costume(app, orange));
    let shown = rt.state.stage.shown.as_ref().expect("still on the stage");
    assert_eq!(shown.slot.file_name(), "PlFxOr.dat");
    assert!((shown.orbit.yaw - 1.0).abs() < 1e-9);
}
