// A window app on Windows: no console beside it (debug builds keep one,
// for what they print).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
//! tgg-studio: the textures.gg desktop app on Rune. It finds the player's
//! Melee NTSC 1.02 ISO and shows what's in it, keeps their library of skins,
//! installs them, edits costumes, and installs tgg-melee and its mods.
//!
//! ```text
//! tgg-studio [--iso PATH] [SKIN...]                         skins open in the Library's review
//! tgg-studio [--iso PATH] --snap out.png [--scene NAME]   headless, as Rune's tooling
//! ```

mod companion;
mod editor;
mod first_run;
mod jobs;
mod keys;
mod library;
mod links;
mod melee;
mod mods;
mod nav;
mod newmod;
mod play;
mod preferences;
mod profile;
mod replays;
mod report;
mod sets;
mod shell;
mod signin;
mod slippi;
mod stage;
mod state;
mod ui;
mod update;
mod window;

#[cfg(test)]
mod flows;
#[cfg(all(test, feature = "melee-iso"))]
mod iso_flows;

use first_run::{FirstRun, Stage};
use rune::AppSpec;
use state::{App, Data, Start};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tgg_app::settings;

/// A screen, as a named edit of the app at launch, for `--snap --scene`.
type Scene = (&'static str, fn(&mut App));

const SCENES: &[Scene] = &[
    // First run: the disc, the game, the mods to start with, and signing in.
    ("first-run-no-disc", |app| {
        first_run_scene(app);
        app.game = None;
        app.mods.disc = mods::Disc::Missing;
    }),
    ("first-run-ready", |app| {
        first_run_scene(app);
        scene_disc(app);
    }),
    // textures.gg has no tgg-melee this app can use (before tgg-melee's
    // first release on mod API 1.0, every new player sees this).
    ("first-run-no-game", |app| {
        first_run_scene(app);
        scene_disc(app);
        app.mods.releases.clear();
        app.mods.registry = mods::Registry::Failed(
            "textures.gg doesn't have a tgg-melee for this version of the app yet.".into(),
        );
    }),
    ("first-run-downloading", |app| {
        first_run_scene(app);
        scene_disc(app);
        if let Some(first) = app.first_run.as_mut() {
            first.stage = Stage::Downloading;
        }
        let progress = tgg_mod::Progress {
            step: 0,
            steps: 1,
            label: "Downloading tgg-melee 0.5.0".into(),
            bytes: 12_400_000,
            total: Some(48_000_000),
        };
        app.mods.work = Some(mods::Work {
            doing: "Installing tgg-melee 0.5.0".into(),
            progress: std::sync::Arc::new(std::sync::Mutex::new(Some(progress))),
            cancel: std::sync::Arc::default(),
            target: None,
            port: true,
        });
    }),
    ("first-run-done", |app| {
        first_run_scene(app);
        scene_disc(app);
        if let Some(first) = app.first_run.as_mut() {
            first.stage = Stage::Done;
        }
        app.signin.who = Some(tgg_account::SignedIn {
            account: tgg_account::Account {
                handle: "ciao".into(),
                display_name: None,
                id: None,
            },
            verified: true,
        });
    }),
    ("first-run-none-picked", |app| {
        first_run_scene(app);
        scene_disc(app);
        if let Some(first) = app.first_run.as_mut() {
            first.use_default_set = false;
        }
    }),
    ("first-run-installed", |app| {
        let net = first_run_scene(app);
        scene_disc(app);
        let never = std::sync::atomic::AtomicBool::new(false);
        if let Some(release) = tgg_mod::port_releases(&*net)
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            let _ = tgg_mod::install_port(&*net, &release, &app.data.ports, &|_| {}, &never);
            mods::actions::read_disk(app);
        }
    }),
    ("first-run-update", |app| {
        let net = first_run_scene(app);
        scene_disc(app);
        let never = std::sync::atomic::AtomicBool::new(false);
        if let Some(release) = tgg_mod::port_releases(&*net)
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            let _ = tgg_mod::install_port(&*net, &release, &app.data.ports, &|_| {}, &never);
            mods::actions::read_disk(app);
        }
        net.publish_port("0.6.0");
        let mut releases = tgg_mod::port_releases(&*net).unwrap_or_default();
        if let Some(release) = releases.first_mut() {
            release.files.game.size = 48_000_000;
        }
        app.mods.releases = releases;
    }),
    ("first-run-disc-warning", |app| {
        first_run_scene(app);
        scene_disc(app);
        app.mods.disc_modified = vec!["PlFc.dat".into()];
    }),
    // Skins as it opens: textures.gg's skins, with their previews (live, for
    // a look).
    ("skins", |app| skins_on(app, melee::Tab::Browse)),
    ("skins-using", |app| skins_on(app, melee::Tab::Using)),
    ("skins-made", |app| skins_on(app, melee::Tab::Made)),
    // A skin's sheet: the first of the player's made for a costume.
    ("skins-sheet", |app| {
        skins_on(app, melee::Tab::Using);
        let id = app
            .library
            .skins()
            .iter()
            .rev()
            .find(|skin| matches!(skin.slot, Some(melee_dat::MeleeSlot::Costume { .. })))
            .map(|skin| skin.id);
        if let Some(id) = id {
            melee::actions::open_skin(app, id);
        }
    }),
    // Make a skin's picker, over the page.
    ("skins-make", |app| {
        skins_on(app, melee::Tab::Using);
        melee::actions::make(app);
    }),
    ("fox-edit", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(skins(app), fox);
            editor::actions::open(app);
        }
    }),
    ("fox-edit-helmet", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(skins(app), fox);
            editor::actions::open(app);
            editor::actions::select(app, Some(1));
        }
    }),
    // A click on the middle of the model, as the player would to pick its texture.
    ("fox-edit-pick", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(skins(app), fox);
            editor::actions::open(app);
            editor::actions::select(app, None);
            if let Some(shown) = &mut app.stage.shown {
                shown.pick = Some([0.5, 0.45]);
            }
        }
    }),
    // Changing the costume's first color outside its textures.
    ("fox-colors", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(skins(app), fox);
            editor::actions::open(app);
            editor::actions::select(app, None);
            editor::colors::pick_swatch(app, 0, 0, &mut rune::Tx::default());
        }
    }),
    // Pokémon Stadium with every vertex-colored group turned a third of the
    // way round the hue circle: its teal sky goes magenta, the model drawn
    // afresh from the file. (Fox's colors are materials his textures
    // replace, so changing them shows nothing.)
    ("stadium-recolored", |app| {
        let Some(stage) = app
            .melee
            .stages
            .iter()
            .copied()
            .find(|stage| stage.name() == "Pokemon Stadium")
        else {
            return;
        };
        melee::actions::open_stage(skins(app), stage);
        editor::actions::open(app);
        editor::actions::select(app, None);
        let groups = app
            .editor
            .as_ref()
            .map_or(0, |document| document.colors.len());
        for group in 0..groups {
            if app
                .editor
                .as_ref()
                .is_some_and(|document| document.colors[group].shifts_hue())
            {
                editor::colors::pick_hue(app, group, &mut rune::Tx::default());
                editor::colors::set_hue(app, 120.0);
                editor::colors::apply(app);
            }
        }
    }),
    // The review before a file the player picked joins their skins: Fox's
    // green costume, offered as a skin, and a file that isn't one.
    ("skins-review", |app| {
        skins_on(app, melee::Tab::Using);
        let (Some(fox), Some(references)) = (melee_dat::Character::from_code("Fx"), &app.game)
        else {
            return;
        };
        let Some(green) = melee_dat::CostumeColor::from_code("Gr") else {
            return;
        };
        let Ok(bytes) = references.game().read_slot(melee_dat::MeleeSlot::Costume {
            character: fox,
            color: green,
        }) else {
            return;
        };
        let folder = std::env::temp_dir().join("tgg-studio-scene");
        let (skin, note) = (folder.join("Fox Green.dat"), folder.join("notes.txt"));
        let written = std::fs::create_dir_all(&folder)
            .and_then(|()| std::fs::write(&skin, bytes))
            .and_then(|()| std::fs::write(&note, "not a skin"));
        if written.is_ok() {
            library::actions::add(app, &[skin, note]);
        }
    }),
    // Leaving an edited costume: the unsaved-changes prompt.
    ("fox-unsaved", |app| {
        let Some(fox) = melee_dat::Character::from_code("Fx") else {
            return;
        };
        melee::actions::open_fighter(skins(app), fox);
        editor::actions::open(app);
        editor::colors::pick_swatch(app, 0, 0, &mut rune::Tx::default());
        editor::colors::set_channel(app, 0, 255.0);
        editor::colors::apply(app);
        editor::actions::leave(app, &mut rune::Tx::default());
    }),
    // Mods against a fake textures.gg, in a folder of its own: Competitive
    // and Replays added from it, Competitive Using and shown.
    ("mods", |app| {
        let net = mods_scene(app, true);
        add_scene_set(app, &*net, "competitive");
        add_scene_set(app, &*net, "replays");
        show_scene_sets(app, &*net, "competitive", "competitive");
    }),
    // The Sets tab inside Mods: Competitive and My Competitive in Your sets,
    // Replays available to add from textures.gg, Competitive shown on the right.
    ("mods-sets", |app| {
        let net = mods_scene(app, true);
        add_scene_set(app, &*net, "competitive");
        make_scene_set(app, &*net, "My Competitive", &["competitive"], &[], &[]);
        show_scene_sets(app, &*net, "competitive", "competitive");
        mods::actions::show_tab(app, mods::Tab::Sets);
    }),
    // Play before tgg-melee is installed: one line saying what to set up.
    ("play-new", |app| {
        mods_scene(app, false);
        app.nav.place = nav::Place::Play;
        app.play.opened = true;
    }),
    // Play: Friday night over Competitive, the queue its mods make, a
    // player's queue open right now, and three last matches.
    ("play", |app| {
        play_scene(app);
        play_queues(app, false);
    }),
    // Play with Vanilla (no mods) active and standing/open queues below.
    ("play-vanilla", |app| {
        play_scene(app);
        play_queues(app, true);
        let _ = tgg_mod::sets::activate(&app.data.tgg, tgg_mod::sets::VANILLA);
        app.sets.active = tgg_mod::sets::VANILLA.to_owned();
    }),
    // Play with a tgg-melee update banner, sidebar indicator, and modified disc warning.
    ("play-port-update", |app| {
        let net = play_scene_net(app);
        play_queues(app, false);
        net.publish_port("0.6.0");
        let mut releases = tgg_mod::port_releases(&*net).unwrap_or_default();
        if let Some(release) = releases.first_mut() {
            release.files.game.size = 48_000_000;
        }
        app.mods.releases = releases;
        app.mods.disc_modified = vec!["PlFc.dat".into()];
    }),
    // A set with a queue of its own: nobody in the standing one is its
    // queue, so the list keeps Competitive under "Always open".
    ("play-own-queue", |app| {
        play_scene(app);
        play_queues(app, true);
    }),
    // Searching in Competitive, seven seconds in.
    ("play-searching", |app| {
        play_scene(app);
        play_queues(app, false);
        searching_scene(app);
    }),
    // A set's game being got ready: what's being done, how far along, Cancel.
    ("play-launching", |app| {
        play_scene(app);
        play_queues(app, false);
        launching_scene(app);
    }),
    // Hosting a room: the invite link with Copy above the code, and who's
    // in so far.
    ("play-friend-host", |app| {
        play_scene(app);
        play_queues(app, false);
        hosting_scene(app);
    }),
    // A room filling up: the host and two friends, three of four.
    ("play-friend-waiting", |app| {
        play_scene(app);
        play_queues(app, false);
        room_scene(app, crate::sets::How::Host, WAITING_THREE);
    }),
    // The game signing in and reaching textures.gg.
    ("play-friend-starting", |app| {
        play_scene(app);
        play_queues(app, false);
        room_scene(app, crate::sets::How::Host, STARTING);
    }),
    // Nobody's signed in: the code to confirm and the page to do it on.
    ("play-friend-signin", |app| {
        play_scene(app);
        play_queues(app, false);
        room_scene(app, crate::sets::How::Host, SIGNING_IN);
    }),
    // A joiner getting the host's mods before it restarts into them.
    ("play-friend-getting-mods", |app| {
        play_scene(app);
        play_queues(app, false);
        room_scene(app, crate::sets::How::Join("KFXD".to_owned()), GETTING_MODS);
    }),
    // The host started: four of four.
    ("play-friend-playing", |app| {
        play_scene(app);
        play_queues(app, false);
        room_scene(app, crate::sets::How::Host, PLAYING);
    }),
    // The room ended, and the game's reason.
    ("play-friend-ended", |app| {
        play_scene(app);
        play_queues(app, false);
        room_scene(app, crate::sets::How::Host, ENDED);
    }),
    // Joining one: a code typed.
    ("play-friend-join", |app| {
        play_scene(app);
        play_queues(app, false);
        app.play.friend = true;
        app.play.join.text = "KFXD".into();
    }),
    // Confirming leaving an active match to join a friend's room.
    ("play-confirm-leave", |app| {
        play_scene(app);
        play_queues(app, false);
        app.sets.session = Some(crate::sets::Session {
            how: crate::sets::How::Offline,
            name: "Friday night".to_owned(),
            set_key: app.sets.active.clone(),
            profile_key: None,
        });
        app.mods.playing = Some(crate::mods::Playing {
            stop: Arc::default(),
        });
        app.companion_override = Some(crate::companion::LiveState {
            pid: 1,
            mod_set_id: String::new(),
            activity: crate::companion::Activity::Match,
            replay_path: String::new(),
            online: crate::companion::Online::default(),
            updated_at: Some(std::time::Instant::now()),
        });
        app.play.confirm = Some(crate::play::PendingAction::Join("KFXD".to_owned()));
    }),
    // What the set says it plays without, above the card.
    ("play-notices", |app| {
        play_scene(app);
        play_queues(app, false);
        notices_scene(app);
    }),
    // The set picker's menu.
    ("play-set-menu", |app| {
        play_scene(app);
        play_queues(app, false);
        app.play.menu = true;
    }),
    // Native code from a publisher the player hasn't trusted, before a game.
    ("play-trust", |app| {
        play_scene(app);
        play_queues(app, false);
        trust_scene(app);
    }),
    // A toast after an install, offering Undo.
    ("toast", |app| {
        skins_on(app, melee::Tab::Using);
        if let Ok(slot) = "PlFxOr.dat".parse() {
            app.tell_undoable(
                "Wrote DARK GOKU FOX HALO into Fox · Orange in your Melee disc.",
                slot,
            );
        }
    }),
    // What the next launch says when edits were left unsaved.
    ("draft", |app| {
        if let Ok(slot) = "PlFxOr.dat".parse() {
            app.tell_with(
                "Unsaved edits to Fox · Orange",
                state::NoticeAction::Reopen(slot),
            );
        }
    }),
    // Friday night: Competitive plus Replays, one mod added, one left out,
    // and one textures.gg withdrew.
    ("mods-own-set", |app| {
        let net = mods_scene(app, true);
        add_scene_set(app, &*net, "competitive");
        add_scene_set(app, &*net, "replays");
        make_scene_set(
            app,
            &*net,
            "Friday night",
            &["competitive"],
            &["tgg.replay-recorder"],
            &["tgg.quick-start"],
        );
        net.withdraw(
            "tgg.skip-results",
            "0.1.0",
            "crashes on Windows",
            tgg_mod::WithdrawMode::Off,
        );
        show_scene_sets(app, &*net, "friday-night", "friday-night");
    }),
    // Installed mods shown: the mods folder, each mod's switch.
    ("mods-installed-set", |app| {
        let net = mods_scene(app, true);
        show_scene_sets(app, &*net, "installed", "installed");
    }),
    // Rules picked: only the mods that change how a match plays.
    ("mods-filter", |app| {
        let net = mods_scene(app, true);
        add_scene_set(app, &*net, "competitive");
        show_scene_sets(app, &*net, "competitive", "competitive");
        mods::actions::set_filter(app, mods::Filter::Rules);
    }),
    // Direct connect's details where the set panel was.
    ("mods-details", |app| {
        let net = mods_scene(app, true);
        add_scene_set(app, &*net, "competitive");
        show_scene_sets(app, &*net, "competitive", "competitive");
        if let Ok(id) = "tgg.direct-connect".parse::<tgg_mod::ModId>() {
            // Its page fetched here, not on a job, so it's there for the picture.
            if let Ok(details) = tgg_mod::mod_details(&*net, &id) {
                for shot in &details.screenshots {
                    scene_image(app, &*net, &shot.url);
                }
                app.mods.details.insert(id.clone(), Some(details));
            }
            mods::actions::select(app, &id);
        }
    }),
    // New set: Online play isn't on textures.gg yet, and the sheet says so.
    ("mods-new-set", |app| {
        mods_scene(app, true);
        mods::actions::open_new_set(app);
    }),
    // Start a mod: A rule, named.
    ("mods-start-a-mod", |app| {
        mods_scene(app, true);
        new_mod_scene(app);
    }),
    // The mod made, where it is and how to run it.
    ("mods-start-a-mod-created", |app| {
        mods_scene(app, true);
        new_mod_scene(app);
        app.new_mod.stage = newmod::Stage::Created(tgg_mod::CreatedMod {
            dir: app.new_mod.folder(),
            id: "neutral-spawns".parse().expect("an id"),
            example: newmod::Kind::Rule.template().into(),
            sdk_version: "0.1.0".into(),
            repository: tgg_mod::Repository::Committed,
        });
    }),
    // Friday night's game running: the page shows it read-only.
    ("mods-running", |app| {
        let net = mods_scene(app, true);
        add_scene_set(app, &*net, "competitive");
        make_scene_set(
            app,
            &*net,
            "Friday night",
            &["competitive"],
            &["tgg.replay-recorder"],
            &[],
        );
        show_scene_sets(app, &*net, "friday-night", "friday-night");
        app.mods.playing = Some(mods::Playing {
            stop: std::sync::Arc::default(),
        });
        app.sets.session = Some(crate::sets::Session {
            how: crate::sets::How::Offline,
            name: "Friday night".into(),
            set_key: "friday-night".into(),
            profile_key: None,
        });
    }),
    // The launch video's start: a player new to mods, against the live
    // textures.gg, its mods and ports in a scratch folder (`--video` with
    // `demo/script.json` drives the rest).
    ("demo", |app| {
        app.jobs.inline = true;
        let folder = std::env::temp_dir().join("tgg-studio-demo");
        let _ = std::fs::remove_dir_all(&folder);
        app.data.mods = folder.join("mods");
        app.data.ports = folder.join("tgg-data").join("tgg").join("ports");
        app.data.tgg = folder.join("tgg-data");
        if let Some(references) = &app.game {
            app.mods.disc = mods::Disc::Found(references.game().path().to_owned());
        }
    }),
    ("settings", preferences::actions::open),
    ("settings-menu", |app| {
        preferences::actions::open(app);
        preferences::actions::toggle_menu(app, preferences::Menu::ImageEditor);
    }),
    ("settings-versions", |app| {
        let net = mods_scene(app, true);
        net.publish_port("0.6.0");
        mods::actions::fetch(app);
        poll_inline(app);
        preferences::actions::open(app);
        mods::actions::show_versions(app, true);
    }),
    ("report", |app| report::actions::open(app, false)),
    // Signing in, its code shown (nothing is asked of the site).
    ("signin", |app| {
        let mut login = signin::Login::asking();
        login.code = Some("ABCD-EFGH".into());
        login.link = Some("https://textures.gg/device".into());
        app.signin.login = Some(login);
    }),
    // Signed in, as ciao.
    ("signed-in", |app| {
        app.signin.who = Some(tgg_account::SignedIn {
            account: tgg_account::Account {
                handle: "ciao".into(),
                display_name: None,
                id: None,
            },
            verified: true,
        });
    }),
    // You, signed in as ciao, with what's been made on this computer.
    ("profile", |app| {
        app.signin.who = Some(tgg_account::SignedIn {
            account: tgg_account::Account {
                display_name: Some("ciao".into()),
                handle: "ciao".into(),
                id: None,
            },
            verified: true,
        });
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Profile);
        poll_inline(app);
        app.jobs.inline = false;
    }),
    // The player's own replays, read and resolved against the live
    // textures.gg before the picture (not deterministic: for a look).
    ("replays", |app| {
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Replays);
        poll_inline(app);
        app.jobs.inline = false;
    }),
    // The same, its first two-player replay chosen, with its stats.
    ("replay-details", |app| {
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Replays);
        poll_inline(app);
        let first = app
            .replays
            .items
            .iter()
            .find(|item| {
                item.info
                    .as_ref()
                    .is_some_and(|info| info.players.len() == 2)
            })
            .map(|item| item.path.clone());
        if let Some(path) = first {
            replays::actions::select(app, &path);
            poll_inline(app);
        }
        app.jobs.inline = false;
    }),
    // What a replay says before it plays when one of its mods was withdrawn
    // (the notices are fixed here, for the picture).
    ("replays-withdrawn", |app| {
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Replays);
        poll_inline(app);
        let found = app.replays.items.iter().find_map(|item| {
            let plan = item.plan.clone()?;
            matches!(plan.game, tgg_mod::GameNeed::Installed(_))
                .then_some((item.path.clone(), plan))
        });
        if let Some((path, plan)) = found {
            app.replays.selected = Some(path.clone());
            app.replays.review = Some(replays::Review {
                path,
                plan,
                remember: false,
                notices: vec![
                    "Playing without Turbo. textures.gg withdrew it (crashes on Windows)."
                        .to_owned(),
                ],
            });
        }
        app.jobs.inline = false;
    }),
    ("no-sidebar", |app| {
        play_scene(app);
        play_queues(app, false);
        app.nav.sidebar = false;
    }),
    ("report-sent", |app| {
        play_scene(app);
        report::actions::open(app, false);
        report::actions::sent(app, Ok("r-94f2c8a1".into()));
    }),
    ("settings-updates", |app| {
        play_scene(app);
        let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
        if let Some(current) = tgg_mod::port_releases(&*net)
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            let never = std::sync::atomic::AtomicBool::new(false);
            let _ = tgg_mod::install_port(&*net, &current, &app.data.ports, &|_| {}, &never);
        }
        net.publish_port("0.6.0");
        let releases = tgg_mod::port_releases(&*net).unwrap_or_default();
        app.mods.net = net;
        app.mods.releases = releases;
        app.mods.registry = mods::Registry::Answered;
        mods::actions::read_disk(app);
        app.update.available = Some(tgg_app::update::Release {
            version: "1.0.5".into(),
            released: Some("2026-11-28".into()),
            notes_url: Some(
                "https://github.com/texturesgg/texturesgg/releases/tag/app-v1.0.5".into(),
            ),
            assets: tgg_app::update::Assets {
                linux: vec![tgg_app::update::Asset {
                    kind: "tar.gz".into(),
                    url: "https://example.com/textures.gg-1.0.5-linux-x86_64.tar.gz".into(),
                    size: 20_600_000,
                    sha256: "deadbeef".into(),
                    min_os: None,
                }],
                ..Default::default()
            },
        });
        preferences::actions::open(app);
    }),
    ("app-update-dialog", |app| {
        play_scene(app);
        let release = tgg_app::update::Release {
            version: "1.0.5".into(),
            released: Some("2026-11-28".into()),
            notes_url: Some(
                "https://github.com/texturesgg/texturesgg/releases/tag/app-v1.0.5".into(),
            ),
            assets: tgg_app::update::Assets {
                linux: vec![tgg_app::update::Asset {
                    kind: "tar.gz".into(),
                    url: "https://example.com/textures.gg-1.0.5-linux-x86_64.tar.gz".into(),
                    size: 20_600_000,
                    sha256: "deadbeef".into(),
                    min_os: None,
                }],
                ..Default::default()
            },
        };
        app.update.available = Some(release.clone());
        app.update.dialog = Some(update::UpdateDialog {
            release,
            method: tgg_app::update::InstallMethod::LinuxLocalInstall(PathBuf::from(
                "/home/player/.local/bin/tgg-studio",
            )),
            stage: update::Stage::Ready,
        });
    }),
    ("app-update-downloading", |app| {
        play_scene(app);
        let release = tgg_app::update::Release {
            version: "1.0.5".into(),
            released: Some("2026-11-28".into()),
            notes_url: None,
            assets: tgg_app::update::Assets {
                linux: vec![tgg_app::update::Asset {
                    kind: "tar.gz".into(),
                    url: "https://example.com/textures.gg-1.0.5-linux-x86_64.tar.gz".into(),
                    size: 20_600_000,
                    sha256: "deadbeef".into(),
                    min_os: None,
                }],
                ..Default::default()
            },
        };
        app.update.available = Some(release.clone());
        let progress =
            std::sync::Arc::new(std::sync::Mutex::new(Some(tgg_app::update::Progress {
                stage: tgg_app::update::Stage::Downloading,
                bytes: 12_800_000,
                total: Some(20_600_000),
            })));
        app.update.dialog = Some(update::UpdateDialog {
            release,
            method: tgg_app::update::InstallMethod::LinuxLocalInstall(PathBuf::from(
                "/home/player/.local/bin/tgg-studio",
            )),
            stage: update::Stage::Working {
                progress,
                cancel: std::sync::Arc::default(),
            },
        });
    }),
];

/// Leading offset for macOS traffic lights; vertical offset centers them on
/// `ui::tokens::size::TOP_BAR`.
const TRAFFIC_LIGHT_X: f32 = 16.0;

fn main() {
    // Started with a link while the app is open (Windows, Linux): the open
    // one takes it, and this one is done.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some(link) = links::in_arguments(&arguments)
        && links::forward(link)
    {
        return;
    }
    tgg_app::log::start("tgg-studio", env!("CARGO_PKG_VERSION"));
    tgg_app::update::cleanup_old_binary();
    let iso = iso_argument();
    // The id its desktop entry and icon are filed under (dist/linux), so
    // the desktop matches the window to them.
    rune::set_app_id("gg.textures.app");
    rune::run_windows(move |displays| {
        let (size, position) = window::opening(displays);
        let spec = AppSpec::<App> {
            min_size: Some(window::MIN_SIZE),
            fonts: ui::fonts::FONTS,
            families: ui::fonts::FAMILIES,
            scenes: SCENES,
            on_key: Some(keys::on_key),
            setup: Some(jobs::setup),
            poll: Some(jobs::poll),
            ..AppSpec::new("textures.gg", size, launch, shell::view)
        };
        let traffic_light_y =
            ((ui::tokens::size::TOP_BAR - rune::Chrome::TRAFFIC_LIGHT) / 2.0).round();
        let app = rune::App::new(spec)
            .titlebar(
                rune::Titlebar::new(ui::tokens::size::TOP_BAR)
                    .traffic_lights(TRAFFIC_LIGHT_X, traffic_light_y),
            )
            .window_options(rune::WindowOptions {
                position,
                // See-through where the sidebar is glass (`ui::tokens::GLASS`):
                // the sidebar fills its part with smoke, everything else fills
                // its own part solid. Where the GPU can't present a
                // see-through window, Rune makes it opaque and the smoke sits
                // on black, close to the solid sidebar.
                transparent: ui::tokens::GLASS,
                ..Default::default()
            })
            // Coming back to the window: a sign-in or sign-out made with `tgg` shows.
            .on_focus(signin::actions::focused)
            // textures-gg:// links, on macOS.
            .on_open_url(links::opened_with)
            .init(move || {
                let data = Data::of_player();
                let loaded = settings::load();
                let mut app = App::new(
                    start(iso.as_deref(), &data, &loaded),
                    loaded,
                    data,
                    persists(),
                );
                open_files(&mut app);
                if let Some(link) = links::in_arguments(rune::extra_args()) {
                    links::open(&mut app, link);
                }
                app
            })
            .compositor(stage::viewport::Viewport::default());
        // Links a later launch passes on (Windows, Linux); not for Rune's
        // headless tooling, whose display isn't a real one.
        if !cfg!(target_os = "macos") && displays.iter().any(|display| display.id.is_some()) {
            links::listen(app.handle());
        }
        [rune::WindowSpec::from_app("main", app)]
    });
    // The PNGs opened in the player's image editor go with the app.
    tgg_app::external::remove_temporary_directory();
}

/// The app as a plain launch finds it, for Rune's tooling. Scenes show the
/// app as set up when a game is found: they aren't the player's first run.
fn launch() -> App {
    let data = Data::of_player();
    let loaded = settings::load();
    let start = match start(None, &data, &loaded) {
        Start::FirstRun {
            game: Some(game), ..
        } => Start::Game(game),
        other => other,
    };
    App::new(start, loaded, data, persists())
}

/// Whether changes to settings are saved: in a window, yes; in Rune's
/// headless runs (`--snap`, `--script`), no, so a scene or a recording that
/// hides the sidebar or picks an editor leaves the player's settings alone.
fn persists() -> bool {
    rune::tooling::headless_run().is_none()
}

/// The game given, else the one chosen before, else the one Slippi Launcher
/// plays or one in Dolphin's folders, remembered so the next launch opens it
/// directly. With none of those, a game found in the player's folders is
/// theirs without a press. First run shows once on a fresh install (until
/// `settings.onboarded` is persisted) and whenever there's no disc or
/// tgg-melee installed; a given or remembered game it can't use says why.
fn start(iso: Option<&Path>, data: &Data, settings: &settings::Settings) -> Start {
    let installed = tgg_mod::installed_ports(&data.ports)
        .is_ok_and(|ports| ports.iter().any(|port| port.current));
    let found = tgg_app::find_game(iso).map(|found| found.map(|(game, _)| game));
    let (game, problem) = match found {
        Ok(Some(game)) => {
            if !snapshot() {
                tgg_app::remember_game(&game);
            }
            match tgg_app::game_references(game) {
                Ok(references) => (Some(references), String::new()),
                Err(error) => (None, error.to_string()),
            }
        }
        Ok(None) => (None, String::new()),
        Err(error) => (None, error.to_string()),
    };
    // Nothing given or saved: the first Melee in the player's folders.
    let game = game.or_else(|| {
        if iso.is_some() {
            return None;
        }
        let choice = tgg_app::discover_games().into_iter().next()?;
        let game = tgg_app::Game::open(&choice.path).ok()?;
        if !snapshot() {
            tgg_app::remember_game(&game);
        }
        tgg_app::game_references(game).ok()
    });
    if settings.onboarded
        && installed
        && let Some(game) = game
    {
        return Start::Game(game);
    }
    let state = FirstRun {
        problem: (!problem.is_empty()).then_some(problem),
        ..FirstRun::default()
    };
    Start::FirstRun { game, state }
}

/// `--iso PATH`, if given. Rune reads its own flags (`--snap`, `--scene`)
/// from the same arguments.
fn iso_argument() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--iso" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

/// For scenes: jobs run inline, and a result can start the next one (the
/// replays read, then they're planned), so hand results over until none is
/// left for the picture to wait on.
fn poll_inline(app: &mut App) {
    for _ in 0..4 {
        jobs::poll_now(app);
    }
}

/// For scenes: Skins on `tab`, the site's listing and pictures read inline
/// when it's Browse (live, for a look).
fn skins_on(app: &mut App, tab: melee::Tab) {
    app.jobs.inline = true;
    nav::actions::go(app, nav::Place::Skins);
    melee::actions::show_tab(app, tab);
    // A headless run doesn't ask the site on its own; a scene does, for a look.
    if tab == melee::Tab::Browse {
        library::site::load(app);
    }
    poll_inline(app);
    app.jobs.inline = false;
}

/// For scenes: Start a mod open, its SDK and the tools found given rather
/// than fetched: GCC and git installed, CMake not.
fn new_mod_scene(app: &mut App) {
    let json = format!(
        r#"{{"api": "{}", "api_version": "1.0", "name": "tgg-melee", "version": "0.1.0",
            "game_abi": "0123456789abcdef", "compiler": "GNU", "compiler_version": "15.3.0",
            "processor": "x86_64", "target": "x86_64-linux-gnu", "include_dirs": [],
            "glibc": "2.34", "symbols": "symbols.txt"}}"#,
        tgg_mod::API
    );
    let sdk = tgg_mod::Sdk::parse(json.as_bytes(), std::path::Path::new("/sdk"))
        .expect("the scene's SDK reads");
    app.new_mod = newmod::NewMod {
        open: true,
        parent: PathBuf::from("/home/player/mods"),
        sdk: Some(Ok(sdk)),
        tools: Some(newmod::library::Tools {
            cc: Some(PathBuf::from("/usr/bin/gcc")),
            cmake: None,
            ninja: None,
            git: Some(PathBuf::from("/usr/bin/git")),
        }),
        ..newmod::NewMod::default()
    };
    app.new_mod.name.text = "Neutral spawns".into();
}

/// For scenes: first run over a fake textures.gg, its releases and sets
/// filled in place so nothing arrives mid-snapshot.
fn first_run_scene(app: &mut App) -> std::sync::Arc<tgg_mod::FakeNet> {
    let folder = std::env::temp_dir().join("tgg-studio-scene-first-run");
    let _ = std::fs::remove_dir_all(&folder);
    app.data.mods = folder.join("mods");
    app.data.ports = folder.join("tgg-data").join("tgg").join("ports");
    app.data.tgg = folder.join("tgg-data");
    let net = std::sync::Arc::new(tgg_mod::FakeNet::new());
    let mut releases = tgg_mod::port_releases(&*net).unwrap_or_default();
    if let Some(release) = releases.first_mut() {
        // A size a player knows, as the real one is.
        release.files.game.size = 48_000_000;
    }
    let sets = tgg_mod::sets(&*net).unwrap_or_default();
    app.mods.net = net.clone();
    app.mods.releases = releases;
    app.mods.sets = sets;
    app.mods.registry = mods::Registry::Answered;
    app.mods.disc = mods::Disc::Missing;
    app.mods.disc_modified.clear();
    app.first_run = Some(FirstRun::default());
    net
}

/// For scenes: the disc found, without needing one on this computer.
fn scene_disc(app: &mut App) {
    app.mods.disc = mods::Disc::Found(PathBuf::from("/scene/melee.iso"));
}

/// For scenes: Mods against a fake textures.gg, its mods and ports in a
/// scratch folder, filled in place so nothing arrives mid-snapshot. With
/// `installed`, tgg-melee is installed, Competitive's mods are in the mods
/// folder with Quick start turned off, and textures.gg has a newer Online
/// rules.
fn mods_scene(app: &mut App, installed: bool) -> std::sync::Arc<tgg_mod::FakeNet> {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    app.first_run = None;
    let folder = std::env::temp_dir().join("tgg-studio-scene-mods");
    let _ = std::fs::remove_dir_all(&folder);
    app.data.mods = folder.join("mods");
    app.data.ports = folder.join("tgg-data").join("tgg").join("ports");
    app.data.tgg = folder.join("tgg-data");
    app.data.skins = folder.join("skins.json");
    app.data.library = folder.join("library");
    app.data.history = folder.join("games");
    let net = Arc::new(tgg_mod::FakeNet::new());
    let releases = tgg_mod::port_releases(&*net).unwrap_or_default();
    let never = AtomicBool::new(false);
    if installed && let Some(release) = releases.first() {
        let _ = tgg_mod::install_port(
            &*net,
            release,
            &app.data.ports,
            &tgg_mod::no_progress,
            &never,
        );
    }
    let ports = tgg_mod::installed_ports(&app.data.ports).unwrap_or_default();
    let port = ports.iter().find(|port| port.current).cloned();
    let sets = tgg_mod::sets(&*net).unwrap_or_default();
    let catalog = port
        .as_ref()
        .and_then(|port| tgg_mod::catalog(&*net, &port.port()).ok());
    if let (true, Some(catalog), Some(competitive)) = (
        installed,
        &catalog,
        sets.iter().find(|set| set.name == "competitive"),
    ) {
        let plan = tgg_mod::plan_set(competitive, catalog, &tgg_mod::Local::empty());
        let _ = tgg_mod::apply_cached(
            &*net,
            &plan,
            &app.data.mods,
            None,
            &tgg_mod::no_progress,
            &never,
        );
        if let Ok(id) = "tgg.quick-start".parse() {
            let _ = tgg_mod::set_enabled(&app.data.mods, &id, false);
        }
    }
    // As the app fetches: before tgg-melee is installed, its newest release's catalog.
    let layout = port.as_ref().map(tgg_mod::PortInstall::port).or_else(|| {
        releases
            .first()
            .map(|release| release.port(std::path::PathBuf::new()))
    });
    let catalog = layout.and_then(|layout| tgg_mod::catalog(&*net, &layout).ok());
    // The registry's listings and their icons, decoded here.
    if let Ok(page) = tgg_mod::mods(&*net, 1) {
        for summary in page.items {
            if let Some(icon) = &summary.icon {
                scene_image(app, &*net, &icon.url);
            }
            app.mods.listings.insert(summary.id.clone(), summary);
        }
    }
    app.mods.net = net.clone();
    app.mods.releases = releases;
    app.mods.ports = ports;
    let layout = port.as_ref().map(tgg_mod::PortInstall::port);
    app.mods.local =
        tgg_mod::local(&app.data.mods, layout.as_ref()).unwrap_or_else(|_| tgg_mod::Local::empty());
    app.mods.skins = tgg_mod::skins(&app.data.skins).unwrap_or_default();
    app.mods.sets = sets;
    app.mods.catalog = catalog;
    app.mods.registry = mods::Registry::Answered;
    app.mods.disc = app.game.as_ref().map_or(mods::Disc::Missing, |references| {
        mods::Disc::Found(references.game().path().to_owned())
    });
    app.nav.place = nav::Place::Mods;
    net
}

/// For scenes: add the textures.gg set `link` to the player's sets as it
/// is, locked.
fn add_scene_set(app: &mut App, net: &dyn tgg_mod::Net, link: &str) {
    let _ = tgg_mod::sets::get(net, &app.data.tgg, link);
}

/// For scenes: a set of the player's named `name`, building on `links`,
/// with `add`'s mods added and `left_out`'s left out.
fn make_scene_set(
    app: &mut App,
    net: &dyn tgg_mod::Net,
    name: &str,
    links: &[&str],
    add: &[&str],
    left_out: &[&str],
) {
    let (data, local) = (app.data.tgg.clone(), app.mods.local.clone());
    let Ok(mut set) = tgg_mod::sets::new(&data, name) else {
        return;
    };
    for link in links {
        match tgg_mod::sets::build_on(net, &data, &local, &set.key, link) {
            Ok(built) => set = built,
            Err(_) => return,
        }
    }
    for id in add {
        if let Ok(id) = id.parse() {
            set.add(id, tgg_mod::sets::Want::Latest);
        }
    }
    for id in left_out {
        if let Ok(id) = id.parse() {
            set.leave_out(&id, true);
        }
    }
    let _ = tgg_mod::sets::save(&data, &set);
}

/// For scenes: the sets as the scene's folder has them, `active` the one
/// Play starts and `chosen` the one the page shows, their plans worked out
/// here so nothing arrives mid-snapshot.
fn show_scene_sets(app: &mut App, net: &dyn tgg_mod::Net, active: &str, chosen: &str) {
    use std::sync::Arc;
    let _ = tgg_mod::sets::activate(&app.data.tgg, active);
    app.sets.list = tgg_mod::sets::list(&app.data.tgg, &app.mods.local).unwrap_or_default();
    app.sets.active = active.to_owned();
    app.mods.chosen_set = Some(chosen.to_owned());
    for key in [active, chosen] {
        let Some(set) = app.sets.get(key).cloned() else {
            continue;
        };
        if set.is_installed() {
            continue;
        }
        if let Ok(plan) = tgg_mod::sets::plan(net, &app.data.tgg, &app.data.mods, &set, false) {
            app.sets
                .plans
                .insert(key.to_owned(), crate::sets::Planned::Ready(Arc::new(plan)));
        }
    }
}

/// For scenes: Play with tgg-melee and Competitive installed, Friday night
/// building on Competitive, a signed-in player and three last matches. A
/// scene runs no frames, so the jobs Mods starts are run here and their
/// answers taken before the scene sets what the shot shows (its queues
/// too, by [`play_queues`]).
fn play_scene(app: &mut App) {
    let _ = play_scene_net(app);
}

fn play_scene_net(app: &mut App) -> Arc<tgg_mod::FakeNet> {
    use tgg_mod::sets;
    app.jobs.inline = true;
    let fake = mods_scene(app, true);
    let net = app.mods.net.clone();
    let (data, mods) = (app.data.tgg.clone(), app.data.mods.clone());
    let local = app.mods.local.clone();
    if let Err(error) = sets::get(&*net, &data, "competitive") {
        tgg_app::log(&format!("scene: adding Competitive: {error}"));
    }
    if let Ok(mut friday) = sets::new(&data, "Friday night") {
        match sets::build_on(&*net, &data, &local, &friday.key, "competitive") {
            Ok(built) => friday = built,
            Err(error) => tgg_app::log(&format!("scene: Friday night on Competitive: {error}")),
        }
        for id in ["tgg.quick-start", "tgg.skip-results", "tgg.legal-stages"] {
            if let Ok(id) = id.parse() {
                friday.add(id, sets::Want::Latest);
            }
        }
        if let Err(error) = sets::save(&data, &friday) {
            tgg_app::log(&format!("scene: saving Friday night: {error}"));
        }
    }
    let list = sets::list(&data, &local).unwrap_or_default();
    let mut plans = std::collections::HashMap::new();
    for set in &list {
        if set.is_installed() {
            continue;
        }
        match sets::plan(&*net, &data, &mods, set, false) {
            Ok(mut plan) => {
                // The fake's packages are data, which only the player sees;
                // Competitive's mods decide the queue, so make them count.
                for mod_ in &mut plan.plan.mods {
                    mod_.netplay = tgg_mod::Netplay::Code;
                }
                plans.insert(set.key.clone(), crate::sets::Planned::Ready(Arc::new(plan)));
            }
            Err(error) => tgg_app::log(&format!("scene: planning {}: {error}", set.name)),
        }
    }
    // What Mods started, now; then the scene's own state, last, so nothing
    // arriving later (there are no later frames) undoes it.
    app.play.opened = true;
    for _ in 0..8 {
        jobs::poll_now(app);
    }
    app.jobs.inline = false;
    app.sets.list = list;
    app.sets.active = "friday-night".to_owned();
    app.sets.plans = plans;
    app.sets.launching = None;
    app.sets.session = None;
    app.signin.who = Some(signed_in("ciao"));
    play_matches(app);
    app.nav.place = nav::Place::Play;
    fake
}

/// For scenes: the queues the meeting point would list. With `own`, the
/// set's queue is one a player made, so Competitive stays in the list.
fn play_queues(app: &mut App, own: bool) {
    let Some(friday) = app.sets.plan("friday-night").map(crate::play::gameplay) else {
        return;
    };
    let competitive = app
        .sets
        .plan("competitive")
        .map(crate::play::gameplay)
        .unwrap_or_default();
    let queues = if own {
        vec![
            play_queue("player", "Friday night", 2, friday),
            play_queue("standing", "Competitive", 14, competitive),
            play_queue(
                "player",
                "First to 3",
                4,
                queue_mods(&[
                    "tgg.quick-start",
                    "tgg.skip-results",
                    "tgg.legal-stages",
                    "tgg.netplay",
                ]),
            ),
        ]
    } else {
        vec![
            play_queue("standing", "Competitive", 14, friday),
            play_queue("player", "Turbo", 1, queue_mods(&["tgg.netplay"])),
        ]
    };
    app.play.queues = Some(Ok(queues));
}

fn play_queue(
    kind: &str,
    label: &str,
    searching: u32,
    mods: Vec<tgg_mod::sets::QueueMod>,
) -> tgg_mod::Queue {
    tgg_mod::Queue {
        kind: kind.to_owned(),
        label: label.to_owned(),
        searching,
        mods,
        set: None,
    }
}

fn queue_mods(ids: &[&str]) -> Vec<tgg_mod::sets::QueueMod> {
    ids.iter()
        .filter_map(|id| {
            Some(tgg_mod::sets::QueueMod {
                id: id.parse().ok()?,
                version: "1.0.0".parse().ok()?,
            })
        })
        .collect()
}

/// For scenes: the player's three last matches, and pictures of the
/// fighters in them and of the one the card shows.
fn play_matches(app: &mut App) {
    use crate::jobs::renders::RenderKey;
    use tgg_mod::{ReplayInfo, ReplayPlayer};
    let me = ReplayPlayer {
        port: 0,
        name: Some("ciao".into()),
        code: None,
        character: Some(2),
        costume: Some(2),
    };
    let others = [
        ("waffle", 20u8, 0u8, 31u16, false),
        ("santi", 15, 1, 28, true),
        ("porkchop", 0, 3, 32, false),
    ];
    let mut items = Vec::new();
    let mut slots = Vec::new();
    for (index, (name, character, costume, stage, lost)) in others.into_iter().enumerate() {
        if let Some(slot) = tgg_app::replays::character(character)
            .and_then(|character| crate::replays::actions::costume_slot(app, character, costume))
        {
            slots.push(slot);
        }
        items.push(crate::replays::Item {
            path: PathBuf::from(format!("/home/player/replays/Game_{index}.slp")),
            source: "tgg-melee".to_owned(),
            info: Some(ReplayInfo {
                started: None,
                frames: Some(60 * 110),
                stage: Some(stage),
                slippi: None,
                played_on: Some("tgg-melee".to_owned()),
                players: vec![
                    me.clone(),
                    ReplayPlayer {
                        port: 1,
                        name: Some(name.to_owned()),
                        code: None,
                        character: Some(character),
                        costume: Some(costume),
                    },
                ],
                winner: Some(if lost { 1 } else { 0 }),
            }),
            profile: Err("a scene's replay".to_owned()),
            plan: None,
            status: crate::replays::Status::Unknown,
        });
    }
    app.replays.items = items;
    app.replays.scanned = true;
    app.replays.planned = true;
    app.replays.folders = vec![(
        "tgg-melee".to_owned(),
        PathBuf::from("/home/player/replays"),
    )];
    let Some(iso) = app
        .game
        .as_ref()
        .map(|references| references.game().path().to_owned())
    else {
        return;
    };
    let mut keys = Vec::new();
    // The player's own tile in each row: Fox, in the costume of the latest.
    if let Some(me) = melee_dat::Character::from_code("Fx")
        .and_then(|fox| crate::replays::actions::costume_slot(app, fox, 2))
    {
        slots.push(me);
    }
    for slot in slots {
        keys.push(RenderKey::Slot(slot));
        app.renders.request(&iso, slot);
    }
    // The card's fighter: the one the latest match was played in.
    let hero = melee_dat::Character::from_code("Fx")
        .and_then(|fox| crate::replays::actions::costume_slot(app, fox, 2));
    app.play.hero = hero;
    if let Some(hero) = hero {
        keys.push(RenderKey::Fighter(hero));
        app.renders.request_fighter(&iso, hero);
    }
    crate::play::actions::stand(app);
    // The shot is taken at once: wait for the worker, so every picture is in.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !keys.iter().all(|key| app.renders.drawn(*key)) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// For scenes: searching in Competitive, seven seconds in.
fn searching_scene(app: &mut App) {
    app.sets.session = Some(crate::sets::Session {
        how: crate::sets::How::Search,
        name: "Friday night".to_owned(),
        set_key: app.sets.active.clone(),
        profile_key: None,
    });
    app.mods.playing = Some(crate::mods::Playing {
        stop: Arc::default(),
    });
    app.companion_override = Some(crate::companion::LiveState {
        pid: 1,
        mod_set_id: String::new(),
        activity: crate::companion::Activity::Menu,
        replay_path: String::new(),
        online: crate::companion::Online {
            state: crate::companion::OnlineState::Searching,
            queue_label: "Competitive".to_owned(),
            search_ms: 7_000,
            ..Default::default()
        },
        updated_at: Some(std::time::Instant::now()),
    });
}

/// For scenes: getting Friday night's mods ready, a third of the way.
fn launching_scene(app: &mut App) {
    let progress = tgg_mod::Progress {
        step: 0,
        steps: 2,
        label: "Downloading Legal stages".into(),
        bytes: 3,
        total: Some(10),
    };
    app.sets.launching = Some(crate::sets::Launching {
        doing: "Getting Friday night's mods".into(),
        progress: Arc::new(std::sync::Mutex::new(Some(progress))),
        cancel: Arc::default(),
    });
}

/// A room's report, as the game writes it, for the sheet's scenes: one
/// line, every field on it.
const STARTING: &str = "{\"state\":\"starting\",\"code\":\"\",\"link\":\"\",\"host\":\"\",\
                        \"players\":[],\"max\":4,\"signin\":{\"url\":\"\",\"code\":\"\"},\
                        \"why\":\"\"}";
const SIGNING_IN: &str = "{\"state\":\"signing-in\",\"code\":\"\",\"link\":\"\",\"host\":\"\",\
                          \"players\":[],\"max\":4,\"signin\":{\"url\":\"https://textures.gg/device\",\
                          \"code\":\"KWFD-QRNT\"},\"why\":\"\"}";
const WAITING_ONE: &str = "{\"state\":\"waiting\",\"code\":\"KFXD\",\"link\":\"textures-gg://join/KFXD\",\
                           \"host\":\"saavy\",\"players\":[\"saavy\"],\"max\":4,\
                           \"signin\":{\"url\":\"\",\"code\":\"\"},\"why\":\"\"}";
const WAITING_THREE: &str = "{\"state\":\"waiting\",\"code\":\"KFXD\",\"link\":\"textures-gg://join/KFXD\",\
                             \"host\":\"saavy\",\"players\":[\"saavy\",\"bo\",\"wren\"],\"max\":4,\
                             \"signin\":{\"url\":\"\",\"code\":\"\"},\"why\":\"\"}";
const GETTING_MODS: &str = "{\"state\":\"getting-mods\",\"code\":\"KFXD\",\"link\":\"textures-gg://join/KFXD\",\
                            \"host\":\"saavy\",\"players\":[\"saavy\",\"bo\"],\"max\":4,\
                            \"signin\":{\"url\":\"\",\"code\":\"\"},\"why\":\"\"}";
const PLAYING: &str = "{\"state\":\"playing\",\"code\":\"KFXD\",\"link\":\"textures-gg://join/KFXD\",\
                       \"host\":\"saavy\",\"players\":[\"saavy\",\"bo\",\"wren\",\"tess\"],\"max\":4,\
                       \"signin\":{\"url\":\"\",\"code\":\"\"},\"why\":\"\"}";
const ENDED: &str = "{\"state\":\"ended\",\"code\":\"\",\"link\":\"\",\"host\":\"saavy\",\
                     \"players\":[],\"max\":4,\"signin\":{\"url\":\"\",\"code\":\"\"},\
                     \"why\":\"The host left, so the room closed.\"}";

/// For scenes: a game started `how` and running, its live companion state
/// parsed from `line`, and the friend sheet open over Play.
fn room_scene(app: &mut App, how: crate::sets::How, line: &str) {
    app.sets.session = Some(crate::sets::Session {
        how,
        name: "Friday night".to_owned(),
        set_key: app.sets.active.clone(),
        profile_key: None,
    });
    app.mods.playing = Some(crate::mods::Playing {
        stop: Arc::default(),
    });
    app.play.friend = true;
    #[derive(serde::Deserialize)]
    struct SceneRoom {
        #[serde(default)]
        state: String,
        #[serde(default)]
        code: String,
        #[serde(default)]
        link: String,
        #[serde(default)]
        host: String,
        #[serde(default)]
        players: Vec<String>,
        #[serde(default)]
        max: u32,
        #[serde(default)]
        signin: crate::companion::SigninInfo,
        #[serde(default)]
        why: String,
    }
    if let Ok(r) = serde_json::from_str::<SceneRoom>(line) {
        let state = match r.state.as_str() {
            "starting" => crate::companion::OnlineState::Connecting,
            "signing-in" => crate::companion::OnlineState::SigningIn,
            "waiting" => crate::companion::OnlineState::RoomWaiting,
            "getting-mods" => crate::companion::OnlineState::GettingMods,
            "playing" => crate::companion::OnlineState::RoomPlaying,
            "ended" => crate::companion::OnlineState::Ended,
            _ => crate::companion::OnlineState::Idle,
        };
        app.companion_override = Some(crate::companion::LiveState {
            pid: 1,
            mod_set_id: String::new(),
            activity: crate::companion::Activity::Menu,
            replay_path: String::new(),
            online: crate::companion::Online {
                state,
                queue_label: String::new(),
                search_ms: 0,
                opponent: String::new(),
                room: crate::companion::RoomInfo {
                    code: r.code,
                    link: r.link,
                    host: r.host,
                    players: r.players,
                    max: r.max,
                },
                signin: r.signin,
                why: r.why,
            },
            updated_at: Some(std::time::Instant::now()),
        });
    }
}

/// For scenes: hosting a room, its link and code out.
fn hosting_scene(app: &mut App) {
    room_scene(app, crate::sets::How::Host, WAITING_ONE);
}

/// For scenes: Friday night's game waits on the player's yes to native code
/// from a publisher other than textures.gg.
fn trust_scene(app: &mut App) {
    let Some(set) = app.sets.get("friday-night").cloned() else {
        return;
    };
    let Some(crate::sets::Planned::Ready(plan)) = app.sets.plans.get("friday-night").cloned()
    else {
        return;
    };
    app.sets.trust = Some(crate::sets::Trust {
        how: crate::sets::How::Search,
        source: crate::sets::Source::Set { set, plan },
        publishers: vec![(
            "u-fizzi".into(),
            "Fizzi".into(),
            vec!["Rollback".into(), "Frame data".into()],
        )],
        remember: false,
    });
}

/// For scenes: what the set plays without, above the card.
fn notices_scene(app: &mut App) {
    let Some(crate::sets::Planned::Ready(plan)) = app.sets.plans.get_mut("friday-night") else {
        return;
    };
    let plan = std::sync::Arc::make_mut(plan);
    plan.notices
        .push("textures.gg took Weekend down, so its mods are now part of Friday night".to_owned());
    if let Ok(id) = "tgg.turbo".parse() {
        plan.mods.push(tgg_mod::sets::SetMod {
            id,
            group: tgg_mod::sets::Group::Added,
            version: None,
            class: None,
            skipped: Some("textures.gg withdrew it (crashes on Windows).".to_owned()),
        });
    }
}

/// The account a scene is signed in as.
fn signed_in(handle: &str) -> tgg_account::SignedIn {
    tgg_account::SignedIn {
        account: tgg_account::Account {
            handle: handle.to_owned(),
            display_name: None,
            id: None,
        },
        verified: true,
    }
}

/// Whether this run is a snapshot (`--snap`), for Rune's tooling.
pub(crate) fn snapshot() -> bool {
    std::env::args().any(|argument| argument == "--snap")
}

/// For scenes: the image at `url`, fetched and decoded now.
fn scene_image(app: &mut App, net: &dyn tgg_mod::Net, url: &str) {
    let Ok(bytes) = net.get(url, 8 * 1024 * 1024) else {
        return;
    };
    let Ok(image) = image::load_from_memory(&bytes) else {
        return;
    };
    let rgba = image.to_rgba8();
    let pixels = rune::Pixels::new();
    pixels.set(rgba.width(), rgba.height(), rgba.into_raw());
    app.mods.images.insert(url.to_owned(), pixels);
}

/// Skin files given on the command line (as a file manager's Open With
/// passes them): to the Library, through its review, once there's a game.
fn open_files(app: &mut App) {
    let mut files = Vec::new();
    let mut args = rune::extra_args().iter();
    while let Some(argument) = args.next() {
        if argument == "--iso" {
            args.next();
            continue;
        }
        let path = PathBuf::from(argument);
        if !argument.starts_with('-') && path.is_file() {
            files.push(path);
        }
    }
    if files.is_empty() || app.game.is_none() {
        return;
    }
    melee::actions::show_mine(app);
    library::actions::add(app, &files);
}

/// A scene's app, on Skins.
fn skins(app: &mut App) -> &mut App {
    app.nav.place = nav::Place::Skins;
    app
}
