//! Mods: the player's sets across the top, every mod textures.gg publishes
//! for the port below, and the set showing on the right with its mods in
//! groups. Pressing a mod's row shows its details where the set panel was;
//! Add puts the mod in the set showing, or in the mods folder when that's
//! Installed mods.
//!
//! A set is the sets library's (`tgg_mod::sets`, core `docs/sets.md`):
//! textures.gg's own are locked and built on, the player's can be changed.
//! Every change to one runs on a job of its own and comes back with the
//! list as it is now, so the cards and the panel always agree with disk.
//! `actions` changes it; `view` draws the page, `panel` the set panel,
//! `sheet` the New set and Rename dialogs, `details` a mod's, and `review`
//! the dialog a native mod's install asks with.

pub mod actions;
pub mod details;
pub mod panel;
pub mod review;
pub mod sheet;
pub mod versions;
pub mod view;

use crate::state::App;
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
    /// The set the page shows on the right; the active one until another is
    /// picked.
    pub chosen_set: Option<String>,
    /// Which of the mods across the page's list show.
    pub filter: Filter,
    /// The order the list shows them in.
    pub sort: Sort,
    /// The menu open on the page, if any.
    pub menu: Option<Menu>,
    /// The New set sheet, while it's open.
    pub new_set: Option<NewSet>,
    /// The Rename sheet's name, while it's open.
    pub rename: Option<TextEdit>,
    /// Whether the "Build on another set" sheet is open.
    pub building_on: bool,
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
    /// The skins tgg-melee plays, as skins.json says.
    pub skins: tgg_mod::Skins,
    /// What the game makes of each skin in its slot, once asked; `None`
    /// while it's being asked or when this tgg-melee can't say.
    pub verdicts: HashMap<(tgg_app::ids::SkinId, melee_dat::MeleeSlot), Option<tgg_mod::Verdict>>,
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

/// What the player's looking for across the official sets: the sets
/// textures.gg publishes that a new set can build on (one level, so only
/// ones that list mods).
#[derive(Default)]
pub struct NewSet {
    /// What it's called.
    pub name: TextEdit,
    /// The textures.gg sets it builds on, by link name.
    pub builds_on: Vec<String>,
}

impl NewSet {
    /// The sheet opened afresh: Online play built on, when textures.gg has
    /// it.
    pub fn opened(sets: &[tgg_mod::Set]) -> Self {
        let online = sets
            .iter()
            .any(|set| set.name == tgg_mod::sets::ONLINE_PLAY);
        Self {
            name: TextEdit::default(),
            builds_on: if online {
                vec![tgg_mod::sets::ONLINE_PLAY.to_owned()]
            } else {
                Vec::new()
            },
        }
    }

    /// Whether it can be made as filled in.
    pub fn named(&self) -> bool {
        !self.name.text.trim().is_empty()
    }
}

/// The order the page lists mods in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    /// Most installed first.
    #[default]
    Popular,
    /// Newest release first.
    Newest,
    Name,
}

impl Sort {
    /// Every order, as its menu lists them.
    pub const ALL: [Sort; 3] = [Sort::Popular, Sort::Newest, Sort::Name];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Popular => "Most installed",
            Sort::Newest => "Newest",
            Sort::Name => "Name",
        }
    }
}

/// A menu the Mods page opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    /// Which set the panel shows, and New set.
    Sets,
    /// What else can be done to the set showing.
    More,
    /// The list's order.
    Sort,
}

/// Which of the mods the page lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Rules,
    Training,
    OnlineModes,
    OnScreen,
}

impl Filter {
    /// Every filter, in the order the chips show.
    pub const ALL: [Filter; 5] = [
        Filter::All,
        Filter::Rules,
        Filter::Training,
        Filter::OnlineModes,
        Filter::OnScreen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Rules => "Rules",
            Filter::Training => "Training",
            Filter::OnlineModes => "Online modes",
            Filter::OnScreen => "On screen",
        }
    }

    /// Whether a mod tagged `tags` shows under it. The registry's tags are
    /// the only thing each mod says about itself that these can read:
    /// `competitive` changes how a match plays, `training` drills it,
    /// `netplay` is what online play is built on, and `visual` is what only
    /// your screen shows.
    pub fn shows(self, tags: &[String]) -> bool {
        let has = |tag: &str| tags.iter().any(|have| have == tag);
        match self {
            Filter::All => true,
            Filter::Rules => has("competitive"),
            Filter::Training => has("training"),
            Filter::OnlineModes => has("netplay"),
            Filter::OnScreen => has("visual"),
        }
    }
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
            chosen_set: None,
            filter: Filter::default(),
            sort: Sort::default(),
            menu: None,
            new_set: None,
            rename: None,
            building_on: false,
            search: TextEdit::default(),
            selected: None,
            fallback: None,
            next_built: None,
            older_with_mods: None,
            versions: false,
            listings: HashMap::new(),
            details: HashMap::new(),
            skins: tgg_mod::Skins::default(),
            verdicts: HashMap::new(),
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

    /// The mods offered that the search matches, and the filter shows, in
    /// the order picked.
    pub fn found(&self) -> Vec<&CatalogEntry> {
        let mut found: Vec<&CatalogEntry> = self
            .offered()
            .into_iter()
            .filter(|entry| {
                let manifest = &entry.manifest;
                self.matches(
                    &manifest.id,
                    &manifest.name,
                    manifest.description.as_deref(),
                )
            })
            .filter(|entry| self.filter.shows(self.tags(&entry.manifest.id)))
            .collect();
        self.order(&mut found, |entry| &entry.manifest.id);
        found
    }

    /// How many mods textures.gg lists, built for this tgg-melee or not.
    pub fn total(&self) -> usize {
        let offered = self.offered();
        let unbuilt = self
            .listings
            .keys()
            .filter(|id| !offered.iter().any(|entry| entry.manifest.id == **id))
            .count();
        offered.len() + unbuilt
    }

    /// Puts `items` in the order picked. The sort is stable, so ties keep
    /// the order they came in (official first, then by name).
    fn order<T>(&self, items: &mut [T], id: impl Fn(&T) -> &ModId) {
        let listed = |item: &T| self.listings.get(id(item));
        match self.sort {
            Sort::Popular => items.sort_by_key(|item| {
                std::cmp::Reverse(listed(item).map_or(0, |summary| summary.installs))
            }),
            Sort::Newest => items.sort_by(|a, b| {
                let when = |item| listed(item).map_or("", |summary| summary.updated_at.as_str());
                when(b).cmp(when(a))
            }),
            Sort::Name => items.sort_by_key(|item| self.name_of(id(item)).to_lowercase()),
        }
    }

    /// The tags the registry gives the mod `id`: its release's, else its
    /// listing's; none when it has none to give.
    pub fn tags(&self, id: &ModId) -> &[String] {
        let released = self
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.mods.iter().find(|entry| entry.manifest.id == *id))
            .map(|entry| entry.manifest.tags.as_slice())
            .filter(|tags| !tags.is_empty());
        released.unwrap_or_else(|| {
            self.listings
                .get(id)
                .map_or(&[], |summary| summary.tags.as_slice())
        })
    }

    /// The mods the registry lists that aren't built for this tgg-melee yet,
    /// that the search matches, in the order picked.
    pub fn found_unbuilt(&self) -> Vec<&ModSummary> {
        let offered = self.offered();
        let mut unbuilt: Vec<&ModSummary> = self
            .listings
            .values()
            .filter(|summary| !offered.iter().any(|entry| entry.manifest.id == summary.id))
            .filter(|summary| {
                self.matches(&summary.id, &summary.name, summary.description.as_deref())
            })
            .filter(|summary| self.filter.shows(&summary.tags))
            .collect();
        // Official first, then by name, under the order picked.
        unbuilt.sort_by_key(|summary| (!summary.publisher.official, summary.name.to_lowercase()));
        self.order(&mut unbuilt, |summary| &summary.id);
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
    /// The one mod it updates, when that's all it does: its progress shows
    /// in that mod's row, not above the page.
    pub target: Option<ModId>,
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

/// The disc tgg-melee plays. Any copy of Melee NTSC 1.02 does: the port
/// plays the game as it shipped from it, skins and all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disc {
    /// Not looked for yet.
    Unknown,
    Looking,
    /// The player's, found or chosen.
    Found(PathBuf),
    /// The player has no Melee NTSC 1.02 disc it found.
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
    /// The disc of the player's it found, if any.
    Disc(Option<PathBuf>),
    /// The game ended: why, when it didn't end well.
    GameEnded(Result<(), String>),
    /// The registry's page for a mod.
    Details(ModId, Result<ModDetails, Error>),
    /// A set changed on disk, or why it couldn't: the sets as they are now,
    /// what to say, and which set to show.
    SetChanged(Box<Reload>),
}

/// A set change that ran on a job.
pub struct Reload {
    /// The sets as they are now, and the active one's key; or why they
    /// couldn't be read.
    pub list: Result<(Vec<tgg_mod::sets::MySet>, String), String>,
    /// What the change says, and the set to show after it; or why it
    /// failed.
    pub result: Result<(Option<String>, Option<String>), String>,
}

/// The set the page shows: the one the player pressed, else the active one
/// when it has mods.
pub fn current_set(app: &App) -> Option<&tgg_mod::sets::MySet> {
    let chosen = app
        .mods
        .chosen_set
        .as_deref()
        .and_then(|key| app.sets.get(key));
    chosen.or_else(|| {
        app.sets
            .active()
            .filter(|set| !set.is_installed() || !set.lines.is_empty())
    })
}

/// "1 mod", "8 mods".
pub fn mod_count(count: usize) -> String {
    if count == 1 {
        "1 mod".to_owned()
    } else {
        format!("{count} mods")
    }
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
