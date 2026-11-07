//! The textures.gg registry, read over its HTTP API: the curated sets, and
//! the catalog of releases built for one game layout.
//!
//! - `GET /api/registry/sets`: a JSON array of [`Set`]s.
//! - `GET /api/registry/catalog?layout=<game layout>`: a [`Catalog`] of every
//!   approved release with a build for that layout, with its `snapshot` id.
//!
//! Package URLs in the catalog may be relative to the catalog's own URL;
//! [`catalog`] returns them absolute.

use crate::catalog::Catalog;
use crate::depends::Range;
use crate::error::Error;
use crate::manifest::ModId;
use crate::net::Net;
use crate::port::Port;
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

/// The registry's sets.
pub fn sets(net: &dyn Net) -> Result<Vec<Set>, Error> {
    let url = format!("{}/api/registry/sets", net.api_url());
    let body = net.get(&url, LIMIT).map_err(Error::from_net)?;
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
/// URLs.
pub fn catalog(net: &dyn Net, port: &Port) -> Result<Catalog, Error> {
    let url = format!(
        "{}/api/registry/catalog?layout={}",
        net.api_url(),
        query_escape(&port.game_abi)
    );
    let body = net.get(&url, LIMIT).map_err(Error::from_net)?;
    let json = std::str::from_utf8(&body)
        .map_err(|_| Error::Registry("the catalog isn't UTF-8".into()))?;
    let mut catalog =
        Catalog::parse(json).map_err(|error| Error::Registry(format!("the catalog: {error}")))?;
    for entry in &mut catalog.mods {
        entry.package.url = absolute(&url, &entry.package.url);
    }
    Ok(catalog)
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
