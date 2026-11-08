//! The mods a player gets from textures.gg: `tgg mod install ID[@RANGE]...`
//! and `--set`, `update`, `remove`, `on`, `off`, `list` and `sets`, over
//! [`tgg_mod`]'s distribution API. Only `install`, `update` and `sets` use the
//! network; the rest read and change the mods folder alone.

use anyhow::{Result, anyhow, bail};
use std::fmt::Display;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tgg_mod::{
    Action, Local, LocalMod, ModId, Net, Plan, Port, Problem, Progress, Range, SetStatus, netplay,
};

/// Where a command's output goes: the terminal, or a buffer in tests.
pub struct Out {
    buffer: Option<Mutex<String>>,
    /// Progress is a bar.
    terminal: bool,
    /// The last progress label printed.
    label: Mutex<String>,
    bar: Mutex<Option<indicatif::ProgressBar>>,
}

impl Out {
    pub fn stdout() -> Self {
        Self {
            buffer: None,
            terminal: crate::ui::interactive(),
            label: Mutex::default(),
            bar: Mutex::default(),
        }
    }

    #[cfg(test)]
    fn buffer() -> Self {
        Self {
            buffer: Some(Mutex::default()),
            terminal: false,
            label: Mutex::default(),
            bar: Mutex::default(),
        }
    }

    #[cfg(test)]
    fn take(&self) -> String {
        std::mem::take(
            &mut *self
                .buffer
                .as_ref()
                .expect("a buffer")
                .lock()
                .expect("lock"),
        )
    }

    fn line(&self, text: impl Display) {
        match &self.buffer {
            Some(buffer) => {
                let mut buffer = buffer.lock().expect("lock");
                buffer.push_str(&text.to_string());
                buffer.push('\n');
            }
            None => println!("{text}"),
        }
    }

    /// Progress: a bar in a terminal; elsewhere each step printed once.
    fn progress(&self, progress: &Progress) {
        let label = format!(
            "[{}/{}] {}",
            progress.step + 1,
            progress.steps,
            progress.label
        );
        if self.terminal && self.buffer.is_none() {
            let mut bar = self.bar.lock().expect("lock");
            let bar = bar.get_or_insert_with(|| {
                crate::ui::bar("", 0).unwrap_or_else(indicatif::ProgressBar::hidden)
            });
            bar.set_length(progress.total.unwrap_or(0));
            bar.set_position(progress.bytes);
            bar.set_message(label);
            return;
        }
        let mut last = self.label.lock().expect("lock");
        if *last != label {
            self.line(&label);
            *last = label;
        }
    }

    /// Clear the progress bar.
    fn progress_done(&self) {
        if let Some(bar) = self.bar.lock().expect("lock").take() {
            bar.finish_and_clear();
        }
    }
}

/// A player's mods folder and game, and where to fetch from.
pub struct Player<'a> {
    pub net: &'a dyn Net,
    /// The mods folder.
    pub mods: PathBuf,
    /// The ports folder, to say when there is a newer tgg-melee.
    pub ports: PathBuf,
    /// The game in use, if one is installed.
    pub port: Option<Port>,
    pub out: Out,
    /// Ask before changing anything.
    pub ask: bool,
    pub cancel: &'a AtomicBool,
}

/// `ID` or `ID@RANGE`, such as `tgg.netplay@^0.2`.
pub fn parse_spec(text: &str) -> Result<(ModId, Range)> {
    let (id, range) = text.split_once('@').unwrap_or((text, "*"));
    let id: ModId = id.parse().map_err(|error| anyhow!("{text}: {error}"))?;
    let range = Range::parse(range).map_err(|error| anyhow!("{text}: {error}"))?;
    Ok((id, range))
}

/// `bytes` as people read sizes.
fn size(bytes: u64) -> String {
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.1} kB", bytes as f64 / 1e3),
        _ => format!("{:.1} MB", bytes as f64 / 1e6),
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

impl Player<'_> {
    fn port(&self) -> Result<&Port> {
        self.port
            .as_ref()
            .ok_or_else(|| anyhow!("tgg-melee isn't installed; run tgg port install"))
    }

    fn local(&self) -> Result<Local> {
        Ok(tgg_mod::local(&self.mods, self.port.as_ref())?)
    }

    /// Install the set `set`, or the mods `specs`.
    pub fn install(&self, set: Option<&str>, specs: &[String]) -> Result<()> {
        let want = specs
            .iter()
            .map(|spec| parse_spec(spec))
            .collect::<Result<Vec<_>>>()?;
        let port = self.port()?;
        let catalog = tgg_mod::catalog(self.net, port)?;
        let local = self.local()?;
        let plan = match set {
            Some(name) => {
                let sets = tgg_mod::sets(self.net)?;
                let set = sets
                    .iter()
                    .find(|set| set.name == name || set.title.eq_ignore_ascii_case(name))
                    .ok_or_else(|| {
                        anyhow!("textures.gg has no set {name}; tgg mod sets lists them")
                    })?;
                self.out.line(format_args!(
                    "{} (set version {}) for tgg-melee {}:",
                    set.title, set.version, port.version
                ));
                tgg_mod::plan_set(set, &catalog, &local)
            }
            None => tgg_mod::plan_mods(&want, &catalog, &local),
        };
        self.carry_out(&plan, "Install")
    }

    /// Update every set and mod tgg installed, or the set `set`.
    pub fn update(&self, set: Option<&str>) -> Result<()> {
        let port = self.port()?;
        let local = self.local()?;
        if let Some(name) = set
            && local.set(name).is_none()
        {
            return Err(tgg_mod::Error::SetNotInstalled(name.to_owned()).into());
        }
        let sets = tgg_mod::sets(self.net)?;
        let catalog = tgg_mod::catalog(self.net, port)?;
        let plan = match set {
            Some(name) => tgg_mod::plan_set_update(name, &sets, &catalog, &local)?,
            None => tgg_mod::plan_updates(&sets, &catalog, &local),
        };
        for planned in &plan.sets {
            if let Some(have) = local.set(&planned.name)
                && have.set.version < planned.version
            {
                self.out.line(format_args!(
                    "{}: set version {} -> {}",
                    planned.title, have.set.version, planned.version
                ));
            }
        }
        self.carry_out(&plan, "Update")?;
        if set.is_none() {
            self.port_update();
        }
        Ok(())
    }

    /// Say when there's a newer tgg-melee than any installed.
    fn port_update(&self) {
        let Ok(installed) = tgg_mod::installed_ports(&self.ports) else {
            return;
        };
        if installed.is_empty() {
            return;
        }
        if let Ok(releases) = tgg_mod::port_releases(self.net)
            && let Some(newer) = tgg_mod::port_update(&installed, &releases)
        {
            self.out.line(format_args!(
                "tgg-melee {} is out: {}, then {}",
                crate::ui::ver(&newer.version),
                crate::ui::cmd("tgg port install"),
                crate::ui::cmd(format!("tgg port use {}", newer.version))
            ));
        }
    }

    /// Show `plan`, ask, and apply it.
    fn carry_out(&self, plan: &Plan, verb: &str) -> Result<()> {
        let problems: Vec<String> = plan.problems().iter().map(Problem::to_string).collect();
        let rows: Vec<(&str, String, String, &str)> = plan
            .steps
            .iter()
            .filter_map(|step| {
                let (action, version) = match &step.action {
                    Action::Install => ("install", step.manifest.version.to_string()),
                    Action::Update { from } => (
                        if *from > step.manifest.version {
                            "change"
                        } else {
                            "update"
                        },
                        format!("{from} -> {}", step.manifest.version),
                    ),
                    Action::Keep => return None,
                };
                Some((
                    action,
                    step.id.to_string(),
                    version,
                    step.manifest.name.as_str(),
                ))
            })
            .collect();
        let changes = rows.len();
        let keeps = plan.steps.len() - changes;
        let ids = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
        let versions = rows.iter().map(|row| row.2.len()).max().unwrap_or(0);
        for (action, id, version, name) in &rows {
            let padded = format!("{action:7}");
            let action = if *action == "install" {
                crate::ui::green(padded)
            } else {
                crate::ui::yellow(padded)
            };
            let version = crate::ui::ver(format!("{version:versions$}"));
            self.out.line(format_args!(
                "  {action} {id:ids$} {version}  {}",
                crate::ui::dim(name)
            ));
        }
        if keeps > 0 {
            self.out.line(format_args!(
                "  {} already installed",
                plural(keeps, "mod", "mods")
            ));
        }
        for problem in &problems {
            self.out.line(format_args!("  ! {problem}"));
        }
        if plan.steps.is_empty() && plan.sets.is_empty() {
            if problems.is_empty() {
                self.out.line("Nothing to do.");
                return Ok(());
            }
            bail!("nothing to {}", verb.to_lowercase());
        }
        if changes == 0 {
            // Nothing to download; the lock still records the set or mods.
            tgg_mod::apply(self.net, plan, &self.mods, &|_| {}, self.cancel)?;
            self.out.line(if verb == "Update" {
                "Everything is up to date."
            } else {
                "Everything is already installed."
            });
            return Ok(());
        }
        let total = format!("{} ({})", plural(changes, "mod", "mods"), size(plan.size()));
        if self.ask && !confirm(&format!("{verb} {total}?"))? {
            self.out.line("Nothing changed.");
            return Ok(());
        }
        let applied = tgg_mod::apply(
            self.net,
            plan,
            &self.mods,
            &|progress| self.out.progress(&progress),
            self.cancel,
        );
        self.out.progress_done();
        let applied = applied?;
        let mut done = Vec::new();
        if !applied.installed.is_empty() {
            done.push(format!("installed {}", applied.installed.len()));
        }
        if !applied.updated.is_empty() {
            done.push(format!("updated {}", applied.updated.len()));
        }
        self.out.line(format_args!("Done: {}.", done.join(", ")));
        Ok(())
    }

    /// The installed mods and sets.
    pub fn list(&self, json: bool) -> Result<()> {
        let local = self.local()?;
        if json {
            let mods: Vec<_> = local
                .mods
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "enabled": m.enabled,
                        "netplay": netplay::classify(&m.manifest),
                        "manifest": m.manifest,
                        "sets": m.sets(),
                        "direct": m.direct(),
                        "dev": m.dev,
                        "fits": m.fits,
                    })
                })
                .collect();
            self.out
                .line(serde_json::to_string_pretty(&mods).expect("json"));
            return Ok(());
        }
        if local.mods.is_empty() && local.sets.is_empty() {
            self.out.line(format_args!(
                "No mods in {}",
                crate::ui::path(self.mods.display())
            ));
            self.out.line("tgg mod sets shows the sets on textures.gg.");
            return Ok(());
        }
        let version = self.port.as_ref().map(|port| port.version.as_str());
        for local_set in &local.sets {
            let status = match local_set.status {
                SetStatus::Complete => "complete",
                SetStatus::Customized => "customized (a mod is off or at another version)",
                SetStatus::Outdated => "has updates (tgg mod update)",
                SetStatus::Incomplete => "incomplete (a mod is missing or not built for this game)",
            };
            self.out.line(format_args!(
                "Set {} ({}, version {}): {status}",
                local_set.set.title, local_set.set.name, local_set.set.version
            ));
        }
        let titles = |m: &LocalMod| -> Vec<String> {
            m.sets()
                .iter()
                .map(|name| {
                    local
                        .set(name)
                        .map_or_else(|| name.clone(), |set| set.set.title.clone())
                })
                .collect()
        };
        let rows: Vec<(&str, String, String, String)> = local
            .mods
            .iter()
            .map(|m| {
                let mut notes = titles(m);
                if m.direct() {
                    notes.push("installed on its own".into());
                }
                if m.dev {
                    notes.push("dev build".into());
                } else if m.lock.is_none() {
                    notes.push("installed by hand".into());
                }
                if m.fits == Some(false) {
                    notes.push(format!("not built for tgg-melee {}", version.unwrap_or("")));
                }
                (
                    if m.enabled { "on" } else { "off" },
                    m.id().to_string(),
                    m.manifest.version.to_string(),
                    notes.join(", "),
                )
            })
            .collect();
        let ids = rows.iter().map(|row| row.1.len()).max().unwrap_or(0);
        let versions = rows.iter().map(|row| row.2.len()).max().unwrap_or(0);
        for (state, id, version, notes) in rows {
            let row = format!("{state:3} {id:ids$} {version:versions$}  {notes}");
            self.out.line(row.trim_end());
        }
        for (id, conflict) in &local.conflicts {
            self.out.line(format_args!(
                "! {id} doesn't load: {} already replaces {}",
                conflict.with, conflict.replaces
            ));
        }
        for (id, missing) in &local.unmet {
            self.out.line(format_args!(
                "! {id} doesn't load without {}",
                missing.join(", ")
            ));
        }
        Ok(())
    }

    /// The registry's sets.
    pub fn sets(&self) -> Result<()> {
        let sets = tgg_mod::sets(self.net)?;
        let local = self.local().ok();
        if sets.is_empty() {
            self.out.line("textures.gg has no sets yet.");
            return Ok(());
        }
        let width = sets.iter().map(|set| set.name.len()).max().unwrap_or(0);
        for set in &sets {
            let installed = local
                .as_ref()
                .and_then(|local| local.set(&set.name))
                .map_or("", |_| ", installed");
            self.out.line(format_args!(
                "{:width$}  {}: {}{installed}",
                set.name,
                set.title,
                plural(set.mods.len(), "mod", "mods")
            ));
            if !set.description.is_empty() {
                self.out
                    .line(format_args!("{:width$}  {}", "", set.description));
            }
        }
        self.out.line(format_args!(
            "{} installs one.",
            crate::ui::cmd("tgg mod install --set NAME")
        ));
        Ok(())
    }

    /// Remove the mods `ids`, or the set `set` and the mods only it wanted.
    pub fn remove(&self, set: Option<&str>, ids: &[ModId]) -> Result<()> {
        let before = self.local()?;
        if let Some(name) = set {
            let title = before
                .set(name)
                .map_or_else(|| name.to_owned(), |set| set.set.title.clone());
            tgg_mod::uninstall_set(&self.mods, name)?;
            let after = self.local()?;
            let removed: Vec<String> = before
                .mods
                .iter()
                .filter(|m| after.get(m.id()).is_none())
                .map(|m| m.id().to_string())
                .collect();
            if removed.is_empty() {
                self.out.line(format_args!("Removed the set {title}"));
            } else {
                self.out.line(format_args!(
                    "Removed the set {title} and {}",
                    removed.join(", ")
                ));
            }
        }
        for id in ids {
            tgg_mod::uninstall(&self.mods, id)?;
            self.out.line(format_args!("Removed {id}"));
            if let Some(m) = before.get(id) {
                for name in m.sets() {
                    if let Some(set) = before.set(name) {
                        self.out.line(format_args!(
                            "The set {} is incomplete without it; {} puts it back",
                            set.set.title,
                            crate::ui::cmd(format!("tgg mod install --set {}", set.set.name))
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Pick a set from the registry's, in an interactive terminal; `None`
    /// when the player backs out.
    pub fn pick_set(&self) -> Result<Option<String>> {
        let sets = tgg_mod::sets(self.net)?;
        if sets.is_empty() {
            bail!("textures.gg has no sets yet");
        }
        let items = sets
            .iter()
            .map(|set| {
                let mods = plural(set.mods.len(), "mod", "mods");
                let hint = if set.description.is_empty() {
                    mods
                } else {
                    format!("{} · {mods}", set.description)
                };
                (set.name.clone(), set.title.clone(), hint)
            })
            .collect();
        crate::ui::pick("Install which set?", items, None)
    }

    /// Pick installed mods to turn on (those off) or off (those on), in an
    /// interactive terminal.
    pub fn pick_mods(&self, on: bool) -> Result<Vec<ModId>> {
        let local = self.local()?;
        let items: Vec<(ModId, String, String)> = local
            .mods
            .iter()
            .filter(|m| m.enabled != on)
            .map(|m| {
                let label = format!("{} {}", m.manifest.name, crate::ui::dim(m.id()));
                (m.id().clone(), label, m.sets().join(", "))
            })
            .collect();
        if items.is_empty() {
            let state = if on { "on" } else { "off" };
            self.out
                .line(format_args!("Every installed mod is {state}."));
            return Ok(Vec::new());
        }
        let verb = if on { "Turn on" } else { "Turn off" };
        crate::ui::pick_many(format!("{verb} which mods?"), items)
    }

    /// Turn the mods `ids` on or off.
    pub fn set_enabled(&self, ids: &[ModId], on: bool) -> Result<()> {
        for id in ids {
            tgg_mod::set_enabled(&self.mods, id, on)?;
            self.out
                .line(format_args!("{id} is {}", if on { "on" } else { "off" }));
        }
        Ok(())
    }
}

/// Ask `question` with a yes default: a prompt in an interactive terminal,
/// else a line read from stdin (no when it ends).
fn confirm(question: &str) -> Result<bool> {
    if crate::ui::interactive() {
        return crate::ui::confirm(question);
    }
    print!("{question} [Y/n] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    if std::io::stdin().lock().read_line(&mut answer)? == 0 {
        println!();
        return Ok(false);
    }
    let answer = answer.trim().to_lowercase();
    Ok(answer.is_empty() || answer == "y" || answer == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgg_mod::{FakeNet, no_progress};

    struct Setup {
        net: FakeNet,
        root: tempfile::TempDir,
        port: Port,
        cancel: AtomicBool,
    }

    /// A player with the fake registry's port installed and no mods.
    fn setup() -> Setup {
        console::set_colors_enabled(false);
        let net = FakeNet::new();
        let root = tempfile::tempdir().expect("temp dir");
        let release = tgg_mod::port_releases(&net).expect("releases")[0].clone();
        let install = tgg_mod::install_port(
            &net,
            &release,
            &root.path().join("ports"),
            &no_progress,
            &AtomicBool::new(false),
        )
        .expect("install the port");
        Setup {
            net,
            port: install.port(),
            root,
            cancel: AtomicBool::new(false),
        }
    }

    impl Setup {
        fn player(&self) -> Player<'_> {
            Player {
                net: &self.net,
                mods: self.root.path().join("mods"),
                ports: self.root.path().join("ports"),
                port: Some(self.port.clone()),
                out: Out::buffer(),
                ask: false,
                cancel: &self.cancel,
            }
        }

        fn local(&self) -> Local {
            tgg_mod::local(&self.root.path().join("mods"), Some(&self.port)).expect("local")
        }
    }

    fn id(text: &str) -> ModId {
        text.parse().expect("an id")
    }

    #[test]
    fn installing_a_set_shows_the_plan_and_installs_it() {
        let setup = setup();
        let player = setup.player();
        player.install(Some("competitive"), &[]).expect("install");
        let out = player.out.take();
        assert!(
            out.starts_with("Competitive (set version 1) for tgg-melee 0.5.0:\n"),
            "{out}"
        );
        assert!(
            out.contains("  install tgg.legal-stages      0.1.1  Legal stages\n"),
            "{out}"
        );
        assert!(out.contains("[1/7] Downloading "), "{out}");
        assert!(out.ends_with("Done: installed 7.\n"), "{out}");
        let local = setup.local();
        assert_eq!(local.mods.len(), 7);
        assert_eq!(
            local.set("competitive").expect("set").status,
            SetStatus::Complete
        );

        // Again: nothing to download.
        player
            .install(Some("Competitive"), &[])
            .expect("install again");
        let out = player.out.take();
        assert!(out.contains("  7 mods already installed\n"), "{out}");
        assert!(out.ends_with("Everything is already installed.\n"), "{out}");
    }

    #[test]
    fn installing_mods_by_id_takes_the_newest_in_range() {
        let setup = setup();
        let player = setup.player();
        player
            .install(
                None,
                &["tgg.legal-stages@0.1.0".into(), "tgg.direct-connect".into()],
            )
            .expect("install");
        let out = player.out.take();
        assert!(out.contains("install tgg.legal-stages   0.1.0"), "{out}");
        // Its dependency comes along.
        assert!(out.contains("install tgg.netplay        0.2.0"), "{out}");
        assert!(!out.contains('!'), "{out}");
        let local = setup.local();
        assert!(
            local
                .get(&id("tgg.legal-stages"))
                .expect("installed")
                .direct()
        );

        let error = player
            .install(None, &["nobody.nothing".into()])
            .unwrap_err();
        assert_eq!(error.to_string(), "nothing to install");
        assert!(
            player
                .out
                .take()
                .contains("! textures.gg has no mod nobody.nothing")
        );
        assert!(parse_spec("tgg.netplay@^x").is_err());
    }

    #[test]
    fn list_shows_sets_and_mods_and_off_on_customizes() {
        let setup = setup();
        let player = setup.player();
        player.list(false).expect("list");
        assert!(player.out.take().starts_with("No mods in "));

        player.install(Some("replays"), &[]).expect("install");
        player.out.take();
        player
            .set_enabled(&[id("tgg.replay-recorder")], false)
            .expect("off");
        assert_eq!(player.out.take(), "tgg.replay-recorder is off\n");
        player.list(false).expect("list");
        let out = player.out.take();
        assert!(
            out.starts_with(
                "Set Replays (replays, version 1): customized (a mod is off or at another version)\n"
            ),
            "{out}"
        );
        assert!(
            out.contains("off tgg.replay-recorder 0.1.0  Replays\n"),
            "{out}"
        );
        assert!(
            out.contains("on  tgg.replay-player   0.2.0  Replays\n"),
            "{out}"
        );

        player
            .set_enabled(&[id("tgg.replay-recorder")], true)
            .expect("on");
        player.out.take();
        player.list(false).expect("list");
        assert!(
            player
                .out
                .take()
                .starts_with("Set Replays (replays, version 1): complete\n")
        );

        player.list(true).expect("json");
        let json: serde_json::Value =
            serde_json::from_str(&player.out.take()).expect("list --json");
        assert_eq!(json[0]["sets"][0], "replays");
        assert_eq!(json[0]["fits"], true);
    }

    #[test]
    fn update_follows_the_newest_set_and_releases() {
        let setup = setup();
        let player = setup.player();
        player.install(Some("competitive"), &[]).expect("install");
        player.update(None).expect("nothing to update");
        assert!(player.out.take().ends_with("Everything is up to date.\n"));

        setup
            .net
            .publish_mod("tgg.netplay", "Netplay", "0.2.1", &[]);
        setup
            .net
            .publish_mod("tgg.ledge-grabs", "Ledge grabs", "0.1.0", &[]);
        let mut set = setup.net.set("competitive").expect("set");
        set.version = 2;
        set.mods.push(tgg_mod::SetMod {
            id: id("tgg.ledge-grabs"),
            range: "^0.1".parse().expect("range"),
        });
        setup.net.publish_set(set);
        setup.net.publish_port("0.6.0");

        player.update(Some("competitive")).expect("update");
        let out = player.out.take();
        assert!(
            out.starts_with("Competitive: set version 1 -> 2\n"),
            "{out}"
        );
        assert!(
            out.contains("  update  tgg.netplay     0.2.0 -> 0.2.1  Netplay\n"),
            "{out}"
        );
        assert!(
            out.contains("  install tgg.ledge-grabs 0.1.0           Ledge grabs\n"),
            "{out}"
        );
        assert!(out.ends_with("Done: installed 1, updated 1.\n"), "{out}");
        let local = setup.local();
        assert_eq!(local.set("competitive").expect("set").set.version, 2);

        player.update(None).expect("update all");
        assert_eq!(
            player.out.take(),
            "  8 mods already installed\nEverything is up to date.\n\
             tgg-melee 0.6.0 is out: tgg port install, then tgg port use 0.6.0\n"
        );
    }

    #[test]
    fn removing_a_set_removes_the_mods_only_it_wanted() {
        let setup = setup();
        let player = setup.player();
        player.install(Some("replays"), &[]).expect("install");
        player
            .install(None, &["tgg.replay-player".into()])
            .expect("install on its own too");
        player.out.take();

        player
            .remove(None, &[id("tgg.replay-recorder")])
            .expect("remove");
        assert_eq!(
            player.out.take(),
            "Removed tgg.replay-recorder\n\
             The set Replays is incomplete without it; tgg mod install --set replays puts it back\n"
        );
        player.remove(Some("replays"), &[]).expect("remove the set");
        assert_eq!(
            player.out.take(),
            "Removed the set Replays and tgg.replay-browser\n"
        );
        let local = setup.local();
        assert_eq!(local.mods.len(), 1, "the one installed on its own stays");
        let error = player
            .remove(None, &[id("tgg.replay-recorder")])
            .unwrap_err();
        assert_eq!(error.to_string(), "tgg.replay-recorder isn't installed");
    }

    #[test]
    fn offline_list_still_works_and_the_rest_say_so() {
        let setup = setup();
        let player = setup.player();
        player.install(Some("replays"), &[]).expect("install");
        player.out.take();
        setup.net.set_offline(true);
        let requests = setup.net.requests().len();

        player.list(false).expect("list offline");
        assert!(
            player
                .out
                .take()
                .starts_with("Set Replays (replays, version 1): complete\n")
        );
        player
            .set_enabled(&[id("tgg.replay-player")], false)
            .expect("off offline");
        assert_eq!(
            setup.net.requests().len(),
            requests,
            "list and off use no network"
        );

        for error in [
            player.install(Some("competitive"), &[]).unwrap_err(),
            player.update(None).unwrap_err(),
            player.sets().unwrap_err(),
        ] {
            assert_eq!(
                error.to_string(),
                "can't reach textures.gg; what's installed still works"
            );
        }
    }

    #[test]
    fn sets_lists_the_registry_and_marks_installed_ones() {
        let setup = setup();
        let player = setup.player();
        player.install(Some("replays"), &[]).expect("install");
        player.out.take();
        player.sets().expect("sets");
        let out = player.out.take();
        assert!(
            out.starts_with("competitive  Competitive: 7 mods\n"),
            "{out}"
        );
        assert!(
            out.contains("replays      Replays: 3 mods, installed\n"),
            "{out}"
        );
    }

    #[test]
    fn with_no_game_install_says_how_to_get_one() {
        let setup = setup();
        let mut player = setup.player();
        player.port = None;
        let error = player.install(Some("replays"), &[]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "tgg-melee isn't installed; run tgg port install"
        );
        player.list(false).expect("list without a game");
    }
}
