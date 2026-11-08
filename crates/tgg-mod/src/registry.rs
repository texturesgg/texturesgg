//! The textures.gg registry, read over its HTTP API: the curated sets, the
//! catalog of releases built for one game layout, and what a store shows of
//! each mod.
//!
//! - `GET /api/registry/sets`: a JSON array of [`Set`]s.
//! - `GET /api/registry/mods?page=&pageSize=`: a [`ModPage`].
//! - `GET /api/registry/mods/<id>`: a [`ModDetails`].
//! - `GET /api/registry/catalog?layout=<game layout>`: a [`Catalog`] of every
//!   approved release with a build for that layout, with its `snapshot` id,
//!   and `ids`, every mod id the registry has, so a plan can tell a mod not
//!   built for the layout from one that doesn't exist.
//!
//! Package URLs in the catalog may be relative to the catalog's own URL;
//! [`catalog`] returns them absolute.

use crate::catalog::Catalog;
use crate::depends::Range;
use crate::error::Error;
use crate::manifest::ModId;
use crate::net::{Net, NetError};
use crate::netplay::Netplay;
use crate::port::Port;
use semver::Version;
use serde::{Deserialize, Serialize};

/// The longest registry answer read.
const LIMIT: u64 = 16 * 1024 * 1024;

/// A named, curated list of mods, installed with one button.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Set {
    /// The set's id, such as `competitive`.
    pub name: String,
    /// What players see, such as "Competitive".
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Bumped whenever the list or a range changes.
    pub version: u32,
    pub mods: Vec<SetMod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMod {
    pub id: ModId,
    /// The releases the set takes, read as a manifest's `depends` are.
    pub range: Range,
}

impl Set {
    /// The range the set takes `id` in, if it lists it.
    pub fn range(&self, id: &ModId) -> Option<&Range> {
        self.mods.iter().find(|m| &m.id == id).map(|m| &m.range)
    }
}

/// The registry's sets; none when it has none yet (it answers 404).
pub fn sets(net: &dyn Net) -> Result<Vec<Set>, Error> {
    let url = format!("{}/api/registry/sets", net.api_url());
    let body = match net.get(&url, LIMIT) {
        Ok(body) => body,
        // A registry with no sets yet.
        Err(NetError::NotFound { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(Error::from_net(error)),
    };
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Sets {
        List(Vec<Set>),
        Wrapped { sets: Vec<Set> },
    }
    match serde_json::from_slice(&body) {
        Ok(Sets::List(sets) | Sets::Wrapped { sets }) => Ok(sets),
        Err(error) => Err(Error::Registry(format!("the sets: {error}"))),
    }
}

/// The registry's catalog for `port`'s game layout, with absolute package
/// URLs; [`Error::NoCatalog`] when it has none for that layout (a 404).
pub fn catalog(net: &dyn Net, port: &Port) -> Result<Catalog, Error> {
    let url = format!(
        "{}/api/registry/catalog?layout={}",
        net.api_url(),
        query_escape(&port.game_abi)
    );
    let body = net.get(&url, LIMIT).map_err(|error| match error {
        NetError::NotFound { .. } => Error::NoCatalog(port.version.clone()),
        error => Error::from_net(error),
    })?;
    let json = std::str::from_utf8(&body)
        .map_err(|_| Error::Registry("the catalog isn't UTF-8".into()))?;
    let mut catalog =
        Catalog::parse(json).map_err(|error| Error::Registry(format!("the catalog: {error}")))?;
    for entry in &mut catalog.mods {
        entry.package.url = absolute(&url, &entry.package.url);
    }
    Ok(catalog)
}

/// How many mods a page of [`mods`] holds.
pub const PAGE_SIZE: u32 = 50;

/// One page of the registry's mods: official ones first, then most installed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPage {
    /// The tgg-melee release each mod's `targets` are counted for: the
    /// newest one the registry builds mods for. None before it builds any.
    pub port: Option<String>,
    pub items: Vec<ModSummary>,
    /// 1-based.
    pub page: u32,
    pub page_size: u32,
    /// Mods on every page.
    pub total: u64,
    pub total_pages: u32,
}

/// What the registry says of a mod in its list, from the mod's newest
/// approved release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSummary {
    pub id: ModId,
    pub name: String,
    /// One line or so, from the manifest.
    pub description: Option<String>,
    /// The name of the textures.gg account it belongs to. Stores show
    /// [`ModSummary::publisher`].
    pub author: String,
    /// Who publishes it, as a store shows them.
    pub publisher: Publisher,
    pub icon: Option<Image>,
    /// From [`crate::manifest::TAGS`] when this tgg was built; the registry
    /// may know newer ones.
    pub tags: Vec<String>,
    /// How many times its packages were downloaded: installs and updates.
    pub installs: u64,
    /// The newest approved release.
    pub version: Version,
    /// When that release was approved, as RFC 3339.
    pub updated_at: String,
    /// The names of the sets that list it.
    pub sets: Vec<String>,
    /// The systems that release has builds for on [`ModPage::port`].
    pub targets: Vec<System>,
    /// What that release counts as for netplay; None before it has a build.
    pub netplay: Option<Netplay>,
}

/// Who publishes a mod: the name textures.gg's admins gave it (first-party
/// mods say "textures.gg"), or else its author's account name; and whether
/// the admins vouch for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publisher {
    pub name: String,
    pub official: bool,
}

/// A mod's page in a store: its [`ModSummary`] and the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    #[serde(flatten)]
    pub summary: ModSummary,
    /// As in [`ModPage::port`].
    pub port: Option<String>,
    /// The README at the newest release, as markdown. Relative links and
    /// images in it point into the mod's source, which isn't served.
    pub readme: Option<String>,
    pub screenshots: Vec<Image>,
    pub homepage: Option<String>,
    /// The mod's source repository, when it is public.
    pub source: Option<String>,
    pub license: Option<String>,
    /// Every approved release, newest first.
    pub releases: Vec<ModRelease>,
    /// What the newest release depends on.
    pub depends: Vec<SetMod>,
}

/// An icon or screenshot, served by the registry under a URL that never
/// changes what it serves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// Absolute.
    pub url: String,
    pub width: u32,
    pub height: u32,
}

/// The systems tgg-melee runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum System {
    Linux,
    Windows,
    Macos,
    /// One this tgg doesn't know yet.
    #[serde(other)]
    Other,
}

impl System {
    /// The system this tgg runs on, if tgg-melee runs on it.
    pub fn this() -> Option<Self> {
        match std::env::consts::OS {
            "linux" => Some(Self::Linux),
            "windows" => Some(Self::Windows),
            "macos" => Some(Self::Macos),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRelease {
    pub version: Version,
    /// When it was approved, as RFC 3339.
    pub date: String,
    /// Its section of the mod's CHANGELOG.md, as markdown.
    pub changelog: Option<String>,
}

/// Page `page` (from 1) of the registry's mods, [`PAGE_SIZE`] to a page; a
/// page past the last has no items.
pub fn mods(net: &dyn Net, page: u32) -> Result<ModPage, Error> {
    let url = format!(
        "{}/api/registry/mods?page={}&pageSize={PAGE_SIZE}",
        net.api_url(),
        page.max(1)
    );
    let body = net.get(&url, LIMIT).map_err(Error::from_net)?;
    let mut page: ModPage = serde_json::from_slice(&body)
        .map_err(|error| Error::Registry(format!("the mods: {error}")))?;
    for summary in &mut page.items {
        resolve_icon(&url, summary);
    }
    Ok(page)
}

/// The registry's page for the mod `id`; [`Error::NoSuchMod`] when it lists
/// none by that id.
pub fn mod_details(net: &dyn Net, id: &ModId) -> Result<ModDetails, Error> {
    let url = format!("{}/api/registry/mods/{id}", net.api_url());
    let body = net.get(&url, LIMIT).map_err(|error| match error {
        NetError::NotFound { .. } => Error::NoSuchMod(id.clone()),
        error => Error::from_net(error),
    })?;
    let mut details: ModDetails = serde_json::from_slice(&body)
        .map_err(|error| Error::Registry(format!("the mod {id}: {error}")))?;
    resolve_icon(&url, &mut details.summary);
    for image in &mut details.screenshots {
        image.url = absolute(&url, &image.url);
    }
    Ok(details)
}

fn resolve_icon(base: &str, summary: &mut ModSummary) {
    if let Some(icon) = &mut summary.icon {
        icon.url = absolute(base, &icon.url);
    }
}

/// `url` resolved against the URL `base` it came from.
fn absolute(base: &str, url: &str) -> String {
    if url.contains("://") {
        return url.to_owned();
    }
    if let Some(path) = url.strip_prefix('/') {
        // The origin of an http(s) base; a folder base keeps the path.
        return match base.split_once("://") {
            Some((scheme, rest)) => {
                let host = rest.split('/').next().unwrap_or(rest);
                format!("{scheme}://{host}/{path}")
            }
            None => url.to_owned(),
        };
    }
    let base = base.split('?').next().unwrap_or(base);
    match base.rfind('/') {
        Some(slash) => format!("{}/{url}", &base[..slash]),
        None => url.to_owned(),
    }
}

fn query_escape(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_urls_resolve_against_the_catalog() {
        let base = "https://api.textures.gg/api/registry/catalog?layout=a/b";
        assert_eq!(
            absolute(base, "../packages/x.zip"),
            "https://api.textures.gg/api/registry/../packages/x.zip"
        );
        assert_eq!(
            absolute(base, "/packages/x.zip"),
            "https://api.textures.gg/packages/x.zip"
        );
        assert_eq!(
            absolute(base, "https://cdn.example/x.zip"),
            "https://cdn.example/x.zip"
        );
    }
}
