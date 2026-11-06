//! The app's whole state: read by the view, changed only by actions between
//! frames. Each feature owns its slice; this module holds the root, what's
//! shared, and how the app starts.

use crate::jobs::Jobs;
use crate::jobs::renders::Renders;
use crate::melee::Melee;
use crate::nav::Nav;
use crate::stage::Stage;
use crate::welcome::Welcome;
use tgg_app::References;
use tgg_app::settings::{self, Settings};

pub struct App {
    /// Preferences, as kept in `editor.json`.
    pub settings: Settings,
    /// Whether changes to settings are written back to disk; off in tests,
    /// so they never touch the player's.
    pub persist: bool,
    /// The player's game; `None` until they choose one, while Welcome shows.
    pub game: Option<References>,
    pub nav: Nav,
    pub welcome: Welcome,
    pub melee: Melee,
    /// The one model showing, for Melee and the editor.
    pub stage: Stage,
    /// Pictures of costumes and stages, shared by every place.
    pub renders: Renders,
    /// Work off the window's thread.
    pub jobs: Jobs,
}

/// Where the app starts.
pub enum Start {
    /// The player's game, found or given.
    Game(References),
    /// Welcome, with the games found in the player's folders and, when an
    /// ISO was given or saved but can't be used, why.
    Welcome(Welcome),
}

impl App {
    /// The app at `start`, with `settings` as read from disk (or the defaults,
    /// in tests). Nothing is read in the background until it runs.
    pub fn new(start: Start, settings: Settings, persist: bool) -> Self {
        let nav = Nav::from_settings(&settings);
        let (game, welcome) = match start {
            Start::Game(references) => (Some(references), Welcome::default()),
            Start::Welcome(welcome) => (None, welcome),
        };
        let melee = game.as_ref().map(|references| Melee::of(references.game())).unwrap_or_default();
        Self { settings, persist, game, nav, welcome, melee, stage: Stage::default(), renders: Renders::default(), jobs: Jobs::default() }
    }

    /// Change the settings here and, when persisting, on disk: read, change
    /// and write the file, so a change another window made isn't lost.
    pub fn change_settings(&mut self, change: impl Fn(&mut Settings)) {
        change(&mut self.settings);
        if self.persist {
            settings::update(change);
        }
    }
}
