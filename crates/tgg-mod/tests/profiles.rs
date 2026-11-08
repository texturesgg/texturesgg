//! Mod profiles end to end against FakeNet: reading them from synthetic
//! replays, resolving, preparing, launching, and trust.

use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tgg_mod::slp::write;
use tgg_mod::{
    Error, FakeNet, Fit, GameNeed, Local, Missing, ModId, PortInstall, Profile, ProfilePlan,
    Publisher, no_progress,
};

struct Player {
    _dir: tempfile::TempDir,
    /// `<data>`: `tgg/` in it holds ports, packages and profiles.
    data: PathBuf,
    mods: PathBuf,
}

impl Player {
    fn ports(&self) -> PathBuf {
        self.data.join("tgg/ports")
    }

    fn local(&self, port: Option<&PortInstall>) -> Local {
        let port = port.map(PortInstall::port);
        tgg_mod::local(&self.mods, port.as_ref()).expect("local")
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.data.join(name);
        std::fs::write(&path, bytes).expect("write");
        path
    }
}

fn player() -> Player {
    let dir = tempfile::tempdir().expect("temp dir");
    Player {
        data: dir.path().to_owned(),
        mods: dir.path().join("mods"),
        _dir: dir,
    }
}

fn go() -> AtomicBool {
    AtomicBool::new(false)
}

fn id(text: &str) -> ModId {
    text.parse().expect("id")
}

fn install_port(net: &FakeNet, player: &Player) -> PortInstall {
    let release = tgg_mod::port_releases(net).expect("releases").remove(0);
    tgg_mod::install_port(net, &release, &player.ports(), &no_progress, &go()).expect("port")
}

fn resolve(net: &FakeNet, player: &Player, profile: &Profile) -> ProfilePlan {
    let layout = tgg_mod::profile_layout(profile, &player.ports()).expect("a layout");
    let port = tgg_mod::Port {
        executable: PathBuf::new(),
        game_abi: layout,
        name: "tgg-melee".into(),
        target: String::new(),
        version: String::new(),
    };
    let catalog = tgg_mod::refresh_catalog(net, &player.data, &port).expect("catalog");
    let mapping = tgg_mod::refresh_mapping(net, &player.data).expect("mapping");
    let current = tgg_mod::ports::installed_ports(&player.ports())
        .expect("ports")
        .into_iter()
        .find(|install| install.current);
    tgg_mod::resolve_profile(
        profile,
        &catalog,
        &mapping,
        &player.ports(),
        &player.local(current.as_ref()),
        &tgg_mod::trust(&player.data).expect("trust"),
    )
}

/// A tgg-melee replay whose metadata carries `profile`.
fn tgg_replay(profile: serde_json::Value) -> Vec<u8> {
    let events = write::events([3, 18, 0], 31, false, 1, &[]);
    let metadata = json!({
        "playedOn": "tgg-melee",
        "tgg": {
            "game": format!("tgg-melee {} {}", FakeNet::PORT_VERSION, FakeNet::LAYOUT),
            "modSetId": "0123456789abcdef",
            "mods": ["tgg.ucf 0.84.0", "tgg.legal-stages 0.1.0"],
            "profile": profile,
        }
    });
    write::replay(&events, Some(&metadata))
}

fn competitive_profile() -> serde_json::Value {
    json!({
        "format": 1,
        "game": {"name": "tgg-melee", "version": FakeNet::PORT_VERSION,
                 "layout": FakeNet::LAYOUT, "settings": {"tgg.ucf": true}},
        "mods": [
            {"id": "tgg.legal-stages", "version": "0.1.0"},
            {"id": "tgg.competitive-rules", "version": "0.1.0",
             "settings": {"stocks": 4, "items": "Off"}}
        ]
    })
}

#[test]
fn a_tgg_replay_reads_its_profile_and_prepares_a_folder_from_the_cache() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let replay = player.file("game.slp", &tgg_replay(competitive_profile()));

    let profile = tgg_mod::profile_from_slp(&replay, None).expect("profile");
    assert_eq!(profile.game.version, FakeNet::PORT_VERSION);
    let ids: Vec<&str> = profile.mods.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "tgg.legal-stages",
            "tgg.competitive-rules",
            "tgg.replay-player"
        ]
    );

    let plan = resolve(&net, &player, &profile);
    assert_eq!(plan.game, GameNeed::Installed(port.clone()));
    assert!(plan.missing.is_empty(), "{:?}", plan.missing);
    assert_eq!(plan.fit, Fit::NeedsProfile);
    // Exactly the replay's legal stages, not the newest.
    let stages = plan.mods.iter().find(|m| m.id == id("tgg.legal-stages"));
    assert_eq!(stages.expect("stages").version.to_string(), "0.1.0");
    assert!(plan.mods.iter().all(|m| !m.cached && !m.needs_trust));

    let prepared =
        tgg_mod::prepare_profile(&net, &plan, &player.data, &no_progress, &go()).expect("prepare");
    assert_eq!(
        prepared.folder,
        player.data.join("tgg/profiles").join(&plan.key)
    );
    for m in &plan.mods {
        assert!(
            prepared
                .folder
                .join(m.id.as_str())
                .join("manifest.json")
                .is_file()
        );
    }
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&prepared.settings).expect("settings"))
            .expect("json");
    assert_eq!(
        settings,
        json!({"tgg.competitive-rules": {"items": "Off", "stocks": 4}})
    );
    let saved = tgg_mod::profile_from_file(&prepared.folder.join("profile.json")).expect("saved");
    assert_eq!(saved, plan.profile);
    let env = prepared.env();
    assert!(env.iter().any(|(name, value)| name == "TGG_SET"
        && value == "tgg.competitive-rules:items=Off;tgg.competitive-rules:stocks=4"));

    // The same replay again: the same key, everything cached.
    let again = resolve(&net, &player, &profile);
    assert_eq!(again.key, plan.key);
    assert!(again.mods.iter().all(|m| m.cached));
    assert_eq!(again.size(), 0);
    let downloads = net.requests().len();
    tgg_mod::prepare_profile(&net, &again, &player.data, &no_progress, &go()).expect("again");
    let fetched: Vec<String> = net.requests().split_off(downloads);
    assert!(
        !fetched.iter().any(|url| url.contains("/packages/")),
        "{fetched:?}"
    );
}

#[test]
fn a_profile_plays_as_installed_when_the_installs_match() {
    let net = FakeNet::new();
    let player = player();
    install_port(&net, &player);
    let want = [
        ("tgg.legal-stages", "0.1.0"),
        ("tgg.replay-player", "0.2.0"),
    ];
    let catalog = tgg_mod::catalog(
        &net,
        &tgg_mod::ports::installed_ports(&player.ports()).expect("ports")[0].port(),
    )
    .expect("catalog");
    let local = player.local(None);
    let specs: Vec<_> = want
        .iter()
        .map(|(id, version)| (self::id(id), version.parse().expect("range")))
        .collect();
    let plan = tgg_mod::plan_mods(&specs, &catalog, &local);
    tgg_mod::apply_cached(
        &net,
        &plan,
        &player.mods,
        Some(&player.data.join("tgg/packages")),
        &no_progress,
        &go(),
    )
    .expect("apply");

    let mut profile = Profile::new(FakeNet::PORT_VERSION);
    profile.mods = serde_json::from_value(json!([
        {"id": "tgg.legal-stages", "version": "0.1.0"},
        {"id": "tgg.replay-player", "version": "0.2.0"}
    ]))
    .expect("mods");
    assert_eq!(resolve(&net, &player, &profile).fit, Fit::AsInstalled);

    // With its built-in UCF off, or a setting the player hasn't saved, it
    // needs its own folder.
    let mut ucf_off = profile.clone();
    ucf_off.game.settings.insert("tgg.ucf".into(), false);
    assert_eq!(resolve(&net, &player, &ucf_off).fit, Fit::NeedsProfile);
    let mut set = profile.clone();
    set.mods[0].settings.insert("starters".into(), json!(5));
    assert_eq!(resolve(&net, &player, &set).fit, Fit::NeedsProfile);
    std::fs::write(
        player.mods.join("settings.json"),
        r#"{"tgg.legal-stages": {"starters": 5}}"#,
    )
    .expect("settings");
    assert_eq!(resolve(&net, &player, &set).fit, Fit::AsInstalled);

    // An extra turned-on mod that ships only assets can't change the game.
    let profile = Profile {
        mods: profile.mods[1..].to_vec(),
        ..profile
    };
    assert_eq!(resolve(&net, &player, &profile).fit, Fit::AsInstalled);
}

#[test]
fn a_slippi_replay_maps_its_codes_through_the_registry() {
    let net = FakeNet::new();
    net.publish_slippi_mods();
    let player = player();
    install_port(&net, &player);
    let gecko = write::gecko(&[
        0x8016_E510, // neutral spawns
        0x8000_0004, // nothing knows it
        0x8012_39A8, // freeze glitch fix: no mod yet, rarely matters
        0x8009_08F4, // UCF 0.8
    ]);
    // Slippi 3.16 on Pokémon Stadium, frozen.
    let events = write::events([3, 16, 0], 3, true, 1, &gecko);
    let replay = player.file(
        "slippi.slp",
        &write::replay(&events, Some(&json!({"playedOn": "dolphin"}))),
    );

    let mapping = tgg_mod::slippi_mapping(&net).expect("mapping");
    let profile = tgg_mod::profile_from_slp(&replay, Some(&mapping)).expect("profile");
    assert_eq!(profile.game.version, "any");
    assert_eq!(profile.label.as_deref(), Some("Slippi 3.16.0"));
    let ids: Vec<&str> = profile.mods.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "tgg.ucf-legacy",
            "tgg.neutral-spawns",
            "tgg.frozen-stadium",
            "tgg.replay-player"
        ]
    );
    assert!(!profile.game.settings["tgg.ucf"]);
    assert_eq!(
        profile.missing,
        [
            Missing::NoModFor {
                code: "0x80000004".into(),
                minor: false
            },
            Missing::NoModFor {
                code: "0x801239A8".into(),
                minor: true
            },
        ]
    );

    // Read without a mapping, the facts stay and resolving maps them.
    let unmapped = tgg_mod::profile_from_slp(&replay, None).expect("profile");
    assert_eq!(unmapped.mods.len(), 1);
    let plan = resolve(&net, &player, &unmapped);
    let mut planned: Vec<&str> = plan.mods.iter().map(|m| m.id.as_str()).collect();
    planned.sort();
    assert_eq!(
        planned,
        [
            "tgg.frozen-stadium",
            "tgg.neutral-spawns",
            "tgg.replay-player",
            "tgg.ucf-legacy"
        ]
    );
    assert_eq!(plan.missing, profile.missing);
    assert!(matches!(plan.game, GameNeed::Installed(ref p) if p.current));

    // 3.19 on Stadium: its Stadium code matters there, and nowhere else.
    for (stage, flagged) in [(3, true), (31, false)] {
        let events = write::events([3, 19, 1], stage, false, 1, &write::gecko(&[0x801C_65C8]));
        let replay = player.file("stadium.slp", &write::replay(&events, None));
        let profile = tgg_mod::profile_from_slp(&replay, Some(&mapping)).expect("profile");
        assert_eq!(!profile.missing.is_empty(), flagged, "stage {stage}");
    }
}

#[test]
fn a_tgg_replay_without_a_profile_reads_its_mods() {
    let player = player();
    let events = write::events([3, 18, 0], 31, false, 1, &[]);
    let metadata = json!({"tgg": {
        "game": "tgg-melee 0.1.0-rc.8 layout8",
        "mods": ["tgg.ucf 0.84.0", "tgg.legal-stages 0.1.1"]
    }});
    let replay = player.file("old.slp", &write::replay(&events, Some(&metadata)));
    let profile = tgg_mod::profile_from_slp(&replay, None).expect("profile");
    assert_eq!(profile.game.version, "0.1.0-rc.8");
    assert_eq!(profile.game.layout.as_deref(), Some("layout8"));
    assert!(!profile.game.settings["tgg.unlocks"]);
    assert_eq!(profile.mods[0].id, id("tgg.legal-stages"));
}

#[test]
fn unofficial_mods_wait_for_the_players_yes() {
    let net = FakeNet::new();
    let player = player();
    install_port(&net, &player);
    net.set_publisher(
        "tgg.legal-stages",
        Publisher {
            name: "someone".into(),
            official: false,
        },
    );
    let replay = player.file("game.slp", &tgg_replay(competitive_profile()));
    let profile = tgg_mod::profile_from_slp(&replay, None).expect("profile");
    let plan = resolve(&net, &player, &profile);
    assert_eq!(plan.untrusted(), ["someone"]);
    let result = tgg_mod::prepare_profile(&net, &plan, &player.data, &no_progress, &go());
    assert!(
        matches!(&result, Err(Error::NeedsTrust { publisher, .. }) if publisher == "someone"),
        "{result:?}"
    );
    tgg_mod::remember_trust(&player.data, "someone", true).expect("remember");
    let plan = resolve(&net, &player, &profile);
    assert!(plan.untrusted().is_empty());
}

#[test]
fn a_profile_for_another_game_version_installs_it_alongside() {
    let net = FakeNet::new();
    let player = player();
    install_port(&net, &player);
    net.publish_port("0.6.0");
    let mut profile = Profile::new("0.6.0");
    profile.game.layout = Some(FakeNet::LAYOUT.into());
    let plan = resolve(&net, &player, &profile);
    assert_eq!(
        plan.game,
        GameNeed::Available {
            version: "0.6.0".into(),
            size: None
        }
    );
    assert_eq!(plan.fit, Fit::NeedsProfile);
    let prepared =
        tgg_mod::prepare_profile(&net, &plan, &player.data, &no_progress, &go()).expect("prepare");
    assert_eq!(prepared.port.version, "0.6.0");
    assert!(!prepared.port.current);
    assert_eq!(
        tgg_mod::ports::current_port(&player.ports()).expect("current"),
        Some(FakeNet::PORT_VERSION.into())
    );
    // Not a release at all.
    profile.game.version = "dev-build".into();
    let plan = resolve(&net, &player, &profile);
    assert!(matches!(plan.game, GameNeed::Unavailable { .. }));
}

#[test]
fn a_bad_profile_file_says_why() {
    let player = player();
    let newer = player.file(
        "newer.json",
        br#"{"format": 2, "game": {"name": "tgg-melee", "version": "any"}}"#,
    );
    assert!(matches!(
        tgg_mod::profile_from_file(&newer),
        Err(Error::ProfileFormat { format: 2, .. })
    ));
    let other = player.file("other.slp", b"not a replay");
    assert!(matches!(
        tgg_mod::profile_from_slp(&other, None),
        Err(Error::BadReplay { .. })
    ));
}

#[cfg(unix)]
#[test]
fn a_launched_profile_game_is_marked_running_until_it_ends() {
    let net = FakeNet::new();
    let player = player();
    let port = install_port(&net, &player);
    let replay = player.file("game.slp", &tgg_replay(competitive_profile()));
    let profile = tgg_mod::profile_from_slp(&replay, None).expect("profile");
    let plan = resolve(&net, &player, &profile);
    let prepared =
        tgg_mod::prepare_profile(&net, &plan, &player.data, &no_progress, &go()).expect("prepare");
    let iso = player.file("melee.iso", b"");
    let log = player.data.join("game.log");
    let mut running =
        tgg_mod::launch_profile(&port, &iso, &prepared, Some(&replay), Some(&log)).expect("launch");
    let games = tgg_mod::profile_games(&player.data);
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].key, plan.key);
    assert!(matches!(
        tgg_mod::launch_profile(&port, &iso, &prepared, None, None),
        Err(Error::GameRunning)
    ));
    assert!(tgg_mod::finish_profile(&player.data).expect("finish"));
    running.kill().expect("kill");
    assert!(tgg_mod::profile_games(&player.data).is_empty());
    assert!(!prepared.folder.exists());
    assert!(!player.data.join("tgg/running.json").exists());
}

#[test]
fn finishing_clears_marks_a_lost_handle_left() {
    let player = player();
    let folder = player.data.join("tgg/profiles/0123");
    std::fs::create_dir_all(&folder).expect("mkdir");
    let mark = json!([{"pid": 4294967, "executable": "/nowhere/tgg-melee", "key": "0123",
                       "folder": folder}]);
    std::fs::write(player.data.join("tgg/running.json"), mark.to_string()).expect("write");
    assert!(!tgg_mod::finish_profile(&player.data).expect("finish"));
    assert!(!folder.exists());
    assert!(!Path::new(&player.data.join("tgg/running.json")).exists());
}
