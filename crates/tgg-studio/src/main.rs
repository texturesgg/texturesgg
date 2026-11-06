//! tgg-studio: the textures.gg desktop app on Rune. It finds the player's
//! Melee NTSC 1.02 ISO and shows what's in it, keeps their library of skins,
//! installs them, and edits costumes; mods follow.
//!
//! ```text
//! tgg-studio [--iso PATH]
//! tgg-studio [--iso PATH] --snap out.png [--scene NAME]   headless, as Rune's tooling
//! ```

mod jobs;
mod library;
mod melee;
mod mods;
mod nav;
mod shell;
mod state;
mod ui;
mod welcome;
mod window;

#[cfg(test)]
mod flows;

use rune::{AppSpec, WindowOptions};
use state::{App, Start};
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
    ("library", |app| app.nav.place = nav::Place::Library),
    ("mods", |app| app.nav.place = nav::Place::Mods),
    ("no-sidebar", |app| app.nav.sidebar = false),
    ("paper", |app| app.settings.set_appearance(settings::Appearance::Paper)),
];

fn main() {
    tgg_app::log::start();
    let iso = iso_argument();
    let spec = AppSpec::<App> {
        min_size: Some((900.0, 600.0)),
        fonts: ui::fonts::FONTS,
        families: ui::fonts::FAMILIES,
        scenes: SCENES,
        on_key: Some(nav::actions::on_key),
        setup: Some(jobs::setup),
        poll: Some(jobs::poll),
        ..AppSpec::new("textures.gg", (1280.0, 800.0), launch, shell::view)
    };
    rune::App::new(spec)
        // The app draws its own title row, so Linux and Windows windows have
        // no system one; macOS keeps its window buttons.
        .window_options(WindowOptions { borderless: cfg!(not(target_os = "macos")), ..Default::default() })
        .init(move || App::new(start(iso.as_deref()), settings::load(), true))
        .run();
}

/// The app as a plain launch finds it, for Rune's tooling.
fn launch() -> App {
    App::new(start(None), settings::load(), true)
}

/// The game given, else the one chosen before; Welcome finds one otherwise,
/// or says why a given one won't do.
fn start(iso: Option<&Path>) -> Start {
    let found = match iso {
        Some(iso) => tgg_app::find_game(Some(iso)).map(|found| found.map(|(game, _)| game)),
        None => Ok(tgg_app::saved_game()),
    };
    let problem = match found {
        Ok(Some(game)) => {
            tgg_app::remember_game(&game);
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
