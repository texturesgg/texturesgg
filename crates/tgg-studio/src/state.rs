//! The app's whole state: read by the view, changed only by actions between
//! frames. Each feature owns its slice; this module holds the root, what's
//! shared, and how the app starts.

use crate::editor::Document;
use crate::jobs::Jobs;
use crate::jobs::renders::Renders;
use crate::library::Page as LibraryPage;
use crate::melee::Melee;
use crate::mods::Mods;
use crate::nav::Nav;
use crate::preferences::Preferences;
use crate::replays::Replays;
use crate::report::Report;
use crate::signin::SignIn;
use crate::stage::Stage;
use crate::welcome::Welcome;
use std::path::PathBuf;
use tgg_app::References;
use tgg_app::library::Library;
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
    /// The player's skins, kept by content hash: shared by every place.
    pub library: Library,
    pub library_page: LibraryPage,
    /// What just happened, said once in the title row.
    pub notice: Option<Notice>,
    /// Where the library and install history are kept.
    pub data: Data,
    /// The one model showing, for Melee and the editor.
    pub stage: Stage,
    /// A costume open for editing, on the stage.
    pub editor: Option<Document>,
    /// Pictures of costumes and stages, shared by every place.
    pub renders: Renders,
    /// Settings, in their dialog.
    pub preferences: Preferences,
    /// A report of a problem, open in its dialog.
    pub report: Option<Report>,
    /// A newer version of the app, when the launch check found one.
    pub update: Option<String>,
    /// Who's signed in to textures.gg, and signing in.
    pub signin: SignIn,
    /// Starting a code mod on this computer.
    pub new_mod: crate::newmod::NewMod,
    /// The player's replays, and playing them with their mods.
    pub replays: Replays,
    /// What You has read from textures.gg.
    pub profile: crate::profile::Profile,
    /// How wide the window is, as last laid out. The place is that less the
    /// sidebar; measured on the window, not the place, because a details
    /// pane sliding in narrows the place and would keep itself there.
    pub window_width: f32,
    /// How tall the window is, as last laid out: toasts sit at its foot.
    pub window_height: f32,
    /// Slippi's Dolphin, which plays the player's ISO.
    pub slippi: crate::slippi::Slippi,
    /// tgg-melee and its mods.
    pub mods: Mods,
    /// Work off the window's thread.
    pub jobs: Jobs,
    /// What tgg-melee had in a slot before the last skin change there, for
    /// the toast's Undo.
    pub port_before: Option<crate::library::port::Before>,
    /// A textures.gg link waiting to be opened (see `links`).
    pub link: Option<crate::links::Link>,
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
    /// The app at `start`, with `settings` as read from disk and its `data`
    /// where the player keeps it (or the defaults and a temporary folder, in
    /// tests). Nothing is read in the background until it runs.
    pub fn new(start: Start, settings: Settings, data: Data, persist: bool) -> Self {
        let nav = Nav::from_settings(&settings);
        let (game, welcome) = match start {
            Start::Game(references) => (Some(references), Welcome::default()),
            Start::Welcome(welcome) => (None, welcome),
        };
        // Where the sign-in is kept when the CLI's place can't be used (tests).
        let credentials = data.library.with_file_name("credentials.json");
        let melee = game
            .as_ref()
            .map(|references| Melee::of(references.game()))
            .unwrap_or_default();
        Self {
            settings,
            persist,
            game,
            nav,
            welcome,
            melee,
            library: Library::open_at(data.library.clone()),
            library_page: LibraryPage::default(),
            notice: None,
            data,
            stage: Stage::default(),
            editor: None,
            renders: Renders::default(),
            mods: Mods::default(),
            slippi: crate::slippi::Slippi::default(),
            preferences: Preferences::default(),
            report: None,
            update: None,
            signin: SignIn::new(persist, credentials),
            new_mod: crate::newmod::NewMod::default(),
            replays: Replays::default(),
            profile: Default::default(),
            window_width: 0.0,
            window_height: 0.0,
            jobs: Jobs::default(),
            link: None,
            port_before: None,
        }
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

/// Where the player's library, install history, mods and ports are kept.
#[derive(Clone, Debug)]
pub struct Data {
    pub library: PathBuf,
    pub history: PathBuf,
    /// The folder tgg-melee loads mods from.
    pub mods: PathBuf,
    /// The folder tgg-melee's versions are installed in.
    pub ports: PathBuf,
    /// tgg's own data folder: the package cache, the cached catalog and
    /// Slippi mapping, profile folders, and trust answers.
    pub tgg: PathBuf,
    /// skins.json, the skins tgg-melee plays (tgg.skins reads it).
    pub skins: PathBuf,
}

impl Data {
    /// The app's data folder, where tgg-editor keeps them too, and the
    /// folders the game and the `tgg` CLI use for mods and ports.
    pub fn of_player() -> Self {
        let root = tgg_app::library::Library::default_root();
        let history = root
            .parent()
            .map_or_else(|| root.join("games"), |data| data.join("games"));
        Self {
            library: root,
            history,
            mods: tgg_mod::default_mods_dir(),
            ports: tgg_mod::default_ports_dir(),
            skins: tgg_mod::skins_file(),
            tgg: tgg_mod::default_data_dir(),
        }
    }

    /// Everything in `folder`; for tests.
    #[cfg(test)]
    pub fn in_folder(folder: &std::path::Path) -> Self {
        Self {
            library: folder.join("library"),
            history: folder.join("games"),
            mods: folder.join("mods"),
            // Where tgg-mod looks for them in `tgg`.
            ports: folder.join("tgg-data").join("tgg").join("ports"),
            tgg: folder.join("tgg-data"),
            skins: folder.join("skins.json"),
        }
    }

    /// The install history kept here.
    pub fn history(&self) -> tgg_app::install::History {
        tgg_app::install::History::open_at(self.history.clone())
    }
}

/// What just happened, in a toast at the place's foot: an install, a skin
/// added, why something couldn't be done. It goes by itself after a few
/// seconds; one saying why something failed stays until it's dismissed or
/// replaced.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    /// Which toast this is, so the timer of an earlier one leaves it be.
    pub id: u64,
    pub text: String,
    pub error: bool,
    /// What it offers to do about it.
    pub action: Option<NoticeAction>,
}

/// What a toast offers to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeAction {
    /// Put back the slot of the Slippi ISO an install just changed.
    Undo(melee_dat::MeleeSlot),
    /// Put back what tgg-melee had before the last skin change.
    UndoPort,
    /// Reopen the unsaved edits an earlier session left of a slot, or
    /// discard them. It stays until answered or dismissed.
    Reopen(melee_dat::MeleeSlot),
}

/// How long a toast shows, in seconds.
const NOTICE_SECONDS: u64 = 5;

impl App {
    /// Say `text` in a toast.
    pub fn tell(&mut self, text: impl Into<String>, error: bool) {
        self.show_notice(text.into(), error, None);
    }

    /// Say `text` in a toast offering to undo the install into `slot`.
    pub fn tell_undoable(&mut self, text: impl Into<String>, slot: melee_dat::MeleeSlot) {
        self.tell_with(text, NoticeAction::Undo(slot));
    }

    /// Say `text` in a toast offering `action`.
    pub fn tell_with(&mut self, text: impl Into<String>, action: NoticeAction) {
        self.show_notice(text.into(), false, Some(action));
    }

    fn show_notice(&mut self, text: String, error: bool, action: Option<NoticeAction>) {
        let id = self.notice.as_ref().map_or(1, |notice| notice.id + 1);
        self.notice = Some(Notice {
            id,
            text,
            error,
            action,
        });
        let lasts = error || matches!(action, Some(NoticeAction::Reopen(_)));
        // Recordings run work inline, where waiting would stall the window.
        if !lasts && !self.jobs.inline {
            self.jobs.spawn("notice", move || {
                std::thread::sleep(std::time::Duration::from_secs(NOTICE_SECONDS));
                crate::jobs::Done::NoticeExpired(id)
            });
        }
    }

    /// The toast `id`'s time is up: it goes, unless another replaced it.
    pub fn notice_expired(&mut self, id: u64) -> bool {
        let current = self.notice.as_ref().is_some_and(|notice| notice.id == id);
        if current {
            self.notice = None;
        }
        current
    }
}

impl App {
    /// Whether the place is wide enough for a list and its details side by
    /// side; narrower, the details slide in over the list.
    pub fn side_by_side(&self) -> bool {
        use crate::ui::tokens::size;
        let sidebar = if self.nav.sidebar {
            size::SIDEBAR + size::LINE
        } else {
            0.0
        };
        self.window_width - sidebar >= size::SIDE_BY_SIDE
    }
}
