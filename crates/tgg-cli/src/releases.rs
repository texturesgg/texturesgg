//! tgg-melee's releases, through [`tgg_mod::ports`]: the mirror they are
//! published to (`TGG_RELEASES` names another, a URL or a folder laid out
//! the same way), and downloads cached in tgg's downloads folder.

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::AtomicBool;
pub use tgg_mod::ports::{PortRelease as Release, ReleaseFile, system, target};
use tgg_mod::{HttpNet, ports};

/// A release mirror.
pub struct Mirror {
    net: HttpNet,
}

impl Mirror {
    pub fn new() -> Self {
        Self {
            net: HttpNet::new(concat!("tgg/", env!("CARGO_PKG_VERSION"))),
        }
    }

    pub fn net(&self) -> &HttpNet {
        &self.net
    }

    /// `version`'s release for this system, or the newest for `latest`
    /// ([`prerelease_note`] says when that is a prerelease).
    pub fn release(&self, version: &str) -> Result<Release> {
        Ok(ports::port_release(&self.net, version)?)
    }

    /// `file` of `release`, from the downloads cache when a copy with the
    /// right hash is there.
    pub fn download(&self, release: &Release, file: &ReleaseFile) -> Result<Vec<u8>> {
        let cache = crate::paths::cache()?;
        let mut told = false;
        let bytes = ports::fetch_release_file(
            &self.net,
            release,
            file,
            Some(&cache),
            &mut |_, _| {
                if !told {
                    told = true;
                    eprintln!("{}", downloading(file));
                }
            },
            &AtomicBool::new(false),
        )?;
        Ok(bytes)
    }
}

/// What tgg says when `latest` found only a prerelease.
pub fn prerelease_note(release: &Release) -> Option<String> {
    release.prerelease.then(|| {
        format!(
            "tgg-melee {} is the newest prerelease (there's no full release yet)",
            release.version
        )
    })
}

/// What tgg says when it starts downloading `file`.
pub fn downloading(file: &ReleaseFile) -> String {
    format!(
        "Downloading {} ({:.1} MB)",
        file.name,
        file.size as f64 / 1_000_000.0
    )
}

/// Install `file` of `release` into `dest`, unpacked beside it and moved in
/// once whole, so an interrupted install leaves nothing behind.
pub fn install(mirror: &Mirror, release: &Release, file: &ReleaseFile, dest: &Path) -> Result<()> {
    let archive = mirror.download(release, file)?;
    ports::install_archive(&archive, file, dest)?;
    Ok(())
}
