//! Mod profiles: the exact mods (and game) a replay, a netplay host or a
//! tournament needs, read from where they come from, and resolved against a
//! catalog into what a player has, can get, and can't. See the crate's
//! `docs/mod-profiles.md` for the formats.
//!
//! - [`profile_from_slp`], [`profile_from_file`], [`profile_from_local`]:
//!   reading, from disk alone.
//! - [`slippi_mapping`]: the registry's Slippi mapping.
//! - [`resolve_profile`]: no network, no writes: a [`ProfilePlan`].
//!
//! Preparing and launching are in [`crate::prepare`].

use crate::catalog::{Catalog, CatalogEntry, PackageRef};
use crate::depends::Range;
use crate::error::Error;
use crate::local::Local;
use crate::manifest::ModId;
use crate::net::{Net, NetError};
use crate::ports::{self, PortInstall};
use crate::registry::Publisher;
use crate::slp;
use crate::trust::Trust;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// The profile format this crate reads and writes.
pub const FORMAT: u32 = 1;
/// The game profiles name.
pub const GAME: &str = "tgg-melee";
/// A game version: whichever port the player uses.
pub const ANY: &str = "any";
/// The mod that plays replays back, which a replay's profile always names.
pub const REPLAY_PLAYER: &str = "tgg.replay-player";
/// The longest profile file read.
const PROFILE_LIMIT: u64 = 4 * 1024 * 1024;
/// The longest mapping read.
const MAPPING_LIMIT: u64 = 16 * 1024 * 1024;

/// A mod's settings, by key, as the port's `settings.json` holds them.
pub type Settings = BTreeMap<String, Value>;

/// An exact list of mods, and the game they need.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub format: u32,
    /// Display text; nothing reads it to decide anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub game: ProfileGame,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ProfileEntry>,
    /// What the source asked for that can't be had.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<Missing>,
    /// The facts of a Slippi replay, which [`resolve_profile`] maps again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slippi: Option<SlippiFacts>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileGame {
    /// [`GAME`].
    pub name: String,
    /// A release, or [`ANY`].
    pub version: String,
    /// The game layout id, when the source knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
}

/// One mod a profile names: an exact `version`, or a `range` (the newest
/// approved release in it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub id: ModId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// The package, for the platform the profile was written on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Counted settings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: Settings,
}

impl ProfileEntry {
    /// The version or range asked for, as written.
    pub fn wanted(&self) -> String {
        match (&self.version, &self.range) {
            (Some(version), _) => version.to_string(),
            (None, Some(range)) => range.as_str().to_owned(),
            (None, None) => "*".to_owned(),
        }
    }

    fn matches(&self, version: &Version) -> bool {
        match (&self.version, &self.range) {
            (Some(want), _) => want == version,
            (None, Some(range)) => range.matches(version),
            (None, None) => true,
        }
    }
}

/// Something a profile's source asked for that can't be had.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "kebab-case")]
pub enum Missing {
    /// The registry has no such mod.
    Unknown { id: ModId },
    /// The registry has the mod, but nothing at `version` (a version or a
    /// range) built for the profile's game.
    NotBuilt { id: ModId, version: String },
    /// No approved release at `version`.
    Unapproved { id: ModId, version: String },
    /// A Slippi code no mod reproduces. `minor` when the mapping lists it
    /// among the codes that rarely change a replay (one plays as is,
    /// likely).
    NoModFor {
        code: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        minor: bool,
    },
}

impl Missing {
    /// Whether the replay likely plays as is despite it.
    pub fn is_minor(&self) -> bool {
        matches!(self, Missing::NoModFor { minor: true, .. })
    }
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Missing::Unknown { id } => write!(f, "textures.gg has no mod {id}"),
            Missing::NotBuilt { id, version } => {
                write!(f, "{id} {version} isn't built for this game")
            }
            Missing::Unapproved { id, version } => write!(f, "{id} {version} isn't approved"),
            Missing::NoModFor { code, minor: false } => {
                write!(f, "no mod reproduces the Slippi code {code}")
            }
            Missing::NoModFor { code, minor: true } => write!(
                f,
                "no mod reproduces the Slippi code {code}, which rarely changes a replay"
            ),
        }
    }
}

/// What a Slippi replay says about its console.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlippiFacts {
    /// The Slippi version that recorded it.
    pub version: String,
    /// Every code its console ran, and the Game Start facts, as the mapping
    /// names them.
    pub codes: Vec<String>,
    /// The stage id from Game Start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<u16>,
}

impl Profile {
    /// An empty profile of `version` of the game.
    pub fn new(version: &str) -> Self {
        Self {
            format: FORMAT,
            label: None,
            game: ProfileGame {
                name: GAME.to_owned(),
                version: version.to_owned(),
                layout: None,
            },
            mods: Vec::new(),
            missing: Vec::new(),
            slippi: None,
        }
    }

    /// A profile from JSON; the message says why it isn't one.
    pub fn from_value(value: Value) -> Result<Self, ProfileError> {
        let format = value
            .get("format")
            .and_then(Value::as_u64)
            .ok_or_else(|| ProfileError::Bad("it has no format".into()))?;
        if format > u64::from(FORMAT) {
            return Err(ProfileError::Format(format as u32));
        }
        let profile: Self =
            serde_json::from_value(value).map_err(|error| ProfileError::Bad(error.to_string()))?;
        if profile.game.name != GAME {
            return Err(ProfileError::Bad(format!(
                "it's for {}, not {GAME}",
                profile.game.name
            )));
        }
        if let Some(entry) = profile
            .mods
            .iter()
            .find(|entry| entry.version.is_some() == entry.range.is_some())
        {
            return Err(ProfileError::Bad(format!(
                "{} needs a version or a range, not both",
                entry.id
            )));
        }
        Ok(profile)
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a profile serializes");
        json.push('\n');
        json
    }

    pub fn get(&self, id: &ModId) -> Option<&ProfileEntry> {
        self.mods.iter().find(|entry| &entry.id == id)
    }

    /// Name [`REPLAY_PLAYER`] (any version) unless it does.
    fn with_replay_player(mut self) -> Self {
        let id: ModId = REPLAY_PLAYER.parse().expect("a valid id");
        if self.get(&id).is_none() {
            self.mods.push(ProfileEntry {
                id,
                version: None,
                range: Some(Range::parse("*").expect("a range")),
                sha256: None,
                settings: Settings::new(),
            });
        }
        self
    }
}

/// Why JSON isn't a profile.
#[derive(Debug)]
pub enum ProfileError {
    Format(u32),
    Bad(String),
}

impl ProfileError {
    fn at(self, path: &Path) -> Error {
        match self {
            ProfileError::Format(format) => Error::ProfileFormat {
                path: path.to_owned(),
                format,
            },
            ProfileError::Bad(why) => Error::BadProfile {
                path: path.to_owned(),
                why,
            },
        }
    }
}

/// The profile in the file `path` (`profile.json`).
pub fn profile_from_file(path: &Path) -> Result<Profile, Error> {
    let size = std::fs::metadata(path).map_err(Error::io(path))?.len();
    if size > PROFILE_LIMIT {
        return Err(Error::BadProfile {
            path: path.to_owned(),
            why: "it's too large".into(),
        });
    }
    let bytes = std::fs::read(path).map_err(Error::io(path))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| Error::BadProfile {
        path: path.to_owned(),
        why: error.to_string(),
    })?;
    Profile::from_value(value).map_err(|error| error.at(path))
}

/// The player's own profile: the port in use and every turned-on mod that
/// counts toward the mod-set id, at its installed version. Settings are
/// left out: the crate can't yet tell a counted setting from one the
/// player keeps to themselves.
pub fn profile_from_local(local: &Local) -> Profile {
    let mut profile = Profile::new(local.port.as_ref().map_or(ANY, |port| &port.version));
    profile.game.layout = local.port.as_ref().map(|port| port.game_abi.clone());
    for m in &local.mods {
        if !m.enabled || m.fits == Some(false) || !crate::netplay::classify(&m.manifest).counts() {
            continue;
        }
        profile.mods.push(ProfileEntry {
            id: m.manifest.id.clone(),
            version: Some(m.manifest.version.clone()),
            range: None,
            sha256: m.lock.as_ref().and_then(|entry| entry.sha256.clone()),
            settings: Settings::new(),
        });
    }
    profile
}

/// The profile of the replay `path`: from `metadata.tgg` for one recorded
/// on tgg-melee, else from its Slippi code list, mapped through `mapping`
/// when there is one (without, the profile keeps the facts for
/// [`resolve_profile`] to map). It reads the file's head and tail alone.
pub fn profile_from_slp(path: &Path, mapping: Option<&SlippiMapping>) -> Result<Profile, Error> {
    let bad = |why: String| Error::BadReplay {
        path: path.to_owned(),
        why,
    };
    let replay =
        slp::read(path, |metadata| metadata.get("tgg").is_some()).map_err(|error| match error {
            slp::SlpError::Io(source) => Error::Io {
                path: path.to_owned(),
                source,
            },
            error => bad(error.to_string()),
        })?;
    let profile = match replay.metadata.as_ref().and_then(|m| m.get("tgg")) {
        Some(tgg) => from_tgg_metadata(tgg).map_err(|error| match error {
            ProfileError::Format(format) => Error::ProfileFormat {
                path: path.to_owned(),
                format,
            },
            ProfileError::Bad(why) => bad(format!("its tgg metadata: {why}")),
        })?,
        None => match &replay.start {
            Some(start) => {
                let profile = from_slippi(start);
                match mapping {
                    Some(mapping) => mapping.apply(&profile),
                    None => profile,
                }
            }
            None => return Err(bad("it has no Game Start".into())),
        },
    };
    Ok(profile.with_replay_player())
}

/// A profile from a tgg-melee replay's `metadata.tgg`.
fn from_tgg_metadata(tgg: &Value) -> Result<Profile, ProfileError> {
    match tgg.get("profile") {
        Some(Value::String(text)) => {
            let value = serde_json::from_str(text).map_err(|e| ProfileError::Bad(e.to_string()))?;
            return Profile::from_value(value);
        }
        Some(value @ Value::Object(_)) => return Profile::from_value(value.clone()),
        _ => {}
    }
    let build = tgg.get("game").and_then(Value::as_str).unwrap_or("");
    let mut parts = build.split_whitespace();
    let (name, version, layout) = (parts.next(), parts.next(), parts.next());
    if name.is_some_and(|name| name != GAME) {
        return Err(ProfileError::Bad(format!("it was played on {build}")));
    }
    let mut profile = Profile::new(version.unwrap_or(ANY));
    profile.game.layout = layout.map(str::to_owned);
    let mods = tgg.get("mods").and_then(Value::as_array);
    for text in mods.into_iter().flatten().filter_map(Value::as_str) {
        let Some((id, version)) = text.split_once(' ') else {
            return Err(ProfileError::Bad(format!("a mod is {text:?}")));
        };
        let id = id
            .parse()
            .map_err(|_| ProfileError::Bad(format!("a mod is {text:?}")))?;
        let version = version
            .parse()
            .map_err(|_| ProfileError::Bad(format!("a mod is {text:?}")))?;
        profile.mods.push(ProfileEntry {
            id,
            version: Some(version),
            range: None,
            sha256: None,
            settings: Settings::new(),
        });
    }
    Ok(profile)
}

/// The Game Start facts, as the mapping names them.
pub const FROZEN_STADIUM: &str = "frozen-stadium";
pub const UCF_OFF: &str = "ucf-off";

fn from_slippi(start: &slp::Start) -> Profile {
    let version = start.version();
    let mut codes = start.codes.clone();
    if start.frozen_stadium && start.stage == slp::POKEMON_STADIUM {
        codes.push(FROZEN_STADIUM.to_owned());
    }
    if start.ucf_off {
        codes.push(UCF_OFF.to_owned());
    }
    let mut profile = Profile::new(ANY);
    profile.label = Some(format!("Slippi {version}"));
    profile.slippi = Some(SlippiFacts {
        version,
        codes,
        stage: Some(start.stage),
    });
    profile
}

/// The registry's Slippi mapping: which mod reproduces each gecko code a
/// Slippi console runs, and which codes don't change gameplay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlippiMapping {
    pub format: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    #[serde(default)]
    pub codes: Vec<SlippiCode>,
    #[serde(default)]
    pub ignore: Vec<SlippiIgnore>,
    /// Gameplay codes no mod reproduces yet that rarely change a replay:
    /// `missing` as minor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minor: Vec<SlippiIgnore>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlippiCode {
    /// `0x801D45EC`, or a Game Start fact ([`FROZEN_STADIUM`], [`UCF_OFF`]).
    pub code: String,
    #[serde(default)]
    pub name: String,
    /// The Slippi versions it holds for.
    #[serde(default = "every_version")]
    pub versions: VersionReq,
    /// The stage ids the code matters on; on any other it is ignored. Empty
    /// for every stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<u16>,
    /// The entry doesn't hold when the console also ran one of these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<String>,
    /// The mod that reproduces it; none when the game itself does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ModId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// Counted settings of that mod the code implies.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: Settings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlippiIgnore {
    pub code: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "every_version")]
    pub versions: VersionReq,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<u16>,
}

fn every_version() -> VersionReq {
    VersionReq::STAR
}

/// How the mapping takes a code.
enum Mapped<'a> {
    Code(&'a SlippiCode),
    Minor,
    Ignored,
    Unmapped,
}

fn same_code(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

impl SlippiMapping {
    /// A mapping that maps nothing.
    pub fn empty() -> Self {
        Self {
            format: FORMAT,
            snapshot: None,
            codes: Vec::new(),
            ignore: Vec::new(),
            minor: Vec::new(),
        }
    }

    fn lookup(&self, code: &str, facts: &SlippiFacts, version: &Version) -> Mapped<'_> {
        let holds = |versions: &VersionReq, stages: &[u16]| {
            versions.matches(version)
                && (stages.is_empty() || facts.stage.is_some_and(|stage| stages.contains(&stage)))
        };
        // An entry for the code that holds for the version but names other
        // stages: the code doesn't matter on this one.
        let elsewhere = |versions: &VersionReq, stages: &[u16]| {
            versions.matches(version) && !holds(versions, stages)
        };
        let unless = |entry: &SlippiCode| {
            entry
                .unless
                .iter()
                .any(|other| facts.codes.iter().any(|ran| same_code(ran, other)))
        };
        let mut ignored = false;
        for entry in self
            .codes
            .iter()
            .filter(|entry| same_code(&entry.code, code))
        {
            if unless(entry) {
                continue;
            }
            if holds(&entry.versions, &entry.stages) {
                return Mapped::Code(entry);
            }
            ignored |= elsewhere(&entry.versions, &entry.stages);
        }
        for entry in self
            .minor
            .iter()
            .filter(|entry| same_code(&entry.code, code))
        {
            if holds(&entry.versions, &entry.stages) {
                return Mapped::Minor;
            }
            ignored |= elsewhere(&entry.versions, &entry.stages);
        }
        if ignored
            || self
                .ignore
                .iter()
                .any(|entry| same_code(&entry.code, code) && entry.versions.matches(version))
        {
            return Mapped::Ignored;
        }
        Mapped::Unmapped
    }

    /// `profile` with its mods and `missing` mapped
    /// from its Slippi facts; a profile without them as it is.
    pub fn apply(&self, profile: &Profile) -> Profile {
        let Some(facts) = &profile.slippi else {
            return profile.clone();
        };
        // A version that doesn't parse reads as the oldest.
        let version = Version::parse(&facts.version).unwrap_or(Version::new(0, 0, 0));
        let mut out = profile.clone();
        out.mods.clear();
        out.missing.clear();
        for code in &facts.codes {
            match self.lookup(code, facts, &version) {
                Mapped::Code(entry) => {
                    let Some(id) = &entry.id else {
                        // A gameplay code no mod reproduces yet
                        out.missing.push(Missing::NoModFor {
                            code: code.clone(),
                            minor: false,
                        });
                        continue;
                    };
                    match out.mods.iter_mut().find(|m| &m.id == id) {
                        Some(have) => have.settings.extend(entry.settings.clone()),
                        None => out.mods.push(ProfileEntry {
                            id: id.clone(),
                            version: None,
                            range: Some(
                                entry
                                    .range
                                    .clone()
                                    .unwrap_or_else(|| Range::parse("*").expect("a range")),
                            ),
                            sha256: None,
                            settings: entry.settings.clone(),
                        }),
                    }
                }
                Mapped::Ignored => {}
                Mapped::Minor => out.missing.push(Missing::NoModFor {
                    code: code.clone(),
                    minor: true,
                }),
                Mapped::Unmapped => out.missing.push(Missing::NoModFor {
                    code: code.clone(),
                    minor: false,
                }),
            }
        }
        // A replay's profile keeps its player.
        let player: ModId = REPLAY_PLAYER.parse().expect("a valid id");
        if profile.get(&player).is_some() {
            out = out.with_replay_player();
        }
        out
    }
}

/// The registry's Slippi mapping (`GET /api/registry/slippi`); an empty one
/// when it has none yet (a 404).
pub fn slippi_mapping(net: &dyn Net) -> Result<SlippiMapping, Error> {
    let url = format!("{}/api/registry/slippi", net.api_url());
    let body = match net.get(&url, MAPPING_LIMIT) {
        Ok(body) => body,
        Err(NetError::NotFound { .. }) => return Ok(SlippiMapping::empty()),
        Err(error) => return Err(Error::from_net(error)),
    };
    parse_mapping(&body).map_err(Error::Registry)
}

pub(crate) fn parse_mapping(bytes: &[u8]) -> Result<SlippiMapping, String> {
    let mapping: SlippiMapping =
        serde_json::from_slice(bytes).map_err(|error| format!("the Slippi mapping: {error}"))?;
    if mapping.format > FORMAT {
        return Err(format!(
            "the Slippi mapping is format {}; this tgg reads format {FORMAT}",
            mapping.format
        ));
    }
    Ok(mapping)
}

/// What a profile needs of the game.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum GameNeed {
    Installed(PortInstall),
    /// Not installed, but tgg can install it alongside: `size` bytes to
    /// download, when tgg has seen the release. `version` may be `latest`
    /// when the player has no port at all.
    Available {
        version: String,
        size: Option<u64>,
    },
    /// Not a release tgg can get.
    Unavailable {
        version: String,
    },
}

impl GameNeed {
    pub fn version(&self) -> &str {
        match self {
            GameNeed::Installed(install) => &install.version,
            GameNeed::Available { version, .. } | GameNeed::Unavailable { version } => version,
        }
    }
}

/// One mod a resolved profile runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileMod {
    pub id: ModId,
    pub name: String,
    pub version: Version,
    pub publisher: Publisher,
    /// Its package is in the package cache.
    pub cached: bool,
    /// The bytes it downloads when it isn't cached.
    pub size: u64,
    /// Fetching it takes the player's yes: its publisher isn't official
    /// and the player hasn't said yes to them.
    pub needs_trust: bool,
    /// What to download.
    pub package: PackageRef,
    pub settings: Settings,
    /// Whether it changes how the game plays (the catalog's
    /// `changes_game`).
    pub changes_game: bool,
}

/// Whether a profile plays with the player's own installs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// The current port and installed mods already satisfy it, every extra
    /// turned-on mod being one that doesn't change the game: play with the
    /// normal mods folder.
    AsInstalled,
    /// Prepare it and launch it with its own folder.
    NeedsProfile,
}

/// A profile resolved against one catalog and the player's installs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePlan {
    pub game: GameNeed,
    pub mods: Vec<ProfileMod>,
    pub missing: Vec<Missing>,
    /// The hash of the resolved profile, which names its folder.
    pub key: String,
    pub fit: Fit,
    /// The resolved profile: exact versions, this platform's packages.
    pub profile: Profile,
}

impl ProfilePlan {
    /// The bytes [`crate::prepare_profile`] downloads for the mods.
    pub fn size(&self) -> u64 {
        self.mods.iter().filter(|m| !m.cached).map(|m| m.size).sum()
    }

    /// The publishers whose yes the plan's mods need.
    pub fn untrusted(&self) -> Vec<&str> {
        let mut publishers: Vec<&str> = self
            .mods
            .iter()
            .filter(|m| m.needs_trust)
            .map(|m| m.publisher.name.as_str())
            .collect();
        publishers.sort();
        publishers.dedup();
        publishers
    }
}

/// The game layout a profile's mods are built for: the one it names, else
/// that of the installed port it needs (the current one for `any`). The
/// catalog to resolve it against is that layout's.
pub fn profile_layout(profile: &Profile, ports: &Path) -> Option<String> {
    if let Some(layout) = &profile.game.layout {
        return Some(layout.clone());
    }
    let installs = ports::installed_ports(ports).ok()?;
    installs
        .into_iter()
        .find(|install| {
            if profile.game.version == ANY {
                install.current
            } else {
                install.version == profile.game.version.trim_start_matches('v')
            }
        })
        .map(|install| install.release.game_abi)
}

/// Resolve `profile` against `catalog` (for its game's layout), the Slippi
/// `mapping`, the ports folder `ports` (`<data>/tgg/ports`; the package
/// cache is beside it), the player's installs `local` and their `trust`.
/// No network, no writes.
pub fn resolve_profile(
    profile: &Profile,
    catalog: &Catalog,
    mapping: &SlippiMapping,
    ports: &Path,
    local: &Local,
    trust: &Trust,
) -> ProfilePlan {
    let profile = mapping.apply(profile);
    let game = game_need(&profile.game.version, ports);
    let target = ports::target();
    let fits = |entry: &&CatalogEntry| {
        entry
            .manifest
            .target
            .as_deref()
            .is_none_or(|t| Some(t) == target)
    };
    let known = |id: &ModId| {
        catalog.mods.iter().any(|entry| entry.manifest.id == *id) || catalog.ids.contains(id)
    };
    let best = |id: &ModId, want: &dyn Fn(&Version) -> bool| {
        catalog
            .mods
            .iter()
            .filter(fits)
            .filter(|entry| entry.manifest.id == *id && want(&entry.manifest.version))
            .max_by(|a, b| a.manifest.version.cmp(&b.manifest.version))
    };

    let mut missing: Vec<Missing> = profile
        .missing
        .iter()
        .filter(|m| matches!(m, Missing::NoModFor { .. } | Missing::Unapproved { .. }))
        .cloned()
        .collect();
    let mut chosen: Vec<(&CatalogEntry, Settings)> = Vec::new();
    let not_found = |id: &ModId, version: String, missing: &mut Vec<Missing>| {
        let item = if known(id) {
            Missing::NotBuilt {
                id: id.clone(),
                version,
            }
        } else {
            Missing::Unknown { id: id.clone() }
        };
        if !missing.contains(&item) {
            missing.push(item);
        }
    };
    for entry in &profile.mods {
        match best(&entry.id, &|version| entry.matches(version)) {
            Some(found) => chosen.push((found, entry.settings.clone())),
            None => not_found(&entry.id, entry.wanted(), &mut missing),
        }
    }
    // What the chosen releases depend on, until nothing changes. Each round
    // adds a mod not chosen yet, so this ends.
    loop {
        let mut added = false;
        let deps: Vec<(ModId, Range)> = chosen
            .iter()
            .flat_map(|(entry, _)| entry.manifest.depends.clone())
            .collect();
        for (dep, range) in deps {
            if chosen.iter().any(|(entry, _)| entry.manifest.id == dep) {
                continue;
            }
            match best(&dep, &|version| range.matches(version)) {
                Some(found) => {
                    chosen.push((found, Settings::new()));
                    added = true;
                }
                None => not_found(&dep, range.as_str().to_owned(), &mut missing),
            }
        }
        if !added {
            break;
        }
    }

    let packages = ports
        .parent()
        .map(|tgg| tgg.join(crate::cache::PACKAGES))
        .unwrap_or_default();
    let mods: Vec<ProfileMod> = chosen
        .iter()
        .map(|(entry, settings)| {
            let publisher = entry.publisher.clone().unwrap_or_else(|| Publisher {
                name: "unknown".into(),
                official: false,
            });
            ProfileMod {
                id: entry.manifest.id.clone(),
                name: entry.manifest.name.clone(),
                version: entry.manifest.version.clone(),
                needs_trust: !trust.allows(&publisher),
                publisher,
                cached: crate::cache::is_cached(&packages, &entry.package),
                size: entry.package.size,
                package: entry.package.clone(),
                settings: settings.clone(),
                changes_game: entry.changes_game,
            }
        })
        .collect();

    let mut resolved = Profile::new(game.version());
    resolved.label = profile.label.clone();
    resolved.game.layout = match &game {
        GameNeed::Installed(install) => Some(install.release.game_abi.clone()),
        _ => profile.game.layout.clone(),
    };
    resolved.mods = mods
        .iter()
        .map(|m| ProfileEntry {
            id: m.id.clone(),
            version: Some(m.version.clone()),
            range: None,
            sha256: Some(m.package.sha256.clone()),
            settings: m.settings.clone(),
        })
        .collect();
    resolved.mods.sort_by(|a, b| a.id.cmp(&b.id));
    resolved.missing = missing.clone();
    let key = key(&resolved);
    let fit = fit(&game, &mods, catalog, local);
    ProfilePlan {
        game,
        mods,
        missing,
        key,
        fit,
        profile: resolved,
    }
}

fn game_need(version: &str, ports: &Path) -> GameNeed {
    let installs = ports::installed_ports(ports).unwrap_or_default();
    if version == ANY {
        return match installs.into_iter().find(|install| install.current) {
            Some(install) => GameNeed::Installed(install),
            None => GameNeed::Available {
                version: "latest".into(),
                size: None,
            },
        };
    }
    let version = version.trim_start_matches('v');
    if let Some(install) = installs.into_iter().find(|i| i.version == version) {
        return GameNeed::Installed(install);
    }
    if Version::parse(version).is_err() {
        return GameNeed::Unavailable {
            version: version.to_owned(),
        };
    }
    GameNeed::Available {
        version: version.to_owned(),
        size: crate::prepare::seen_release(ports, version).map(|release| release.size()),
    }
}

/// The hash of a resolved profile: the game and each
/// mod's version, package and settings. 16 hex digits.
fn key(resolved: &Profile) -> String {
    let fields = serde_json::json!({
        "game": resolved.game.version,
        "mods": resolved.mods.iter().map(|m| serde_json::json!({
            "id": m.id, "version": m.version, "sha256": m.sha256, "settings": m.settings,
        })).collect::<Vec<_>>(),
    });
    crate::package::sha256_hex(fields.to_string().as_bytes())[..16].to_owned()
}

fn fit(game: &GameNeed, mods: &[ProfileMod], catalog: &Catalog, local: &Local) -> Fit {
    let current = matches!(game, GameNeed::Installed(install) if install.current);
    let installed = mods.iter().all(|m| {
        local.get(&m.id).is_some_and(|have| {
            have.enabled && have.fits != Some(false) && have.manifest.version == m.version
        }) && m.settings.iter().all(|(key, value)| {
            local
                .settings
                .get(m.id.as_str())
                .and_then(|saved| saved.get(key))
                == Some(value)
        })
    });
    let extras_harmless = local
        .mods
        .iter()
        .filter(|have| have.enabled && !mods.iter().any(|m| m.id == *have.id()))
        .all(|have| {
            !crate::netplay::classify(&have.manifest).counts()
                || catalog.mods.iter().any(|entry| {
                    entry.manifest.id == *have.id()
                        && entry.manifest.version == have.manifest.version
                        && !entry.changes_game
                })
        });
    if current && installed && extras_harmless {
        Fit::AsInstalled
    } else {
        Fit::NeedsProfile
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn id(text: &str) -> ModId {
        text.parse().expect("id")
    }

    #[test]
    fn a_profile_file_round_trips_and_newer_formats_are_refused() {
        let value = json!({
            "format": 1,
            "label": "Competitive v3",
            "game": {"name": "tgg-melee", "version": "0.1.0-rc.9"},
            "mods": [
                {"id": "tgg.neutral-spawns", "version": "0.1.2", "sha256": "ab"},
                {"id": "tgg.competitive-rules", "version": "0.1.2",
                 "settings": {"stocks": 4, "stock-time": 8}},
                {"id": "tgg.stadium-preload", "range": "^0.1"}
            ],
            "missing": [{"id": "tgg.example", "version": "1.0.0", "why": "unknown"},
                        {"why": "no-mod-for", "code": "0x801D24FC"}]
        });
        let profile = Profile::from_value(value).expect("a profile");
        assert_eq!(
            profile.missing,
            [
                Missing::Unknown {
                    id: id("tgg.example")
                },
                Missing::NoModFor {
                    code: "0x801D24FC".into(),
                    minor: false
                }
            ]
        );
        let again: Value = serde_json::from_str(&profile.to_json()).expect("json");
        assert_eq!(Profile::from_value(again).expect("again"), profile);

        let newer = json!({"format": 2, "game": {"name": "tgg-melee", "version": "any"}});
        assert!(matches!(
            Profile::from_value(newer),
            Err(ProfileError::Format(2))
        ));
        let both = json!({"format": 1, "game": {"name": "tgg-melee", "version": "any"},
                          "mods": [{"id": "a.b", "version": "1.0.0", "range": "^1"}]});
        assert!(matches!(
            Profile::from_value(both),
            Err(ProfileError::Bad(_))
        ));
    }

    #[test]
    fn tgg_metadata_without_a_profile_reads_its_game_and_mods() {
        let tgg = json!({
            "game": "tgg-melee 0.1.0-rc.8 abc123",
            "modSetId": "0011",
            "mods": ["tgg.ucf 0.84.0", "tgg.neutral-spawns 0.1.2"]
        });
        let profile = from_tgg_metadata(&tgg).expect("profile");
        assert_eq!(profile.game.version, "0.1.0-rc.8");
        assert_eq!(profile.game.layout.as_deref(), Some("abc123"));
        assert_eq!(profile.mods.len(), 2);
        assert_eq!(profile.mods[0].id.as_str(), "tgg.ucf");
        assert_eq!(profile.mods[0].version, Some(Version::new(0, 84, 0)));
        assert_eq!(profile.mods[1].version, Some(Version::new(0, 1, 2)));
    }

    #[test]
    fn the_mapping_maps_codes_by_version_and_unless() {
        let mapping = crate::fake::slippi_mapping();
        let facts = |version: &str, codes: &[&str]| {
            let mut profile = Profile::new(ANY);
            profile.slippi = Some(SlippiFacts {
                version: version.into(),
                codes: codes.iter().map(|c| (*c).to_owned()).collect(),
                stage: Some(31),
            });
            mapping.apply(&profile)
        };
        // Slippi 3.16 with UCF 0.8: tgg.ucf 0.8.0.
        let legacy = facts("3.16.0", &["0x800908F4", "0x8016E510", "0x801D45EC"]);
        let ids: Vec<&str> = legacy.mods.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            ["tgg.ucf", "tgg.neutral-spawns", "tgg.stadium-preload"]
        );
        assert_eq!(
            legacy.mods[0].range,
            Some("0.8.0".parse().expect("a range"))
        );
        assert!(legacy.missing.is_empty());
        // 3.19 with UCF 0.84: a newer tgg.ucf, no preload, and an unknown
        // code missing.
        let modern = facts(
            "3.19.1",
            &["0x8006B460", "0x800908F4", "0x801D45EC", "0x80000001"],
        );
        let ids: Vec<&str> = modern.mods.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["tgg.ucf"]);
        assert_eq!(
            modern.mods[0].range,
            Some(">=0.84".parse().expect("a range"))
        );
        assert_eq!(
            modern.missing,
            [
                Missing::NoModFor {
                    code: "0x801D45EC".into(),
                    minor: false
                },
                Missing::NoModFor {
                    code: "0x80000001".into(),
                    minor: false
                }
            ]
        );
    }
}
