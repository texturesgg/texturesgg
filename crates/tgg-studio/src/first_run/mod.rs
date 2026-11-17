//! First run: the one screen shown until the player has a Melee disc and
//! tgg-melee installed. Four numbered steps (the disc, the game, the mods
//! to start with, signing in), one button, and nothing downloaded until it's
//! pressed. `actions` finds and chooses the disc, installs the game, makes
//! the picked sets and leaves for Play; `view` draws the screen.

pub mod actions;
pub mod view;

pub use actions::Done;

/// The sets first run offers, by link name, in the order they show.
pub const LINKS: [&str; 3] = ["competitive", "practice", "replays"];

/// How far setting up has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the player to press the button.
    #[default]
    Ready,
    /// tgg-melee downloading, as `app.mods.work` reports it.
    Downloading,
    /// Off the window's thread: making the picked sets.
    SettingUp,
    /// Everything is installed; Play is the one button.
    Done,
}

/// The sets the player starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picks {
    pub competitive: bool,
    pub practice: bool,
    pub replays: bool,
}

impl Default for Picks {
    fn default() -> Self {
        Self {
            competitive: true,
            practice: false,
            replays: false,
        }
    }
}

impl Picks {
    /// Whether `link` is picked.
    pub fn on(&self, link: &str) -> bool {
        match link {
            "competitive" => self.competitive,
            "practice" => self.practice,
            "replays" => self.replays,
            _ => false,
        }
    }

    /// Turn `link` on or off.
    pub fn set(&mut self, link: &str, on: bool) {
        match link {
            "competitive" => self.competitive = on,
            "practice" => self.practice = on,
            "replays" => self.replays = on,
            _ => {}
        }
    }

    /// Flip `link`.
    pub fn toggle(&mut self, link: &str) {
        self.set(link, !self.on(link));
    }

    /// The picked links, in the order they show.
    pub fn links(&self) -> Vec<String> {
        LINKS
            .into_iter()
            .filter(|link| self.on(link))
            .map(str::to_owned)
            .collect()
    }

    /// How many sets are picked.
    pub fn count(&self) -> usize {
        LINKS.into_iter().filter(|link| self.on(link)).count()
    }
}

/// First run's own state: the picks, how far setup got, and why it couldn't
/// go further, in the player's words.
#[derive(Default)]
pub struct FirstRun {
    pub picks: Picks,
    pub stage: Stage,
    pub problem: Option<String>,
}
