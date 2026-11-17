//! Which publishers a player lets profiles (a lobby's or a replay's mods)
//! fetch native code from, remembered per publisher account id in
//! `<data>/tgg/trust.json` and shared by every front end:
//!
//! ```json
//! { "ids": { "4fJk…": true, "9aQ2…": false } }
//! ```
//!
//! Only native mods (a library the game loads) ask: scripts run in the
//! game's sandbox and files carry no code. Official mods never ask. A
//! publisher the registry gives no id is asked about every time. Answers
//! remembered by name before ids existed are ignored, so those players are
//! asked once more.

use crate::error::Error;
use crate::registry::Publisher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FILE: &str = "trust.json";

/// The player's answers, by publisher account id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trust {
    #[serde(default)]
    pub ids: BTreeMap<String, bool>,
}

impl Trust {
    /// The answer remembered for the publisher account `id`, if any.
    pub fn answer(&self, id: &str) -> Option<bool> {
        self.ids.get(id).copied()
    }

    /// Whether `publisher`'s native mods fetch without asking.
    pub fn allows(&self, publisher: &Publisher) -> bool {
        publisher.official
            || publisher
                .id
                .as_deref()
                .is_some_and(|id| self.answer(id) == Some(true))
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

/// Remember the player's answer for `publisher`, by its account id; a
/// publisher without one isn't remembered.
pub fn remember_trust(data: &Path, publisher: &Publisher, allow: bool) -> Result<(), Error> {
    let Some(id) = &publisher.id else {
        return Ok(());
    };
    let mut trust = trust(data)?;
    trust.ids.insert(id.clone(), allow);
    let json = serde_json::to_string_pretty(&trust).expect("trust serializes") + "\n";
    crate::cache::write_atomic(&path(data), json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publisher(id: Option<&str>, name: &str) -> Publisher {
        Publisher {
            id: id.map(Into::into),
            name: name.into(),
            official: false,
        }
    }

    #[test]
    fn answers_are_remembered_by_account_id_not_name() {
        let data = tempfile::tempdir().expect("temp dir");
        let someone = publisher(Some("acct-1"), "someone");
        assert!(!trust(data.path()).expect("trust").allows(&someone));
        remember_trust(data.path(), &someone, true).expect("remember");
        remember_trust(data.path(), &publisher(Some("acct-2"), "other"), false).expect("remember");
        let trust = trust(data.path()).expect("trust");
        assert!(trust.allows(&someone));
        // A new name on the same account is still trusted; the old name on
        // another account isn't.
        assert!(trust.allows(&publisher(Some("acct-1"), "renamed")));
        assert!(!trust.allows(&publisher(Some("acct-3"), "someone")));
        assert_eq!(trust.answer("acct-2"), Some(false));
        assert!(trust.allows(&Publisher {
            id: None,
            name: "textures.gg".into(),
            official: true
        }));
    }

    #[test]
    fn a_publisher_without_an_id_is_asked_every_time() {
        let data = tempfile::tempdir().expect("temp dir");
        let nameless = publisher(None, "someone");
        remember_trust(data.path(), &nameless, true).expect("remember");
        assert!(!trust(data.path()).expect("trust").allows(&nameless));
    }

    #[test]
    fn answers_remembered_by_name_are_ignored() {
        let data = tempfile::tempdir().expect("temp dir");
        let file = path(data.path());
        std::fs::create_dir_all(file.parent().expect("folder")).expect("mkdir");
        std::fs::write(&file, r#"{ "publishers": { "someone": true } }"#).expect("write");
        let trust = trust(data.path()).expect("trust");
        assert!(!trust.allows(&publisher(Some("acct-1"), "someone")));
    }
}
