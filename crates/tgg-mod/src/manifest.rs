//! What a mod says about itself, in its package's `manifest.json`.
//!
//! An author writes the identity fields, `depends`, and what a store shows
//! (`icon`, `screenshots`, `tags`, `homepage`), and may pin `tier`. Packing
//! adds `tier`, `game_abi`, `target`, `api_version`, `state`, `hooks`, `events`, `exports`
//! and `imports`, read from the library, `sim_writes`, from the SDK's record beside it, and `files` and `assets`, read from
//! the mod's folders, so they never come from the author.
//!
//! A mod's netplay class is mostly not written by its author: the game works
//! out whether a mod counts, and refuses a manifest with a `netplay` field. The
//! registry's builder may write the `display` class its review assigned. The
//! one class an author writes is `offline`, which the game enforces itself.
//! [`crate::netplay`] gives the same answer from a manifest.

use crate::decls::Hooks;
use crate::depends::Range;
use crate::files::ModFile;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A mod's id: 1 to 64 lowercase letters, digits, `.`, `-` or `_`, not
/// starting with `.`. It names the mod's folder once installed, so a value of
/// this type is always safe to join onto a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModId(String);

impl ModId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ModId {
    type Error = ManifestError;

    fn try_from(id: String) -> Result<Self, ManifestError> {
        let valid = !id.is_empty()
            && id.len() <= 64
            && !id.starts_with('.')
            && id.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
            });
        if valid {
            Ok(Self(id))
        } else {
            Err(ManifestError::Id(id))
        }
    }
}

impl std::str::FromStr for ModId {
    type Err = ManifestError;

    fn from_str(id: &str) -> Result<Self, ManifestError> {
        id.to_owned().try_into()
    }
}

impl From<ModId> for String {
    fn from(id: ModId) -> Self {
        id.0
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for ModId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The manifest API: `tgg-melee/0`.
    pub api: String,
    pub id: ModId,
    pub name: String,
    pub version: Version,
    /// The library in the package; absent for a mod without one. A package
    /// with a library always names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Every mod this one needs, each with a version range; the game loads
    /// it after them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub depends: BTreeMap<ModId, Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// The mod's icon in a store: a PNG under `assets/`, by its path there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Screenshots a store shows, in order: PNGs under `assets/`, by their
    /// paths there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    /// What a store files the mod under, from [`TAGS`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The mod's web page, an http or https URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The API tier the library is built for: `stable` (no game layout, loads
    /// on any game with its target and API version) or `game` (loads only on
    /// its layout). Packing writes it from the library; an author may write it
    /// first to pin it, and packing refuses a library that doesn't match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,
    /// The game layout a game-tier library was built against, from the
    /// library; absent for the stable tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_abi: Option<String>,
    /// The target triple the library was built for, from the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The mod API `major.minor` the library was built against, from the
    /// library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    /// Bytes of the mod's own state that roll back with the game, from the
    /// library; absent when it keeps none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<u64>,
    /// The functions the library hooks, from the library.
    #[serde(default, skip_serializing_if = "Hooks::is_empty")]
    pub hooks: Hooks,
    /// The events the library subscribes to, from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    /// The functions the library offers other mods, from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
    /// What the library takes from other mods, as `provider-id/export-name`,
    /// from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
    /// What the library can change in the game's simulation through the API:
    /// `call <function>` for each sim write it calls and `event <name>` for
    /// each SIM event it subscribes to whose argument it may change, as the
    /// SDK's build recorded it (`sim-writes.txt` beside the library). A
    /// game-tier library can also change anything it hooks. Every entry is
    /// one of the library's imports or events.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sim_writes: Vec<String>,
    /// The disc files the package replaces or adds under `files/`, in byte
    /// order of path, from packing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ModFile>,
    /// The new files the package ships under `assets/`, which the game
    /// serves at `/mods/<id>/<path>`, in byte order of path, from packing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<ModFile>,
    /// The Slippi gecko codes the mod reproduces, which the registry's Slippi
    /// mapping is built from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slippi: Vec<SlippiCodeDecl>,
    /// The commit of the reviewed source the registry built this package
    /// from (40 lowercase hex digits), written by its builder
    /// (`tgg mod build --source-commit`). Peers on different systems compare
    /// it, since their libraries differ. Absent for a package built anywhere
    /// else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `display`: the class a moderator assigned when reviewing the source,
    /// written by the registry's builder (`tgg mod build --class display`). A
    /// display mod doesn't count toward the mod-set id when its records allow
    /// it ([`crate::netplay::display_check`]). `offline`: written by the
    /// author; the game switches the mod off for a whole netplay session, so
    /// it never counts. Absent otherwise: the mod counts by its contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<ModClass>,
}

/// A library's API tier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Only `tgg/`: no game types, no layout record.
    Stable,
    /// The game's headers or `tgg/game/`: carries the layout it was built on.
    Game,
}

/// A mod's netplay class: `display` the registry assigns on review, `offline`
/// the author declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModClass {
    /// Changes what a player sees or keeps for themselves, never the match.
    Display,
    /// For playing alone (save states, a scripted CPU): the game switches it
    /// off while a netplay session is on.
    Offline,
}

/// A Slippi gecko code a mod reproduces: its address (`0x801D45EC`) or a
/// Game Start fact (`frozen-stadium`), its name, and the Slippi versions it
/// holds for (a semver requirement, `*` for every one), unless the console
/// also ran one of the codes in `unless`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlippiCodeDecl {
    pub code: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "every_version")]
    pub versions: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<String>,
}

fn every_version() -> String {
    "*".to_owned()
}

/// Whether `text` is a git commit id: 40 lowercase hex digits.
pub fn is_commit(text: &str) -> bool {
    text.len() == 40
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The tags a manifest's `tags` may name.
pub const TAGS: &[&str] = &[
    "competitive",
    "netplay",
    "replays",
    "training",
    "visual",
    "audio",
    "quality-of-life",
    "stage",
    "fighter",
];
/// The most tags a manifest has.
pub const MAX_TAGS: usize = 5;
/// The most screenshots a manifest lists.
pub const MAX_SCREENSHOTS: usize = 8;
/// The longest homepage URL, in bytes.
pub const MAX_HOMEPAGE: usize = 256;

/// The library's name when the manifest names none: `mod.so`, or
/// [`WINDOWS_ENTRY`] for a Windows build.
pub const DEFAULT_ENTRY: &str = "mod.so";
/// A Windows mod's library when the manifest names none.
pub const WINDOWS_ENTRY: &str = "mod.dll";

/// The library's name for a mod built for `target` that names no entry.
pub fn default_entry(target: Option<&str>) -> &'static str {
    if target.is_some_and(|target| target.contains("-windows")) {
        WINDOWS_ENTRY
    } else {
        DEFAULT_ENTRY
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest.json is not a valid manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manifest.json is not UTF-8")]
    Utf8,
    #[error("the manifest targets {0}; this format is {api}", api = crate::API)]
    Api(String),
    #[error(
        "the id {0:?} must be 1 to 64 lowercase letters, digits, '.', '-' or '_', not starting with '.'"
    )]
    Id(String),
    #[error("the manifest needs a name")]
    Name,
    #[error("the entry {0:?} must be a file name, not a path")]
    Entry(String),
    #[error(
        "manifest.json has a netplay field; remove it, the game works out what counts for netplay"
    )]
    Netplay,
    #[error("the mod depends on itself")]
    DependsOnItself,
    #[error("the source {0:?} must be a commit: 40 lowercase hex digits")]
    Source(String),
    #[error("the {0} {1:?} must be the path of a .png under assets/")]
    Image(&'static str, String),
    #[error("a mod lists at most {MAX_SCREENSHOTS} screenshots")]
    Screenshots,
    #[error("the tag {0:?} isn't one of {tags}", tags = TAGS.join(", "))]
    Tag(String),
    #[error("a mod has at most {MAX_TAGS} tags, each once")]
    Tags,
    #[error("the homepage {0:?} must be an http or https URL")]
    Homepage(String),
}

impl Manifest {
    /// Read and check a manifest. Every manifest this crate hands out passed
    /// [`Manifest::validate`].
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let json = std::str::from_utf8(bytes).map_err(|_| ManifestError::Utf8)?;
        let value: serde_json::Value = serde_json::from_str(json)?;
        // The game refuses the field outright, so a manifest with it never
        // gets as far as a package.
        if value.get("netplay").is_some() {
            return Err(ManifestError::Netplay);
        }
        let manifest: Self = serde_json::from_value(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// The library's file name in a package or a mod's folder.
    pub fn library_name(&self) -> &str {
        self.entry
            .as_deref()
            .unwrap_or_else(|| default_entry(self.target.as_deref()))
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a manifest serializes");
        json.push('\n');
        json
    }

    /// Check the fields the game and installers rely on that the types alone
    /// don't.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.api != crate::API {
            return Err(ManifestError::Api(self.api.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Name);
        }
        if let Some(entry) = &self.entry
            && (entry.is_empty() || entry.contains(['/', '\\']) || entry.starts_with('.'))
        {
            return Err(ManifestError::Entry(entry.clone()));
        }
        if self.depends.contains_key(&self.id) {
            return Err(ManifestError::DependsOnItself);
        }
        if let Some(source) = &self.source
            && !is_commit(source)
        {
            return Err(ManifestError::Source(source.clone()));
        }
        Ok(())
    }

    /// Check what a store shows: `icon` and `screenshots` are paths of
    /// `.png` files under `assets/`, `tags` come from [`TAGS`], and
    /// `homepage` is a web URL. Packing checks it, and that the images are
    /// PNGs in the package; reading a manifest doesn't, so a tag added later
    /// doesn't stop an older tgg from installing the mod.
    pub fn check_listing(&self) -> Result<(), ManifestError> {
        let png = |kind: &'static str, path: &String| {
            let valid = crate::files::check_path("assets", path).is_ok()
                && path.to_ascii_lowercase().ends_with(".png");
            if valid {
                Ok(())
            } else {
                Err(ManifestError::Image(kind, path.clone()))
            }
        };
        if let Some(icon) = &self.icon {
            png("icon", icon)?;
        }
        for screenshot in &self.screenshots {
            png("screenshot", screenshot)?;
        }
        if self.screenshots.len() > MAX_SCREENSHOTS {
            return Err(ManifestError::Screenshots);
        }
        if let Some(tag) = self.tags.iter().find(|tag| !TAGS.contains(&tag.as_str())) {
            return Err(ManifestError::Tag(tag.clone()));
        }
        let mut tags: Vec<&String> = self.tags.iter().collect();
        tags.sort();
        tags.dedup();
        if self.tags.len() > MAX_TAGS || tags.len() != self.tags.len() {
            return Err(ManifestError::Tags);
        }
        if let Some(homepage) = &self.homepage {
            let host = homepage
                .strip_prefix("https://")
                .or_else(|| homepage.strip_prefix("http://"))
                .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or(""));
            let valid = homepage.len() <= MAX_HOMEPAGE
                && !homepage
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control())
                && host.is_some_and(|host| !host.is_empty());
            if !valid {
                return Err(ManifestError::Homepage(homepage.clone()));
            }
        }
        Ok(())
    }

    /// A manifest with only the fields an author writes.
    pub fn new(id: ModId, name: String, version: Version) -> Self {
        Self {
            api: crate::API.to_owned(),
            id,
            name,
            version,
            entry: None,
            depends: BTreeMap::new(),
            description: None,
            license: None,
            icon: None,
            screenshots: Vec::new(),
            tags: Vec::new(),
            homepage: None,
            tier: None,
            game_abi: None,
            target: None,
            api_version: None,
            state: None,
            hooks: Hooks::default(),
            events: Vec::new(),
            exports: Vec::new(),
            imports: Vec::new(),
            sim_writes: Vec::new(),
            files: Vec::new(),
            assets: Vec::new(),
            slippi: Vec::new(),
            source: None,
            class: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_with_a_netplay_field_is_refused() {
        let json = br#"{"api": "tgg-melee/0", "id": "me.x", "name": "X", "version": "1.0.0",
            "netplay": "cosmetic"}"#;
        assert!(matches!(Manifest::parse(json), Err(ManifestError::Netplay)));
    }
}
