//! `tgg docs`: the mod docs for the installed game, on textures.gg or, when
//! the site can't be reached, the copy in the SDK.
//!
//! The docs live at `https://textures.gg/docs/mods/<major.minor>/<page>`, by
//! the game's minor version, so every release of one minor shares them;
//! `<page>` is a file of the SDK's `docs/` without `.md`.

use anyhow::{Result, ensure};
use std::path::PathBuf;
use std::time::Duration;

const SITE: &str = "https://textures.gg/docs/mods";

/// `major.minor` of a version such as `0.3.2`; anything else is `latest`.
pub fn minor(version: &str) -> String {
    match semver::Version::parse(version) {
        Ok(version) => format!("{}.{}", version.major, version.minor),
        Err(_) => "latest".to_owned(),
    }
}

/// The docs page `page` for the game version `version`.
pub fn url(version: &str, page: &str) -> String {
    format!("{SITE}/{}/{page}", minor(version))
}

fn reachable(url: &str) -> bool {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(4)))
        .max_redirects(0)
        .build()
        .into();
    match agent.head(url).call() {
        Ok(_) | Err(ureq::Error::StatusCode(_)) => true,
        Err(_) => false,
    }
}

pub fn run(page: Option<&str>) -> Result<()> {
    let page = page.unwrap_or("README").trim_end_matches(".md");
    ensure!(
        !page.is_empty()
            && page
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "{page:?} isn't a docs page; pages are named like writing-mods or hooks"
    );
    let installed = match crate::ports::current_version()? {
        Some(_) => Some(crate::ports::resolve(None)?),
        None => None,
    };
    let version = installed
        .as_ref()
        .map(|i| i.version.clone())
        .unwrap_or_else(|| "latest".into());
    let url = url(&version, page);
    if reachable(&url) {
        println!("{url}");
        let _ = open::that_detached(&url);
        return Ok(());
    }
    // Offline: the SDK's own copy, if it's installed.
    let local: Option<PathBuf> = installed.and_then(|installed| {
        let release = installed.release().ok()?;
        let file = crate::paths::sdks()
            .ok()?
            .join(release.game_abi)
            .join("docs")
            .join(format!("{page}.md"));
        file.is_file().then_some(file)
    });
    match local {
        Some(file) => {
            println!("{}", file.display());
            let _ = open::that_detached(&file);
            Ok(())
        }
        None => {
            println!("{url}");
            anyhow::bail!("textures.gg can't be reached, and there's no SDK copy of {page}")
        }
    }
}
