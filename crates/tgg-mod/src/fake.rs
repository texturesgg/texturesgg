//! [`FakeNet`]: a registry and release mirror in memory, for tests and the
//! app's flow tests, so they can drive installing, updating, turning mods
//! on and off and uninstalling with no network.

use crate::catalog::{self, Catalog, CatalogEntry, PackageRef};
use crate::manifest::Manifest;
use crate::net::{Net, NetError};
use crate::package::{self, Package};
use crate::ports::{self, PortRelease, ReleaseFile, ReleaseFiles, Toolchain};
use crate::registry::{Image, ModDetails, ModPage, ModRelease, ModSummary, Set, SetMod, System};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// A registry and release mirror in memory.
///
/// [`FakeNet::new`] serves:
///
/// - a catalog for the game layout [`FakeNet::LAYOUT`] of the Competitive
///   and Replays mods, each a small package that installs (it ships only an
///   asset, so it fits every game): `tgg.online-rules`,
///   `tgg.competitive-rules`, `tgg.legal-stages` (0.1.0 and 0.1.1),
///   `tgg.skip-results`, `tgg.quick-start`, `tgg.netplay` 0.2.0,
///   `tgg.direct-connect` (depends on `tgg.netplay ^0.2`),
///   `tgg.replay-player` 0.2.0, `tgg.replay-browser` (depends on
///   `tgg.replay-player ^0.2`) and `tgg.replay-recorder`; every other is
///   0.1.0. Any other layout gets a catalog with no builds. Every catalog
///   names every published mod in `ids`.
/// - the sets `competitive` and `replays`, version 1, taking each mod at
///   `^<its version>`, minor version included.
/// - store listings of every mod (`/api/registry/mods`, `/mods/<id>`): each
///   with a 64 by 64 icon of its own color, the tags `competitive` and
///   `netplay` (Competitive) or `replays` and `quality-of-life`
///   (Replays), a README, a changelog per release, and as many installs as
///   its packages have been downloaded from it.
/// - tgg-melee [`FakeNet::PORT_VERSION`] for this system, whose game is a
///   shell script that runs for `$TGG_FAKE_GAME_SECONDS` (default 30)
///   seconds, so it launches on Linux and macOS.
///
/// The `publish_*` methods add to it as a test goes, and
/// [`FakeNet::set_offline`] makes every request fail as if there were no
/// connection.
pub struct FakeNet {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    offline: bool,
    catalog: Vec<CatalogEntry>,
    revision: u32,
    sets: Vec<Set>,
    /// Package zips and mirror files by URL.
    files: BTreeMap<String, Vec<u8>>,
    latest: Option<String>,
    requests: Vec<String>,
    /// Each mod's tags, by id.
    tags: BTreeMap<String, Vec<String>>,
    /// Each mod's package downloads, by id.
    installs: BTreeMap<String, u64>,
}

impl Default for FakeNet {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeNet {
    pub const API: &str = "https://fake.textures.gg";
    pub const RELEASES: &str = "https://fake.textures.gg/tgg-melee";
    /// The game layout the port release has and the catalog serves.
    pub const LAYOUT: &str = "fake-layout";
    pub const PORT_VERSION: &str = "0.5.0";

    /// A registry and mirror with nothing in them.
    pub fn empty() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    /// The registry and mirror described on [`FakeNet`].
    pub fn new() -> Self {
        let net = Self::empty();
        let competitive = [
            ("tgg.online-rules", "Online rules", "0.1.0", None),
            ("tgg.competitive-rules", "Competitive rules", "0.1.0", None),
            ("tgg.legal-stages", "Legal stages", "0.1.0", None),
            ("tgg.legal-stages", "Legal stages", "0.1.1", None),
            ("tgg.skip-results", "Skip results", "0.1.0", None),
            ("tgg.quick-start", "Quick start", "0.1.0", None),
            ("tgg.netplay", "Netplay", "0.2.0", None),
            (
                "tgg.direct-connect",
                "Direct connect",
                "0.1.0",
                Some(("tgg.netplay", "^0.2")),
            ),
        ];
        let replays = [
            ("tgg.replay-player", "Replay player", "0.2.0", None),
            (
                "tgg.replay-browser",
                "Replays",
                "0.1.0",
                Some(("tgg.replay-player", "^0.2")),
            ),
            ("tgg.replay-recorder", "Replay recorder", "0.1.0", None),
        ];
        for (id, name, version, depends) in competitive.iter().chain(&replays) {
            let depends: Vec<(&str, &str)> = depends.iter().copied().collect();
            net.publish_mod(id, name, version, &depends);
        }
        for (id, ..) in &competitive {
            net.set_tags(id, &["competitive", "netplay"]);
        }
        for (id, ..) in &replays {
            net.set_tags(id, &["replays", "quality-of-life"]);
        }
        let set = |name: &str, title: &str, description: &str, mods: &[(&str, &str, &str, _)]| {
            let mut listed: Vec<SetMod> = Vec::new();
            for (id, _, version, _) in mods {
                let id = id.parse().expect("a fake mod id");
                if listed.iter().any(|m| m.id == id) {
                    continue;
                }
                let minor = version.rsplit_once('.').expect("a version").0;
                listed.push(SetMod {
                    id,
                    range: format!("^{minor}").parse().expect("a range"),
                });
            }
            Set {
                name: name.into(),
                title: title.into(),
                description: description.into(),
                version: 1,
                mods: listed,
            }
        };
        net.publish_set(set(
            "competitive",
            "Competitive",
            "Online rules, competitive rules, legal stages, skip results, quick start and netplay.",
            &competitive,
        ));
        net.publish_set(set(
            "replays",
            "Replays",
            "Watch, browse and record replays.",
            &replays,
        ));
        net.publish_port(Self::PORT_VERSION);
        net
    }

    /// Make every request fail as if there were no connection, or stop.
    pub fn set_offline(&self, offline: bool) {
        self.state().offline = offline;
    }

    /// Give the mod `id` the store tags `tags`.
    pub fn set_tags(&self, id: &str, tags: &[&str]) {
        self.state()
            .tags
            .insert(id.into(), tags.iter().map(|&tag| tag.into()).collect());
    }

    /// Every URL requested so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.state().requests.clone()
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Publish `package` in the catalog, replacing the same id and version.
    pub fn publish(&self, package: &Package) {
        let zip = package.to_zip();
        let sha256 = package::sha256_hex(&zip);
        let url = format!("/packages/{sha256}.zip");
        let mut state = self.state();
        state
            .files
            .insert(format!("{}{url}", Self::API), zip.clone());
        let (icon, _) = icon(package.manifest.id.as_str());
        state.files.insert(icon_url(&icon), icon);
        state.catalog.retain(|entry| {
            entry.manifest.id != package.manifest.id
                || entry.manifest.version != package.manifest.version
        });
        state.catalog.push(CatalogEntry {
            manifest: package.manifest.clone(),
            netplay: crate::netplay::classify(&package.manifest),
            package: PackageRef {
                url,
                sha256,
                size: zip.len() as u64,
                signature: None,
            },
        });
        state.revision += 1;
    }

    /// Publish a small package of the mod `id` at `version` that ships one
    /// asset, depending on each `(id, range)` of `depends`.
    pub fn publish_mod(
        &self,
        id: &str,
        name: &str,
        version: &str,
        depends: &[(&str, &str)],
    ) -> Package {
        let mut manifest = Manifest::new(
            id.parse().expect("a valid id"),
            name.into(),
            version.parse().expect("a valid version"),
        );
        manifest.description = Some(format!("{name}, for tests."));
        for (dep, range) in depends {
            manifest.depends.insert(
                dep.parse().expect("a valid id"),
                range.parse().expect("a valid range"),
            );
        }
        let assets = BTreeMap::from([(
            "about.txt".to_owned(),
            format!("{name} {version}\n").into_bytes(),
        )]);
        let package = Package::pack(
            manifest,
            None,
            Default::default(),
            assets,
            Default::default(),
        )
        .expect("a fake package packs");
        self.publish(&package);
        package
    }

    /// Publish `set`, replacing the set of the same name.
    pub fn publish_set(&self, set: Set) {
        let mut state = self.state();
        state.sets.retain(|have| have.name != set.name);
        state.sets.push(set);
    }

    /// The set `name` as served.
    pub fn set(&self, name: &str) -> Option<Set> {
        self.state()
            .sets
            .iter()
            .find(|set| set.name == name)
            .cloned()
    }

    /// Publish tgg-melee `version` for this system with the layout
    /// [`FakeNet::LAYOUT`], and make it the newest release.
    pub fn publish_port(&self, version: &str) {
        let target = ports::target().unwrap_or("x86_64-linux-gnu");
        let top = format!("tgg-melee-{version}-{target}");
        let game = tarball(&top, &[(ports::EXECUTABLE, FAKE_GAME.as_bytes(), 0o755)]);
        let sdk = tarball(
            &format!("{top}-sdk"),
            &[("README.txt", b"A fake SDK.\n".as_slice(), 0o644)],
        );
        let file = |name: String, bytes: &[u8]| ReleaseFile {
            name,
            size: bytes.len() as u64,
            sha256: package::sha256_hex(bytes),
        };
        let release = PortRelease {
            version: version.into(),
            commit: "0".repeat(40),
            api: crate::API.into(),
            api_version: "0.1".into(),
            game_abi: Self::LAYOUT.into(),
            target: target.into(),
            glibc: "2.31".into(),
            toolchain: Toolchain {
                gcc: "14".into(),
                nixpkgs: "fake".into(),
            },
            files: ReleaseFiles {
                game: file(format!("{top}.tar.gz"), &game),
                sdk: file(format!("{top}-sdk.tar.gz"), &sdk),
                debug: None,
            },
            prerelease: false,
        };
        let base = format!("{}/{version}", Self::RELEASES);
        let mut state = self.state();
        state
            .files
            .insert(format!("{base}/{}", release.files.game.name), game);
        state
            .files
            .insert(format!("{base}/{}", release.files.sdk.name), sdk);
        state.files.insert(
            format!("{base}/release-{target}.json"),
            serde_json::to_vec_pretty(&release).expect("a release serializes"),
        );
        state.latest = Some(version.into());
    }

    /// Every mod's summary, most installed first, as the registry lists them.
    fn summaries(state: &State) -> Vec<ModSummary> {
        let mut newest: BTreeMap<&str, &CatalogEntry> = BTreeMap::new();
        for entry in &state.catalog {
            let id = entry.manifest.id.as_str();
            if newest
                .get(id)
                .is_none_or(|seen| seen.manifest.version < entry.manifest.version)
            {
                newest.insert(id, entry);
            }
        }
        let mut summaries: Vec<ModSummary> = newest
            .into_values()
            .map(|entry| {
                let id = entry.manifest.id.as_str();
                let (bytes, size) = icon(id);
                ModSummary {
                    id: entry.manifest.id.clone(),
                    name: entry.manifest.name.clone(),
                    description: entry.manifest.description.clone(),
                    author: "textures.gg".into(),
                    icon: Some(Image {
                        url: icon_url(&bytes),
                        width: size,
                        height: size,
                    }),
                    tags: state.tags.get(id).cloned().unwrap_or_default(),
                    installs: state.installs.get(id).copied().unwrap_or(0),
                    version: entry.manifest.version.clone(),
                    updated_at: RELEASED.into(),
                    sets: state
                        .sets
                        .iter()
                        .filter(|set| set.range(&entry.manifest.id).is_some())
                        .map(|set| set.name.clone())
                        .collect(),
                    targets: System::this().into_iter().collect(),
                    netplay: Some(entry.netplay),
                }
            })
            .collect();
        summaries.sort_by(|a, b| b.installs.cmp(&a.installs).then_with(|| a.id.cmp(&b.id)));
        summaries
    }

    fn mods_json(state: &State, query: &str) -> Vec<u8> {
        let param = |name: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
                .and_then(|value| value.parse::<u32>().ok())
        };
        let page = param("page").unwrap_or(1).max(1);
        let page_size = param("pageSize").unwrap_or(50).clamp(1, 100);
        let all = Self::summaries(state);
        let total = all.len() as u64;
        let items = all
            .into_iter()
            .skip(((page - 1) * page_size) as usize)
            .take(page_size as usize)
            .collect();
        let page = ModPage {
            port: Some(Self::PORT_VERSION.into()),
            items,
            page,
            page_size,
            total,
            total_pages: total.div_ceil(page_size as u64) as u32,
        };
        serde_json::to_vec_pretty(&page).expect("a page serializes")
    }

    fn details_json(state: &State, id: &str) -> Option<Vec<u8>> {
        let summary = Self::summaries(state)
            .into_iter()
            .find(|summary| summary.id.as_str() == id)?;
        let mut releases: Vec<&CatalogEntry> = state
            .catalog
            .iter()
            .filter(|entry| entry.manifest.id.as_str() == id)
            .collect();
        releases.sort_by(|a, b| b.manifest.version.cmp(&a.manifest.version));
        let newest = releases[0];
        let details = ModDetails {
            port: Some(Self::PORT_VERSION.into()),
            readme: Some(format!(
                "# {}\n\n{}\n",
                summary.name,
                summary.description.as_deref().unwrap_or("")
            )),
            screenshots: Vec::new(),
            homepage: None,
            source: None,
            license: newest.manifest.license.clone(),
            releases: releases
                .iter()
                .map(|entry| ModRelease {
                    version: entry.manifest.version.clone(),
                    date: RELEASED.into(),
                    changelog: Some(format!("- Version {}.", entry.manifest.version)),
                })
                .collect(),
            depends: newest
                .manifest
                .depends
                .iter()
                .map(|(id, range)| SetMod {
                    id: id.clone(),
                    range: range.clone(),
                })
                .collect(),
            summary,
        };
        Some(serde_json::to_vec_pretty(&details).expect("details serialize"))
    }

    fn catalog_json(state: &State, layout: &str) -> Vec<u8> {
        let catalog = Catalog {
            schema: catalog::SCHEMA,
            snapshot: Some(format!("fake-{}", state.revision)),
            mods: if layout == Self::LAYOUT {
                state.catalog.clone()
            } else {
                Vec::new()
            },
            ids: {
                let mut ids: Vec<_> = state
                    .catalog
                    .iter()
                    .map(|entry| entry.manifest.id.clone())
                    .collect();
                ids.sort();
                ids.dedup();
                ids
            },
        };
        catalog.to_json().into_bytes()
    }
}

/// When every fake release was approved.
const RELEASED: &str = "2026-11-01T00:00:00.000Z";

/// The fake icon of the mod `id` and its size: a square of a color of
/// its own.
fn icon(id: &str) -> (Vec<u8>, u32) {
    const SIZE: u32 = 64;
    let hash = crate::package::sha256_hex(id.as_bytes());
    let rgb = u32::from_str_radix(&hash[..6], 16).expect("hex");
    (
        png(
            SIZE,
            SIZE,
            rgb.to_be_bytes()[1..].try_into().expect("3 bytes"),
        ),
        SIZE,
    )
}

fn icon_url(png: &[u8]) -> String {
    format!(
        "{}/api/registry/images/{}.png",
        FakeNet::API,
        crate::package::sha256_hex(png)
    )
}

/// A PNG `width` by `height` of the color `rgb`.
pub(crate) fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    use std::io::Write;
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = out[start..].iter().fold(!0u32, |crc, &byte| {
            (0..8).fold(crc ^ u32::from(byte), |crc, _| {
                if crc & 1 == 1 {
                    0xedb8_8320 ^ (crc >> 1)
                } else {
                    crc >> 1
                }
            })
        });
        out.extend_from_slice(&(!crc).to_be_bytes());
    }
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8-bit RGB, deflate, no filter, no interlace.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    // Each row: filter type 0 (none), then its pixels.
    let row: Vec<u8> = std::iter::once(0)
        .chain(rgb.iter().copied().cycle().take(3 * width as usize))
        .collect();
    let pixels = row.repeat(height as usize);
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    zlib.write_all(&pixels).expect("write to memory");
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib.finish().expect("write to memory"));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The fake port's game: it runs for a while and ends.
const FAKE_GAME: &str = "#!/bin/sh\n\
i=0\n\
while [ \"$i\" -lt \"${TGG_FAKE_GAME_SECONDS:-30}\" ]; do sleep 1; i=$((i + 1)); done\n";

/// A gzipped tarball of `files` (name, contents, mode) under the folder
/// `top`.
fn tarball(top: &str, files: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    let mut folder = tar::Header::new_gnu();
    folder.set_entry_type(tar::EntryType::Directory);
    folder.set_mode(0o755);
    folder.set_size(0);
    builder
        .append_data(&mut folder, format!("{top}/"), std::io::empty())
        .expect("write to memory");
    for (name, bytes, mode) in files {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mode(*mode);
        header.set_size(bytes.len() as u64);
        builder
            .append_data(&mut header, format!("{top}/{name}"), *bytes)
            .expect("write to memory");
    }
    builder
        .into_inner()
        .expect("write to memory")
        .finish()
        .expect("write to memory")
}

impl Net for FakeNet {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
        let mut state = self.state();
        state.requests.push(url.to_owned());
        if state.offline {
            return Err(NetError::Unreachable {
                url: url.into(),
                reason: "no connection".into(),
            });
        }
        let body = if url == format!("{}/api/registry/sets", Self::API) {
            serde_json::to_vec_pretty(&state.sets).expect("sets serialize")
        } else if let Some(layout) =
            url.strip_prefix(&format!("{}/api/registry/catalog?layout=", Self::API))
        {
            Self::catalog_json(&state, layout)
        } else if let Some(query) = url.strip_prefix(&format!("{}/api/registry/mods?", Self::API)) {
            Self::mods_json(&state, query)
        } else if let Some(id) = url.strip_prefix(&format!("{}/api/registry/mods/", Self::API)) {
            match Self::details_json(&state, id) {
                Some(body) => body,
                None => return Err(NetError::NotFound { url: url.into() }),
            }
        } else if url == format!("{}/latest.json", Self::RELEASES) {
            match &state.latest {
                Some(version) => format!("{{\"version\": \"{version}\"}}").into_bytes(),
                None => return Err(NetError::NotFound { url: url.into() }),
            }
        } else {
            let Some(body) = state.files.get(url).cloned() else {
                return Err(NetError::NotFound { url: url.into() });
            };
            // A package download is an install of its mod.
            let downloaded = state
                .catalog
                .iter()
                .find(|entry| url == format!("{}{}", Self::API, entry.package.url))
                .map(|entry| entry.manifest.id.to_string());
            if let Some(id) = downloaded {
                *state.installs.entry(id).or_default() += 1;
            }
            body
        };
        if body.len() as u64 > limit {
            return Err(NetError::TooLarge {
                url: url.into(),
                limit,
            });
        }
        Ok(body)
    }

    fn api_url(&self) -> &str {
        Self::API
    }

    fn releases_url(&self) -> &str {
        Self::RELEASES
    }
}
