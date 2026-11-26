//! The player's sets end to end against FakeNet: keeping them, stacking on
//! textures.gg sets, launching one through the profile path, `latest`
//! online and offline, and the mods that decide a set's queue.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tgg_mod::sets::{self, Class, Group, INSTALLED, MySet, SetPlan, Want};
use tgg_mod::{Error, FakeNet, Local, ModId, PortInstall, Set, SetMod, WithdrawMode, no_progress};

struct Player {
    _dir: tempfile::TempDir,
    data: PathBuf,
    mods: PathBuf,
    port: PortInstall,
}

impl Player {
    fn local(&self) -> Local {
        tgg_mod::local(&self.mods, Some(&self.port.port())).expect("local")
    }

    /// Install `ids` into the player's own mods folder, as `tgg mod
    /// install` does.
    fn install(&self, net: &FakeNet, ids: &[&str]) {
        let catalog = tgg_mod::catalog(net, &self.port.port()).expect("catalog");
        let want: Vec<_> = ids
            .iter()
            .map(|m| (id(m), "*".parse().expect("range")))
            .collect();
        let plan = tgg_mod::plan_mods(&want, &catalog, &self.local());
        tgg_mod::apply_cached(
            net,
            &plan,
            &self.mods,
            Some(&tgg_mod::packages_dir(&self.data)),
            &no_progress,
            &go(),
        )
        .expect("install");
    }

    fn plan(&self, net: &FakeNet, set: &MySet, skins: bool) -> SetPlan {
        sets::plan(net, &self.data, &self.mods, set, skins).expect("plan")
    }

    fn set(&self, key: &str) -> MySet {
        sets::load(&self.data, &self.local(), key).expect("set")
    }
}

fn player(net: &FakeNet) -> Player {
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().to_owned();
    let release = tgg_mod::port_releases(net).expect("releases").remove(0);
    let port = tgg_mod::install_port(net, &release, &data.join("tgg/ports"), &no_progress, &go())
        .expect("port");
    Player {
        mods: data.join("mods"),
        data,
        port,
        _dir: dir,
    }
}

fn go() -> AtomicBool {
    AtomicBool::new(false)
}

fn id(text: &str) -> ModId {
    text.parse().expect("id")
}

fn publish_set(net: &FakeNet, name: &str, mods: &[(&str, &str)], builds_on: &[&str]) {
    net.publish_set(Set {
        name: name.into(),
        title: name.into(),
        description: String::new(),
        version: 1,
        mods: mods
            .iter()
            .map(|(m, range)| SetMod {
                id: id(m),
                range: range.parse().expect("range"),
            })
            .collect(),
        owner: None,
        builds_on: builds_on.iter().map(|s| (*s).to_owned()).collect(),
    });
}

#[cfg(unix)]
fn listing(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn row<'a>(plan: &'a SetPlan, m: &str) -> &'a sets::SetMod {
    plan.mods
        .iter()
        .find(|row| row.id.as_str() == m)
        .unwrap_or_else(|| panic!("no {m} in {:#?}", plan.mods))
}

/// The sets folder's rules in one pass: names are the player's and unique,
/// keys are safe file names that follow them, the mods folder is a set of
/// its own, a set added from textures.gg is locked, and building on is one
/// level and four sets deep.
#[test]
fn sets_are_kept_by_name_with_one_active() {
    let net = FakeNet::new();
    let player = player(&net);
    player.install(&net, &["tgg.skip-results"]);
    let local = player.local();
    let data = &player.data;

    assert_eq!(sets::active(data), INSTALLED);
    let mine = sets::new(data, " Friday: night ").expect("new");
    assert_eq!(
        (mine.key.as_str(), mine.name.as_str()),
        ("friday-night", "Friday: night")
    );
    assert!(matches!(
        sets::new(data, "FRIDAY: NIGHT"),
        Err(Error::SetExists(_))
    ));
    assert!(matches!(
        sets::new(data, "Installed mods"),
        Err(Error::SetExists(_))
    ));

    sets::activate(data, &mine.key).expect("use");
    let renamed = sets::rename(data, &local, &mine.key, "Fridays").expect("rename");
    assert_eq!(renamed.key, "fridays");
    assert_eq!(sets::active(data), "fridays");

    // Building on: live set lines, one level, four at most.
    publish_set(&net, "stacked", &[], &["competitive"]);
    for name in ["a", "b", "c"] {
        publish_set(&net, name, &[], &[]);
    }
    for link in ["competitive", "replays", "a", "b", "competitive"] {
        sets::build_on(&net, data, &local, "fridays", link).expect("build on");
    }
    let built: Vec<String> = player.set("fridays").builds_on().map(Into::into).collect();
    assert_eq!(built, ["competitive", "replays", "a", "b"]);
    assert!(matches!(
        sets::build_on(&net, data, &local, "fridays", "c"),
        Err(Error::TooManySets)
    ));
    assert!(matches!(
        sets::build_on(&net, data, &local, "fridays", "stacked"),
        Err(Error::CantBuildOn(_))
    ));

    // A set from textures.gg is one locked line, added once.
    let competitive = sets::get(&net, data, "competitive").expect("get");
    assert!(competitive.locked);
    assert_eq!(
        sets::get(&net, data, "competitive").expect("again").key,
        competitive.key
    );
    assert!(matches!(
        sets::build_on(&net, data, &local, &competitive.key, "replays"),
        Err(Error::SetLocked(_))
    ));

    // "Save as a new set": the mods folder's textures.gg mods, at latest.
    let copy = sets::copy(data, &local, INSTALLED, None).expect("copy");
    assert_eq!(copy.name, "Installed mods copy");
    assert_eq!(
        copy.lines,
        [sets::Line::Mod(sets::ModLine::new(
            id("tgg.skip-results"),
            Want::Latest
        ))]
    );
    assert!(matches!(
        sets::rename(data, &local, INSTALLED, "Mine"),
        Err(Error::InstalledSet)
    ));

    let names: Vec<String> = sets::list(data, &local)
        .expect("list")
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(
        names,
        [
            "Installed mods",
            "Competitive",
            "Fridays",
            "Installed mods copy"
        ]
    );

    sets::remove(data, "fridays").expect("remove");
    assert_eq!(sets::active(data), INSTALLED);
    assert!(matches!(
        sets::load(data, &local, "../fridays"),
        Err(Error::NoMySet(_))
    ));
}

/// Stacking: a set's own lines win (a version, or left out), two sets that
/// disagree meet at the newest release both accept or skip the mod saying
/// why, a withdrawn mod is skipped rather than stopping the set, skins come
/// along without being written in, and a set textures.gg takes down is
/// copied in.
#[test]
fn a_set_builds_on_sets_and_its_own_lines_win() {
    let net = FakeNet::new();
    net.publish_mod("tgg.skins", "Skins", "0.1.0", &[]);
    publish_set(&net, "old-stages", &[("tgg.legal-stages", "0.1.0")], &[]);
    publish_set(&net, "new-stages", &[("tgg.legal-stages", ">=0.1.1")], &[]);
    let player = player(&net);
    let data = &player.data;
    let local = player.local();
    let mine = sets::new(data, "Mine").expect("new");
    for link in ["competitive", "replays"] {
        sets::build_on(&net, data, &local, &mine.key, link).expect("build on");
    }
    let mut mine = player.set("mine");
    mine.add(
        id("tgg.legal-stages"),
        Want::Exact("0.1.0".parse().expect("v")),
    );
    mine.leave_out(&id("tgg.quick-start"), true);
    sets::save(data, &mine).expect("save");

    let plan = player.plan(&net, &mine, true);
    assert!(plan.notices.is_empty(), "{:?}", plan.notices);
    let stages = row(&plan, "tgg.legal-stages");
    assert_eq!(
        stages.version.as_ref().map(ToString::to_string).as_deref(),
        Some("0.1.0")
    );
    assert!(matches!(&stages.group, Group::FromSet { set, .. } if set == "competitive"));
    assert_eq!(row(&plan, "tgg.quick-start").group, Group::LeftOut);
    assert!(!plan.plan.mods.iter().any(|m| m.id == id("tgg.quick-start")));
    assert!(matches!(
        row(&plan, "tgg.replay-player").group,
        Group::FromSet { .. }
    ));
    assert_eq!(row(&plan, "tgg.skins").group, Group::Skins);
    assert!(
        player.set("mine").mod_line(&id("tgg.skins")).is_none(),
        "skins aren't written in"
    );

    // Two sets that take versions with nothing in common.
    let mut stacked = player.set("mine");
    stacked.drop_line("tgg.legal-stages");
    stacked
        .lines
        .retain(|line| !matches!(line, sets::Line::Set { .. }));
    sets::save(data, &stacked).expect("save");
    sets::build_on(&net, data, &local, "mine", "old-stages").expect("old");
    sets::build_on(&net, data, &local, "mine", "new-stages").expect("new");
    let plan = player.plan(&net, &player.set("mine"), false);
    assert_eq!(
        row(&plan, "tgg.legal-stages").skipped.as_deref(),
        Some("The sets it comes from don't share a version of it.")
    );

    // Withdrawn: skipped with the reason, and the rest still prepares.
    sets::build_on(&net, data, &local, "mine", "competitive").expect("competitive");
    net.withdraw("tgg.skip-results", "0.1.0", "it desyncs", WithdrawMode::Off);
    let mine = player.set("mine");
    let plan = player.plan(&net, &mine, false);
    assert_eq!(
        row(&plan, "tgg.skip-results").skipped.as_deref(),
        Some("textures.gg withdrew it (it desyncs).")
    );
    let prepared = sets::prepare(&net, &plan, data, &mine, &no_progress, &go()).expect("prepare");
    assert!(prepared.folder.join("tgg.netplay").is_dir());
    assert!(!prepared.folder.join("tgg.skip-results").exists());

    // Taken down: its mods become the set's own lines, said once.
    net.remove_set("competitive");
    let plan = player.plan(&net, &mine, false);
    assert_eq!(
        plan.notices,
        ["textures.gg took Competitive down, so its mods are now part of Mine"]
    );
    let mine = player.set("mine");
    assert!(!mine.builds_on().any(|link| link == "competitive"));
    assert!(mine.mod_line(&id("tgg.netplay")).is_some());
    assert_eq!(row(&plan, "tgg.netplay").group, Group::Added);
}

/// A set runs from its own folder built from the package cache: nothing in
/// the player's mods folder changes, and the game keeps the set's own
/// settings.
#[cfg(unix)]
#[test]
fn a_set_runs_from_its_own_folder_and_leaves_the_players_mods_alone() {
    let net = FakeNet::new();
    let player = player(&net);
    player.install(&net, &["tgg.replay-recorder"]);
    let before = listing(&player.mods);
    let mine = sets::new(&player.data, "Comp").expect("new");
    sets::build_on(
        &net,
        &player.data,
        &player.local(),
        &mine.key,
        "competitive",
    )
    .expect("on");
    let mine = player.set("comp");

    let plan = player.plan(&net, &mine, false);
    assert_eq!(plan.offline, None);
    assert_eq!(plan.skipped().count(), 0, "{:?}", plan.mods);
    let prepared =
        sets::prepare(&net, &plan, &player.data, &mine, &no_progress, &go()).expect("prepare");
    assert!(
        prepared
            .folder
            .join("tgg.legal-stages/manifest.json")
            .is_file()
    );
    assert!(!prepared.folder.join("tgg.replay-recorder").exists());
    let settings = sets::settings_file(&player.data, "comp");
    assert!(
        prepared
            .env()
            .contains(&("TGG_SETTINGS".into(), settings.into_os_string()))
    );

    let iso = player.data.join("melee.iso");
    std::fs::write(&iso, b"").expect("iso");
    let mut running =
        tgg_mod::launch_profile(&prepared.port, &iso, &prepared, None, None).expect("launch");
    assert_eq!(listing(&player.mods), before);
    running.kill().expect("kill");
    let _ = running.wait();
    assert!(!prepared.folder.exists(), "the folder goes with the game");
}

/// `latest` takes the newest release on each launch online, and the one the
/// last launch used when offline, from the package cache.
#[test]
fn latest_follows_updates_online_and_keeps_the_last_offline() {
    let net = FakeNet::new();
    let player = player(&net);
    let mut mine = sets::new(&player.data, "Stages").expect("new");
    mine.add(id("tgg.legal-stages"), Want::Latest);
    sets::save(&player.data, &mine).expect("save");
    let version = |plan: &SetPlan| plan.plan.mods[0].version.to_string();

    let plan = player.plan(&net, &mine, false);
    assert_eq!(version(&plan), "0.1.1");
    sets::prepare(&net, &plan, &player.data, &mine, &no_progress, &go()).expect("prepare");
    let mine = player.set("stages");
    assert_eq!(mine.resolved[&id("tgg.legal-stages")].to_string(), "0.1.1");

    net.publish_mod("tgg.legal-stages", "Legal stages", "0.2.0", &[]);
    let plan = player.plan(&net, &mine, false);
    assert_eq!((version(&plan), plan.offline), ("0.2.0".into(), None));

    net.set_offline(true);
    let plan = player.plan(&net, &mine, false);
    assert!(plan.offline.is_some());
    assert_eq!(version(&plan), "0.1.1");
    assert!(plan.plan.mods[0].cached);
    sets::prepare(&net, &plan, &player.data, &mine, &no_progress, &go()).expect("offline");
}

/// A set's queue is its gameplay mods: display mods don't change it, a mod
/// a gameplay mod depends on is gameplay too, and a left-out mod isn't
/// there.
#[test]
fn display_mods_and_left_out_mods_dont_change_a_sets_queue() {
    let net = FakeNet::new();
    for native in [
        "tgg.direct-connect",
        "tgg.competitive-rules",
        "tgg.quick-start",
    ] {
        net.set_native(native);
    }
    let player = player(&net);
    let local = player.local();
    let a = sets::new(&player.data, "A").expect("a");
    sets::build_on(&net, &player.data, &local, &a.key, "competitive").expect("on");
    let mut b = sets::copy(&player.data, &local, &a.key, Some("B")).expect("b");
    b.add(id("tgg.replay-browser"), Want::Latest);
    b.leave_out(&id("tgg.quick-start"), true);

    let plan_a = player.plan(&net, &player.set("a"), false);
    let plan_b = player.plan(&net, &b, false);
    assert_eq!(
        row(&plan_a, "tgg.netplay").class,
        Some(Class::Gameplay),
        "a dependency"
    );
    assert_eq!(row(&plan_a, "tgg.legal-stages").class, Some(Class::Display));
    assert_eq!(
        row(&plan_b, "tgg.replay-browser").class,
        Some(Class::Display)
    );

    let queue = |plan: &SetPlan| -> Vec<String> {
        plan.gameplay()
            .iter()
            .map(|m| format!("{} {}", m.id, m.version))
            .collect()
    };
    assert_eq!(
        queue(&plan_a),
        [
            "tgg.competitive-rules 0.1.0",
            "tgg.direct-connect 0.1.0",
            "tgg.netplay 0.2.0",
            "tgg.quick-start 0.1.0"
        ]
    );
    assert_eq!(
        queue(&plan_b),
        [
            "tgg.competitive-rules 0.1.0",
            "tgg.direct-connect 0.1.0",
            "tgg.netplay 0.2.0"
        ]
    );
    b.leave_out(&id("tgg.quick-start"), false);
    assert_eq!(queue(&plan_a), queue(&player.plan(&net, &b, false)));
}
