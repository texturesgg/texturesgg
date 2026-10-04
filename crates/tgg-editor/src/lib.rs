//! The textures.gg desktop app: a gpui-ce app hosting the hsd-render
//! viewport on gpui's own wgpu device. The binary (`main.rs`) reads the
//! launch files, builds a [`MeleeModel`], and calls [`run`].
//!
//! The editor is desktop-only; the renderer beneath it stays
//! wasm-clean for the site.

mod costumes;
mod disk;
mod editor;
mod error;
mod game;
mod game_page;
mod ids;
mod install;
#[cfg(all(test, feature = "melee-iso"))]
mod iso_tests;
mod library;
mod library_page;
mod log;
mod menus;
mod moves;
mod net;
mod open_file;
mod page;
mod renders;
mod report;
mod review;
mod settings;
mod settings_page;
mod shell;
mod stage;
mod stress_test;
#[cfg(test)]
mod test_dat;
mod timeline;
mod update;
mod viewport;
mod welcome;

use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
pub use error::Error;
pub use game::{Found, Game, GameChoice, GameError, References};
use gpui::{App, AppContext, Bounds, KeyBinding, WindowBounds, WindowOptions, actions, px, size};
use library::Library;
pub(crate) use log::log;
use melee_dat::{FighterAttachOutcome, MeleeModelKind, MeleeReferenceCatalog, MeleeReferenceStore};

actions!(
    app,
    [
        /// Quit, asking about unsaved edits first.
        Quit,
        /// Show or hide the sidebar.
        ToggleSidebar,
        /// Open Settings over the screen.
        OpenSettings,
        /// Show what a problem report would send, to copy or send it.
        ReportProblem
    ]
);
pub use melee_dat::MeleeModel;
use shell::Shell;
pub use shell::{Costume, Start};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use tgg_ui::Theme;

/// `model`'s current pose, ready to draw, framed on its focus when it has
/// one.
pub(crate) fn geometry_of(model: &mut MeleeModel) -> Result<hsd_render::PreparedGeometry, Error> {
    let focus = model.focus();
    let (scene, work) = model.evaluate()?;
    Ok(hsd_render::PreparedGeometry::new(scene, work)?.with_focus(focus))
}

/// A costume ready to show.
pub struct Loaded {
    pub model: MeleeModel,
    pub title: String,
    /// The reference files the costume needs, when it is a stock costume
    /// the catalog knows; its texture names come from them too.
    pub store: Option<Rc<MeleeReferenceStore>>,
}

/// Parse a costume and attach its fighter's animations when `references`
/// supply its reference DATs; otherwise, or when attachment fails, keep the
/// bind pose.
pub fn load_model(
    name: &str,
    bytes: &[u8],
    references: Option<&References>,
) -> Result<Loaded, Error> {
    // Fighter envelope skinning needs the references the idle does.
    let policy = match references {
        Some(_) => HsdDrawEvaluationPolicy::MELEE_FIGHTER,
        None => HsdDrawEvaluationPolicy::GENERIC_HSD,
    };
    let mut model = MeleeModel::open(bytes, policy)?;
    let mut store = None;
    if let (MeleeModelKind::Static, Some(references)) = (model.kind(), references)
        && let Some(found) = references.store_for(model.scene())
    {
        let (attached, outcome) = model.attach_fighter(references.catalog, &found);
        if let FighterAttachOutcome::Failed(error) = outcome {
            log(&format!("idle unavailable, showing the bind pose: {error}"));
        }
        model = attached;
        store = Some(found);
    }
    let title = match model.fighter() {
        Some(playback) => format!("{name} · {}", playback.fighter_name()),
        None if model.kind() == MeleeModelKind::Stage => name.to_owned(),
        None => format!("{name} · bind pose"),
    };
    Ok(Loaded {
        model,
        title,
        store,
    })
}

pub struct Launch {
    pub start: Start,
    /// Kept so costumes opened later play their animations.
    pub references: Option<References>,
    pub exit_after: Option<Duration>,
}

/// The player's game: the `iso` given, else the one chosen before or found
/// through Slippi. `Ok(None)` when none is found; a given ISO that isn't
/// Melee NTSC 1.02 is an error in the player's words.
pub fn find_game(iso: Option<&Path>) -> Result<Option<(Game, Found)>, GameError> {
    match iso {
        Some(path) => Game::open(path).map(|game| Some((game, Found::Given))),
        None => Ok(Game::find(settings::load().iso_path.as_deref())),
    }
}

/// Remember `game` as the player's, so the next launch opens it directly,
/// and its folder, so other ISOs there are listed when they change games.
pub fn remember_game(game: &Game) {
    let mut settings = settings::load();
    let before = settings.clone();
    settings.iso_path = Some(game.path().to_owned());
    if let Some(folder) = game.path().parent()
        && !settings.game_folders.iter().any(|known| known == folder)
    {
        settings.game_folders.push(folder.to_owned());
    }
    if settings != before {
        settings::save(&settings);
    }
}

/// The game the player chose before, if it is still Melee NTSC 1.02.
pub fn saved_game() -> Option<Game> {
    let path = settings::load().iso_path?;
    Game::open(&path)
        .inspect_err(|error| log(&format!("not reopening {}: {error}", path.display())))
        .ok()
}

/// Every Melee NTSC 1.02 ISO in the player's game folders.
pub fn discover_games() -> Vec<GameChoice> {
    Game::discover(&settings::load().game_folders)
}

/// References read from the player's game, or the library in the app's
/// data folder where the game's were installed over.
pub fn game_references(game: Game) -> Result<References, Error> {
    Ok(References::new(
        MeleeReferenceCatalog::checked_in(),
        game,
        Library::default_root(),
    ))
}

/// Open the editor window and run the application.
pub fn run(launch: Launch) {
    let Launch {
        start,
        references,
        exit_after,
    } = launch;
    let launch = move |cx: &mut App| {
        Theme::init(settings::load().appearance(), cx);
        tgg_ui::init(cx);
        cx.bind_keys([
            KeyBinding::new(editor::keys::QUIT, Quit, None),
            KeyBinding::new(editor::keys::TOGGLE_SIDEBAR, ToggleSidebar, None),
            KeyBinding::new(editor::keys::SETTINGS, OpenSettings, None),
            KeyBinding::new(editor::keys::SAVE_AND_INSTALL, editor::SaveAndInstall, None),
            KeyBinding::new(editor::keys::OPEN, editor::OpenDat, None),
            KeyBinding::new(editor::keys::UNDO, editor::Undo, None),
            KeyBinding::new(editor::keys::REDO, editor::Redo, None),
            KeyBinding::new(editor::keys::EDIT_EXTERNALLY, editor::EditExternally, None),
            KeyBinding::new(
                editor::keys::PLAYBACK,
                editor::TogglePlayback,
                Some("Viewport"),
            ),
            KeyBinding::new(editor::keys::SAVE, editor::Save, None),
            KeyBinding::new(editor::keys::SAVE_AS, editor::SaveAs, None),
            KeyBinding::new(editor::keys::IMPORT, editor::ImportPng, None),
            KeyBinding::new(editor::keys::EXPORT, editor::ExportPng, None),
        ]);
        cx.set_menus(menus::menus());
        if let Err(error) = tgg_ui::fonts::load(cx) {
            log(&format!("failed to load fonts: {error:#}"));
        }
        cx.open_window(
            WindowOptions {
                app_id: Some("gg.textures.app".into()),
                titlebar: Some(tgg_ui::title_bar::options("textures.gg editor")),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1024.0), px(768.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                Theme::global(cx).apply(window);
                let shell = cx.new(|cx| {
                    let mut shell = Shell::new(start, references, window, cx);
                    if log::crashed_before() {
                        shell.report_problem(true, window, cx);
                    }
                    // A timed run is a measurement; it stays offline.
                    if exit_after.is_none() {
                        shell.check_for_update(cx);
                    }
                    shell
                });
                // Closing with unsaved edits asks first.
                let closing = shell.clone();
                window.on_window_should_close(cx, move |_, cx| {
                    closing.update(cx, |shell, cx| shell.should_close(cx))
                });
                shell
            },
        )
        .expect("open the editor window");
        // PNGs exported for external editing don't outlive the app.
        cx.on_app_quit(|_| {
            editor::external::remove_temporary_directory();
            async {}
        })
        .detach();
        cx.activate(true);
        if let Some(duration) = exit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(duration).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    };
    gpui_platform::application().run(launch);
}

/// Keep this session's diagnostics, and any panic, in the player's log file.
/// Call it first; until then diagnostics go to stderr alone.
pub fn start_log() {
    log::start();
}
