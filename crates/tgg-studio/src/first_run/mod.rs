//! First run: the one screen shown once on a fresh install until the player
//! has a Melee disc, tgg-melee installed, and their starting set chosen. Four
//! numbered steps (the disc, the game, the mods to start with, signing in),
//! one button, and nothing downloaded until it's pressed. `actions` finds and
//! chooses the disc, installs or updates the game, sets up the starting set
//! and leaves for Play; `view` draws the screen.

pub mod actions;
pub mod view;

pub use actions::Done;

/// The single default set first run recommends, by link name.
pub const DEFAULT_SET: &str = "competitive";

/// How far setting up has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the player to press the button.
    #[default]
    Ready,
    /// tgg-melee downloading, as `app.mods.work` reports it.
    Downloading,
    /// Off the window's thread: setting up the starting set.
    SettingUp,
    /// Everything is set up; Play is the one button.
    Done,
}

/// First run's own state: whether to start with the default set, how far
/// setup got, and why it couldn't go further, in the player's words.
pub struct FirstRun {
    pub use_default_set: bool,
    pub stage: Stage,
    pub problem: Option<String>,
}

impl Default for FirstRun {
    fn default() -> Self {
        Self {
            use_default_set: true,
            stage: Stage::Ready,
            problem: None,
        }
    }
}
