//! The distribution flow end to end against FakeNet: installing the port,
//! sets and mods, updating, turning mods on and off, uninstalling, and what
//! stops a change.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tgg_mod::{
    Action, Error, FakeNet, HttpNet, Local, ModId, Net, PortInstall, Problem, SetStatus,
    no_progress,
};

struct Player {
    _dir: tempfile::TempDir,
    mods: PathBuf,
    ports: PathBuf,
}

fn player() -> Player {
    let dir = tempfile::tempdir().expect("temp dir");
    Player {
        mods: dir.path().join("mods"),
        ports: dir.path().join("ports"),
        _dir: dir,
    }
}

fn id(text: &str) -> ModId {
    text.parse().expect("id")
}

fn go() -> AtomicBool {
    AtomicBool::new(false)
}

fn install_port(net: &FakeNet, player: &Player) -> PortInstall {
    let release = tgg_mod::port_releases(net).expect("releases").remove(0);
    tgg_mod::install_port(net, &release, &player.ports, &no_progress, &go()).expect("port")
}

fn local(player: &Player, port: &PortInstall) -> Local {
    tgg_mod::local(&player.mods, Some(&port.port())).expect("local")
}

fn install_set(net: &FakeNet, player: &Player, port: &PortInstall, name: &str) {
    let set = tgg_mod::sets(net)
        .expect("sets")
        .into_iter()
        .find(|set| set.name == name)
        .expect("set");
    let catalog = tgg_mod::catalog(net, &port.port()).expect("catalog");
    let plan = tgg_mod::plan_set(&set, &catalog, &local(player, port));
    assert!(plan.problems().is_empty(), "{:?}", plan.problems());
    tgg_mod::apply(net, &plan, &player.mods, &no_progress, &go()).expect("apply");
}

#[test]
fn installing_a_set_shows_its_size_then_installs_it_complete() {
    let net = FakeNet::new();
    let player = player();
    let empty = tgg_mod::local(&player.mods, None).expect("local before a port");
    assert_eq!(empty, Local::empty());

    let port = install_port(&net, &player);
    assert!(port.current);
    assert_eq!(port.port().game_abi, FakeNet::LAYOUT);
    let sets = tgg_mod::sets(&net).expect("sets");
    let competitive = sets.iter().find(|s| s.name == "competitive").expect("set");
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    assert!(catalog.snapshot.is_some());
    assert!(
        catalog
            .mods
            .iter()
            .all(|m| m.package.url.starts_with(FakeNet::API))
    );

    let plan = tgg_mod::plan_set(competitive, &catalog, &local(&player, &port));
    assert_eq!(plan.snapshot, catalog.snapshot);
    assert_eq!(plan.steps.len(), competitive.mods.len());
    assert!(plan.steps.iter().all(|s| s.action == Action::Install));
    assert!(plan.size() > 0);
    let stages = plan
        .steps
        .iter()
        .find(|s| s.id == id("tgg.legal-stages"))
        .expect("legal stages");
    assert_eq!(stages.manifest.version.to_string(), "0.1.1");

    let labels = std::sync::Mutex::new(Vec::new());
    let progress = |p: tgg_mod::Progress| labels.lock().unwrap().push(p.label);
    let applied = tgg_mod::apply(&net, &plan, &player.mods, &progress, &go()).expect("apply");
    assert_eq!(applied.installed.len(), competitive.mods.len());
    assert!(
        labels
            .lock()
            .unwrap()
            .contains(&"Downloading Legal stages".to_owned())
    );

    let state = local(&player, &port);
    assert_eq!(state.sets.len(), 1);
    assert_eq!(state.sets[0].status, SetStatus::Complete);
    assert!(state.unmet.is_empty());
    let netplay = state.get(&id("tgg.netplay")).expect("netplay");
    assert!(netplay.enabled);
    assert_eq!(netplay.fits, Some(true));
    assert_eq!(netplay.sets(), ["competitive"]);
    assert!(netplay.lock.as_ref().expect("lock").sha256.is_some());

    // Installing it again changes nothing.
    let again = tgg_mod::plan_set(competitive, &catalog, &state);
    assert!(again.is_empty());
    assert!(again.steps.iter().all(|s| s.action == Action::Keep));
}

#[test]
fn turning_a_set_mod_off_customizes_the_set_and_on_restores_it() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "competitive");

    tgg_mod::set_enabled(&player.mods, &id("tgg.quick-start"), false).expect("off");
    let state = local(&player, &port);
    assert_eq!(state.sets[0].status, SetStatus::Customized);
    assert!(!state.get(&id("tgg.quick-start")).expect("mod").enabled);

    tgg_mod::set_enabled(&player.mods, &id("tgg.quick-start"), true).expect("on");
    assert_eq!(local(&player, &port).sets[0].status, SetStatus::Complete);

    assert!(matches!(
        tgg_mod::set_enabled(&player.mods, &id("tgg.nope"), true),
        Err(Error::NotInstalled(_))
    ));
}

#[test]
fn updating_follows_the_newest_set_and_releases() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "competitive");
    tgg_mod::set_enabled(&player.mods, &id("tgg.legal-stages"), false).expect("off");

    // A newer release, and a set version that drops one mod and adds one.
    net.publish_mod("tgg.legal-stages", "Legal stages", "0.1.2", &[]);
    net.publish_mod("tgg.legal-stages", "Legal stages", "0.2.0", &[]);
    net.publish_mod("tgg.ledge-grabs", "Ledge grabs", "1.0.0", &[]);
    let mut set = net.set("competitive").expect("set");
    set.version = 2;
    set.mods.retain(|m| m.id != id("tgg.skip-results"));
    set.mods.push(tgg_mod::SetMod {
        id: id("tgg.ledge-grabs"),
        range: "^1".parse().expect("range"),
    });
    net.publish_set(set);

    let sets = tgg_mod::sets(&net).expect("sets");
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let mut state = local(&player, &port);
    state.check_updates(&sets, &catalog);
    assert_eq!(state.sets[0].status, SetStatus::Outdated);
    assert_eq!(state.sets[0].update, Some(2));
    let stages = state.get(&id("tgg.legal-stages")).expect("stages");
    // 0.2.0 is outside the set's ^0.1.
    assert_eq!(
        stages.update.as_ref().map(ToString::to_string).as_deref(),
        Some("0.1.2")
    );

    let plan = tgg_mod::plan_updates(&sets, &catalog, &state);
    assert!(plan.problems().is_empty(), "{:?}", plan.problems());
    let action = |name: &str| {
        plan.steps
            .iter()
            .find(|s| s.id == id(name))
            .map(|s| s.action.clone())
    };
    assert!(matches!(
        action("tgg.legal-stages"),
        Some(Action::Update { .. })
    ));
    assert_eq!(action("tgg.ledge-grabs"), Some(Action::Install));
    assert_eq!(action("tgg.netplay"), Some(Action::Keep));
    assert_eq!(action("tgg.skip-results"), None);
    tgg_mod::apply(&net, &plan, &player.mods, &no_progress, &go()).expect("apply");

    let state = local(&player, &port);
    assert_eq!(state.sets[0].set.version, 2);
    // Still off: an update keeps the player's choice.
    assert_eq!(state.sets[0].status, SetStatus::Customized);
    let stages = state.get(&id("tgg.legal-stages")).expect("stages");
    assert_eq!(stages.manifest.version.to_string(), "0.1.2");
    assert!(!stages.enabled);
    let dropped = state.get(&id("tgg.skip-results")).expect("still installed");
    assert!(dropped.sets().is_empty());
}

#[test]
fn uninstalling_a_set_keeps_mods_installed_on_their_own_or_by_another_set() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "replays");
    install_set(&net, &player, &port, "competitive");
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let plan = tgg_mod::plan_mods(
        &[(id("tgg.replay-recorder"), "*".parse().expect("range"))],
        &catalog,
        &local(&player, &port),
    );
    assert_eq!(plan.steps[0].action, Action::Keep);
    tgg_mod::apply(&net, &plan, &player.mods, &no_progress, &go()).expect("apply");

    tgg_mod::uninstall_set(&player.mods, "replays").expect("uninstall set");
    let state = local(&player, &port);
    assert!(state.set("replays").is_none());
    assert!(state.get(&id("tgg.replay-player")).is_none());
    assert!(
        state
            .get(&id("tgg.replay-recorder"))
            .expect("direct")
            .direct()
    );
    assert_eq!(state.sets[0].status, SetStatus::Complete);

    tgg_mod::uninstall(&player.mods, &id("tgg.netplay")).expect("uninstall");
    let state = local(&player, &port);
    assert_eq!(state.sets[0].status, SetStatus::Incomplete);
    assert!(!state.lock.mods.contains_key(&id("tgg.netplay")));
    assert_eq!(state.unmet.len(), 1, "direct connect needs netplay");
    assert!(matches!(
        tgg_mod::uninstall_set(&player.mods, "replays"),
        Err(Error::SetNotInstalled(_))
    ));
}

#[test]
fn plans_report_what_cant_be_had_or_wont_load() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let state = local(&player, &port);
    let plan = tgg_mod::plan_mods(
        &[
            (id("tgg.direct-connect"), "*".parse().expect("range")),
            (id("tgg.netplay"), "^0.3".parse().expect("range")),
        ],
        &catalog,
        &state,
    );
    assert!(plan.problems().iter().any(|p| matches!(
        p,
        Problem::Unavailable { id: missing, .. } if *missing == id("tgg.netplay")
    )));
    let unmet = plan
        .problems()
        .iter()
        .find(|p| matches!(p, Problem::Unmet { .. }))
        .expect("unmet");
    assert_eq!(
        unmet.to_string(),
        "tgg.direct-connect needs tgg.netplay ^0.2"
    );

    // Another layout's catalog has nothing for this game.
    let mut other = port.port();
    other.game_abi = "other-layout".into();
    let empty = tgg_mod::catalog(&net, &other).expect("catalog");
    let set = net.set("replays").expect("set");
    assert_eq!(
        tgg_mod::plan_set(&set, &empty, &state).problems().len(),
        set.mods.len()
    );
}

#[test]
fn a_mod_built_for_another_layout_leaves_its_set_incomplete() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "replays");
    let path = player.mods.join("tgg.replay-player/manifest.json");
    let mut manifest =
        tgg_mod::Manifest::parse(&std::fs::read(&path).expect("read")).expect("manifest");
    manifest.game_abi = Some("other-layout".into());
    std::fs::write(&path, manifest.to_json()).expect("write");

    let state = local(&player, &port);
    assert_eq!(
        state.get(&id("tgg.replay-player")).expect("mod").fits,
        Some(false)
    );
    assert_eq!(state.sets[0].status, SetStatus::Incomplete);
    // Without a port nothing is checked.
    let unchecked = tgg_mod::local(&player.mods, None).expect("local");
    assert_eq!(
        unchecked.get(&id("tgg.replay-player")).expect("mod").fits,
        None
    );
}

#[test]
fn a_package_that_isnt_the_planned_one_is_refused() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let set = net.set("replays").expect("set");
    let mut plan = tgg_mod::plan_set(&set, &catalog, &local(&player, &port));
    plan.steps[0].download.as_mut().expect("download").sha256 = "0".repeat(64);
    let result = tgg_mod::apply(&net, &plan, &player.mods, &no_progress, &go());
    assert!(matches!(result, Err(Error::Checksum { .. })), "{result:?}");
    let state = local(&player, &port);
    assert!(state.mods.is_empty());
    // The set is recorded, missing its mods.
    assert_eq!(state.sets[0].status, SetStatus::Incomplete);
}

#[test]
fn cancelling_between_mods_keeps_what_finished_in_the_lock() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let set = net.set("competitive").expect("set");
    let plan = tgg_mod::plan_set(&set, &catalog, &local(&player, &port));
    let cancel = go();
    let progress = |p: tgg_mod::Progress| {
        if p.label.starts_with("Installing") {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    let result = tgg_mod::apply(&net, &plan, &player.mods, &progress, &cancel);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    let state = local(&player, &port);
    assert_eq!(state.mods.len(), 1);
    assert_eq!(state.lock.mods.len(), 1);
}

#[test]
fn another_tgg_holding_the_folder_makes_changes_busy() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "replays");
    std::fs::write(
        player.mods.join(tgg_mod::lock::BUSY_FILE),
        std::process::id().to_string(),
    )
    .expect("busy");
    assert!(matches!(
        tgg_mod::set_enabled(&player.mods, &id("tgg.replay-player"), false),
        Err(Error::Busy)
    ));
    assert!(matches!(
        tgg_mod::uninstall_set(&player.mods, "replays"),
        Err(Error::Busy)
    ));
}

#[test]
fn offline_the_registry_fails_and_local_state_still_reads() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "replays");
    net.set_offline(true);
    let error = tgg_mod::sets(&net).expect_err("offline");
    assert!(matches!(error, Error::Offline(_)));
    assert_eq!(
        error.to_string(),
        "can't reach textures.gg; what's installed still works"
    );
    assert!(std::error::Error::source(&error).is_some());
    assert!(matches!(
        tgg_mod::port_releases(&net),
        Err(Error::Offline(_))
    ));
    assert_eq!(local(&player, &port).mods.len(), 3);
}

#[test]
fn a_newer_port_becomes_current_and_the_older_stays_until_removed() {
    let net = FakeNet::new();
    let player = player();
    let old = install_port(&net, &player);
    let installed = tgg_mod::installed_ports(&player.ports).expect("installed");
    let releases = tgg_mod::port_releases(&net).expect("releases");
    assert!(tgg_mod::port_update(&installed, &releases).is_none());

    net.publish_port("0.6.0");
    let releases = tgg_mod::port_releases(&net).expect("releases");
    let update = tgg_mod::port_update(&installed, &releases).expect("update");
    assert_eq!(update.version, "0.6.0");
    let new =
        tgg_mod::install_port(&net, &update, &player.ports, &no_progress, &go()).expect("install");
    assert!(new.current);
    let installed = tgg_mod::installed_ports(&player.ports).expect("installed");
    assert_eq!(installed.len(), 2);
    assert!(!installed[0].current && installed[1].current);

    tgg_mod::remove_port(&player.ports, "0.6.0").expect("remove");
    let installed = tgg_mod::installed_ports(&player.ports).expect("installed");
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].version, old.version);
    assert!(installed[0].current);
}

#[test]
fn a_damaged_port_download_is_refused() {
    let net = FakeNet::new();
    let player = player();
    let mut release = tgg_mod::port_releases(&net).expect("releases").remove(0);
    release.files.game.sha256 = "0".repeat(64);
    let result = tgg_mod::install_port(&net, &release, &player.ports, &no_progress, &go());
    assert!(
        matches!(result, Err(Error::ReleaseChecksum { .. })),
        "{result:?}"
    );
    assert!(
        tgg_mod::installed_ports(&player.ports)
            .expect("installed")
            .is_empty()
    );
}

/// Copy the fake mirror's newest release into `folder` as a prerelease.
fn folder_mirror(net: &FakeNet, folder: &Path) {
    let release = tgg_mod::port_releases(net).expect("releases").remove(0);
    let version = folder.join(&release.version);
    std::fs::create_dir_all(&version).expect("folder");
    let target = &release.target;
    let name = format!("release-{target}.json");
    let url = format!("{}/{}/{name}", FakeNet::RELEASES, release.version);
    std::fs::write(version.join(&name), net.get(&url, 1 << 20).expect("json")).expect("write");
    for file in [&release.files.game, &release.files.sdk] {
        let url = format!("{}/{}/{}", FakeNet::RELEASES, release.version, file.name);
        std::fs::write(
            version.join(&file.name),
            net.get(&url, 1 << 20).expect("file"),
        )
        .expect("write");
    }
    std::fs::write(
        folder.join("latest-prerelease.json"),
        format!("{{\"version\": \"{}\"}}", release.version),
    )
    .expect("write");
}

#[test]
fn a_folder_mirror_installs_and_reports_a_prerelease() {
    let net = FakeNet::new();
    let player = player();
    let mirror = player._dir.path().join("mirror");
    folder_mirror(&net, &mirror);
    let http = HttpNet::with_urls(
        "tgg-test",
        "https://unused.invalid",
        &mirror.to_string_lossy(),
    );
    let release = tgg_mod::port_releases(&http).expect("releases").remove(0);
    assert!(release.prerelease);
    let port = tgg_mod::install_port(&http, &release, &player.ports, &no_progress, &go())
        .expect("install");
    assert!(port.executable().is_file());
    assert!(matches!(
        tgg_mod::ports::port_release(&http, "9.9.9"),
        Err(Error::NoRelease { .. })
    ));
}

#[cfg(unix)]
#[test]
fn while_the_game_runs_the_mods_folder_doesnt_change() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "replays");
    let iso = player._dir.path().join("melee.iso");
    std::fs::write(&iso, b"").expect("iso");

    let mut running = tgg_mod::launch(&port, &iso, &player.mods).expect("launch");
    // The fake game is a script; give the shell a moment to start it.
    std::thread::sleep(std::time::Duration::from_millis(200));
    // The check reads the pid file, so it holds for any front end.
    assert!(tgg_mod::game_running(&player.mods));
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let set = net.set("competitive").expect("set");
    let plan = tgg_mod::plan_set(&set, &catalog, &local(&player, &port));
    for result in [
        tgg_mod::apply(&net, &plan, &player.mods, &no_progress, &go()).map(|_| ()),
        tgg_mod::set_enabled(&player.mods, &id("tgg.replay-player"), false),
        tgg_mod::uninstall(&player.mods, &id("tgg.replay-player")),
        tgg_mod::uninstall_set(&player.mods, "replays"),
        tgg_mod::launch(&port, &iso, &player.mods).map(|_| ()),
    ] {
        assert!(matches!(result, Err(Error::GameRunning)), "{result:?}");
    }

    running.kill().expect("kill");
    assert!(!tgg_mod::game_running(&player.mods));
    assert!(!player.mods.join(tgg_mod::lock::PID_FILE).exists());
    tgg_mod::set_enabled(&player.mods, &id("tgg.replay-player"), false).expect("off");
}

#[test]
fn a_stale_pid_file_doesnt_block() {
    let player = player();
    std::fs::create_dir_all(&player.mods).expect("mods");
    // This test's own process isn't the game.
    let me = std::env::current_exe().expect("exe");
    std::fs::write(
        player.mods.join(tgg_mod::lock::PID_FILE),
        format!("{}\n/opt/tgg-melee/tgg-melee\n", std::process::id()),
    )
    .expect("pid file");
    assert!(!tgg_mod::game_running(&player.mods));
    std::fs::write(
        player.mods.join(tgg_mod::lock::PID_FILE),
        format!("{}\n{}\n", std::process::id(), me.display()),
    )
    .expect("pid file");
    assert!(tgg_mod::game_running(&player.mods));
}

#[test]
fn updating_one_set_leaves_the_others_and_mods_installed_on_their_own() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    install_set(&net, &player, &port, "competitive");
    install_set(&net, &player, &port, "replays");
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let plan = tgg_mod::plan_mods(
        &[(id("tgg.legal-stages"), "0.1.0".parse().expect("range"))],
        &catalog,
        &local(&player, &port),
    );
    assert!(plan.problems().is_empty());

    net.publish_mod("tgg.netplay", "Netplay", "0.2.1", &[]);
    net.publish_mod("tgg.replay-player", "Replay player", "0.2.1", &[]);
    let sets = tgg_mod::sets(&net).expect("sets");
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let state = local(&player, &port);
    let plan = tgg_mod::plan_set_update("replays", &sets, &catalog, &state).expect("plan");
    let changes: Vec<&ModId> = plan
        .steps
        .iter()
        .filter(|step| step.action != Action::Keep)
        .map(|step| &step.id)
        .collect();
    assert_eq!(changes, [&id("tgg.replay-player")]);
    assert_eq!(plan.sets.len(), 1);
    assert!(matches!(
        tgg_mod::plan_set_update("nothing", &sets, &catalog, &state),
        Err(Error::SetNotInstalled(_))
    ));
}

#[test]
fn a_mod_the_registry_lacks_is_unknown_and_one_not_built_is_unavailable() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let state = local(&player, &port);
    let want = [
        (id("nobody.nothing"), "*".parse().expect("range")),
        (id("tgg.netplay"), "*".parse().expect("range")),
    ];
    let unknown = Problem::Unknown {
        id: id("nobody.nothing"),
    };

    // Another layout's catalog has no builds, but names every mod.
    let mut other = port.port();
    other.game_abi = "other-layout".into();
    let mut catalog = tgg_mod::catalog(&net, &other).expect("catalog");
    let plan = tgg_mod::plan_mods(&want, &catalog, &state);
    assert_eq!(plan.problems().len(), 2);
    assert!(plan.problems().contains(&unknown));
    let unavailable = Problem::Unavailable {
        id: id("tgg.netplay"),
        ranges: vec!["*".parse().expect("range")],
    };
    assert!(plan.problems().contains(&unavailable));
    assert_eq!(
        unavailable.to_string(),
        "tgg.netplay isn't built for this game yet"
    );
    assert_eq!(unknown.to_string(), "textures.gg has no mod nobody.nothing");

    // Without the registry's ids, a mod a set lists is known.
    catalog.ids.clear();
    let set = net.set("replays").expect("set");
    let plan = tgg_mod::plan_set(&set, &catalog, &state);
    assert!(
        plan.problems()
            .iter()
            .all(|p| matches!(p, Problem::Unavailable { .. }))
    );
    let plan = tgg_mod::plan_mods(&want, &catalog, &state);
    assert!(
        plan.problems()
            .iter()
            .all(|p| matches!(p, Problem::Unknown { .. }))
    );
}

#[test]
fn plans_bring_what_their_mods_depend_on() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let state = local(&player, &port);

    let plan = tgg_mod::plan_mods(
        &[(id("tgg.direct-connect"), "*".parse().expect("range"))],
        &catalog,
        &state,
    );
    assert!(plan.problems().is_empty(), "{:?}", plan.problems());
    let netplay = plan
        .steps
        .iter()
        .find(|step| step.id == id("tgg.netplay"))
        .expect("netplay comes along");
    assert!(netplay.direct);

    // A set that lists a mod but not what it needs.
    let mut set = net.set("competitive").expect("set");
    set.name = "connect".into();
    set.mods.retain(|m| m.id == id("tgg.direct-connect"));
    let plan = tgg_mod::plan_set(&set, &catalog, &state);
    assert_eq!(plan.steps.len(), 2);
    tgg_mod::apply(&net, &plan, &player.mods, &no_progress, &go()).expect("apply");
    let state = local(&player, &port);
    assert_eq!(
        state.get(&id("tgg.netplay")).expect("installed").sets(),
        ["connect"]
    );
    assert!(state.unmet.is_empty());
    tgg_mod::uninstall_set(&player.mods, "connect").expect("uninstall");
    assert!(local(&player, &port).mods.is_empty());

    // Mods that need each other, and one that needs what isn't there.
    net.publish_mod("test.a", "A", "0.1.0", &[("test.b", "^0.1")]);
    net.publish_mod("test.b", "B", "0.1.0", &[("test.a", "^0.1")]);
    net.publish_mod("test.c", "C", "0.1.0", &[("test.a", "^9")]);
    let catalog = tgg_mod::catalog(&net, &port.port()).expect("catalog");
    let state = local(&player, &port);
    let plan = tgg_mod::plan_mods(
        &[(id("test.a"), "*".parse().expect("range"))],
        &catalog,
        &state,
    );
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.problems().is_empty());
    let plan = tgg_mod::plan_mods(
        &[(id("test.c"), "*".parse().expect("range"))],
        &catalog,
        &state,
    );
    assert_eq!(
        plan.problems(),
        [Problem::Unmet {
            id: id("test.c"),
            missing: vec!["test.a ^9".into()],
        }]
    );
}

/// A registry that answers 404 to everything.
struct Empty;

impl Net for Empty {
    fn get(&self, url: &str, _: u64) -> Result<Vec<u8>, tgg_mod::NetError> {
        Err(tgg_mod::NetError::NotFound { url: url.into() })
    }

    fn api_url(&self) -> &str {
        "https://empty.textures.gg"
    }

    fn releases_url(&self) -> &str {
        "https://empty.textures.gg/tgg-melee"
    }
}

#[test]
fn a_registry_with_nothing_yet_says_so() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    assert!(tgg_mod::sets(&Empty).expect("no sets").is_empty());
    let error = tgg_mod::catalog(&Empty, &port.port()).expect_err("no catalog");
    assert_eq!(
        error.to_string(),
        format!(
            "textures.gg has no mods for tgg-melee {} yet",
            FakeNet::PORT_VERSION
        )
    );
}
