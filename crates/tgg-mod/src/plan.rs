//! Plans: what installing a set, some mods, or the updates will do,
//! resolved against one catalog snapshot and the mods folder, with no
//! network and no writes; and applying a plan.

use crate::catalog::{Catalog, PackageRef};
use crate::depends::Range;
use crate::error::{Error, OnProgress, Progress};
use crate::install::{self, Conflict, Installed, ModsDir};
use crate::local::{Local, fits};
use crate::lock::{self, Lock, LockEntry};
use crate::manifest::{Manifest, ModId};
use crate::net::Net;
use crate::package::{self, Package};
use crate::registry::Set;
use semver::Version;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// The largest package downloaded.
const PACKAGE_LIMIT: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The catalog snapshot the plan was resolved against.
    pub snapshot: Option<String>,
    /// One per mod, by id.
    pub steps: Vec<Step>,
    /// The sets the plan installs or updates, as the lock records them.
    pub sets: Vec<Set>,
    pub problems: Vec<Problem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: ModId,
    /// The release the step leaves installed.
    pub manifest: Manifest,
    pub action: Action,
    /// What to download: absent for [`Action::Keep`].
    pub download: Option<PackageRef>,
    /// The sets that want the mod.
    pub sets: Vec<String>,
    /// Wanted on its own.
    pub direct: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Update {
        from: Version,
    },
    /// Already installed at a version that fits.
    Keep,
}

/// What stops part of a plan, or a mod in it, from working.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No release in range has a build for this game.
    Unavailable { id: ModId, ranges: Vec<Range> },
    /// The mod replaces a function another turned-on mod replaces.
    Conflict { id: ModId, conflict: Conflict },
    /// Dependencies or imports nothing installed or planned meets.
    Unmet { id: ModId, missing: Vec<String> },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Unavailable { id, ranges } if ranges.is_empty() => {
                write!(f, "{id} has no build for this game")
            }
            Problem::Unavailable { id, ranges } => {
                let ranges: Vec<&str> = ranges.iter().map(Range::as_str).collect();
                write!(
                    f,
                    "{id} has no build for this game at {}",
                    ranges.join(" and ")
                )
            }
            Problem::Conflict { id, conflict } => write!(
                f,
                "{id} and {} both replace {}; only one of them loads",
                conflict.with, conflict.replaces
            ),
            Problem::Unmet { id, missing } => {
                write!(f, "{id} needs {}", missing.join(", "))
            }
        }
    }
}

impl Plan {
    /// The bytes the plan downloads.
    pub fn size(&self) -> u64 {
        self.downloads().map(|(_, package)| package.size).sum()
    }

    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// Whether applying the plan changes nothing on disk but the lock.
    pub fn is_empty(&self) -> bool {
        self.downloads().next().is_none()
    }

    fn downloads(&self) -> impl Iterator<Item = (&Step, &PackageRef)> {
        self.steps
            .iter()
            .filter_map(|step| step.download.as_ref().map(|package| (step, package)))
    }
}

/// One mod a plan wants.
struct Want {
    ranges: Vec<Range>,
    sets: Vec<String>,
    direct: bool,
}

fn resolve(
    wants: BTreeMap<ModId, Want>,
    catalog: &Catalog,
    local: &Local,
    newest: bool,
    sets: Vec<Set>,
) -> Plan {
    let mut steps = Vec::new();
    let mut problems = Vec::new();
    for (id, want) in wants {
        let fitting = |manifest: &Manifest| {
            want.ranges
                .iter()
                .all(|range| range.matches(&manifest.version))
                && local.port.as_ref().is_none_or(|port| fits(manifest, port))
        };
        let best = catalog
            .mods
            .iter()
            .filter(|entry| entry.manifest.id == id && fitting(&entry.manifest))
            .max_by(|a, b| a.manifest.version.cmp(&b.manifest.version));
        let installed = local.get(&id);
        let keep = installed.is_some_and(|m| {
            m.dev
                || (m.fits != Some(false)
                    && fitting(&m.manifest)
                    && !(newest
                        && best.is_some_and(|best| best.manifest.version > m.manifest.version)))
        });
        let (manifest, action, download) = match (installed, best) {
            (Some(m), _) if keep => (m.manifest.clone(), Action::Keep, None),
            (_, Some(best)) => (
                best.manifest.clone(),
                match installed {
                    Some(m) => Action::Update {
                        from: m.manifest.version.clone(),
                    },
                    None => Action::Install,
                },
                Some(best.package.clone()),
            ),
            (_, None) => {
                problems.push(Problem::Unavailable {
                    id,
                    ranges: want.ranges,
                });
                continue;
            }
        };
        steps.push(Step {
            id,
            manifest,
            action,
            download,
            sets: want.sets,
            direct: want.direct,
        });
    }

    // The folder as it will be, to check each planned mod against.
    let mut after: Vec<Installed> = local
        .installed()
        .into_iter()
        .filter(|m| !steps.iter().any(|step| step.id == m.manifest.id))
        .collect();
    for step in &steps {
        let enabled = local.get(&step.id).is_none_or(|m| m.enabled);
        after.push(Installed {
            manifest: step.manifest.clone(),
            enabled,
        });
    }
    for step in &steps {
        for conflict in install::conflicts(&step.manifest, &after, None) {
            problems.push(Problem::Conflict {
                id: step.id.clone(),
                conflict,
            });
        }
        let missing = install::unmet(&step.manifest, &after);
        if !missing.is_empty() {
            problems.push(Problem::Unmet {
                id: step.id.clone(),
                missing,
            });
        }
    }
    Plan {
        snapshot: catalog.snapshot.clone(),
        steps,
        sets,
        problems,
    }
}

/// Installing `set`: the newest release of each mod it lists that fits the
/// set's range and the game, leaving alone a mod installed at a fitting
/// version.
pub fn plan_set(set: &Set, catalog: &Catalog, local: &Local) -> Plan {
    let wants = set
        .mods
        .iter()
        .map(|m| {
            (
                m.id.clone(),
                Want {
                    ranges: vec![m.range.clone()],
                    sets: vec![set.name.clone()],
                    direct: false,
                },
            )
        })
        .collect();
    resolve(wants, catalog, local, false, vec![set.clone()])
}

/// Installing mods on their own, each at the newest release in its range
/// that fits the game.
pub fn plan_mods(want: &[(ModId, Range)], catalog: &Catalog, local: &Local) -> Plan {
    let mut wants: BTreeMap<ModId, Want> = BTreeMap::new();
    for (id, range) in want {
        let entry = wants.entry(id.clone()).or_insert(Want {
            ranges: Vec::new(),
            sets: Vec::new(),
            direct: true,
        });
        entry.ranges.push(range.clone());
    }
    resolve(wants, catalog, local, false, Vec::new())
}

/// Updating everything tgg installed: each installed set re-planned against
/// its newest version in `sets` (new mods installed, newer fitting releases
/// replacing older ones; a mod the set dropped stays installed and loses
/// that origin), and each mod installed on its own updated to its newest
/// fitting release.
pub fn plan_updates(sets: &[Set], catalog: &Catalog, local: &Local) -> Plan {
    let mut wants: BTreeMap<ModId, Want> = BTreeMap::new();
    let mut planned_sets = Vec::new();
    for installed in &local.lock.sets {
        let set = sets
            .iter()
            .find(|set| set.name == installed.name && set.version >= installed.version)
            .unwrap_or(installed);
        for m in &set.mods {
            let want = wants.entry(m.id.clone()).or_insert(Want {
                ranges: Vec::new(),
                sets: Vec::new(),
                direct: false,
            });
            want.ranges.push(m.range.clone());
            want.sets.push(set.name.clone());
        }
        planned_sets.push(set.clone());
    }
    for (id, entry) in &local.lock.mods {
        if !entry.direct || local.get(id).is_none() {
            continue;
        }
        let want = wants.entry(id.clone()).or_insert(Want {
            ranges: Vec::new(),
            sets: Vec::new(),
            direct: true,
        });
        want.direct = true;
    }
    resolve(wants, catalog, local, true, planned_sets)
}

/// What [`apply`] changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub installed: Vec<ModId>,
    pub updated: Vec<ModId>,
}

/// Carry out `plan` in the mods folder `mods`: download each package,
/// check it against the plan's SHA-256, and install it, one mod at a time,
/// each atomically. The lock is written last, also when a step fails or the
/// work is cancelled, so it matches what is on disk.
pub fn apply(
    net: &dyn Net,
    plan: &Plan,
    mods: &Path,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<Applied, Error> {
    let _busy = lock::take(mods)?;
    let dir = ModsDir::new(mods.to_owned());
    let mut lock = Lock::read(mods)?;
    let mut applied = Applied::default();
    let downloads: Vec<(&Step, &PackageRef)> = plan.downloads().collect();
    let steps = downloads.len();
    let mut result = Ok(());
    for (index, (step, package)) in downloads.iter().enumerate() {
        result = install_step(net, &dir, step, package, index, steps, progress, cancel);
        if result.is_err() {
            break;
        }
        record(&mut lock, step, Some(package.sha256.clone()));
        match step.action {
            Action::Update { .. } => applied.updated.push(step.id.clone()),
            _ => applied.installed.push(step.id.clone()),
        }
    }
    for step in &plan.steps {
        if step.action == Action::Keep {
            let sha256 = lock
                .mods
                .get(&step.id)
                .and_then(|entry| entry.sha256.clone());
            record(&mut lock, step, sha256);
        }
    }
    for set in &plan.sets {
        match lock.sets.iter_mut().find(|have| have.name == set.name) {
            Some(have) => *have = set.clone(),
            None => lock.sets.push(set.clone()),
        }
        for (id, entry) in &mut lock.mods {
            if set.range(id).is_none() {
                entry.sets.retain(|name| *name != set.name);
            }
        }
    }
    lock.mods
        .retain(|id, _| dir.folder(id, true).exists() || dir.folder(id, false).exists());
    lock.write(mods)?;
    result.map(|()| applied)
}

#[allow(clippy::too_many_arguments)]
fn install_step(
    net: &dyn Net,
    dir: &ModsDir,
    step: &Step,
    package: &PackageRef,
    index: usize,
    steps: usize,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<(), Error> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let name = &step.manifest.name;
    let label = format!("Downloading {name}");
    let report = |label: &str, bytes: u64| {
        progress(Progress {
            step: index,
            steps,
            label: label.to_owned(),
            bytes,
            total: Some(package.size),
        })
    };
    report(&label, 0);
    let bytes = net
        .download(
            &package.url,
            package.size.min(PACKAGE_LIMIT),
            &mut |bytes, _| report(&label, bytes),
            cancel,
        )
        .map_err(Error::from_net)?;
    if bytes.len() as u64 != package.size || package::sha256_hex(&bytes) != package.sha256 {
        return Err(Error::Checksum {
            id: step.id.clone(),
            expected: package.sha256.clone(),
        });
    }
    let opened = Package::from_zip(&bytes).map_err(|source| Error::Package {
        id: step.id.clone(),
        source,
    })?;
    if opened.manifest.id != step.id || opened.manifest.version != step.manifest.version {
        return Err(Error::PackageMismatch {
            id: step.id.clone(),
            version: step.manifest.version.clone(),
        });
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    report(&format!("Installing {name}"), package.size);
    dir.install(&opened).map_err(Error::io(dir.root()))
}

fn record(lock: &mut Lock, step: &Step, sha256: Option<String>) {
    let entry = lock.mods.entry(step.id.clone()).or_insert(LockEntry {
        version: step.manifest.version.clone(),
        sha256: None,
        sets: Vec::new(),
        direct: false,
    });
    entry.version = step.manifest.version.clone();
    entry.sha256 = sha256;
    for set in &step.sets {
        if !entry.sets.contains(set) {
            entry.sets.push(set.clone());
        }
    }
    entry.direct |= step.direct;
}
