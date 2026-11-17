//! The player's sets, and starting a game from one: the engine Play, Mods
//! and first run share. A set is a name and a list of mods and textures.gg
//! sets it builds on (`tgg_mod::sets`, core `docs/sets.md`); the active one
//! is what Play starts.
//!
//! Starting a game ([`How`]) works the same from every place: the set's
//! plan is worked out off the window's thread, native code from a
//! publisher the player hasn't trusted asks first ([`Trust`], a sheet),
//! then its mods are fetched into its own folder and the game starts there
//! with the environment `How` gives it. The built-in set, Installed mods,
//! runs from the mods folder as it always has, after `check_withdrawn`.
//! `actions` changes it; `view` draws the trust sheet.

pub mod actions;
pub mod view;

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tgg_mod::sets::{MySet, QueueMod, SetPlan};
use tgg_mod::{Prepared, Progress, ProfilePlan};

#[derive(Default)]
pub struct Sets {
    /// The player's sets, Installed mods first, then by name, as last read.
    pub list: Vec<MySet>,
    /// The active set's key.
    pub active: String,
    /// Each set's plan by key, once asked for.
    pub plans: HashMap<String, Planned>,
    /// A game being got ready: what's being done, and its progress.
    pub launching: Option<Launching>,
    /// Native code waiting on the player's yes before a game starts.
    pub trust: Option<Trust>,
    /// How the game that's running was started, while it runs.
    pub session: Option<Session>,
    /// Why the sets couldn't be read.
    pub problem: Option<String>,
    /// What the last check of withdrawals before Installed mods' game
    /// turned off or warned about, in the player's words.
    pub said: Vec<String>,
}

impl Sets {
    /// The active set, once the list is read.
    pub fn active(&self) -> Option<&MySet> {
        self.list.iter().find(|set| set.key == self.active)
    }

    /// The set `key`.
    pub fn get(&self, key: &str) -> Option<&MySet> {
        self.list.iter().find(|set| set.key == key)
    }

    /// The plan for `key`, once it's worked out.
    pub fn plan(&self, key: &str) -> Option<&SetPlan> {
        match self.plans.get(key)? {
            Planned::Ready(plan) => Some(plan),
            Planned::Working | Planned::Failed(_) => None,
        }
    }

    /// Whether a game is being got ready or waits on the player's trust.
    pub fn busy(&self) -> bool {
        self.launching.is_some() || self.trust.is_some()
    }
}

/// A set's plan: being worked out, worked out, or why it couldn't be.
#[derive(Clone)]
pub enum Planned {
    Working,
    Ready(Arc<SetPlan>),
    Failed(String),
}

/// How a game starts, and what the game's environment says to do first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum How {
    /// Its menus, offline.
    Offline,
    /// Find a match in VS Mode, in the queue the set's gameplay mods make.
    Search,
    /// Make a room for a friend to join.
    Host,
    /// Join the room with this code.
    Join(String),
    /// Search in a queue someone else runs, with its mods for this session.
    Queue {
        /// The queue's name, as the list gives it.
        label: String,
        mods: Vec<QueueMod>,
    },
}

impl How {
    /// What the game starts with in its environment (direct-connect's
    /// `TGG_DIRECT_CONNECT`).
    pub fn env(&self) -> Vec<(String, OsString)> {
        let connect = match self {
            How::Offline => return Vec::new(),
            How::Search | How::Queue { .. } => "search".to_owned(),
            How::Host => "room".to_owned(),
            How::Join(code) => format!("room:{}", code.trim()),
        };
        vec![("TGG_DIRECT_CONNECT".to_owned(), connect.into())]
    }
}

/// A game being got ready.
pub struct Launching {
    pub how: How,
    /// What's being done, for the player: "Getting Friday night's mods".
    pub doing: String,
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub cancel: Arc<AtomicBool>,
}

/// What a game starts from, once planned.
#[derive(Clone)]
pub enum Source {
    /// One of the player's sets (not Installed mods).
    Set { set: MySet, plan: Arc<SetPlan> },
    /// A queue's mods, for one session.
    Queue { plan: ProfilePlan },
}

impl Source {
    fn profile_plan(&self) -> &ProfilePlan {
        match self {
            Source::Set { plan, .. } => &plan.plan,
            Source::Queue { plan } => plan,
        }
    }
}

/// Native code from publishers the player hasn't trusted, before a game.
pub struct Trust {
    pub how: How,
    pub source: Source,
    /// Each publisher: its key (account id, else name), name, and the names
    /// of its mods the game needs.
    pub publishers: Vec<(String, String, Vec<String>)>,
    /// Remember the yes, so they aren't asked again.
    pub remember: bool,
}

/// The game running, and how it was started.
#[derive(Clone, Debug)]
pub struct Session {
    pub how: How,
    /// The set's name, or the queue's.
    pub name: String,
    pub started: Instant,
}

/// What a set job found.
pub enum Done {
    /// The sets as read, and the active one's key; or why not.
    Listed(Result<(Vec<MySet>, String), String>),
    /// `key`'s plan, or why it couldn't be worked out.
    Planned {
        key: String,
        result: Result<SetPlan, String>,
    },
    /// Planned before a game: start getting it ready, or ask for trust.
    ReadyToPrepare {
        how: How,
        result: Result<Source, String>,
    },
    /// Installed mods checked against textures.gg's withdrawals: start it.
    Checked {
        how: How,
        notices: Vec<tgg_mod::WithdrawNotice>,
    },
    /// The game's folder is ready: start it.
    Prepared {
        how: How,
        name: String,
        result: Result<Prepared, String>,
    },
    /// A set changed on disk (made, renamed, a line added): read them again.
    Changed(Result<String, String>),
}
