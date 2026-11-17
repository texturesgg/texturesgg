//! Acting on releases the registry withdrew. A moderator can pull an
//! approved release; the catalog then lists its packages' SHA-256s under
//! `withdrawn` with a reason and a mode. Each time tgg has a fresh catalog
//! (installing, updating, launching the game, fetching a lobby's or a
//! replay's mods), [`apply_withdrawn`] compares it with what the lock says
//! tgg installed:
//!
//! - `off`: the mod's folder is renamed to `.<id>`, which the game skips, and
//!   the lock remembers why. The player can turn it back on; tgg won't turn
//!   it off again for the same withdrawal.
//! - `warn`: the mod stays on and the player is told why, every time.
//! - restored (no longer listed): a mod tgg turned off is turned back on.
//!
//! A mod tgg didn't install (no lock entry or no package hash) is left
//! alone: tgg can't tell which release it is.

use crate::catalog::{Catalog, WithdrawMode};
use crate::error::Error;
use crate::install::ModsDir;
use crate::lock::{self, Lock};
use crate::manifest::ModId;
use std::fmt;
use std::path::Path;

/// What [`apply_withdrawn`] did to one installed mod, for the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithdrawNotice {
    pub id: ModId,
    pub version: String,
    pub reason: String,
    pub action: WithdrawAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WithdrawAction {
    /// Turned off.
    TurnedOff,
    /// Left on: the withdrawal only warns.
    Warned,
    /// Turned back on: the release was restored.
    TurnedOn,
}

impl fmt::Display for WithdrawNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (id, version, reason) = (&self.id, &self.version, &self.reason);
        match self.action {
            WithdrawAction::TurnedOff => write!(
                f,
                "Turned off {id} {version}: {reason}. textures.gg withdrew it. You can turn it back on with tgg mod on {id}"
            ),
            WithdrawAction::Warned => {
                write!(f, "{id} {version} was withdrawn: {reason}. It's still on")
            }
            WithdrawAction::TurnedOn => {
                write!(f, "Turned {id} {version} back on: textures.gg restored it")
            }
        }
    }
}

/// Act on `catalog`'s withdrawn list in the mods folder `mods`; what was
/// done, for the player. Nothing changes while the game runs from `mods`
/// ([`Error::GameRunning`]) or another tgg holds it ([`Error::Busy`]).
pub fn apply_withdrawn(mods: &Path, catalog: &Catalog) -> Result<Vec<WithdrawNotice>, Error> {
    let mut lock = Lock::read(mods)?;
    let tracked = lock
        .mods
        .iter()
        .any(|(_, entry)| entry.withdrawn.is_some() || entry.sha256.is_some());
    if !tracked {
        return Ok(Vec::new());
    }
    let _busy = lock::take(mods)?;
    let dir = ModsDir::new(mods.to_owned());
    let mut notices = Vec::new();
    let mut changed = false;
    for (id, entry) in &mut lock.mods {
        let on = dir.folder(id, true).exists();
        let off = dir.folder(id, false).exists();
        if !on && !off {
            continue;
        }
        let listed = entry
            .sha256
            .as_deref()
            .and_then(|sha| catalog.withdrawn(sha));
        let version = entry.version.to_string();
        match (listed, &entry.withdrawn) {
            // Already turned off for this; if the player turned it back on,
            // that's their call.
            (Some(withdrawn), Some(_)) if withdrawn.mode == WithdrawMode::Off => {}
            // Off by the player's own hand stays theirs: nothing to undo later.
            (Some(withdrawn), _) if withdrawn.mode == WithdrawMode::Off => {
                if on {
                    dir.set_enabled(id, false).map_err(Error::io(mods))?;
                    entry.withdrawn = Some(withdrawn.reason.clone());
                    changed = true;
                    notices.push(WithdrawNotice {
                        id: id.clone(),
                        version,
                        reason: withdrawn.reason.clone(),
                        action: WithdrawAction::TurnedOff,
                    });
                }
            }
            (Some(withdrawn), marked) => {
                // Warn only; a mod tgg turned off before goes back on.
                if marked.is_some() {
                    if off {
                        dir.set_enabled(id, true).map_err(Error::io(mods))?;
                    }
                    entry.withdrawn = None;
                    changed = true;
                }
                notices.push(WithdrawNotice {
                    id: id.clone(),
                    version,
                    reason: withdrawn.reason.clone(),
                    action: WithdrawAction::Warned,
                });
            }
            (None, Some(reason)) => {
                if off {
                    dir.set_enabled(id, true).map_err(Error::io(mods))?;
                    notices.push(WithdrawNotice {
                        id: id.clone(),
                        version,
                        reason: reason.clone(),
                        action: WithdrawAction::TurnedOn,
                    });
                }
                entry.withdrawn = None;
                changed = true;
            }
            (None, None) => {}
        }
    }
    if changed {
        lock.write(mods)?;
    }
    Ok(notices)
}

/// Before launching `port` with the mods folder `mods`: fetch the catalog
/// for its layout (kept in `data` as the last one seen) and act on its
/// withdrawn list. Offline, the last catalog seen is used, so a withdrawal
/// a lobby or replay fetch saw still counts; with none, nothing changes.
/// A game already running from `mods` is left alone.
pub fn check_withdrawn(
    net: &dyn crate::net::Net,
    data: &Path,
    mods: &Path,
    port: &crate::port::Port,
) -> Result<Vec<WithdrawNotice>, Error> {
    let catalog = match crate::cache::refresh_catalog(net, data, port) {
        Ok(catalog) => catalog,
        Err(Error::Offline(_) | Error::NoCatalog(_)) => {
            match crate::cache::cached_catalog(data, &port.game_abi) {
                Some(catalog) => catalog,
                None => return Ok(Vec::new()),
            }
        }
        Err(error) => return Err(error),
    };
    match apply_withdrawn(mods, &catalog) {
        Err(Error::GameRunning | Error::Busy) => Ok(Vec::new()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Withdrawn;
    use crate::lock::LockEntry;

    fn setup(enabled: bool) -> (tempfile::TempDir, ModsDir, ModId) {
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = ModsDir::new(temp.path().to_owned());
        let id: ModId = "me.mod".parse().expect("id");
        std::fs::create_dir_all(dir.folder(&id, enabled)).expect("mkdir");
        let mut lock = Lock::default();
        lock.mods.insert(
            id.clone(),
            LockEntry {
                version: semver::Version::new(1, 1, 0),
                sha256: Some("a".repeat(64)),
                sets: Vec::new(),
                direct: true,
                withdrawn: None,
            },
        );
        lock.write(temp.path()).expect("lock");
        (temp, dir, id)
    }

    fn catalog(mode: WithdrawMode) -> Catalog {
        let mut catalog = Catalog::empty();
        catalog.withdrawn.push(Withdrawn {
            id: "me.mod".parse().expect("id"),
            version: "1.1.0".into(),
            reason: "it crashes on stage select".into(),
            mode,
            sha256: vec!["b".repeat(64), "a".repeat(64)],
        });
        catalog
    }

    #[test]
    fn a_withdrawn_mod_is_turned_off_once_and_back_on_when_restored() {
        let (temp, dir, id) = setup(true);
        let mods = temp.path();
        let notices = apply_withdrawn(mods, &catalog(WithdrawMode::Off)).expect("apply");
        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices[0].to_string(),
            "Turned off me.mod 1.1.0: it crashes on stage select. textures.gg withdrew it. \
             You can turn it back on with tgg mod on me.mod"
        );
        assert!(dir.folder(&id, false).exists() && !dir.folder(&id, true).exists());

        // Said once.
        assert!(
            apply_withdrawn(mods, &catalog(WithdrawMode::Off))
                .expect("again")
                .is_empty()
        );

        // The player turns it back on: theirs to decide.
        dir.set_enabled(&id, true).expect("on");
        assert!(
            apply_withdrawn(mods, &catalog(WithdrawMode::Off))
                .expect("theirs")
                .is_empty()
        );
        assert!(dir.folder(&id, true).exists());

        // Off again, then restored: back on, and the lock forgets.
        dir.set_enabled(&id, false).expect("off");
        let notices = apply_withdrawn(mods, &Catalog::empty()).expect("restored");
        assert_eq!(notices[0].action, WithdrawAction::TurnedOn);
        assert!(dir.folder(&id, true).exists());
        assert_eq!(Lock::read(mods).expect("lock").mods[&id].withdrawn, None);
    }

    #[test]
    fn a_warning_leaves_the_mod_on_and_says_why_each_time() {
        let (temp, dir, id) = setup(true);
        for _ in 0..2 {
            let notices = apply_withdrawn(temp.path(), &catalog(WithdrawMode::Warn)).expect("warn");
            assert_eq!(
                notices[0].to_string(),
                "me.mod 1.1.0 was withdrawn: it crashes on stage select. It's still on"
            );
        }
        assert!(dir.folder(&id, true).exists());
    }

    #[test]
    fn a_mod_the_player_turned_off_stays_theirs() {
        let (temp, dir, id) = setup(false);
        assert!(
            apply_withdrawn(temp.path(), &catalog(WithdrawMode::Off))
                .expect("off")
                .is_empty()
        );
        // Restoring later doesn't turn on what tgg never turned off.
        assert!(
            apply_withdrawn(temp.path(), &Catalog::empty())
                .expect("restored")
                .is_empty()
        );
        assert!(dir.folder(&id, false).exists());
    }

    #[test]
    fn an_unknown_mode_reads_as_off() {
        let json = r#"{"schema":1,"mods":[],"withdrawn":[{"id":"me.mod","version":"1.1.0",
            "reason":"r","mode":"explode","sha256":["aa"]}]}"#;
        let catalog = Catalog::parse(json).expect("parse");
        assert_eq!(catalog.withdrawn[0].mode, WithdrawMode::Off);
        assert!(catalog.withdrawn("aa").is_some());
    }
}
