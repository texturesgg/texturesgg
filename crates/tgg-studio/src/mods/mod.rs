//! Mods: tgg-melee (the Melee port with a mod loader built in) and the mods
//! textures.gg publishes for it. The page installs the port and Play starts
//! it with the player's unmodified disc; under that, three tabs: the curated
//! sets, every mod built for the port (searchable), and what's installed
//! with each mod's switch and update. Pressing a mod shows its details in a
//! pane beside the list.
//!
//! Everything comes from `tgg-mod`: the network only when the page opens or
//! the player asks (no polling), installs and launches on threads of their
//! own, and what's installed read from disk as it changes. `actions` changes
//! it; `view` draws the page, `details` the pane and `review` the dialog
//! shown before an install.

pub mod actions;
pub mod details;
pub mod review;
pub mod view;

use rune::TextEdit;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tgg_mod::{Applied, Catalog, CatalogEntry, Error, Local, ModId, Net, Plan, PortInstall, PortRelease, Progress, Set};

/// The name tgg-studio gives textures.gg when it asks.
const USER_AGENT: &str = concat!("tgg-studio/", env!("CARGO_PKG_VERSION"));

pub struct Mods {
    /// Where the registry and port releases come from: textures.gg, or a
    /// fake one in tests.
    pub net: Arc<dyn Net>,
    /// The tgg-melee versions installed; one is current.
    pub ports: Vec<PortInstall>,
    /// tgg-melee's releases for this system, newest first, as last fetched.
    pub releases: Vec<PortRelease>,
    /// The sets textures.gg offers, as last fetched.
    pub sets: Vec<Set>,
    /// What's built for the current port's layout (or, before one is
    /// installed, the newest release's), as last fetched.
    pub catalog: Option<Catalog>,
    /// What the mods folder holds, checked against the catalog.
    pub local: Local,
    pub registry: Registry,
    /// Why the registry has nothing for this port, when it answered so.
    pub no_mods: Option<String>,
    /// Why what's installed couldn't be read.
    pub problem: Option<String>,
    /// An install under way.
    pub work: Option<Work>,
    /// A plan shown before it's applied.
    pub review: Option<Review>,
    /// The disc tgg-melee plays.
    pub disc: Disc,
    /// The game, while it runs.
    pub playing: Option<Playing>,
    pub tab: Tab,
    /// What the player's looking for among every mod.
    pub search: TextEdit,
    /// The mod whose details show beside the list.
    pub selected: Option<ModId>,
}

/// Which list the page shows under the game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Sets,
    All,
    Installed,
}

impl Default for Mods {
    fn default() -> Self {
        Self {
            net: Arc::new(tgg_mod::HttpNet::new(USER_AGENT)),
            ports: Vec::new(),
            releases: Vec::new(),
            sets: Vec::new(),
            catalog: None,
            local: Local::empty(),
            registry: Registry::NotAsked,
            no_mods: None,
            problem: None,
            work: None,
            review: None,
            disc: Disc::Unknown,
            playing: None,
            tab: Tab::default(),
            search: TextEdit::default(),
            selected: None,
        }
    }
}

impl Mods {
    /// The newest release of each mod textures.gg has built, by name.
    pub fn offered(&self) -> Vec<&CatalogEntry> {
        let mut newest: Vec<&CatalogEntry> = Vec::new();
        for entry in self.catalog.iter().flat_map(|catalog| &catalog.mods) {
            match newest.iter_mut().find(|known| known.manifest.id == entry.manifest.id) {
                Some(known) if known.manifest.version < entry.manifest.version => *known = entry,
                Some(_) => {}
                None => newest.push(entry),
            }
        }
        newest.sort_by_key(|entry| entry.manifest.name.to_lowercase());
        newest
    }

    /// The mods offered that the search matches: its words, each in the
    /// name, id or description, without case.
    pub fn found(&self) -> Vec<&CatalogEntry> {
        let query = self.search.text.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        self.offered().into_iter().filter(|entry| {
            let manifest = &entry.manifest;
            let haystack = format!("{} {} {}", manifest.name, manifest.id, manifest.description.as_deref().unwrap_or_default()).to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        }).collect()
    }

    /// The newest release textures.gg has of the mod `id`.
    pub fn newest(&self, id: &ModId) -> Option<&CatalogEntry> {
        self.offered().into_iter().find(|entry| entry.manifest.id == *id)
    }

    /// The sets that list the mod `id`, by title.
    pub fn sets_with(&self, id: &ModId) -> Vec<String> {
        self.sets.iter().filter(|set| set.mods.iter().any(|m| m.id == *id)).map(|set| set.title.clone()).collect()
    }

    /// A mod's name, from what's installed or the catalog, else its id.
    pub fn name_of(&self, id: &ModId) -> String {
        self.local.get(id).map(|m| m.manifest.name.clone())
            .or_else(|| self.newest(id).map(|entry| entry.manifest.name.clone()))
            .unwrap_or_else(|| id.to_string())
    }

    /// The tgg-melee version in use.
    pub fn port(&self) -> Option<&PortInstall> {
        self.ports.iter().find(|port| port.current)
    }

    /// A newer tgg-melee than every installed one, when there is one.
    pub fn port_update(&self) -> Option<PortRelease> {
        if self.ports.is_empty() {
            return None;
        }
        tgg_mod::port_update(&self.ports, &self.releases)
    }

    /// Whether anything installed has a newer version.
    pub fn has_updates(&self) -> bool {
        self.local.mods.iter().any(|m| m.update.is_some()) || self.local.sets.iter().any(|set| set.update.is_some())
    }

    /// A set's title, by its name, from the registry or the lock.
    pub fn set_title(&self, name: &str) -> String {
        self.sets.iter().chain(self.local.lock.sets.iter()).find(|set| set.name == name)
            .map_or_else(|| name.to_owned(), |set| set.title.clone())
    }
}

/// Whether textures.gg has been asked what it offers, and how it answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registry {
    NotAsked,
    Asking,
    Answered,
    /// Nothing answered; what's installed still works.
    Offline,
    /// It answered, but not with what was asked: why.
    Failed(String),
}

/// An install under way: what it is, how far it's got, and its cancel flag.
pub struct Work {
    pub doing: String,
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub cancel: Arc<AtomicBool>,
}

impl Work {
    /// How far along, from 0 to 1: whole steps, and the current step's bytes.
    pub fn fraction(&self) -> f32 {
        let Some(progress) = self.progress.lock().ok().and_then(|progress| progress.clone()) else { return 0.0 };
        let within = progress.total.filter(|total| *total > 0).map_or(0.0, |total| progress.bytes as f32 / total as f32);
        ((progress.step as f32 + within.min(1.0)) / progress.steps.max(1) as f32).clamp(0.0, 1.0)
    }

    /// What the current step does, once one has started.
    pub fn label(&self) -> String {
        self.progress.lock().ok().and_then(|progress| progress.as_ref().map(|progress| progress.label.clone()))
            .unwrap_or_else(|| self.doing.clone())
    }
}

/// A plan, before the player says to apply it.
pub struct Review {
    pub title: String,
    pub plan: Plan,
}

/// The disc tgg-melee plays: an unmodified one, since netplay and replays
/// want the same game files on every side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disc {
    /// Not looked for yet.
    Unknown,
    Looking,
    /// The player's, found or chosen.
    Found(PathBuf),
    /// None of the player's discs is unmodified.
    Missing,
}

/// The game, running: the flag that stops it.
pub struct Playing {
    pub stop: Arc<AtomicBool>,
}

/// What a Mods job found.
pub enum Done {
    /// What textures.gg offers: tgg-melee's releases, the sets, and the
    /// catalog for the current port, when there is one.
    Fetched {
        releases: Result<Vec<PortRelease>, Error>,
        sets: Result<Vec<Set>, Error>,
        catalog: Option<Result<Catalog, Error>>,
    },
    PortInstalled(Result<PortInstall, Error>),
    Applied(Result<Applied, Error>),
    /// An unmodified disc of the player's, if one was found.
    Disc(Option<PathBuf>),
    /// The game ended: why, when it didn't end well.
    GameEnded(Result<(), String>),
}

/// `bytes` as a player reads a size: "840 KB", "12.4 MB".
pub fn size_text(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    let bytes = bytes as f64;
    if bytes < KB * KB {
        format!("{:.0} KB", (bytes / KB).max(1.0))
    } else if bytes < KB * KB * KB {
        format!("{:.1} MB", bytes / KB / KB)
    } else {
        format!("{:.2} GB", bytes / KB / KB / KB)
    }
}

#[cfg(test)]
mod tests {
    use super::size_text;

    #[test]
    fn sizes_read_as_a_player_reads_them() {
        assert_eq!(size_text(12), "1 KB");
        assert_eq!(size_text(840_000), "840 KB");
        assert_eq!(size_text(12_400_000), "12.4 MB");
        assert_eq!(size_text(2_500_000_000), "2.50 GB");
    }
}
