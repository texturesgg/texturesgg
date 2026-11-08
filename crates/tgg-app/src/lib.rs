//! What the textures.gg app does, with no UI: the player's game and what each
//! of its slots holds, the library of skins, installing them and undoing
//! installs, the settings kept between sessions, the log, and the update
//! check. tgg-studio draws it.
//!
//! Copied from tgg-editor, which keeps its own copies while both apps exist;
//! a change to one of these modules there is carried here by hand.

pub mod colors;
pub mod costumes;
pub mod disk;
pub mod error;
pub mod external;
pub mod game;
pub mod ids;
pub mod image_apps;
pub mod install;
pub mod library;
pub mod log;
pub mod moves;
pub mod net;
pub mod replays;
pub mod settings;
#[cfg(any(test, feature = "test-support"))]
pub mod test_dat;
pub mod textures;
pub mod update;

use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
pub use error::Error;
pub use game::{Found, Game, GameChoice, GameError, References};
use library::Library;
pub use log::log;
use melee_dat::{
    FighterAttachOutcome, MeleeModel, MeleeModelKind, MeleeReferenceCatalog, MeleeReferenceStore,
    MeleeSlot, SharedModel, parse_filename,
};
use std::path::Path;
use std::rc::Rc;

/// A costume ready to show.
pub struct Loaded {
    pub model: MeleeModel,
    pub title: String,
    /// The reference files the costume needs, when it is a stock costume
    /// the catalog knows; its texture names come from them too.
    pub store: Option<Rc<MeleeReferenceStore>>,
    /// The models its fighter's costumes share, with their files, to draw
    /// on it as its moves spawn them; none but for a stock costume.
    pub shared: Vec<(SharedModel, Rc<Vec<u8>>)>,
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
    // The shared models of whose costume it is: its root names say, else
    // its file name.
    let shared = references
        .filter(|_| model.fighter().is_some())
        .and_then(|references| {
            let roots = model
                .scene()
                .roots
                .iter()
                .filter_map(|root| root.name.as_deref());
            references
                .catalog
                .costume_slot(roots)
                .or_else(|| parse_filename(name))
                .and_then(MeleeSlot::character)
                .map(|character| references.shared_models(character))
        });
    Ok(Loaded {
        model,
        title,
        store,
        shared: shared.unwrap_or_default(),
    })
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
