//! Starting a game from a set, against the fake textures.gg and its fake
//! tgg-melee, whose game writes the environment it was started with beside
//! itself.

use super::How;
use crate::mods::Disc;
use crate::state::{App, Data, Start};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use tgg_mod::FakeNet;

/// Hand the jobs' results to the app until `done` holds.
fn until(app: &mut App, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(app) {
        assert!(Instant::now() < deadline, "the jobs finished in time");
        std::thread::sleep(Duration::from_millis(10));
        crate::jobs::poll_now(app);
    }
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn find_a_match_starts_the_active_sets_game_searching_from_its_own_folder() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let net = Arc::new(FakeNet::new());
    let mut app = App::new(
        Start::FirstRun {
            game: None,
            state: Default::default(),
        },
        Default::default(),
        Data::in_folder(folder.path()),
        false,
    );
    app.mods.net = net.clone();
    let releases = tgg_mod::port_releases(&*net).expect("the fake's releases");
    tgg_mod::install_port(
        &*net,
        &releases[0],
        &app.data.ports,
        &tgg_mod::no_progress,
        &AtomicBool::new(false),
    )
    .expect("tgg-melee installs");
    let disc = folder.path().join("melee.iso");
    std::fs::write(&disc, b"a disc").expect("write the disc");
    app.mods.disc = Disc::Found(disc);
    // The player's set, building on Competitive, made active.
    let data = app.data.tgg.clone();
    let set = tgg_mod::sets::new(&data, "Friday night").expect("a new set");
    tgg_mod::sets::build_on(
        &*net,
        &data,
        &tgg_mod::Local::empty(),
        &set.key,
        "competitive",
    )
    .expect("it builds on Competitive");
    crate::mods::actions::read_disk(&mut app);
    until(&mut app, |app| app.sets.get(&set.key).is_some());
    super::actions::activate(&mut app, &set.key);

    super::actions::launch(&mut app, How::Search);
    until(&mut app, |app| {
        app.sets.trust.is_some() || app.mods.playing.is_some() || !app.sets.busy()
    });
    if app.sets.trust.is_some() {
        super::actions::trust_yes(&mut app);
        until(&mut app, |app| {
            app.mods.playing.is_some() || !app.sets.busy()
        });
    }
    assert!(
        app.mods.playing.is_some(),
        "the game started: {:?}",
        app.notice.as_ref().map(|notice| &notice.text)
    );
    assert_eq!(
        app.sets
            .session
            .as_ref()
            .map(|session| session.name.as_str()),
        Some("Friday night")
    );

    let port = app.mods.port().expect("tgg-melee").executable();
    let written = port.with_file_name(format!(
        "{}.env",
        port.file_name().expect("a file name").to_string_lossy()
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !written.is_file() {
        assert!(Instant::now() < deadline, "the game wrote its environment");
        std::thread::sleep(Duration::from_millis(10));
    }
    let env = std::fs::read_to_string(&written).expect("read the game's environment");
    assert!(
        env.lines().any(|line| line == "TGG_DIRECT_CONNECT=search"),
        "it searches: {env}"
    );
    let profiles = app.data.tgg.join("tgg").join("profiles");
    assert!(
        env.lines()
            .any(|line| line.starts_with("TGG_MODS_DIR=")
                && line.contains(&*profiles.to_string_lossy())),
        "its mods come from the set's own folder, not the mods folder: {env}"
    );

    crate::mods::actions::stop(&mut app);
    until(&mut app, |app| app.mods.playing.is_none());
    assert!(app.sets.session.is_none(), "the session ends with the game");
}
