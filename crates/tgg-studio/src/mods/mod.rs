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
pub mod versions;
pub mod view;

use rune::TextEdit;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tgg_mod::{
    Applied, Catalog, CatalogEntry, Error, Local, ModDetails, ModId, ModSummary, Net, Plan,
    PortInstall, PortRelease, Progress, Set,
};

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
    /// An older tgg-melee every installed mod is built for, when some
    /// aren't built for the current one.
    pub fallback: Option<Fallback>,
    /// The mods built for the newer tgg-melee on offer, when its layout
    /// differs from the current one's, as last fetched.
    pub next_built: Option<Vec<ModId>>,
    /// When nothing is built for the current tgg-melee: the newest older one
    /// installed that has mods, and how many.
    pub older_with_mods: Option<(String, usize)>,
    /// The versions of tgg-melee show, in a dialog.
    pub versions: bool,
    /// What the registry says about each mod it lists: author, icon, tags,
    /// installs; empty until it answers (or when it can't).
    pub listings: HashMap<ModId, ModSummary>,
    /// Each mod's registry page, once asked for: `None` while it's on its way.
    pub details: HashMap<ModId, Option<ModDetails>>,
    /// Icons and screenshots by URL, filled as they arrive.
    pub images: HashMap<String, rune::Pixels>,
    /// The parts of the chosen mod's details shown in full.
    pub expanded: Vec<Expand>,
}

/// An older tgg-melee to go back to: its version, and how many installed
/// mods aren't built for the current one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fallback {
    pub version: String,
    pub unfit: usize,
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
            fallback: None,
            next_built: None,
            older_with_mods: None,
            versions: false,
            listings: HashMap::new(),
            details: HashMap::new(),
            images: HashMap::new(),
            expanded: Vec::new(),
        }
    }
}

impl Mods {
    /// The newest release of each mod textures.gg has built, by name.
    pub fn offered(&self) -> Vec<&CatalogEntry> {
        let mut newest: Vec<&CatalogEntry> = Vec::new();
        for entry in self.catalog.iter().flat_map(|catalog| &catalog.mods) {
            match newest
                .iter_mut()
                .find(|known| known.manifest.id == entry.manifest.id)
            {
                Some(known) if known.manifest.version < entry.manifest.version => *known = entry,
                Some(_) => {}
                None => newest.push(entry),
            }
        }
        // Official mods first, then by name.
        newest.sort_by_key(|entry| {
            (
                !self.official(&entry.manifest.id),
                entry.manifest.name.to_lowercase(),
            )
        });
        newest
    }

    /// The mods offered that the search matches.
    pub fn found(&self) -> Vec<&CatalogEntry> {
        self.offered()
            .into_iter()
            .filter(|entry| {
                let manifest = &entry.manifest;
                self.matches(
                    &manifest.id,
                    &manifest.name,
                    manifest.description.as_deref(),
                )
            })
            .collect()
    }

    /// The mods the registry lists that aren't built for this tgg-melee yet,
    /// by name, that the search matches.
    pub fn found_unbuilt(&self) -> Vec<&ModSummary> {
        let offered = self.offered();
        let mut unbuilt: Vec<&ModSummary> = self
            .listings
            .values()
            .filter(|summary| !offered.iter().any(|entry| entry.manifest.id == summary.id))
            .filter(|summary| {
                self.matches(&summary.id, &summary.name, summary.description.as_deref())
            })
            .collect();
        unbuilt.sort_by_key(|summary| (!summary.publisher.official, summary.name.to_lowercase()));
        unbuilt
    }

    /// Whether the search matches a mod: each of its words in the mod's
    /// name, id, description, author or tags, without case.
    fn matches(&self, id: &ModId, name: &str, description: Option<&str>) -> bool {
        let query = self.search.text.to_lowercase();
        let listed = self.listings.get(id);
        let author = listed
            .map(|summary| format!("{} {}", summary.publisher.name, summary.author))
            .unwrap_or_default();
        let tags = listed
            .map(|summary| summary.tags.join(" "))
            .unwrap_or_default();
        let haystack = format!(
            "{name} {id} {} {author} {tags}",
            description.unwrap_or_default()
        )
        .to_lowercase();
        query.split_whitespace().all(|word| haystack.contains(word))
    }

    /// Whether textures.gg vouches for the mod `id`: its own, or one its
    /// admins call official.
    pub fn official(&self, id: &ModId) -> bool {
        self.listings
            .get(id)
            .is_some_and(|summary| summary.publisher.official)
    }

    /// The icon of the mod `id`, once it's arrived.
    pub fn icon(&self, id: &ModId) -> Option<&rune::Pixels> {
        let url = &self.listings.get(id)?.icon.as_ref()?.url;
        self.images.get(url)
    }

    /// The newest release textures.gg has of the mod `id`.
    pub fn newest(&self, id: &ModId) -> Option<&CatalogEntry> {
        self.offered()
            .into_iter()
            .find(|entry| entry.manifest.id == *id)
    }

    /// The sets that list the mod `id`, by title.
    pub fn sets_with(&self, id: &ModId) -> Vec<String> {
        self.sets
            .iter()
            .filter(|set| set.mods.iter().any(|m| m.id == *id))
            .map(|set| set.title.clone())
            .collect()
    }

    /// The installed mods the newer tgg-melee on offer has no build of yet,
    /// by name.
    pub fn unbuilt_for_update(&self) -> Vec<String> {
        let Some(built) = &self.next_built else {
            return Vec::new();
        };
        self.local
            .mods
            .iter()
            .filter(|m| !built.contains(m.id()))
            .map(|m| m.manifest.name.clone())
            .collect()
    }

    /// A mod's name, from what's installed or the catalog, else its id.
    pub fn name_of(&self, id: &ModId) -> String {
        self.local
            .get(id)
            .map(|m| m.manifest.name.clone())
            .or_else(|| self.newest(id).map(|entry| entry.manifest.name.clone()))
            .unwrap_or_else(|| readable(id))
    }

    /// The tgg-melee version in use.
    pub fn port(&self) -> Option<&PortInstall> {
        self.ports.iter().find(|port| port.current)
    }

    /// The newest tgg-melee newer than the one in use, when there is one: a
    /// release textures.gg offers, or one already installed (the player went
    /// back to an older version). Updating to an installed one switches.
    pub fn port_update(&self) -> Option<PortRelease> {
        let current = self.port()?;
        let installed: Vec<PortRelease> =
            self.ports.iter().map(|port| port.release.clone()).collect();
        let known: Vec<PortRelease> = self
            .releases
            .iter()
            .cloned()
            .chain(installed.into_iter().filter(|release| {
                !self
                    .releases
                    .iter()
                    .any(|known| known.version == release.version)
            }))
            .collect();
        tgg_mod::port_update(std::slice::from_ref(current), &known)
    }

    /// Whether tgg-melee `version` is installed already.
    pub fn has_port(&self, version: &str) -> bool {
        self.ports.iter().any(|port| port.version == version)
    }

    /// Whether anything installed has a newer version.
    pub fn has_updates(&self) -> bool {
        self.local.mods.iter().any(|m| m.update.is_some())
            || self.local.sets.iter().any(|set| set.update.is_some())
    }

    /// A set's title, by its name, from the registry or the lock.
    pub fn set_title(&self, name: &str) -> String {
        self.sets
            .iter()
            .chain(self.local.lock.sets.iter())
            .find(|set| set.name == name)
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
        let Some(progress) = self
            .progress
            .lock()
            .ok()
            .and_then(|progress| progress.clone())
        else {
            return 0.0;
        };
        let within = progress
            .total
            .filter(|total| *total > 0)
            .map_or(0.0, |total| progress.bytes as f32 / total as f32);
        ((progress.step as f32 + within.min(1.0)) / progress.steps.max(1) as f32).clamp(0.0, 1.0)
    }

    /// What the current step does, once one has started.
    pub fn label(&self) -> String {
        self.progress
            .lock()
            .ok()
            .and_then(|progress| progress.as_ref().map(|progress| progress.label.clone()))
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
        /// What's built for the newer tgg-melee on offer, when its layout
        /// differs.
        next: Option<Result<Catalog, Error>>,
        /// When nothing is built for the current tgg-melee: an older
        /// installed one that has mods, and how many.
        older: Option<(String, usize)>,
        /// Every mod the registry lists, with its author, icon and tags.
        listings: Result<Vec<ModSummary>, Error>,
    },
    PortInstalled(Result<PortInstall, Error>),
    Applied(Result<Applied, Error>),
    /// An unmodified disc of the player's, if one was found.
    Disc(Option<PathBuf>),
    /// The game ended: why, when it didn't end well.
    GameEnded(Result<(), String>),
    /// The registry's page for a mod.
    Details(ModId, Result<ModDetails, Error>),
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

/// A name for the mod `id` when nothing textures.gg sent names it: its last
/// part, dashes as spaces, capitalized ("tgg.neutral-spawns" reads
/// "Neutral spawns").
pub fn readable(id: &ModId) -> String {
    let id = id.to_string();
    let last = id
        .rsplit('.')
        .next()
        .unwrap_or(&id)
        .replace(['-', '_'], " ");
    let mut characters = last.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

#[cfg(test)]
mod readable_tests {
    #[test]
    fn an_id_reads_as_a_name() {
        let id = "tgg.neutral-spawns".parse().expect("an id");
        assert_eq!(super::readable(&id), "Neutral spawns");
    }
}

/// A count as a player reads it: "940", "1.2k", "34k", "1.5M".
pub fn count_text(count: u64) -> String {
    match count {
        0..1_000 => count.to_string(),
        1_000..10_000 => format!("{:.1}k", count as f64 / 1e3),
        10_000..1_000_000 => format!("{}k", count / 1_000),
        _ => format!("{:.1}M", count as f64 / 1e6),
    }
}

#[cfg(test)]
mod count_tests {
    #[test]
    fn counts_read_short() {
        assert_eq!(super::count_text(940), "940");
        assert_eq!(super::count_text(1_234), "1.2k");
        assert_eq!(super::count_text(34_500), "34k");
        assert_eq!(super::count_text(1_500_000), "1.5M");
    }
}

/// A part of a mod's details that shows only its start until the player
/// asks for the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expand {
    Readme,
    Releases,
}
