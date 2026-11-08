//! What tgg keeps under `<data>/tgg/` so it needn't fetch twice, and so a
//! replay list works offline:
//!
//! - `packages/<sha256>.zip`: every package downloaded, by its SHA-256,
//!   shared by [`crate::apply`] and profiles. A cached package is checked
//!   against its SHA-256 before use, and its time touched, so
//!   [`prune_packages`] keeps what is used.
//! - `cache/catalog-<layout>.json` and `cache/slippi.json`: the last catalog
//!   for each game layout and the last Slippi mapping, as served, with
//!   their `snapshot`.

use crate::catalog::{Catalog, PackageRef};
use crate::error::Error;
use crate::lock::Lock;
use crate::manifest::ModId;
use crate::net::Net;
use crate::port::Port;
use crate::profile::{self, SlippiMapping};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

/// The package cache's folder in `<data>/tgg`.
pub const PACKAGES: &str = "packages";
const CACHE: &str = "cache";
const MAPPING: &str = "slippi.json";
/// The largest package downloaded.
pub(crate) const PACKAGE_LIMIT: u64 = 1024 * 1024 * 1024;

/// `<data>/tgg`.
pub fn tgg_dir(data: &Path) -> PathBuf {
    data.join("tgg")
}

/// `<data>/tgg/packages`.
pub fn packages_dir(data: &Path) -> PathBuf {
    tgg_dir(data).join(PACKAGES)
}

/// Write `bytes` to `path` in one step, making its folder.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let folder = path.parent().expect("a file has a folder");
    std::fs::create_dir_all(folder).map_err(Error::io(folder))?;
    let name = path
        .file_name()
        .expect("a file has a name")
        .to_string_lossy();
    let staging = folder.join(format!(".{name}.{}.new", std::process::id()));
    std::fs::write(&staging, bytes).map_err(Error::io(&staging))?;
    std::fs::rename(&staging, path).map_err(Error::io(path))
}

/// A SHA-256 as the registry gives it: 64 lower-case hex digits, which
/// alone name a file in the cache.
fn valid_sha256(sha256: &str) -> bool {
    sha256.len() == 64
        && sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn package_path(packages: &Path, sha256: &str) -> Option<PathBuf> {
    valid_sha256(sha256).then(|| packages.join(format!("{sha256}.zip")))
}

/// Whether `package` is in the cache `packages`, by its size (cheap enough
/// for a list; [`fetch_package`] checks the hash).
pub(crate) fn is_cached(packages: &Path, package: &PackageRef) -> bool {
    package_path(packages, &package.sha256)
        .and_then(|path| std::fs::metadata(path).ok())
        .is_some_and(|meta| meta.len() == package.size)
}

fn verified(bytes: &[u8], package: &PackageRef) -> bool {
    bytes.len() as u64 == package.size && crate::package::sha256_hex(bytes) == package.sha256
}

/// The bytes of the package of `id`, from the cache `packages` when it has
/// them right, else downloaded, checked against the SHA-256 and kept there.
pub(crate) fn fetch_package(
    net: &dyn Net,
    packages: Option<&Path>,
    id: &ModId,
    package: &PackageRef,
    on_bytes: &mut dyn FnMut(u64, Option<u64>),
    cancel: &AtomicBool,
) -> Result<Vec<u8>, Error> {
    let cached = packages.and_then(|packages| package_path(packages, &package.sha256));
    if let Some(path) = &cached
        && let Ok(bytes) = std::fs::read(path)
    {
        if verified(&bytes, package) {
            // Used now: pruning goes by this time.
            if let Ok(file) = std::fs::File::options().append(true).open(path) {
                let _ = file.set_modified(SystemTime::now());
            }
            on_bytes(package.size, Some(package.size));
            return Ok(bytes);
        }
        let _ = std::fs::remove_file(path);
    }
    let bytes = net
        .download(
            &package.url,
            package.size.min(PACKAGE_LIMIT),
            on_bytes,
            cancel,
        )
        .map_err(Error::from_net)?;
    if !verified(&bytes, package) {
        return Err(Error::Checksum {
            id: id.clone(),
            expected: package.sha256.clone(),
        });
    }
    if let Some(path) = &cached {
        // A cache that can't be written costs a download next time, no more.
        let _ = write_atomic(path, &bytes);
    }
    Ok(bytes)
}

fn file_name_part(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

fn catalog_path(data: &Path, layout: &str) -> PathBuf {
    tgg_dir(data)
        .join(CACHE)
        .join(format!("catalog-{}.json", file_name_part(layout)))
}

/// Keep `catalog` as the last one seen for the game layout `layout`.
pub fn save_catalog(data: &Path, layout: &str, catalog: &Catalog) -> Result<(), Error> {
    write_atomic(&catalog_path(data, layout), catalog.to_json().as_bytes())
}

/// The last catalog seen for `layout`, if any.
pub fn cached_catalog(data: &Path, layout: &str) -> Option<Catalog> {
    let json = std::fs::read_to_string(catalog_path(data, layout)).ok()?;
    Catalog::parse(&json).ok()
}

/// The registry's catalog for `port`'s layout ([`crate::catalog()`]), kept
/// as the last one seen.
pub fn refresh_catalog(net: &dyn Net, data: &Path, port: &Port) -> Result<Catalog, Error> {
    let catalog = crate::registry::catalog(net, port)?;
    save_catalog(data, &port.game_abi, &catalog)?;
    Ok(catalog)
}

fn mapping_path(data: &Path) -> PathBuf {
    tgg_dir(data).join(CACHE).join(MAPPING)
}

/// Keep `mapping` as the last one seen.
pub fn save_mapping(data: &Path, mapping: &SlippiMapping) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(mapping).expect("a mapping serializes") + "\n";
    write_atomic(&mapping_path(data), json.as_bytes())
}

/// The last Slippi mapping seen, if any.
pub fn cached_mapping(data: &Path) -> Option<SlippiMapping> {
    let bytes = std::fs::read(mapping_path(data)).ok()?;
    profile::parse_mapping(&bytes).ok()
}

/// The registry's Slippi mapping ([`crate::slippi_mapping`]), kept as the
/// last one seen.
pub fn refresh_mapping(net: &dyn Net, data: &Path) -> Result<SlippiMapping, Error> {
    let mapping = profile::slippi_mapping(net)?;
    save_mapping(data, &mapping)?;
    Ok(mapping)
}

/// Remove each cached package that neither the lock in `mods` nor a
/// profile folder names and that hasn't been used for `older_than`. The
/// paths removed.
pub fn prune_packages(
    data: &Path,
    mods: &Path,
    older_than: Duration,
) -> Result<Vec<PathBuf>, Error> {
    let packages = packages_dir(data);
    let entries = match std::fs::read_dir(&packages) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::io(packages)(error)),
    };
    let mut named: Vec<String> = Lock::read(mods)
        .map(|lock| lock.mods.into_values().filter_map(|e| e.sha256).collect())
        .unwrap_or_default();
    named.extend(crate::prepare::profile_packages(data));
    let now = SystemTime::now();
    let mut removed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(Error::io(&packages))?;
        let path = entry.path();
        let Some(sha256) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".zip"))
        else {
            continue;
        };
        if named.iter().any(|named| named == sha256) {
            continue;
        }
        let used = entry.metadata().and_then(|meta| meta.modified()).ok();
        let old = used
            .and_then(|used| now.duration_since(used).ok())
            .is_some_and(|age| age >= older_than);
        if old {
            std::fs::remove_file(&path).map_err(Error::io(&path))?;
            removed.push(path);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeNet;

    #[test]
    fn a_cached_package_is_used_and_a_damaged_one_fetched_again() {
        let net = FakeNet::new();
        let data = tempfile::tempdir().expect("temp dir");
        let packages = packages_dir(data.path());
        let catalog = crate::registry::catalog(
            &net,
            &Port {
                executable: PathBuf::new(),
                game_abi: FakeNet::LAYOUT.into(),
                name: "tgg-melee".into(),
                target: String::new(),
                version: FakeNet::PORT_VERSION.into(),
            },
        )
        .expect("catalog");
        let entry = &catalog.mods[0];
        let id = &entry.manifest.id;
        let go = AtomicBool::new(false);
        let fetch = || {
            fetch_package(
                &net,
                Some(&packages),
                id,
                &entry.package,
                &mut |_, _| {},
                &go,
            )
        };
        let bytes = fetch().expect("download");
        assert!(is_cached(&packages, &entry.package));
        let downloads = || {
            net.requests()
                .iter()
                .filter(|url| **url == entry.package.url)
                .count()
        };
        assert_eq!(downloads(), 1);
        assert_eq!(fetch().expect("cached"), bytes);
        assert_eq!(downloads(), 1);
        let path = packages.join(format!("{}.zip", entry.package.sha256));
        std::fs::write(&path, b"damaged").expect("damage");
        assert_eq!(fetch().expect("again"), bytes);
        assert_eq!(downloads(), 2);
    }

    #[test]
    fn pruning_keeps_what_the_lock_names_and_what_is_recent() {
        let data = tempfile::tempdir().expect("temp dir");
        let mods = data.path().join("mods");
        let packages = packages_dir(data.path());
        std::fs::create_dir_all(&packages).expect("mkdir");
        let (locked, loose) = ("a".repeat(64), "b".repeat(64));
        for sha in [&locked, &loose] {
            std::fs::write(packages.join(format!("{sha}.zip")), b"zip").expect("write");
        }
        let mut lock = Lock::default();
        lock.mods.insert(
            "tgg.x".parse().expect("id"),
            crate::lock::LockEntry {
                version: semver::Version::new(1, 0, 0),
                sha256: Some(locked.clone()),
                sets: Vec::new(),
                direct: true,
            },
        );
        std::fs::create_dir_all(&mods).expect("mkdir");
        lock.write(&mods).expect("lock");
        let week = Duration::from_secs(7 * 24 * 3600);
        assert!(
            prune_packages(data.path(), &mods, week)
                .expect("prune")
                .is_empty()
        );
        let removed = prune_packages(data.path(), &mods, Duration::ZERO).expect("prune");
        assert_eq!(removed, [packages.join(format!("{loose}.zip"))]);
    }

    #[test]
    fn catalogs_and_mappings_are_kept_for_offline_use() {
        let net = FakeNet::new();
        let data = tempfile::tempdir().expect("temp dir");
        assert!(cached_mapping(data.path()).is_none());
        let mapping = refresh_mapping(&net, data.path()).expect("mapping");
        assert_eq!(cached_mapping(data.path()), Some(mapping));
        let port = Port {
            executable: PathBuf::new(),
            game_abi: FakeNet::LAYOUT.into(),
            name: "tgg-melee".into(),
            target: String::new(),
            version: FakeNet::PORT_VERSION.into(),
        };
        let catalog = refresh_catalog(&net, data.path(), &port).expect("catalog");
        assert_eq!(cached_catalog(data.path(), FakeNet::LAYOUT), Some(catalog));
        assert!(cached_catalog(data.path(), "other").is_none());
    }
}
