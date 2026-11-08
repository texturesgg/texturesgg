//! Mod profiles from the command line: `tgg replay <file.slp>`, `tgg port
//! run --profile <file>`, `tgg profile show` and `tgg profile prepare
//! --machine` (JSON lines for the port's `tgg_restart_with`; the line
//! schema is in core's docs/mod-profiles.md), over [`tgg_mod::profile`].

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use tgg_mod::{
    Catalog, Error, Fit, GameNeed, Net, Port, PortInstall, Prepared, Profile, ProfilePlan,
    SlippiMapping,
};

#[derive(Subcommand)]
pub enum ProfileCommand {
    /// Show what a replay or profile file needs, and what of it you have.
    Show {
        /// A replay (.slp) or a profile (.json).
        source: PathBuf,
        /// Print the plan as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Fetch what a replay or profile needs and build its mods folder.
    Prepare {
        /// A replay (.slp) or a profile (.json).
        source: PathBuf,
        /// Print JSON lines (plan, needs-trust, progress, done or error)
        /// for a program to read.
        #[arg(long)]
        machine: bool,
        /// Fetch this publisher's mods (repeatable).
        #[arg(long = "trust", value_name = "PUBLISHER")]
        trust: Vec<String>,
        /// Remember each --trust answer for next time.
        #[arg(long)]
        remember: bool,
    },
}

/// Where profiles are resolved from and prepared in.
pub struct Ctx<'a> {
    pub net: &'a dyn Net,
    /// The local data folder (`<data>/tgg/...`).
    pub data: PathBuf,
    /// The player's mods folder.
    pub mods: PathBuf,
    pub cancel: &'a AtomicBool,
}

impl Ctx<'_> {
    fn ports(&self) -> PathBuf {
        tgg_mod::cache::tgg_dir(&self.data).join("ports")
    }
}

fn http(api: &str) -> tgg_mod::HttpNet {
    let releases = std::env::var("TGG_RELEASES")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| tgg_mod::net::DEFAULT_RELEASES.to_owned());
    tgg_mod::HttpNet::with_urls(concat!("tgg/", env!("CARGO_PKG_VERSION")), api, &releases)
}

fn is_replay(source: &Path) -> bool {
    source
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("slp"))
}

/// The profile of `source`: a replay or a profile file.
pub fn load(source: &Path, mapping: Option<&SlippiMapping>) -> Result<Profile> {
    Ok(if is_replay(source) {
        tgg_mod::profile_from_slp(source, mapping)?
    } else {
        tgg_mod::profile_from_file(source)?
    })
}

fn empty_catalog() -> Catalog {
    Catalog {
        schema: tgg_mod::catalog::SCHEMA,
        snapshot: None,
        mods: Vec::new(),
        ids: Vec::new(),
    }
}

/// `profile` resolved against the registry, or what tgg last saw of it
/// when textures.gg can't be reached (then the note says so).
pub fn plan(ctx: &Ctx, profile: &Profile) -> Result<(ProfilePlan, Option<String>)> {
    let mut offline: Option<String> = None;
    let mapping = match tgg_mod::refresh_mapping(ctx.net, &ctx.data) {
        Ok(mapping) => mapping,
        Err(error @ Error::Offline(_)) => {
            offline = Some(error.to_string());
            tgg_mod::cached_mapping(&ctx.data).unwrap_or_else(SlippiMapping::empty)
        }
        Err(error) => return Err(error.into()),
    };
    let ports = ctx.ports();
    let layout = match tgg_mod::profile_layout(profile, &ports) {
        Some(layout) => Some(layout),
        // The game it needs isn't installed and it doesn't say its layout:
        // the release does.
        None if offline.is_none() => {
            let version = match profile.game.version.as_str() {
                tgg_mod::profile::ANY => "latest",
                version => version,
            };
            tgg_mod::ports::port_release(ctx.net, version)
                .ok()
                .map(|release| release.game_abi)
        }
        None => None,
    };
    let catalog = match &layout {
        None => empty_catalog(),
        Some(layout) if offline.is_some() => {
            tgg_mod::cached_catalog(&ctx.data, layout).unwrap_or_else(empty_catalog)
        }
        Some(layout) => {
            let port = Port {
                executable: PathBuf::new(),
                game_abi: layout.clone(),
                name: "tgg-melee".into(),
                target: String::new(),
                version: String::new(),
            };
            match tgg_mod::refresh_catalog(ctx.net, &ctx.data, &port) {
                Ok(catalog) => catalog,
                Err(Error::NoCatalog(_)) => empty_catalog(),
                Err(error @ Error::Offline(_)) => {
                    offline = Some(error.to_string());
                    tgg_mod::cached_catalog(&ctx.data, layout).unwrap_or_else(empty_catalog)
                }
                Err(error) => return Err(error.into()),
            }
        }
    };
    let current = tgg_mod::ports::installed_ports(&ports)?
        .into_iter()
        .find(|install| install.current);
    let local = tgg_mod::local(&ctx.mods, current.as_ref().map(PortInstall::port).as_ref())?;
    let trust = tgg_mod::trust(&ctx.data)?;
    let plan = tgg_mod::resolve_profile(profile, &catalog, &mapping, &ports, &local, &trust);
    Ok((plan, offline))
}

fn size(bytes: u64) -> String {
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.1} kB", bytes as f64 / 1e3),
        _ => format!("{:.1} MB", bytes as f64 / 1e6),
    }
}

fn game_json(game: &GameNeed) -> Value {
    match game {
        GameNeed::Installed(install) => {
            json!({"version": install.version, "state": "installed", "current": install.current})
        }
        GameNeed::Available { version, size } => {
            json!({"version": version, "state": "available", "size": size})
        }
        GameNeed::Unavailable { version } => json!({"version": version, "state": "unavailable"}),
    }
}

fn fit_name(fit: Fit) -> &'static str {
    match fit {
        Fit::AsInstalled => "as-installed",
        Fit::NeedsProfile => "needs-profile",
    }
}

/// The plan as the `plan` line gives it.
pub fn plan_json(plan: &ProfilePlan) -> Value {
    json!({
        "event": "plan",
        "key": plan.key,
        "fit": fit_name(plan.fit),
        "label": plan.profile.label,
        "game": game_json(&plan.game),
        "mods": plan.mods.iter().map(|m| json!({
            "id": m.id,
            "name": m.name,
            "version": m.version.to_string(),
            "publisher": m.publisher,
            "cached": m.cached,
            "size": m.size,
            "needs_trust": m.needs_trust,
        })).collect::<Vec<_>>(),
        "missing": plan.missing,
    })
}

/// The plan for a person.
pub fn describe(source: &Path, plan: &ProfilePlan, offline: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    let what = plan.profile.label.clone().unwrap_or_else(|| {
        source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    lines.push(crate::ui::heading(what));
    if let Some(offline) = offline {
        lines.push(format!("{offline}; showing what tgg saw last"));
    }
    lines.push(match &plan.game {
        GameNeed::Installed(install) if install.current => {
            format!("Game: tgg-melee {}", crate::ui::ver(&install.version))
        }
        GameNeed::Installed(install) => format!(
            "Game: tgg-melee {} (installed; runs for this session)",
            crate::ui::ver(&install.version)
        ),
        GameNeed::Available { version, size: s } => format!(
            "Game: needs tgg-melee {}: available{}",
            crate::ui::ver(version),
            s.map(|s| format!(", {}", size(s))).unwrap_or_default()
        ),
        GameNeed::Unavailable { version } => {
            format!("Game: needs tgg-melee {version}, which tgg can't get")
        }
    });
    if !plan.mods.is_empty() {
        lines.push("Mods:".into());
    }
    for m in &plan.mods {
        let mut notes = vec![format!("by {}", m.publisher.name)];
        notes.push(if m.cached {
            "cached".to_owned()
        } else {
            size(m.size)
        });
        if m.needs_trust {
            notes.push("asks first".into());
        }
        lines.push(format!(
            "  {} {}  {}  ({})",
            m.id,
            crate::ui::ver(&m.version),
            m.name,
            notes.join(", ")
        ));
    }
    if !plan.missing.is_empty() {
        lines.push("Can't be had:".into());
        for missing in &plan.missing {
            lines.push(format!("  {missing}"));
        }
    }
    let serious = plan.missing.iter().any(|m| !m.is_minor());
    lines.push(match (plan.fit, serious, plan.missing.is_empty()) {
        (_, true, _) => crate::ui::yellow("It may not play back as it was played."),
        (Fit::AsInstalled, _, true) => crate::ui::green("Plays with your mods as they are."),
        (Fit::AsInstalled, _, false) => {
            crate::ui::green("Plays with your mods as they are, likely as it was played.")
        }
        (Fit::NeedsProfile, _, true) => "Plays with its own mods, for this session.".into(),
        (Fit::NeedsProfile, _, false) => {
            "Plays with its own mods, for this session, likely as it was played.".into()
        }
    });
    lines
}

/// Everything after the plan line of `tgg profile prepare --machine`.
pub fn prepare_machine(
    ctx: &Ctx,
    source: &Path,
    trusted: &[String],
    remember: bool,
    emit: &(dyn Fn(Value) + Sync),
) -> Result<()> {
    let profile = load(source, tgg_mod::cached_mapping(&ctx.data).as_ref())?;
    let (mut plan, _) = plan(ctx, &profile)?;
    emit(plan_json(&plan));
    if remember {
        for publisher in trusted {
            tgg_mod::remember_trust(&ctx.data, publisher, true)?;
        }
    }
    for m in &mut plan.mods {
        if trusted.contains(&m.publisher.name) {
            m.needs_trust = false;
        }
    }
    let untrusted = plan.untrusted();
    if !untrusted.is_empty() {
        let publishers: Vec<Value> = untrusted
            .iter()
            .map(|name| {
                let mods: Vec<&str> = plan
                    .mods
                    .iter()
                    .filter(|m| m.needs_trust && m.publisher.name == *name)
                    .map(|m| m.id.as_str())
                    .collect();
                json!({"name": name, "mods": mods})
            })
            .collect();
        emit(json!({"event": "needs-trust", "publishers": publishers}));
        return Ok(());
    }
    if plan.fit == Fit::AsInstalled
        && let GameNeed::Installed(install) = &plan.game
    {
        emit(json!({
            "event": "done",
            "fit": fit_name(plan.fit),
            "key": plan.key,
            "folder": null,
            "settings": null,
            "port": {"version": install.version, "executable": install.executable()},
            "env": {},
        }));
        return Ok(());
    }
    let last = std::sync::Mutex::new((usize::MAX, 0u64));
    let progress = |progress: tgg_mod::Progress| {
        let mut last = last.lock().expect("lock");
        // A line per step, and per 2% of a download.
        let step = progress.total.map_or(u64::MAX, |total| (total / 50).max(1));
        if last.0 != progress.step
            || progress.bytes >= last.1 + step
            || Some(progress.bytes) == progress.total
        {
            *last = (progress.step, progress.bytes);
            emit(json!({
                "event": "progress",
                "step": progress.step,
                "steps": progress.steps,
                "label": progress.label,
                "bytes": progress.bytes,
                "total": progress.total,
            }));
        }
    };
    let prepared = tgg_mod::prepare_profile(ctx.net, &plan, &ctx.data, &progress, ctx.cancel)?;
    let env: serde_json::Map<String, Value> = prepared
        .env()
        .into_iter()
        .map(|(name, value)| (name, Value::String(value.to_string_lossy().into_owned())))
        .collect();
    emit(json!({
        "event": "done",
        "fit": fit_name(plan.fit),
        "key": prepared.key,
        "folder": prepared.folder,
        "settings": prepared.settings,
        "port": {"version": prepared.port.version, "executable": prepared.port.executable()},
        "env": env,
    }));
    Ok(())
}

fn ctx<'a>(net: &'a dyn Net, cancel: &'a AtomicBool) -> Result<Ctx<'a>> {
    Ok(Ctx {
        net,
        data: crate::paths::data()?,
        mods: tgg_mod::default_mods_dir(),
        cancel,
    })
}

pub fn run(command: ProfileCommand, api: &str) -> Result<()> {
    let net = http(api);
    let signals = crate::signals::handle()?;
    let ctx = ctx(&net, signals.stop_flag())?;
    match command {
        ProfileCommand::Show { source, json } => {
            let profile = load(&source, tgg_mod::cached_mapping(&ctx.data).as_ref())?;
            let (plan, offline) = plan(&ctx, &profile)?;
            if json {
                let mut value = plan_json(&plan);
                value["profile"] = serde_json::to_value(&plan.profile)?;
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                for line in describe(&source, &plan, offline.as_deref()) {
                    println!("{line}");
                }
            }
            Ok(())
        }
        ProfileCommand::Prepare {
            source,
            machine,
            trust,
            remember,
        } => {
            if !machine {
                let prepared = prepare_here(&ctx, &source, true)?;
                if let Some(prepared) = prepared {
                    println!("Ready: {}", crate::ui::path(prepared.folder.display()));
                }
                return Ok(());
            }
            let emit = |value: Value| println!("{value}");
            let result = prepare_machine(&ctx, &source, &trust, remember, &emit);
            if let Err(error) = &result {
                let message = match error.downcast_ref::<Error>() {
                    Some(error) => error.to_string(),
                    None => format!("{error:#}"),
                };
                emit(json!({"event": "error", "message": message}));
            }
            result
        }
    }
}

/// Resolve `source`, show it, ask what needs asking in a terminal (or take
/// `yes`), and prepare it; `None` when it plays as installed.
fn prepare_here(ctx: &Ctx, source: &Path, yes: bool) -> Result<Option<Prepared>> {
    let profile = load(source, tgg_mod::cached_mapping(&ctx.data).as_ref())?;
    let (mut plan, offline) = plan(ctx, &profile)?;
    for line in describe(source, &plan, offline.as_deref()) {
        println!("{line}");
    }
    let ask = !yes && crate::ui::interactive();
    if plan.missing.iter().any(|m| !m.is_minor()) && !yes {
        if !ask {
            bail!("it may not play back as it was played; --yes plays it anyway");
        }
        if !crate::ui::confirm("Play it anyway?")? {
            bail!("not played");
        }
    }
    for publisher in plan
        .untrusted()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    {
        let names: Vec<&str> = plan
            .mods
            .iter()
            .filter(|m| m.needs_trust && m.publisher.name == publisher)
            .map(|m| m.name.as_str())
            .collect();
        let allow = if yes {
            true
        } else if ask {
            let answer = crate::ui::confirm(format!(
                "This used {} by {publisher}. Get it for playback?",
                names.join(", ")
            ))?;
            tgg_mod::remember_trust(&ctx.data, &publisher, answer)?;
            answer
        } else {
            bail!(
                "{} by {publisher} needs your yes; run it in a terminal, or pass --yes",
                names.join(", ")
            );
        };
        if !allow {
            bail!("not fetching {publisher}'s mods");
        }
        for m in &mut plan.mods {
            if m.publisher.name == publisher {
                m.needs_trust = false;
            }
        }
    }
    if plan.fit == Fit::AsInstalled {
        return Ok(None);
    }
    let bar = crate::ui::bar("", 0);
    let shown = std::sync::Mutex::new(String::new());
    let progress = |progress: tgg_mod::Progress| match &bar {
        Some(bar) => {
            bar.set_length(progress.total.unwrap_or(0));
            bar.set_position(progress.bytes);
            bar.set_message(progress.label);
        }
        None => {
            let mut shown = shown.lock().expect("lock");
            if *shown != progress.label {
                println!("{}", progress.label);
                *shown = progress.label;
            }
        }
    };
    let prepared = tgg_mod::prepare_profile(ctx.net, &plan, &ctx.data, &progress, ctx.cancel);
    if let Some(bar) = &bar {
        bar.finish_and_clear();
    }
    Ok(Some(prepared?))
}

/// `tgg replay` and `tgg port run --profile`: prepare `source` and play it,
/// the replay `replay` when given.
pub fn play(
    api: &str,
    source: &Path,
    replay: Option<&Path>,
    yes: bool,
    verbose: bool,
) -> Result<()> {
    let net = http(api);
    let signals = crate::signals::handle()?;
    let ctx = ctx(&net, signals.stop_flag())?;
    let prepared = prepare_here(&ctx, source, yes)?;
    let iso = crate::config::iso_or_ask()?;
    let log = crate::paths::state()?.join(crate::ports::GAME_LOG);
    let log = (!verbose).then_some(log.as_path());
    let mut env = Vec::new();
    if let Some(replay) = replay {
        env.push((
            "TGG_REPLAY_PLAYER_FILE".to_owned(),
            std::path::absolute(replay)?.into_os_string(),
        ));
    }
    let (mut running, version, executable) = match &prepared {
        Some(prepared) => {
            let replay = replay.map(std::path::absolute).transpose()?;
            let running =
                tgg_mod::launch_profile(&prepared.port, &iso, prepared, replay.as_deref(), log)?;
            (running, &prepared.port.version, prepared.port.executable())
        }
        None => {
            let ports = ctx.ports();
            let current = tgg_mod::ports::installed_ports(&ports)?
                .into_iter()
                .find(|install| install.current)
                .ok_or_else(|| anyhow!("tgg-melee isn't installed; run tgg port install"))?;
            let running = tgg_mod::ports::launch_with_env(
                &current,
                &iso,
                &ctx.mods,
                std::iter::empty::<&str>(),
                &env,
                log,
            )?;
            let executable = current.executable();
            let version = current.version.clone();
            println!("Starting tgg-melee {version}");
            signals.set_game(Some(running.id()));
            return wait(running, &signals, &executable, log);
        }
    };
    println!("Starting tgg-melee {version}");
    signals.set_game(Some(running.id()));
    let status = running.wait();
    signals.set_game(None);
    finish(status, &signals, &executable, log)
}

fn wait(
    mut running: tgg_mod::Running,
    signals: &crate::signals::Signals,
    executable: &Path,
    log: Option<&Path>,
) -> Result<()> {
    let status = running.wait();
    signals.set_game(None);
    finish(status, signals, executable, log)
}

fn finish(
    status: std::io::Result<std::process::ExitStatus>,
    signals: &crate::signals::Signals,
    executable: &Path,
    log: Option<&Path>,
) -> Result<()> {
    let status = status.with_context(|| format!("running {}", executable.display()))?;
    if !status.success()
        && !signals.stopping()
        && let Some(log) = log
    {
        crate::ports::print_tail(log);
    }
    crate::ports::check_exit(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tgg_mod::slp::write;
    use tgg_mod::{FakeNet, Publisher, no_progress};

    struct Setup {
        net: FakeNet,
        dir: tempfile::TempDir,
        cancel: AtomicBool,
    }

    fn setup() -> Setup {
        console::set_colors_enabled(false);
        let net = FakeNet::new();
        net.publish_slippi_mods();
        let dir = tempfile::tempdir().expect("temp dir");
        let release = tgg_mod::port_releases(&net).expect("releases")[0].clone();
        tgg_mod::install_port(
            &net,
            &release,
            &dir.path().join("tgg/ports"),
            &no_progress,
            &AtomicBool::new(false),
        )
        .expect("port");
        Setup {
            net,
            dir,
            cancel: AtomicBool::new(false),
        }
    }

    impl Setup {
        fn ctx(&self) -> Ctx<'_> {
            Ctx {
                net: &self.net,
                data: self.dir.path().to_owned(),
                mods: self.dir.path().join("mods"),
                cancel: &self.cancel,
            }
        }

        fn slippi_replay(&self) -> PathBuf {
            let gecko = write::gecko(&[0x8016_E510, 0x8000_0004]);
            let events = write::events([3, 18, 0], 31, false, 1, &gecko);
            let path = self.dir.path().join("Game.slp");
            std::fs::write(&path, write::replay(&events, None)).expect("write");
            path
        }
    }

    #[test]
    fn show_says_what_a_slippi_replay_needs() {
        let setup = setup();
        let replay = setup.slippi_replay();
        let profile = load(&replay, None).expect("profile");
        let (plan, offline) = plan(&setup.ctx(), &profile).expect("plan");
        assert_eq!(offline, None);
        let text = describe(&replay, &plan, None).join("\n");
        assert!(
            text.starts_with("Slippi 3.18.0\nGame: tgg-melee 0.5.0\nMods:\n"),
            "{text}"
        );
        assert!(text.contains("  tgg.neutral-spawns 0.1.0  Neutral spawns  (by textures.gg, "));
        assert!(
            text.contains("  no mod reproduces the Slippi code 0x80000004\n"),
            "{text}"
        );
        assert!(
            text.ends_with("It may not play back as it was played."),
            "{text}"
        );

        // Offline: from what tgg saw last.
        setup.net.set_offline(true);
        let (again, offline) = super::plan(&setup.ctx(), &profile).expect("offline plan");
        assert!(offline.is_some());
        assert_eq!(again.key, plan.key);
    }

    #[test]
    fn machine_lines_give_the_plan_progress_and_the_folder() {
        let setup = setup();
        let replay = setup.slippi_replay();
        let lines = std::sync::Mutex::new(Vec::new());
        let emit = |value: Value| lines.lock().expect("lock").push(value);
        prepare_machine(&setup.ctx(), &replay, &[], false, &emit).expect("prepare");
        let lines = lines.into_inner().expect("lines");
        let events: Vec<&str> = lines
            .iter()
            .map(|l| l["event"].as_str().expect("event"))
            .collect();
        assert_eq!(events.first(), Some(&"plan"));
        assert_eq!(events.last(), Some(&"done"));
        assert!(events.contains(&"progress"));
        let plan = &lines[0];
        assert_eq!(plan["fit"], "needs-profile");
        assert_eq!(plan["game"]["state"], "installed");
        assert_eq!(plan["missing"][0]["why"], "no-mod-for");
        let done = lines.last().expect("done");
        let folder = PathBuf::from(done["folder"].as_str().expect("folder"));
        assert!(folder.join("tgg.neutral-spawns/manifest.json").is_file());
        assert_eq!(done["env"]["TGG_MODS_DIR"], done["folder"]);
        assert!(
            done["port"]["executable"]
                .as_str()
                .expect("exe")
                .ends_with("tgg-melee")
        );
    }

    #[test]
    fn machine_lines_stop_for_trust_until_it_is_given() {
        let setup = setup();
        setup.net.set_publisher(
            "tgg.neutral-spawns",
            Publisher {
                name: "someone".into(),
                official: false,
            },
        );
        let replay = setup.slippi_replay();
        let run = |trusted: &[String], remember: bool| {
            let lines = std::sync::Mutex::new(Vec::new());
            let emit = |value: Value| lines.lock().expect("lock").push(value);
            prepare_machine(&setup.ctx(), &replay, trusted, remember, &emit).expect("prepare");
            lines.into_inner().expect("lines")
        };
        let lines = run(&[], false);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[1],
            json!({"event": "needs-trust",
                   "publishers": [{"name": "someone", "mods": ["tgg.neutral-spawns"]}]})
        );
        let lines = run(&["someone".to_owned()], true);
        assert_eq!(lines.last().expect("line")["event"], "done");
        // Remembered: no question next time.
        let lines = run(&[], false);
        assert_eq!(lines[0]["mods"][0]["needs_trust"], false);
    }
}
