//! Starting a game from a set, against the fake textures.gg and its fake
//! tgg-melee, whose game writes the environment it was started with beside
//! itself.

use super::How;
use crate::mods::Disc;
use crate::state::{App, Data, Start};
use std::path::Path;
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

/// An app with the fake tgg-melee installed, a disc, and Friday night
/// (which builds on Competitive) as its set.
fn ready(folder: &Path) -> (App, Arc<FakeNet>) {
    let net = Arc::new(FakeNet::new());
    let mut app = App::new(
        Start::FirstRun {
            game: None,
            state: Default::default(),
        },
        Default::default(),
        Data::in_folder(folder),
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
    let disc = folder.join("melee.iso");
    std::fs::write(&disc, b"a disc").expect("write the disc");
    app.mods.disc = Disc::Found(disc);
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
    (app, net)
}

/// Start `how` from the active set, saying yes to native code when it asks,
/// and give back what the game was started with: the lines it wrote of its
/// environment.
fn started_with(app: &mut App, how: How) -> String {
    super::actions::launch(app, how);
    until(app, |app| {
        app.sets.trust.is_some() || app.mods.playing.is_some() || !app.sets.busy()
    });
    if app.sets.trust.is_some() {
        super::actions::trust_yes(app);
        until(app, |app| app.mods.playing.is_some() || !app.sets.busy());
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
    crate::mods::actions::stop(app);
    until(app, |app| app.mods.playing.is_none());
    assert!(app.sets.session.is_none(), "the session ends with the game");
    env
}

/// The value of `name` in the environment the game wrote, if it has one.
fn value<'a>(env: &'a str, name: &str) -> Option<&'a str> {
    env.lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn find_a_match_starts_the_active_sets_game_searching_from_its_own_folder() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let (mut app, _net) = ready(folder.path());
    let env = started_with(&mut app, How::Search);
    assert_eq!(value(&env, "TGG_DIRECT_CONNECT"), Some("search"));
    assert_eq!(
        value(&env, "TGG_DIRECT_CONNECT_REPORT"),
        None,
        "a search makes no room, so it reports nothing: {env}"
    );
    let profiles = app.data.tgg.join("tgg").join("profiles");
    assert!(
        env.lines()
            .any(|line| line.starts_with("TGG_MODS_DIR=")
                && line.contains(&*profiles.to_string_lossy())),
        "its mods come from the set's own folder, not the mods folder: {env}"
    );
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn hosting_a_room_starts_the_game_with_a_report_file_of_its_own() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let (mut app, _net) = ready(folder.path());
    let report = super::actions::room_report(&app);
    // The last run's lines are still there when the next one starts.
    if let Some(parent) = report.parent() {
        std::fs::create_dir_all(parent).expect("its folder");
    }
    std::fs::write(&report, "{\"state\":\"ended\"}\n").expect("the last run's report");

    let env = started_with(&mut app, How::Host);
    assert_eq!(value(&env, "TGG_DIRECT_CONNECT"), Some("room"));
    assert_eq!(
        value(&env, "TGG_DIRECT_CONNECT_REPORT"),
        Some(&*report.to_string_lossy()),
        "the game is told where to report: {env}"
    );
    assert_eq!(
        std::fs::read_to_string(&report).expect("read the report"),
        "",
        "this run starts the report again"
    );
    assert_eq!(
        report.parent(),
        app.data.ports.parent(),
        "the report is in the app's data folder, not the game's"
    );
}

#[cfg_attr(windows, ignore = "the fake game is a shell script")]
#[test]
fn joining_a_room_starts_the_game_with_the_code_and_a_report() {
    let folder = tempfile::tempdir().expect("a temporary folder");
    let (mut app, _net) = ready(folder.path());
    let env = started_with(&mut app, How::Join("KFXD".into()));
    assert_eq!(value(&env, "TGG_DIRECT_CONNECT"), Some("room:KFXD"));
    assert!(
        value(&env, "TGG_DIRECT_CONNECT_REPORT").is_some(),
        "a joiner reads the host's mods from the report: {env}"
    );
}
