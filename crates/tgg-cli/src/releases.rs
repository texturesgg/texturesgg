//! tgg-melee's releases: where they are published, what each holds, and
//! downloading and unpacking their archives.
//!
//! Each release is a folder of the mirror, `<version>/`, holding
//! `release.json` and the archives it names; `latest.json` names the newest
//! release. Every archive is checked against the size and SHA-256 its
//! `release.json` gives before it is unpacked.

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Where releases are published, unless `TGG_RELEASES` names another mirror
/// (a URL, or a folder laid out the same way).
pub const DEFAULT_MIRROR: &str = "https://dl.textures.gg/tgg-melee";

/// The only build tgg-melee releases so far.
pub const TARGET: &str = "x86_64-linux-gnu";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub commit: String,
    pub api: String,
    pub api_version: String,
    pub game_abi: String,
    pub target: String,
    pub glibc: String,
    pub toolchain: Toolchain,
    pub files: ReleaseFiles,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Toolchain {
    pub gcc: String,
    /// The nixpkgs revision whose GCC built the game.
    pub nixpkgs: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseFiles {
    pub game: ReleaseFile,
    pub sdk: ReleaseFile,
    pub debug: ReleaseFile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

impl ReleaseFile {
    /// The folder at the top of the archive: its name without `.tar.gz`,
    /// and without `-debug` for the debug archive, whose folder is the
    /// game's so it unpacks over it.
    fn top(&self) -> Result<&str> {
        let stem = self
            .name
            .strip_suffix(".tar.gz")
            .ok_or_else(|| anyhow!("{} is not a .tar.gz", self.name))?;
        Ok(stem.strip_suffix("-debug").unwrap_or(stem))
    }
}

#[derive(Deserialize)]
struct Latest {
    version: String,
}

/// A release mirror.
pub struct Mirror {
    base: String,
    agent: ureq::Agent,
}

impl Mirror {
    pub fn new() -> Self {
        let base = std::env::var("TGG_RELEASES")
            .ok()
            .filter(|base| !base.is_empty())
            .unwrap_or_else(|| DEFAULT_MIRROR.to_owned());
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("tgg/", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            base: base.trim_end_matches('/').to_owned(),
            agent,
        }
    }

    fn is_remote(&self) -> bool {
        self.base.starts_with("https://") || self.base.starts_with("http://")
    }

    /// Open `path` of the mirror for reading.
    fn open(&self, path: &str) -> Result<Box<dyn Read>> {
        let location = format!("{}/{path}", self.base);
        if !self.is_remote() {
            let file = std::fs::File::open(&location).with_context(|| location.clone())?;
            return Ok(Box::new(file));
        }
        let response = self
            .agent
            .get(&location)
            .call()
            .map_err(|error| match error {
                ureq::Error::StatusCode(404) => anyhow!("{location} doesn't exist"),
                error => anyhow!("downloading {location}: {error}"),
            })?;
        Ok(Box::new(response.into_body().into_reader()))
    }

    fn json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut text = String::new();
        self.open(path)?
            .take(1024 * 1024)
            .read_to_string(&mut text)
            .with_context(|| format!("reading {path}"))?;
        serde_json::from_str(&text).with_context(|| format!("reading {path}"))
    }

    /// The newest release's version.
    pub fn latest(&self) -> Result<String> {
        Ok(self.json::<Latest>("latest.json")?.version)
    }

    /// `version`'s release, or the newest for `latest`.
    pub fn release(&self, version: &str) -> Result<Release> {
        let version = if version == "latest" {
            self.latest()?
        } else {
            version.trim_start_matches('v').to_owned()
        };
        let release: Release = self
            .json(&format!("{version}/release.json"))
            .with_context(|| format!("no tgg-melee {version} in {}", self.base))?;
        ensure!(
            release.version == version,
            "{version}/release.json is for version {}",
            release.version
        );
        ensure!(
            release.api == tgg_mod::API,
            "tgg-melee {version} loads {} mods; this tgg handles {} mods. Update tgg.",
            release.api,
            tgg_mod::API
        );
        Ok(release)
    }

    /// Download `file` of `release` into the downloads cache, unless a copy
    /// with the right hash is there, and return its path.
    pub fn download(&self, release: &Release, file: &ReleaseFile) -> Result<PathBuf> {
        let folder = crate::paths::cache()?.join(&release.version);
        let path = folder.join(&file.name);
        if path.is_file() && hash_file(&path)? == (file.size, file.sha256.clone()) {
            return Ok(path);
        }
        std::fs::create_dir_all(&folder).with_context(|| folder.display().to_string())?;
        eprintln!(
            "Downloading {} ({:.1} MB)",
            file.name,
            file.size as f64 / 1_000_000.0
        );
        let partial = folder.join(format!("{}.partial", file.name));
        let mut reader = self
            .open(&format!("{}/{}", release.version, file.name))?
            .take(file.size + 1);
        let mut out =
            std::fs::File::create(&partial).with_context(|| partial.display().to_string())?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let mut buffer = vec![0; 1 << 16];
        loop {
            let read = reader
                .read(&mut buffer)
                .with_context(|| format!("downloading {}", file.name))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            out.write_all(&buffer[..read])?;
            size += read as u64;
        }
        out.flush()?;
        let sha256 = hex(&hasher.finalize());
        if size != file.size || sha256 != file.sha256 {
            let _ = std::fs::remove_file(&partial);
            bail!(
                "{} doesn't match its release.json ({size} bytes, SHA-256 {sha256}); try again",
                file.name
            );
        }
        std::fs::rename(&partial, &path)?;
        Ok(path)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = std::fs::File::open(path).with_context(|| path.display().to_string())?;
    let mut hasher = Sha256::new();
    let size = std::io::copy(&mut file, &mut hasher)?;
    Ok((size, hex(&hasher.finalize())))
}

/// Unpack the archive at `path`, whose entries all sit in the folder `top`
/// of `file`, into `dest`, without that folder. Only plain files and folders
/// are taken, and none may leave `dest`.
pub fn unpack(path: &Path, file: &ReleaseFile, dest: &Path) -> Result<()> {
    let top = file.top()?;
    let archive = std::fs::File::open(path).with_context(|| path.display().to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    std::fs::create_dir_all(dest).with_context(|| dest.display().to_string())?;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.into_owned();
        let relative = name
            .strip_prefix(top)
            .map_err(|_| anyhow!("{} holds {}, outside {top}/", file.name, name.display()))?
            .to_owned();
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "{} holds {}",
            file.name,
            name.display()
        );
        let target = dest.join(&relative);
        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&target).with_context(|| target.display().to_string())?;
            }
            tar::EntryType::Regular => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                entry
                    .unpack(&target)
                    .with_context(|| target.display().to_string())?;
            }
            other => bail!(
                "{} holds {} of type {other:?}; releases hold only files and folders",
                file.name,
                name.display()
            ),
        }
    }
    Ok(())
}

/// Install `file` of `release` into `dest`, unpacked beside it and moved in
/// once whole, so an interrupted install leaves nothing behind.
pub fn install(mirror: &Mirror, release: &Release, file: &ReleaseFile, dest: &Path) -> Result<()> {
    let archive = mirror.download(release, file)?;
    let parent = dest.parent().expect("an install folder has a parent");
    let name = dest
        .file_name()
        .expect("an install folder has a name")
        .to_string_lossy();
    let staging = parent.join(format!(".{name}.partial"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    unpack(&archive, file, &staging)?;
    std::fs::rename(&staging, dest).with_context(|| dest.display().to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release archive of `entries`, each a path and an entry type.
    fn archive(dir: &Path, entries: &[(&str, tar::EntryType)]) -> PathBuf {
        let path = dir.join("archive.tar.gz");
        let file = std::fs::File::create(&path).expect("create");
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::fast(),
        ));
        for (name, kind) in entries {
            let mut header = tar::Header::new_gnu();
            // Written raw, so a hostile path reaches the reader as it would.
            header.as_gnu_mut().expect("gnu").name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(*kind);
            let data: &[u8] = if *kind == tar::EntryType::Regular {
                b"hi"
            } else {
                b""
            };
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, data).expect("append");
        }
        builder
            .into_inner()
            .expect("finish")
            .finish()
            .expect("gzip");
        path
    }

    #[test]
    fn unpacking_takes_only_files_and_folders_inside_the_release_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = ReleaseFile {
            name: "tgg-melee-0.1.0-x86_64-linux.tar.gz".into(),
            size: 0,
            sha256: String::new(),
        };
        let top = "tgg-melee-0.1.0-x86_64-linux";
        let good = archive(
            dir.path(),
            &[
                (top, tar::EntryType::Directory),
                (&format!("{top}/lib/libz.so.1"), tar::EntryType::Regular),
            ],
        );
        unpack(&good, &file, &dir.path().join("ok")).expect("unpack");
        assert!(dir.path().join("ok/lib/libz.so.1").is_file());

        for (name, kind) in [
            (format!("{top}/../escape"), tar::EntryType::Regular),
            ("elsewhere/x".to_owned(), tar::EntryType::Regular),
            (format!("{top}/link"), tar::EntryType::Symlink),
        ] {
            let bad = archive(dir.path(), &[(&name, kind)]);
            assert!(
                unpack(&bad, &file, &dir.path().join("bad")).is_err(),
                "{name}"
            );
        }
        assert!(!dir.path().join("escape").exists());
    }
}
