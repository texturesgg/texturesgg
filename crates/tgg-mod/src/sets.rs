//! The player's sets: named lists a player switches between in one click
//! (Competitive, Practice, one they made). A line in a set is a mod or a
//! textures.gg set it builds on. A set runs through the profile path
//! ([`plan`], [`prepare`], then [`crate::launch_profile`]): its own mods
//! folder is built from the package cache for the session, so switching
//! uninstalls nothing and the player's mods folder is never touched.
//!
//! Each set is a file in `<data>/tgg/sets/`, named by a key made from its
//! name (`friday-night.json`):
//!
//! ```json
//! {
//!   "format": 1,
//!   "name": "Friday night",
//!   "lines": [
//!     { "set": "competitive" },
//!     { "mod": "tgg.prevent-wobbling" },
//!     { "mod": "tgg.ucf", "version": "^0.84" },
//!     { "mod": "tgg.quick-start", "left_out": true }
//!   ],
//!   "resolved": { "tgg.ucf": "0.84.0", "tgg.prevent-wobbling": "1.0.0" }
//! }
//! ```
//!
//! - A set line names a textures.gg set by its link name and follows it as
//!   it is now. A set can build on up to 4, each one that lists only mods
//!   and (for now) is by textures.gg.
//! - A mod line takes `latest` (left out of the file), an exact version or
//!   a range. `left_out` keeps the mod from loading, whether the line is the
//!   set's own or leaves out a mod of a set it builds on.
//! - `locked`: the set is a textures.gg set the player added; it has one
//!   set line and can't be changed, only built on.
//! - `resolved`: the versions the last launch used, for launching offline.
//!
//! Resolving applies the stacking rules: a mod loads once; the set's own
//! line wins (its version, or left out); when two sets it builds on take
//! different versions, the newest release both accept, and with none the
//! mod is skipped with that reason; a set it builds on that textures.gg
//! took down has its mods copied into the set as they were. A mod that
//! can't be had (withdrawn, not built for the game, unknown) is skipped and
//! the reason given, never a refusal to start.
//!
//! Beside each, `<key>.settings.json` is the set's own settings file: the
//! game reads and saves mod settings there while it runs from the set.
//! `.active` holds the key of the active set.
//!
//! The player's mods folder is the built-in set [`INSTALLED`] ("Installed
//! mods"): no file, always there, active until the player picks another,
//! and launched the way the game always has been.
//!
//! The game's data folder (saves, the memory card, `skins.json`) is shared
//! by every set. Skins aren't lines: when the player has a skin on,
//! [`plan`] adds `tgg.skins` (which never counts for netplay).

use crate::cache::{self, tgg_dir};
use crate::catalog::Catalog;
use crate::depends::Range;
use crate::error::{Error, OnProgress};
use crate::local::Local;
use crate::manifest::ModId;
use crate::net::Net;
use crate::netplay::Netplay;
use crate::prepare::Prepared;
use crate::profile::{ANY, Profile, ProfileEntry, ProfilePlan, Settings};
use crate::registry::Set;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// The set file format this crate reads and writes.
pub const FORMAT: u32 = 1;
/// The key of the built-in set: the player's mods folder.
pub const INSTALLED: &str = "installed";
/// The built-in set's name.
pub const INSTALLED_NAME: &str = "Installed mods";
/// The key of the built-in set that launches Melee with no mods at all.
pub const VANILLA: &str = "vanilla";
/// The built-in unmodded set's name.
pub const VANILLA_NAME: &str = "Vanilla";
/// The textures.gg set a new set builds on unless the player says not to:
/// netplay, direct connect, http and unlocks, the Online menu.
pub const ONLINE_PLAY: &str = "online-play";
/// The mod that loads the player's skins, added to whatever set they play
/// when they have a skin on.
pub const SKINS: &str = "tgg.skins";
/// How many sets a set can build on.
pub const MOST_SET_LINES: usize = 4;
/// How many lines a set can have.
pub const MOST_LINES: usize = 100;
const LONGEST_NAME: usize = 40;
const LONGEST_KEY: usize = 40;
const SETS: &str = "sets";
const ACTIVE: &str = ".active";

/// One of the player's sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MySet {
    /// Its file's name, made from its name; [`INSTALLED`] for the built-in
    /// one.
    pub key: String,
    /// What players see.
    pub name: String,
    /// A textures.gg set the player added, which follows the published
    /// one and can't be changed.
    pub locked: bool,
    pub lines: Vec<Line>,
    /// The version of each mod the last launch used, for launching offline.
    pub resolved: BTreeMap<ModId, Version>,
}

/// A line of a set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Line {
    /// A textures.gg set it builds on, by link name (`competitive`).
    Set {
        set: String,
    },
    Mod(ModLine),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModLine {
    #[serde(rename = "mod")]
    pub id: ModId,
    #[serde(default = "latest", skip_serializing_if = "is_latest")]
    pub version: Want,
    /// Not loaded and not counted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub left_out: bool,
}

fn latest() -> Want {
    Want::Latest
}

fn is_latest(want: &Want) -> bool {
    *want == Want::Latest
}

impl ModLine {
    pub fn new(id: ModId, version: Want) -> Self {
        Self {
            id,
            version,
            left_out: false,
        }
    }
}

/// The releases a mod line takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Want {
    /// The newest approved release that works with the game.
    Latest,
    Exact(Version),
    /// The newest approved release in the range.
    Range(Range),
}

impl TryFrom<String> for Want {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        text.parse()
    }
}

impl From<Want> for String {
    fn from(want: Want) -> String {
        want.to_string()
    }
}

impl std::str::FromStr for Want {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        if text == "latest" || text == "*" {
            return Ok(Want::Latest);
        }
        if let Ok(version) = Version::parse(text) {
            return Ok(Want::Exact(version));
        }
        Range::parse(text)
            .map(Want::Range)
            .map_err(|_| format!("{text:?} isn't latest, a version or a range"))
    }
}

impl std::fmt::Display for Want {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Want::Latest => f.write_str("latest"),
            Want::Exact(version) => write!(f, "{version}"),
            Want::Range(range) => f.write_str(range.as_str()),
        }
    }
}

impl Want {
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Want::Latest => true,
            Want::Exact(want) => want == version,
            Want::Range(range) => range.matches(version),
        }
    }

    fn entry(&self, id: &ModId) -> ProfileEntry {
        let (version, range) = match self {
            Want::Exact(version) => (Some(version.clone()), None),
            Want::Range(range) => (None, Some(range.clone())),
            Want::Latest => (None, Some(Range::parse("*").expect("a range"))),
        };
        ProfileEntry {
            id: id.clone(),
            version,
            range,
            sha256: None,
            settings: Settings::new(),
        }
    }
}

/// How a mod plays with others, for the app to group a set's mods by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// It changes the game: the other player needs it too, and it decides
    /// the queue.
    Gameplay,
    /// Only the player sees it.
    Display,
    /// It's off in netplay.
    Offline,
}

impl Class {
    /// The class of a package of [`Netplay`] class `netplay`, before
    /// counting what gameplay mods depend on. Costumes count until the game
    /// has compared them with the disc, so they're gameplay here.
    pub fn of(netplay: Netplay) -> Self {
        match netplay {
            Netplay::Offline => Class::Offline,
            Netplay::Data | Netplay::Display => Class::Display,
            Netplay::Code | Netplay::Files | Netplay::Costumes => Class::Gameplay,
        }
    }
}

/// A mod that decides a set's queue, in the shape the meeting point's queue
/// list gives a queue's `mods`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueMod {
    pub id: ModId,
    pub version: Version,
}

/// Each mod a resolved plan loads with its [`Class`], as the game works it
/// out at load: the package's own class, and gameplay for a mod a gameplay
/// mod depends on (unless it's offline). By id.
pub fn classes(plan: &ProfilePlan) -> Vec<(ModId, Version, Class)> {
    let mut classes: Vec<(ModId, Version, Class)> = plan
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.version.clone(), Class::of(m.netplay)))
        .collect();
    // Each round makes one more mod gameplay, so this ends.
    loop {
        let needed: Vec<ModId> = plan
            .mods
            .iter()
            .filter(|m| {
                classes
                    .iter()
                    .any(|(id, _, class)| *id == m.id && *class == Class::Gameplay)
            })
            .flat_map(|m| m.depends.clone())
            .collect();
        let Some(dep) = classes
            .iter_mut()
            .find(|(id, _, class)| *class == Class::Display && needed.contains(id))
        else {
            break;
        };
        dep.2 = Class::Gameplay;
    }
    classes.sort_by(|a, b| a.0.cmp(&b.0));
    classes
}

/// Where a mod in a set comes from, for the app's groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Group {
    /// From a set it builds on: its link name and title.
    FromSet { set: String, title: String },
    /// The set's own line.
    Added,
    /// Left out, by the set's own line.
    LeftOut,
    /// The skin loader, there because the player has a skin on.
    Skins,
    /// Not listed: a mod the set's mods depend on.
    Needed,
}

/// One mod of a resolved set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetMod {
    pub id: ModId,
    pub group: Group,
    /// The version it loads at; `None` when it's left out or skipped.
    pub version: Option<Version>,
    /// Its class, when it loads.
    pub class: Option<Class>,
    /// Why it's skipped, in the player's words, when it is.
    pub skipped: Option<String>,
}

/// A set resolved for launching: the profile plan, each mod with its group,
/// and anything to tell the player.
#[derive(Clone, Debug)]
pub struct SetPlan {
    pub plan: ProfilePlan,
    /// Why textures.gg couldn't be reached, when it couldn't: the plan uses
    /// the versions the set last used.
    pub offline: Option<String>,
    /// By group, then id.
    pub mods: Vec<SetMod>,
    /// What changed that the player should hear once ("Competitive was
    /// taken down …").
    pub notices: Vec<String>,
}

impl SetPlan {
    /// The gameplay mods it loads, by id: what decides its queue. Two sets
    /// whose lists are equal search in the same queue, as long as the
    /// settings both players must share are equal too.
    pub fn gameplay(&self) -> Vec<QueueMod> {
        classes(&self.plan)
            .into_iter()
            .filter(|(_, _, class)| *class == Class::Gameplay)
            .map(|(id, version, _)| QueueMod { id, version })
            .collect()
    }

    /// The mods skipped, and why.
    pub fn skipped(&self) -> impl Iterator<Item = &SetMod> {
        self.mods.iter().filter(|m| m.skipped.is_some())
    }
}

/// A set file.
#[derive(Serialize, Deserialize)]
struct File {
    format: u32,
    name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    locked: bool,
    #[serde(default)]
    lines: Vec<Line>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    resolved: BTreeMap<ModId, Version>,
}

impl MySet {
    /// The built-in set of the player's mods folder `local`: every mod in it
    /// that fits the game, at its installed version, left out when it's
    /// off. Change it with [`crate::set_enabled`], installing and
    /// uninstalling, as the Mods page always has.
    pub fn installed(local: &Local) -> Self {
        Self {
            key: INSTALLED.to_owned(),
            name: INSTALLED_NAME.to_owned(),
            locked: false,
            lines: local
                .mods
                .iter()
                .filter(|m| m.fits != Some(false))
                .map(|m| {
                    Line::Mod(ModLine {
                        left_out: !m.enabled,
                        ..ModLine::new(
                            m.manifest.id.clone(),
                            Want::Exact(m.manifest.version.clone()),
                        )
                    })
                })
                .collect(),
            resolved: BTreeMap::new(),
        }
    }

    /// Whether it's the built-in set, which launches with the player's own
    /// mods folder rather than through [`plan`].
    pub fn is_installed(&self) -> bool {
        self.key == INSTALLED
    }

    /// The built-in unmodded set: launches plain Melee with no mods.
    pub fn vanilla() -> Self {
        Self {
            key: VANILLA.to_owned(),
            name: VANILLA_NAME.to_owned(),
            locked: true,
            lines: Vec::new(),
            resolved: BTreeMap::new(),
        }
    }

    /// Whether it's the built-in unmodded set, which launches plain Melee
    /// with an empty mods folder rather than through [`plan`].
    pub fn is_vanilla(&self) -> bool {
        self.key == VANILLA
    }

    /// The sets it builds on, by link name.
    pub fn builds_on(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().filter_map(|line| match line {
            Line::Set { set } => Some(set.as_str()),
            Line::Mod(_) => None,
        })
    }

    /// Its own line for the mod `id`.
    pub fn mod_line(&self, id: &ModId) -> Option<&ModLine> {
        self.lines.iter().find_map(|line| match line {
            Line::Mod(m) if &m.id == id => Some(m),
            _ => None,
        })
    }

    fn mod_line_mut(&mut self, id: &ModId) -> Option<&mut ModLine> {
        self.lines.iter_mut().find_map(|line| match line {
            Line::Mod(m) if &m.id == id => Some(m),
            _ => None,
        })
    }

    /// Add the mod `id` at `version`, or change the version its line takes
    /// (and stop leaving it out).
    pub fn add(&mut self, id: ModId, version: Want) {
        match self.mod_line_mut(&id) {
            Some(m) => {
                m.version = version;
                m.left_out = false;
            }
            None => self.lines.push(Line::Mod(ModLine::new(id, version))),
        }
    }

    /// Leave the mod `id` out, or stop leaving it out. Leaving out a mod
    /// with no line of its own (one of a set it builds on) adds a line.
    pub fn leave_out(&mut self, id: &ModId, out: bool) {
        match self.mod_line_mut(id) {
            Some(m) => m.left_out = out,
            None if out => self.lines.push(Line::Mod(ModLine {
                left_out: true,
                ..ModLine::new(id.clone(), Want::Latest)
            })),
            None => {}
        }
    }

    /// Take out the line for the mod `id` or the set `id`; whether there
    /// was one. A mod of a set it builds on loads again.
    pub fn drop_line(&mut self, id: &str) -> bool {
        let before = self.lines.len();
        self.lines.retain(|line| match line {
            Line::Set { set } => set != id,
            Line::Mod(m) => m.id.as_str() != id,
        });
        before != self.lines.len()
    }

    fn file(&self) -> File {
        File {
            format: FORMAT,
            name: self.name.clone(),
            locked: self.locked,
            lines: self.lines.clone(),
            resolved: self.resolved.clone(),
        }
    }
}

/// `<data>/tgg/sets`.
pub fn sets_dir(data: &Path) -> PathBuf {
    tgg_dir(data).join(SETS)
}

/// An empty mods folder used when launching [`VANILLA`] (no mods).
pub fn vanilla_mods_dir(data: &Path) -> PathBuf {
    tgg_dir(data).join(VANILLA)
}

fn path(data: &Path, key: &str) -> PathBuf {
    sets_dir(data).join(format!("{key}.json"))
}

/// The settings file a set's game reads and saves:
/// `<data>/tgg/sets/<key>.settings.json`.
pub fn settings_file(data: &Path, key: &str) -> PathBuf {
    sets_dir(data).join(format!("{key}.settings.json"))
}

fn read(path: &Path, key: &str) -> Result<MySet, Error> {
    let bad = |why: String| Error::BadSet {
        path: path.to_owned(),
        why,
    };
    let bytes = std::fs::read(path).map_err(Error::io(path))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| bad(error.to_string()))?;
    let format = value.get("format").and_then(serde_json::Value::as_u64);
    if format.is_none_or(|format| format > u64::from(FORMAT)) {
        return Err(bad("it's from a newer tgg. Update tgg".into()));
    }
    let file: File = serde_json::from_value(value).map_err(|error| bad(error.to_string()))?;
    Ok(MySet {
        key: key.to_owned(),
        name: file.name,
        locked: file.locked,
        lines: file.lines,
        resolved: file.resolved,
    })
}

fn write(data: &Path, set: &MySet) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(&set.file()).expect("a set serializes") + "\n";
    cache::write_atomic(&path(data, &set.key), json.as_bytes())
}

/// The player's sets, the built-in one (from `local`) first, then by name.
/// A file that can't be read is left out.
pub fn list(data: &Path, local: &Local) -> Result<Vec<MySet>, Error> {
    let mut saved = saved(data)?;
    saved.sort_by_key(|set| set.name.to_lowercase());
    saved.insert(0, MySet::installed(local));
    Ok(saved)
}

fn saved(data: &Path) -> Result<Vec<MySet>, Error> {
    let dir = sets_dir(data);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::io(dir)(error)),
    };
    Ok(entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let key = name.strip_suffix(".json")?;
            if !valid_key(key) {
                return None;
            }
            read(&entry.path(), key).ok()
        })
        .collect())
}

/// The set `key`.
pub fn load(data: &Path, local: &Local, key: &str) -> Result<MySet, Error> {
    if key == INSTALLED {
        return Ok(MySet::installed(local));
    }
    if key == VANILLA {
        return Ok(MySet::vanilla());
    }
    let path = path(data, key);
    if !valid_key(key) || !path.is_file() {
        return Err(Error::NoMySet(key.to_owned()));
    }
    read(&path, key)
}

/// Save the changes to `set`'s lines. Not the built-in one (its lines are
/// the mods folder) nor a locked one.
pub fn save(data: &Path, set: &MySet) -> Result<(), Error> {
    if set.is_installed() {
        return Err(Error::InstalledSet);
    }
    if set.is_vanilla() {
        return Err(Error::SetLocked(VANILLA_NAME.to_owned()));
    }
    let on_disk = load(data, &Local::empty(), &set.key)?;
    if on_disk.locked {
        return Err(Error::SetLocked(set.name.clone()));
    }
    if set.lines.len() > MOST_LINES {
        return Err(Error::BadSetName(format!(
            "a set can have {MOST_LINES} lines at most"
        )));
    }
    if set.builds_on().count() > MOST_SET_LINES {
        return Err(Error::TooManySets);
    }
    write(data, set)
}

/// A new set named `name` with no lines. Build it on sets with
/// [`build_on`] ("New set" builds on [`ONLINE_PLAY`] by default).
pub fn new(data: &Path, name: &str) -> Result<MySet, Error> {
    create(data, name, false, Vec::new())
}

/// Add the textures.gg set `link` (`competitive`) to the player's sets: a
/// locked set with that one line, named as the set is. The one already
/// added, if it was.
pub fn get(net: &dyn Net, data: &Path, link: &str) -> Result<MySet, Error> {
    let set = find(&cache::refresh_sets(net, data)?, link)?;
    let line = Line::Set {
        set: set.name.clone(),
    };
    if let Some(have) = saved(data)?
        .into_iter()
        .find(|have| have.locked && have.lines == [line.clone()])
    {
        return Ok(have);
    }
    let name = free_name(data, &set.title)?;
    create(data, &name, true, vec![line])
}

/// Build the set `key` on the textures.gg set `link`: a set line, which
/// follows that set as it is now. A set that builds on others can't be
/// built on, nor (for now) one not by textures.gg; a set builds on 4 at
/// most. The set comes back saved.
pub fn build_on(
    net: &dyn Net,
    data: &Path,
    local: &Local,
    key: &str,
    link: &str,
) -> Result<MySet, Error> {
    let mut mine = editable(data, local, key)?;
    let set = find(&cache::refresh_sets(net, data)?, link)?;
    if !set.can_build_on() {
        return Err(Error::CantBuildOn(set.title));
    }
    if mine.builds_on().any(|have| have == set.name) {
        return Ok(mine);
    }
    if mine.builds_on().count() >= MOST_SET_LINES {
        return Err(Error::TooManySets);
    }
    mine.lines.push(Line::Set { set: set.name });
    write(data, &mine)?;
    Ok(mine)
}

/// A new set of the mods the player has on in `local`: each one tgg
/// installed from textures.gg and that fits the game, at `latest`. Also the
/// mods left out because textures.gg doesn't have them (a dev build, or one
/// put in the mods folder by hand).
pub fn from_installed(
    data: &Path,
    local: &Local,
    name: &str,
) -> Result<(MySet, Vec<ModId>), Error> {
    let mut lines = Vec::new();
    let mut skipped = Vec::new();
    for m in local
        .mods
        .iter()
        .filter(|m| m.enabled && m.fits != Some(false))
    {
        if m.lock.is_none() || m.dev {
            skipped.push(m.manifest.id.clone());
        } else {
            lines.push(Line::Mod(ModLine::new(m.manifest.id.clone(), Want::Latest)));
        }
    }
    Ok((create(data, name, false, lines)?, skipped))
}

/// A copy of the set `key` named `name`, else "<its name> copy", with its
/// settings. A copy of a locked set isn't locked; a copy of the built-in
/// one is [`from_installed`]'s.
pub fn copy(data: &Path, local: &Local, key: &str, name: Option<&str>) -> Result<MySet, Error> {
    let from = load(data, local, key)?;
    let name = match name {
        Some(name) => name.to_owned(),
        None => free_name(data, &format!("{} copy", from.name))?,
    };
    if from.is_installed() {
        return Ok(from_installed(data, local, &name)?.0);
    }
    let mut copy = create(data, &name, false, from.lines)?;
    copy.resolved = from.resolved;
    write(data, &copy)?;
    let settings = settings_file(data, key);
    if settings.is_file() {
        let to = settings_file(data, &copy.key);
        std::fs::copy(&settings, &to).map_err(Error::io(to))?;
    }
    Ok(copy)
}

/// Name the set `key` `name`. Its key follows the name, so the renamed set
/// comes back.
pub fn rename(data: &Path, local: &Local, key: &str, name: &str) -> Result<MySet, Error> {
    if key == INSTALLED {
        return Err(Error::InstalledSet);
    }
    if key == VANILLA {
        return Err(Error::SetLocked(VANILLA_NAME.to_owned()));
    }
    let mut set = load(data, local, key)?;
    let name = check_name(data, name, Some(key))?;
    let new_key = if slug(&name) == slug(&set.name) {
        key.to_owned()
    } else {
        free_key(data, &name)
    };
    set.name = name;
    if new_key == key {
        write(data, &set)?;
        return Ok(set);
    }
    let was_active = active(data) == key;
    set.key = new_key.clone();
    write(data, &set)?;
    let settings = settings_file(data, key);
    if settings.is_file() {
        let to = settings_file(data, &new_key);
        std::fs::rename(&settings, &to).map_err(Error::io(to))?;
    }
    let old = path(data, key);
    std::fs::remove_file(&old).map_err(Error::io(old))?;
    if was_active {
        activate(data, &new_key)?;
    }
    Ok(set)
}

/// Remove the set `key` and its settings. When it was active, the built-in
/// one is.
pub fn remove(data: &Path, key: &str) -> Result<(), Error> {
    if key == INSTALLED {
        return Err(Error::InstalledSet);
    }
    if key == VANILLA {
        return Err(Error::SetLocked(VANILLA_NAME.to_owned()));
    }
    let file = path(data, key);
    if !valid_key(key) || !file.is_file() {
        return Err(Error::NoMySet(key.to_owned()));
    }
    let was_active = active(data) == key;
    std::fs::remove_file(&file).map_err(Error::io(&file))?;
    let settings = settings_file(data, key);
    if settings.is_file() {
        std::fs::remove_file(&settings).map_err(Error::io(settings))?;
    }
    if was_active {
        activate(data, INSTALLED)?;
    }
    Ok(())
}

/// The key of the active set: the one the player last picked, else (or when
/// it's gone) [`INSTALLED`].
pub fn active(data: &Path) -> String {
    std::fs::read_to_string(sets_dir(data).join(ACTIVE))
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| key == VANILLA || (valid_key(key) && path(data, key).is_file()))
        .unwrap_or_else(|| INSTALLED.to_owned())
}

/// Make the set `key` the active one.
pub fn activate(data: &Path, key: &str) -> Result<(), Error> {
    let file = sets_dir(data).join(ACTIVE);
    if key == INSTALLED {
        return match std::fs::remove_file(&file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(Error::io(file)(error))
            }
            _ => Ok(()),
        };
    }
    if key != VANILLA && (!valid_key(key) || !path(data, key).is_file()) {
        return Err(Error::NoMySet(key.to_owned()));
    }
    cache::write_atomic(&file, format!("{key}\n").as_bytes())
}

/// Whether the player has a skin on (`skins.json` in the game's data folder
/// names one): then [`plan`] adds [`SKINS`].
pub fn skins_on() -> bool {
    crate::skins::skins(&crate::skins::skins_file()).is_ok_and(|skins| !skins.slots.is_empty())
}

/// What a set wants of each mod, before the catalog is looked at.
struct Wanted {
    id: ModId,
    wants: Vec<Want>,
    group: Group,
}

/// `set` resolved for the player with the mods folder `mods`, as
/// [`crate::plan_profile`] resolves a profile, with the stacking rules:
/// online, each mod's newest approved release that works with the current
/// game; offline (the plan says why), the versions the last launch used.
/// `skins`: add [`SKINS`] ([`skins_on`]). A set it builds on that
/// textures.gg took down has its mods copied into the set (saved), and a
/// notice says so. Not for the built-in set ([`MySet::is_installed`]).
pub fn plan(
    net: &dyn Net,
    data: &Path,
    mods: &Path,
    set: &MySet,
    skins: bool,
) -> Result<SetPlan, Error> {
    let mut set = set.clone();
    let mut notices = Vec::new();
    let before = cache::cached_sets(data).unwrap_or_default();
    let published = match cache::refresh_sets(net, data) {
        Ok(published) => {
            take_down(data, &mut set, &before, &published, &mut notices)?;
            published
        }
        Err(Error::Offline(_)) => before,
        Err(error) => return Err(error),
    };

    // Each mod once: the sets it builds on in order, then its own lines.
    let mut wanted: Vec<Wanted> = Vec::new();
    let mut left_out: Vec<ModId> = Vec::new();
    let mut gone: Vec<String> = Vec::new();
    for link in set.builds_on() {
        let Some(found) = published.iter().find(|s| s.name == link) else {
            gone.push(link.to_owned());
            continue;
        };
        for m in &found.mods {
            let want = match m.range.as_str() {
                "*" => Want::Latest,
                _ => Want::Range(m.range.clone()),
            };
            match wanted.iter_mut().find(|w| w.id == m.id) {
                Some(w) => w.wants.push(want),
                None => wanted.push(Wanted {
                    id: m.id.clone(),
                    wants: vec![want],
                    group: Group::FromSet {
                        set: found.name.clone(),
                        title: found.title.clone(),
                    },
                }),
            }
        }
    }
    for link in gone {
        notices.push(format!("textures.gg has no set {link}, so it isn't loaded"));
    }
    for line in &set.lines {
        let Line::Mod(m) = line else { continue };
        if m.left_out {
            wanted.retain(|w| w.id != m.id);
            left_out.push(m.id.clone());
            continue;
        }
        match wanted.iter_mut().find(|w| w.id == m.id) {
            Some(w) => w.wants = vec![m.version.clone()],
            None => wanted.push(Wanted {
                id: m.id.clone(),
                wants: vec![m.version.clone()],
                group: Group::Added,
            }),
        }
    }
    let skins_id: ModId = SKINS.parse().expect("a mod id");
    if skins && !left_out.contains(&skins_id) && !wanted.iter().any(|w| w.id == skins_id) {
        wanted.push(Wanted {
            id: skins_id,
            wants: vec![Want::Latest],
            group: Group::Skins,
        });
    }

    // Two sets that take different versions: the newest release both
    // accept, worked out against the catalog the plan uses.
    let conflicts: RefCell<Vec<ModId>> = RefCell::new(Vec::new());
    let profile = |offline: bool, catalog: &Catalog| {
        conflicts.borrow_mut().clear();
        let mut profile = Profile::new(ANY);
        profile.label = Some(set.name.clone());
        for w in &wanted {
            let pinned = set
                .resolved
                .get(&w.id)
                .filter(|version| offline && w.wants.iter().all(|want| want.matches(version)));
            let entry = match (pinned, w.wants.as_slice()) {
                (Some(version), _) => Want::Exact(version.clone()).entry(&w.id),
                (None, [want]) => want.entry(&w.id),
                (None, wants) => match newest_accepted(catalog, &w.id, wants) {
                    Some(version) => Want::Exact(version).entry(&w.id),
                    None => {
                        conflicts.borrow_mut().push(w.id.clone());
                        continue;
                    }
                },
            };
            profile.mods.push(entry);
        }
        profile
    };
    let (plan, offline) =
        crate::prepare::plan_profile_with(net, data, mods, &Profile::new(ANY), &profile)?;
    let conflicts = conflicts.into_inner();

    let classes = classes(&plan);
    let loaded = |id: &ModId| classes.iter().find(|(found, ..)| found == id);
    let mut out: Vec<SetMod> = Vec::new();
    for w in &wanted {
        let skipped = if conflicts.contains(&w.id) {
            Some("The sets it comes from don't share a version of it.".to_owned())
        } else {
            plan.missing.iter().find_map(|missing| {
                missing_id(missing)
                    .is_some_and(|id| *id == w.id)
                    .then(|| skipped_why(missing))
            })
        };
        let found = loaded(&w.id);
        out.push(SetMod {
            id: w.id.clone(),
            group: w.group.clone(),
            version: found.map(|(_, version, _)| version.clone()),
            class: found.map(|(.., class)| *class),
            skipped: if found.is_none() {
                skipped.or_else(|| Some("textures.gg doesn't have it.".into()))
            } else {
                None
            },
        });
    }
    for id in left_out {
        out.push(SetMod {
            id,
            group: Group::LeftOut,
            version: None,
            class: None,
            skipped: None,
        });
    }
    for (id, version, class) in &classes {
        if !out.iter().any(|m| m.id == *id) {
            out.push(SetMod {
                id: id.clone(),
                group: Group::Needed,
                version: Some(version.clone()),
                class: Some(*class),
                skipped: None,
            });
        }
    }
    Ok(SetPlan {
        plan,
        offline,
        mods: out,
        notices,
    })
}

fn missing_id(missing: &crate::profile::Missing) -> Option<&ModId> {
    use crate::profile::Missing;
    match missing {
        Missing::Unknown { id }
        | Missing::NotBuilt { id, .. }
        | Missing::Unapproved { id, .. }
        | Missing::Withdrawn { id, .. } => Some(id),
        Missing::NoModFor { .. } => None,
    }
}

/// Why a set's mod is skipped, as a sentence that follows its name.
fn skipped_why(missing: &crate::profile::Missing) -> String {
    use crate::profile::Missing;
    match missing {
        Missing::NotBuilt { .. } => "It isn't built for your tgg-melee yet.".to_owned(),
        Missing::Unapproved { .. } => "It isn't approved yet.".to_owned(),
        Missing::Withdrawn { reason, .. } => format!("textures.gg withdrew it ({reason})."),
        Missing::Unknown { .. } | Missing::NoModFor { .. } => {
            "textures.gg doesn't have it.".to_owned()
        }
    }
}

/// The newest release of `id` in `catalog` for this system that every one
/// of `wants` accepts.
fn newest_accepted(catalog: &Catalog, id: &ModId, wants: &[Want]) -> Option<Version> {
    let target = crate::ports::target();
    catalog
        .mods
        .iter()
        .filter(|entry| entry.manifest.id == *id)
        .filter(|entry| {
            entry
                .manifest
                .target
                .as_deref()
                .is_none_or(|t| Some(t) == target)
        })
        .map(|entry| &entry.manifest.version)
        .filter(|version| wants.iter().all(|want| want.matches(version)))
        .max()
        .cloned()
}

/// Copy into `set` the mods of each set it builds on that was in `before`
/// but isn't in `published`: textures.gg took it down. Saved, and said.
fn take_down(
    data: &Path,
    set: &mut MySet,
    before: &[Set],
    published: &[Set],
    notices: &mut Vec<String>,
) -> Result<(), Error> {
    let gone: Vec<Set> = set
        .builds_on()
        .filter(|link| !published.iter().any(|s| s.name == *link))
        .filter_map(|link| before.iter().find(|s| s.name == link).cloned())
        .collect();
    if gone.is_empty() {
        return Ok(());
    }
    for old in gone {
        set.drop_line(&old.name);
        for m in &old.mods {
            if set.mod_line(&m.id).is_none() {
                let want = match m.range.as_str() {
                    "*" => Want::Latest,
                    _ => Want::Range(m.range.clone()),
                };
                set.lines.push(Line::Mod(ModLine::new(m.id.clone(), want)));
            }
        }
        notices.push(format!(
            "textures.gg took {} down, so its mods are now part of {}",
            old.title, set.name
        ));
    }
    set.locked = false;
    if !set.is_installed() {
        write(data, set)?;
    }
    Ok(())
}

/// [`crate::prepare_profile`] for `plan` (skipped mods aren't fetched).
/// The game it prepares reads and saves `set`'s own settings file, and the
/// versions it uses are written to the set for launching offline. Launch it
/// with [`crate::launch_profile`].
pub fn prepare(
    net: &dyn Net,
    plan: &SetPlan,
    data: &Path,
    set: &MySet,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<Prepared, Error> {
    let mut prepared = crate::prepare::prepare_profile(net, &plan.plan, data, progress, cancel)?;
    prepared.settings = settings_file(data, &set.key);
    let resolved: BTreeMap<ModId, Version> = plan
        .plan
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.version.clone()))
        .collect();
    if !set.is_installed() {
        // Read again: the player may have changed it meanwhile.
        let mut fresh = read(&path(data, &set.key), &set.key)?;
        if fresh.resolved != resolved {
            fresh.resolved = resolved;
            write(data, &fresh)?;
        }
    }
    Ok(prepared)
}

fn editable(data: &Path, local: &Local, key: &str) -> Result<MySet, Error> {
    if key == INSTALLED {
        return Err(Error::InstalledSet);
    }
    let set = load(data, local, key)?;
    if set.locked {
        return Err(Error::SetLocked(set.name));
    }
    Ok(set)
}

fn find(sets: &[Set], link: &str) -> Result<Set, Error> {
    sets.iter()
        .find(|found| found.name == link)
        .cloned()
        .ok_or_else(|| Error::NoSuchSet(link.to_owned()))
}

fn create(data: &Path, name: &str, locked: bool, lines: Vec<Line>) -> Result<MySet, Error> {
    let name = check_name(data, name, None)?;
    let set = MySet {
        key: free_key(data, &name),
        name,
        locked,
        lines,
        resolved: BTreeMap::new(),
    };
    write(data, &set)?;
    Ok(set)
}

/// `name`, trimmed, if a set can take it: not empty, not too long, no
/// control characters, and no other set's (but `own`'s), whatever the case.
fn check_name(data: &Path, name: &str, own: Option<&str>) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::BadSetName("a set needs a name".into()));
    }
    if name.chars().count() > LONGEST_NAME {
        return Err(Error::BadSetName(format!(
            "a set's name can be {LONGEST_NAME} characters at most"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(Error::BadSetName(
            "a set's name can't have control characters".into(),
        ));
    }
    let taken = |other: &str| other.to_lowercase() == name.to_lowercase();
    if taken(INSTALLED_NAME)
        || taken(VANILLA_NAME)
        || saved(data)?
            .iter()
            .any(|set| Some(set.key.as_str()) != own && taken(&set.name))
    {
        return Err(Error::SetExists(name.to_owned()));
    }
    Ok(name.to_owned())
}

/// `base`, or `base 2`, `base 3`… whichever no set has.
fn free_name(data: &Path, base: &str) -> Result<String, Error> {
    let names: Vec<String> = saved(data)?
        .into_iter()
        .map(|set| set.name.to_lowercase())
        .collect();
    let free = |name: &str| {
        let lower = name.to_lowercase();
        lower != INSTALLED_NAME.to_lowercase()
            && lower != VANILLA_NAME.to_lowercase()
            && !names.contains(&lower)
    };
    if free(base) {
        return Ok(base.to_owned());
    }
    Ok((2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| free(name))
        .expect("a free name"))
}

/// A file name for a set called `name`: lower-case ASCII letters and
/// digits, with `-` between runs of them, never a Windows device name or
/// [`INSTALLED`], and `-2`, `-3`… when taken.
fn free_key(data: &Path, name: &str) -> String {
    let base = slug(name);
    let free = |key: &str| key != INSTALLED && key != VANILLA && !path(data, key).exists();
    if free(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|key| free(key))
        .expect("a free key")
}

fn slug(name: &str) -> String {
    let mut key = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            key.push(c);
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    let mut key: String = key.chars().take(LONGEST_KEY).collect();
    while key.ends_with('-') {
        key.pop();
    }
    const DEVICES: &[&str] = &["con", "prn", "aux", "nul"];
    let device = DEVICES.contains(&key.as_str())
        || (key.len() == 4
            && (key.starts_with("com") || key.starts_with("lpt"))
            && key.as_bytes()[3].is_ascii_digit());
    if key.is_empty() {
        "set".to_owned()
    } else if device {
        format!("{key}-set")
    } else {
        key
    }
}

/// A key [`slug`] could have made: so one read from `.active` or given by a
/// caller can't name a path outside the sets folder.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_safe_file_names() {
        assert_eq!(slug("Friday night"), "friday-night");
        assert_eq!(slug("  Ledge dash: practice!! "), "ledge-dash-practice");
        assert_eq!(slug("CON"), "con-set");
        assert_eq!(slug("com1"), "com1-set");
        assert_eq!(slug("ベスト"), "set");
        assert_eq!(slug(&"a".repeat(100)).len(), LONGEST_KEY);
        assert!(!valid_key("../x") && !valid_key("friday-night.settings"));
    }

    #[test]
    fn lines_read_as_the_file_writes_them() {
        let json = r#"[{"set":"competitive"},{"mod":"tgg.ucf","version":"^0.84"},
            {"mod":"tgg.quick-start","left_out":true},{"mod":"tgg.skip-results"}]"#;
        let lines: Vec<Line> = serde_json::from_str(json).expect("lines");
        assert_eq!(
            lines[0],
            Line::Set {
                set: "competitive".into()
            }
        );
        let Line::Mod(quick) = &lines[2] else {
            panic!("a mod line")
        };
        assert!(quick.left_out && quick.version == Want::Latest);
        let back = serde_json::to_string(&lines).expect("json");
        assert!(back.contains(r#"{"mod":"tgg.skip-results"}"#), "{back}");
    }

    #[test]
    fn vanilla_set_activates_and_stays_reserved() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(active(dir.path()), INSTALLED);

        activate(dir.path(), VANILLA).expect("activate vanilla");
        assert_eq!(active(dir.path()), VANILLA);

        let loaded = load(dir.path(), &Local::empty(), VANILLA).expect("load vanilla");
        assert!(loaded.is_vanilla() && loaded.locked && loaded.lines.is_empty());

        assert!(matches!(
            new(dir.path(), "Vanilla"),
            Err(Error::SetExists(_))
        ));
        assert!(matches!(
            remove(dir.path(), VANILLA),
            Err(Error::SetLocked(_))
        ));
    }
}
