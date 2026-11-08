//! Which publishers' mods a player lets profiles fetch, remembered per
//! publisher in `<data>/tgg/trust.json` and shared by every front end:
//!
//! ```json
//! { "publishers": { "someone": true, "someone-else": false } }
//! ```
//!
//! Official mods never ask.

use crate::error::Error;
use crate::registry::Publisher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FILE: &str = "trust.json";

/// The player's answers, by publisher name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trust {
    #[serde(default)]
    pub publishers: BTreeMap<String, bool>,
}

impl Trust {
    /// The answer remembered for `publisher`, if any.
    pub fn answer(&self, publisher: &str) -> Option<bool> {
        self.publishers.get(publisher).copied()
    }

    /// Whether `publisher`'s mods fetch without asking.
    pub fn allows(&self, publisher: &Publisher) -> bool {
        publisher.official || self.answer(&publisher.name) == Some(true)
    }
}

fn path(data: &Path) -> PathBuf {
    crate::cache::tgg_dir(data).join(FILE)
}

/// The answers the player gave, from `<data>/tgg/trust.json`; none before
/// the first.
pub fn trust(data: &Path) -> Result<Trust, Error> {
    let path = path(data);
    match std::fs::read(&path) {
        // A damaged file reads as no answers: the player is asked again.
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Trust::default()),
        Err(error) => Err(Error::io(path)(error)),
    }
}

/// Remember the player's answer for `publisher`.
pub fn remember_trust(data: &Path, publisher: &str, allow: bool) -> Result<(), Error> {
    let mut trust = trust(data)?;
    trust.publishers.insert(publisher.to_owned(), allow);
    let json = serde_json::to_string_pretty(&trust).expect("trust serializes") + "\n";
    crate::cache::write_atomic(&path(data), json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_remembered_per_publisher() {
        let data = tempfile::tempdir().expect("temp dir");
        let someone = Publisher {
            name: "someone".into(),
            official: false,
        };
        assert!(!trust(data.path()).expect("trust").allows(&someone));
        remember_trust(data.path(), "someone", true).expect("remember");
        remember_trust(data.path(), "other", false).expect("remember");
        let trust = trust(data.path()).expect("trust");
        assert!(trust.allows(&someone));
        assert_eq!(trust.answer("other"), Some(false));
        assert!(trust.allows(&Publisher {
            name: "textures.gg".into(),
            official: true
        }));
    }
}
