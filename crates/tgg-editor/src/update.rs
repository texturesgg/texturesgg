//! The update check: the one call the app makes on its own,
//! once at launch, and Settings turns it off. It reads the latest version
//! from a small file beside the site's assets and, when that is newer, the
//! top bar links to the download page. Nothing is downloaded or installed.

use crate::net;

/// Where a release publishes its version: `{"version": "0.2.0"}`.
const LATEST_URL: &str = "https://assets.textures.gg/editor/latest.json";
/// Where the player gets it.
pub(crate) const DOWNLOAD_URL: &str = "https://textures.gg/download";

/// The latest version's file, or the one `TGG_UPDATE_URL` names.
fn latest_url() -> String {
    std::env::var("TGG_UPDATE_URL").unwrap_or_else(|_| LATEST_URL.into())
}

/// The version published at `url` when it is newer than this app.
fn check_at(url: &str) -> Result<Option<String>, String> {
    #[derive(serde::Deserialize)]
    struct Latest {
        version: String,
    }
    let body = net::get(url, 4096).map_err(|error| error.to_string())?;
    let latest: Latest = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    Ok(newer(&latest.version, env!("CARGO_PKG_VERSION")).then_some(latest.version))
}

/// A newer version than this app, when one is published. A failed check
/// is logged and is no news.
pub(crate) fn check() -> Option<String> {
    check_at(&latest_url())
        .inspect_err(|error| crate::log(&format!("update check failed: {error}")))
        .ok()
        .flatten()
}

/// Whether `latest` is a later `major.minor.patch` than `current`. A
/// version that isn't three numbers is never newer.
fn newer(latest: &str, current: &str) -> bool {
    fn parts(version: &str) -> Option<[u64; 3]> {
        let mut numbers = version.split('.').map(|part| part.parse().ok());
        let parts = [numbers.next()??, numbers.next()??, numbers.next()??];
        numbers.next().is_none().then_some(parts)
    }
    match (parts(latest), parts(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{check_at, newer};
    use crate::net::tests::serve_once;

    #[test]
    fn only_a_later_three_number_version_is_newer() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
        for odd in ["", "2", "1.0", "1.0.0.0", "1.0.0-beta", "v9.9.9", "9.9.x"] {
            assert!(!newer(odd, "0.1.0"), "{odd}");
        }
    }

    #[test]
    fn the_check_reads_the_published_version() {
        let (url, served) = serve_once("200 OK", r#"{"version":"999.0.0"}"#);
        assert_eq!(check_at(&url), Ok(Some("999.0.0".into())));
        served.join().expect("served");

        let (url, served) = serve_once("200 OK", r#"{"version":"0.0.1"}"#);
        assert_eq!(check_at(&url), Ok(None));
        served.join().expect("served");

        let (url, served) = serve_once("404 Not Found", "{}");
        assert!(check_at(&url).is_err());
        served.join().expect("served");
    }
}
