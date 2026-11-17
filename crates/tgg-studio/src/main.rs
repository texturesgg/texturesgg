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
#[expect(
    dead_code,
    reason = "first run, Play and Mods use the sets, in the pull requests after this one"
)]
mod sets;
mod shell;
mod signin;
mod slippi;
mod stage;
mod state;
mod ui;
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
            first.picks = first_run::Picks {
                competitive: false,
                practice: false,
                replays: false,
            };
        }
    }),
    ("stages", |app| skins(app).melee.tab = melee::Tab::Stages),
    ("skins", |app| {
        skins(app);
    }),
    ("fox", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(skins(app), fox);
        }
    }),
    ("fox-shine", |app| show_shared(app, "Fx", "Shine")),
    ("fox-laser", |app| show_shared(app, "Fx", "Laser")),
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
    ("fox-moves", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(skins(app), fox);
            melee::actions::toggle_pane(app, melee::Pane::Moves);
        }
    }),
    ("final-destination", |app| {
        if let Some(stage) = app
            .melee
            .stages
            .iter()
            .copied()
            .find(|stage| stage.name() == "Final Destination")
        {
            melee::actions::open_stage(skins(app), stage);
        }
    }),
    ("library", melee::actions::show_mine),
    // Review over Library: Fox's green costume, offered as a skin, and a
    // file that isn't one.
    ("library-review", |app| {
        melee::actions::show_mine(app);
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
    // Mods against a fake textures.gg, in a folder of its own: nothing
    // installed; tgg-melee and Competitive installed, a mod turned off and
    // one with an update; the review before installing Replays; and an
    // install under way.
    ("mods", |app| mods_scene(app, false)),
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
    // Hosting a room: its code, large, with Copy.
    ("play-friend-host", |app| {
        play_scene(app);
        play_queues(app, false);
        hosting_scene(app);
    }),
    // Joining one: a code typed.
    ("play-friend-join", |app| {
        play_scene(app);
        play_queues(app, false);
        app.play.friend = true;
        app.play.join.text = "FOXD-7K2Q".into();
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
    // A toast after an install, offering Undo.
    ("toast", |app| {
        skins(app);
        if let Ok(slot) = "PlFxOr.dat".parse() {
            app.tell_undoable(
                "Installed DARK GOKU FOX HALO into Fox · Orange in Slippi.",
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
    ("mods-installed", |app| mods_scene(app, true)),
    // Updating Competitive to a version without Quick start, Online rules
    // newer.
    ("mods-update-review", |app| {
        mods_scene(app, true);
        if let Some(set) = app
            .mods
            .sets
            .iter_mut()
            .find(|set| set.name == "competitive")
        {
            set.version += 1;
            set.mods.retain(|m| m.id.as_str() != "tgg.quick-start");
        }
        mods::actions::read_disk(app);
        mods::actions::review_set(app, "competitive");
    }),
    // tgg-melee's versions: the one installed, and a newer one offered.
    ("mods-versions", |app| {
        mods_scene(app, true);
        let newer = app.mods.releases.first().cloned().map(|mut release| {
            release.version = "0.6.0".into();
            release
        });
        app.mods.releases.splice(0..0, newer);
        mods::actions::show_versions(app, true);
    }),
    // Online rules updating on its own, its progress in its row.
    ("mods-updating-one", |app| {
        mods_scene(app, true);
        mods::actions::show_tab(app, mods::Tab::Installed);
        let progress = tgg_mod::Progress {
            step: 0,
            steps: 1,
            label: "Downloading Online rules".into(),
            bytes: 1,
            total: Some(2),
        };
        app.mods.work = Some(mods::Work {
            doing: "Updating Online rules".into(),
            progress: std::sync::Arc::new(std::sync::Mutex::new(Some(progress))),
            cancel: std::sync::Arc::default(),
            target: "tgg.online-rules".parse().ok(),
        });
    }),
    // Before textures.gg first answers: placeholders where the collections go.
    ("mods-loading", |app| {
        mods_scene(app, true);
        app.mods.sets.clear();
        app.mods.catalog = None;
        app.mods.registry = mods::Registry::Asking;
    }),
    ("mods-installed-tab", |app| {
        mods_scene(app, true);
        mods::actions::show_tab(app, mods::Tab::Installed);
    }),
    ("mods-all", |app| {
        mods_scene(app, true);
        mods::actions::show_tab(app, mods::Tab::All);
    }),
    // Direct connect's details beside every mod.
    ("mods-details", |app| {
        mods_scene(app, true);
        mods::actions::show_tab(app, mods::Tab::All);
        if let Ok(id) = "tgg.direct-connect".parse::<tgg_mod::ModId>() {
            // Its page fetched here, not on a job, so it's there for the picture.
            let net = app.mods.net.clone();
            if let Ok(details) = tgg_mod::mod_details(&*net, &id) {
                for shot in &details.screenshots {
                    scene_image(app, &*net, &shot.url);
                }
                app.mods.details.insert(id.clone(), Some(details));
            }
            mods::actions::select(app, &id);
        }
    }),
    ("mods-review", |app| {
        mods_scene(app, true);
        mods::actions::review_set(app, "replays");
    }),
    ("mods-installing", |app| {
        mods_scene(app, false);
        // The work alone, with nothing downloading: the picture is of the bar.
        let progress = tgg_mod::Progress {
            step: 0,
            steps: 1,
            label: "Downloading tgg-melee 0.5.0".into(),
            bytes: 4,
            total: Some(10),
        };
        app.mods.work = Some(mods::Work {
            doing: "Installing tgg-melee 0.5.0".into(),
            progress: std::sync::Arc::new(std::sync::Mutex::new(Some(progress))),
            cancel: std::sync::Arc::default(),
            target: None,
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
    // New mod over Mods, filled in, CMake missing (the SDK and tools as
    // given here, not looked for).
    ("new-mod", |app| {
        mods_scene(app, true);
        new_mod_scene(app);
        app.new_mod.name.text = "Neutral spawns".into();
    }),
    // New mod for the player's own tgg-melee: its real SDK and tools.
    ("new-mod-here", |app| {
        mods::actions::read_disk(app);
        newmod::actions::open(app);
    }),
    // A mod just started.
    ("new-mod-created", |app| {
        mods_scene(app, true);
        new_mod_scene(app);
        app.new_mod.name.text = "Neutral spawns".into();
        app.new_mod.stage = newmod::Stage::Created(newmod::library::CreatedMod {
            dir: app.new_mod.folder(),
            id: "neutral-spawns".parse().expect("an id"),
            example: newmod::TEMPLATE.into(),
            sdk_version: "0.1.0".into(),
            repository: newmod::library::Repository::Committed,
        });
    }),
    ("settings", preferences::actions::open),
    ("settings-menu", |app| {
        preferences::actions::open(app);
        preferences::actions::toggle_menu(app, preferences::Menu::ImageEditor);
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
    // $TGG_SCENE_HANDLE names whose profile textures.gg is asked for (for a
    // look; the default has none).
    ("profile", |app| {
        let handle = std::env::var("TGG_SCENE_HANDLE").unwrap_or_else(|_| "ciao".into());
        app.signin.who = Some(tgg_account::SignedIn {
            account: tgg_account::Account {
                display_name: Some(handle.clone()),
                handle,
                id: None,
            },
            verified: true,
        });
        // Read before the picture is taken.
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Profile);
        jobs::poll_now(app);
        app.jobs.inline = false;
    }),
    // The player's own replays, read and resolved against the live
    // textures.gg before the picture (not deterministic: for a look).
    ("replays", |app| {
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Replays);
        jobs::poll_now(app);
        app.jobs.inline = false;
    }),
    // The same, its first two-player replay chosen, with its stats.
    ("replay-details", |app| {
        app.jobs.inline = true;
        nav::actions::go(app, nav::Place::Replays);
        jobs::poll_now(app);
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
            jobs::poll_now(app);
        }
        app.jobs.inline = false;
    }),
    ("no-sidebar", |app| app.nav.sidebar = false),
];

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
        let app = rune::App::new(spec)
            .window_options(rune::WindowOptions {
                position,
                // On macOS the system blurs what's behind the window under
                // the sidebar (`shell::sidebar`); everything else fills its
                // own part. Rune blurs behind windows nowhere else.
                transparent: cfg!(target_os = "macos"),
                ..Default::default()
            })
            // Coming back to the window: a sign-in or sign-out made with `tgg` shows.
            .on_focus(signin::actions::focused)
            // textures-gg:// links, on macOS.
            .on_open_url(links::opened_with)
            .init(move || {
                let data = Data::of_player();
                let mut app = App::new(start(iso.as_deref(), &data), settings::load(), data, true);
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
    let start = match start(None, &data) {
        Start::FirstRun {
            game: Some(game), ..
        } => Start::Game(game),
        other => other,
    };
    App::new(start, settings::load(), data, true)
}

/// The game given, else the one chosen before, else the one Slippi Launcher
/// plays or one in Dolphin's folders, remembered so the next launch opens it
/// directly. With none of those, a game found in the player's folders is
/// theirs without a press. First run shows until there is one and tgg-melee
/// is installed; a given or remembered game it can't use says why.
fn start(iso: Option<&Path>, data: &Data) -> Start {
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
    if installed && let Some(game) = game {
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

/// For scenes: open the fighter `code` names and play the move that shows
/// its shared model `name`.
fn show_shared(app: &mut App, code: &str, name: &str) {
    let Some(character) = melee_dat::Character::from_code(code) else {
        return;
    };
    melee::actions::open_fighter(skins(app), character);
    if let Some(model) = melee_dat::SharedModel::of(character).find(|model| model.name() == name) {
        stage::actions::play_action(app, model.action());
    }
}

/// For scenes: New mod open, the SDK's examples and the tools found given
/// rather than looked for: GCC and git installed, CMake not.
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
    let examples = vec![
        newmod::TEMPLATE.into(),
        "damage-log".into(),
        "lcancel-trainer".into(),
    ];
    app.new_mod = newmod::NewMod {
        open: true,
        parent: PathBuf::from("/home/player/mods"),
        example: newmod::TEMPLATE.into(),
        sdk: Some(Ok((sdk, examples))),
        tools: Some(newmod::library::Tools {
            cc: Some(PathBuf::from("/usr/bin/gcc")),
            cmake: None,
            ninja: None,
            git: Some(PathBuf::from("/usr/bin/git")),
        }),
        ..newmod::NewMod::default()
    };
}

/// For scenes: first run over a fake textures.gg, its releases and sets
/// filled in place so nothing arrives mid-snapshot.
fn first_run_scene(app: &mut App) -> &mut App {
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
    let mut sets = tgg_mod::sets(&*net).unwrap_or_default();
    // The fake has Competitive and Replays; Practice is textures.gg's third.
    if let Some(mut practice) = sets.iter().find(|set| set.name == "competitive").cloned() {
        practice.name = "practice".into();
        practice.title = "Practice".into();
        practice.description = "Drills, trainers, save states and a practice stage.".into();
        sets.push(practice);
    }
    app.mods.net = net;
    app.mods.releases = releases;
    app.mods.sets = sets;
    app.mods.registry = mods::Registry::Answered;
    app.mods.disc = mods::Disc::Missing;
    app.first_run = Some(FirstRun::default());
    app
}

/// For scenes: the disc found, without needing one on this computer.
fn scene_disc(app: &mut App) {
    app.mods.disc = mods::Disc::Found(PathBuf::from("/scene/melee.iso"));
}

/// For scenes: Mods against a fake textures.gg, its mods and ports in a
/// scratch folder, filled in place so nothing arrives mid-snapshot. With
/// `installed`, tgg-melee and Competitive are installed, Quick start turned
/// off, and textures.gg has a newer Online rules.
fn mods_scene(app: &mut App, installed: bool) {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let folder = std::env::temp_dir().join("tgg-studio-scene-mods");
    let _ = std::fs::remove_dir_all(&folder);
    app.data.mods = folder.join("mods");
    app.data.ports = folder.join("tgg-data").join("tgg").join("ports");
    app.data.tgg = folder.join("tgg-data");
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
    let port = tgg_mod::installed_ports(&app.data.ports)
        .ok()
        .and_then(|ports| ports.into_iter().find(|port| port.current));
    let sets = tgg_mod::sets(&*net).unwrap_or_default();
    let catalog = port
        .as_ref()
        .and_then(|port| tgg_mod::catalog(&*net, &port.port()).ok());
    if let (Some(catalog), Some(competitive)) =
        (&catalog, sets.iter().find(|set| set.name == "competitive"))
    {
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
        net.publish_mod("tgg.online-rules", "Online rules", "0.1.1", &[]);
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
    app.mods.net = net;
    app.mods.releases = releases;
    app.mods.sets = sets;
    app.mods.catalog = catalog;
    app.mods.registry = mods::Registry::Answered;
    app.mods.disc = app.game.as_ref().map_or(mods::Disc::Missing, |references| {
        mods::Disc::Found(references.game().path().to_owned())
    });
    nav::actions::go(app, nav::Place::Mods);
}

/// For scenes: Play with tgg-melee and Competitive installed, Friday night
/// building on Competitive, a signed-in player and three last matches.
/// FakeNet has no queue endpoint, and a scene runs no frames, so the jobs
/// Mods starts are run here and their answers taken before the scene sets
/// what the shot shows.
fn play_scene(app: &mut App) {
    use tgg_mod::sets;
    app.jobs.inline = true;
    mods_scene(app, true);
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
) -> crate::play::Queue {
    crate::play::Queue {
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
        started: std::time::Instant::now() - std::time::Duration::from_secs(7),
    });
    app.mods.playing = Some(crate::mods::Playing {
        stop: Arc::default(),
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

/// For scenes: hosting a room, its code out.
fn hosting_scene(app: &mut App) {
    app.sets.session = Some(crate::sets::Session {
        how: crate::sets::How::Host,
        name: "Friday night".to_owned(),
        started: std::time::Instant::now() - std::time::Duration::from_secs(12),
    });
    app.mods.playing = Some(crate::mods::Playing {
        stop: Arc::default(),
    });
    app.play.friend = true;
    // What the game writes when it makes the room; Play reads the log each
    // second while a game runs, so the code has to be there.
    let log = crate::sets::actions::game_log(app);
    if let Some(folder) = log.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(&log, "tgg.direct-connect: meet: in room FOXD-7K2Q\n");
    app.play.log.room = Some("FOXD-7K2Q".into());
}

/// For scenes: what the set plays without, above the card.
fn notices_scene(app: &mut App) {
    let Some(crate::sets::Planned::Ready(plan)) = app.sets.plans.get_mut("friday-night") else {
        return;
    };
    let plan = std::sync::Arc::make_mut(plan);
    plan.notices
        .push("Competitive changed: added Prevent wobbling".to_owned());
    if let Ok(id) = "tgg.turbo".parse() {
        plan.mods.push(tgg_mod::sets::SetMod {
            id,
            group: tgg_mod::sets::Group::Added,
            version: None,
            class: None,
            skipped: Some("withdrawn (crashes on Windows)".to_owned()),
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
