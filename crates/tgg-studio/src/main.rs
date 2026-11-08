//! tgg-studio: the textures.gg desktop app on Rune. It finds the player's
//! Melee NTSC 1.02 ISO and shows what's in it, keeps their library of skins,
//! installs them, edits costumes, and installs tgg-melee and its mods.
//!
//! ```text
//! tgg-studio [--iso PATH] [SKIN...]                         skins open in the Library's review
//! tgg-studio [--iso PATH] --snap out.png [--scene NAME]   headless, as Rune's tooling
//! ```

mod editor;
mod jobs;
mod keys;
mod library;
mod melee;
mod mods;
mod nav;
mod preferences;
mod report;
mod shell;
mod stage;
mod state;
mod ui;
mod welcome;
mod window;

#[cfg(test)]
mod flows;
#[cfg(all(test, feature = "melee-iso"))]
mod iso_flows;

use rune::{AppSpec, WindowOptions};
use state::{App, Data, Start};
use std::path::{Path, PathBuf};
use tgg_app::settings;
use welcome::Welcome;

/// A screen, as a named edit of the app at launch, for `--snap --scene`.
type Scene = (&'static str, fn(&mut App));

const SCENES: &[Scene] = &[
    ("welcome", |app| {
        app.game = None;
        app.welcome.games = tgg_app::discover_games();
    }),
    ("stages", |app| app.melee.tab = melee::Tab::Stages),
    ("fox", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(app, fox);
        }
    }),
    ("fox-shine", |app| show_shared(app, "Fx", "Shine")),
    ("fox-laser", |app| show_shared(app, "Fx", "Laser")),
    ("fox-edit", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(app, fox);
            editor::actions::open(app);
        }
    }),
    ("fox-edit-helmet", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(app, fox);
            editor::actions::open(app);
            editor::actions::select(app, Some(1));
        }
    }),
    // A click on the middle of the model, as the player would to pick its texture.
    ("fox-edit-pick", |app| {
        if let Some(fox) = melee_dat::Character::from_code("Fx") {
            melee::actions::open_fighter(app, fox);
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
            melee::actions::open_fighter(app, fox);
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
        melee::actions::open_stage(app, stage);
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
            melee::actions::open_fighter(app, fox);
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
            melee::actions::open_stage(app, stage);
        }
    }),
    ("library", |app| nav::actions::go(app, nav::Place::Library)),
    // Review over Library: Fox's green costume, offered as a skin, and a
    // file that isn't one.
    ("library-review", |app| {
        nav::actions::go(app, nav::Place::Library);
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
        melee::actions::open_fighter(app, fox);
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
        app.data.ports = folder.join("ports");
        if let Some(references) = &app.game {
            app.mods.disc = mods::Disc::Found(references.game().path().to_owned());
        }
    }),
    ("settings", preferences::actions::open),
    ("report", |app| report::actions::open(app, false)),
    ("no-sidebar", |app| app.nav.sidebar = false),
    ("paper", |app| {
        app.settings.set_appearance(settings::Appearance::Paper)
    }),
];

fn main() {
    tgg_app::log::start("tgg-studio", env!("CARGO_PKG_VERSION"));
    let iso = iso_argument();
    let spec = AppSpec::<App> {
        min_size: Some((900.0, 600.0)),
        fonts: ui::fonts::FONTS,
        families: ui::fonts::FAMILIES,
        scenes: SCENES,
        on_key: Some(keys::on_key),
        setup: Some(jobs::setup),
        poll: Some(jobs::poll),
        ..AppSpec::new("textures.gg", (1280.0, 800.0), launch, shell::view)
    };
    rune::App::new(spec)
        // The app draws its own title row, so Linux and Windows windows have
        // no system one; macOS keeps its window buttons.
        .window_options(WindowOptions {
            borderless: cfg!(not(target_os = "macos")),
            ..Default::default()
        })
        .init(move || {
            let mut app = App::new(
                start(iso.as_deref()),
                settings::load(),
                Data::of_player(),
                true,
            );
            open_files(&mut app);
            app
        })
        .compositor(stage::viewport::Viewport::default())
        .run();
    // The PNGs opened in the player's image editor go with the app.
    tgg_app::external::remove_temporary_directory();
}

/// The app as a plain launch finds it, for Rune's tooling.
fn launch() -> App {
    App::new(start(None), settings::load(), Data::of_player(), true)
}

/// The game given, else the one chosen before, else the one Slippi Launcher
/// plays or one in Dolphin's folders, remembered so the next launch opens
/// it directly. Welcome lists the games found otherwise, or says why a
/// given one won't do. A snapshot remembers nothing: it's not the player
/// choosing.
fn start(iso: Option<&Path>) -> Start {
    let found = tgg_app::find_game(iso).map(|found| found.map(|(game, _)| game));
    let problem = match found {
        Ok(Some(game)) => {
            if !snapshot() {
                tgg_app::remember_game(&game);
            }
            match tgg_app::game_references(game) {
                Ok(references) => return Start::Game(references),
                Err(error) => error.to_string(),
            }
        }
        Ok(None) => String::new(),
        Err(error) => error.to_string(),
    };
    Start::Welcome(Welcome {
        games: tgg_app::discover_games(),
        problem: (!problem.is_empty()).then_some(problem),
    })
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
    melee::actions::open_fighter(app, character);
    if let Some(model) = melee_dat::SharedModel::of(character).find(|model| model.name() == name) {
        stage::actions::play_action(app, model.action());
    }
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
    app.data.ports = folder.join("ports");
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
        let _ = tgg_mod::apply(&*net, &plan, &app.data.mods, &tgg_mod::no_progress, &never);
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

/// Whether this run is a snapshot (`--snap`), for Rune's tooling.
fn snapshot() -> bool {
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
    nav::actions::go(app, nav::Place::Library);
    library::actions::add(app, &files);
}
