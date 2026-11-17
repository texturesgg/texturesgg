//! `tgg set`: your sets, over [`tgg_mod::sets`]. A set runs with its own
//! mods for that session; your mods folder is the set "Installed mods" and
//! isn't touched.

use anyhow::{Result, anyhow, bail};
use clap::Subcommand;
use std::path::Path;
use tgg_mod::sets::{self, Class, Group, INSTALLED, MySet, ONLINE_PLAY, Want};
use tgg_mod::{Error, Local, ModId};

#[derive(Subcommand)]
pub enum SetCommand {
    /// List your sets. * marks the one the game starts with.
    List,
    /// Add a set from textures.gg, like competitive.
    Get { name: String },
    /// Make a set. It builds on Online play (the Online menu) unless you
    /// name other sets or say --empty.
    New {
        name: String,
        /// Build on this set from textures.gg (up to 4).
        #[arg(long = "on", value_name = "SET")]
        on: Vec<String>,
        /// Start with nothing in it.
        #[arg(long, conflicts_with = "on")]
        empty: bool,
    },
    /// Save the mods you have on now as a new set.
    Save { name: String },
    /// Show a set's mods by where they come from, what each resolves to,
    /// and what decides its queue.
    Show {
        /// [default: the active one]
        set: Option<String>,
    },
    /// Add a mod to a set, or build it on a set from textures.gg (--on).
    Add {
        set: String,
        /// A mod id, like tgg.ucf.
        #[arg(required_unless_present = "on", conflicts_with = "on")]
        r#mod: Option<ModId>,
        /// latest, a version like 1.2.0, or a range like ^1.2.
        #[arg(long, default_value = "latest", conflicts_with = "on")]
        version: String,
        /// Build on this set from textures.gg.
        #[arg(long, value_name = "SET")]
        on: Option<String>,
    },
    /// Take a mod or a set it builds on out of a set.
    Drop {
        set: String,
        /// A mod id, or the link name of a set it builds on.
        line: String,
    },
    /// Load a mod in a set again.
    On { set: String, r#mod: ModId },
    /// Leave a mod out of a set, including one from a set it builds on.
    Off { set: String, r#mod: ModId },
    /// Copy a set.
    Copy {
        set: String,
        /// [default: "<name> copy"]
        name: Option<String>,
    },
    /// Rename a set.
    Rename { set: String, name: String },
    /// Remove a set. Its mods stay in the download cache.
    Remove { set: String },
    /// Start the game with a set (tgg port run, the app's Play).
    Use { set: String },
    /// Run the game with a set now.
    Run {
        /// [default: the active one]
        set: Option<String>,
        /// Download native mods without asking.
        #[arg(short, long)]
        yes: bool,
        /// Show the game's own output as it runs.
        #[arg(short, long)]
        verbose: bool,
    },
}

fn local() -> Result<Local> {
    let ports = crate::paths::ports()?;
    let current = tgg_mod::installed_ports(&ports)?
        .into_iter()
        .find(|install| install.current);
    Ok(tgg_mod::local(
        &tgg_mod::default_mods_dir(),
        current.map(|install| install.port()).as_ref(),
    )?)
}

/// The set a player named: by its name in any case, or its key.
fn find(data: &Path, local: &Local, name: &str) -> Result<MySet> {
    let wanted = name.trim().to_lowercase();
    sets::list(data, local)?
        .into_iter()
        .find(|set| set.key == wanted || set.name.to_lowercase() == wanted)
        .ok_or_else(|| anyhow!("you have no set called {name}. tgg set list shows them"))
}

/// The set a player named, if they can change its lines.
fn editable(data: &Path, local: &Local, name: &str) -> Result<MySet> {
    let set = find(data, local, name)?;
    if set.is_installed() {
        bail!(Error::InstalledSet);
    }
    if set.locked {
        bail!(Error::SetLocked(set.name));
    }
    Ok(set)
}

fn class_name(class: Class) -> &'static str {
    match class {
        Class::Gameplay => "has to match in netplay",
        Class::Display => "only you see it",
        Class::Offline => "offline only",
    }
}

pub fn run(command: SetCommand, api: &str) -> Result<()> {
    let data = crate::paths::data()?;
    let local = local()?;
    let net = || crate::profiles::http(api);
    match command {
        SetCommand::List => {
            let active = sets::active(&data);
            let published = tgg_mod::cache::cached_sets(&data).unwrap_or_default();
            for set in sets::list(&data, &local)? {
                let mark = if set.key == active { "*" } else { " " };
                println!(
                    "{mark} {}  {}",
                    set.name,
                    crate::ui::dim(summary(&set, &published))
                );
            }
            Ok(())
        }
        SetCommand::Get { name } => {
            let set = sets::get(&net(), &data, &name)?;
            println!("Added {}", set.name);
            Ok(())
        }
        SetCommand::New { name, on, empty } => {
            let set = sets::new(&data, &name)?;
            let (links, default) = match (empty, on.is_empty()) {
                (true, _) => (Vec::new(), false),
                (false, true) => (vec![ONLINE_PLAY.to_owned()], true),
                (false, false) => (on, false),
            };
            for link in links {
                match sets::build_on(&net(), &data, &local, &set.key, &link) {
                    Err(Error::NoSuchSet(_)) if default => println!(
                        "textures.gg has no Online play set yet, so {} doesn't have the Online menu",
                        set.name
                    ),
                    result => {
                        result?;
                    }
                }
            }
            println!("Made {}", set.name);
            Ok(())
        }
        SetCommand::Save { name } => {
            let (set, skipped) = sets::from_installed(&data, &local, &name)?;
            println!("Saved {} with {}", set.name, mods(set.lines.len()));
            for id in skipped {
                println!("Left out {id}: it isn't on textures.gg");
            }
            Ok(())
        }
        SetCommand::Show { set } => {
            let name = set.unwrap_or_else(|| sets::active(&data));
            show(api, &data, &find(&data, &local, &name)?)
        }
        SetCommand::Add {
            set,
            r#mod,
            version,
            on,
        } => {
            let mut set = editable(&data, &local, &set)?;
            if let Some(link) = on {
                let set = sets::build_on(&net(), &data, &local, &set.key, &link)?;
                println!("{} builds on {link}", set.name);
                return Ok(());
            }
            let id = r#mod.expect("clap asks for a mod without --on");
            let version: Want = version.parse().map_err(|error: String| anyhow!(error))?;
            set.add(id.clone(), version);
            sets::save(&data, &set)?;
            println!("Added {id} to {}", set.name);
            Ok(())
        }
        SetCommand::Drop { set, line } => {
            let mut set = editable(&data, &local, &set)?;
            if !set.drop_line(&line) {
                bail!("{} has no line for {line}", set.name);
            }
            sets::save(&data, &set)?;
            println!("Took {line} out of {}", set.name);
            Ok(())
        }
        SetCommand::On { set, r#mod } => {
            let mut set = editable(&data, &local, &set)?;
            set.leave_out(&r#mod, false);
            sets::save(&data, &set)?;
            println!("{} loads {}", set.name, r#mod);
            Ok(())
        }
        SetCommand::Off { set, r#mod } => {
            let mut set = editable(&data, &local, &set)?;
            set.leave_out(&r#mod, true);
            sets::save(&data, &set)?;
            println!("Left {} out of {}", r#mod, set.name);
            Ok(())
        }
        SetCommand::Copy { set, name } => {
            let from = find(&data, &local, &set)?;
            let copy = sets::copy(&data, &local, &from.key, name.as_deref())?;
            println!("Made {}", copy.name);
            Ok(())
        }
        SetCommand::Rename { set, name } => {
            let from = find(&data, &local, &set)?;
            let renamed = sets::rename(&data, &local, &from.key, &name)?;
            println!("Renamed {} to {}", from.name, renamed.name);
            Ok(())
        }
        SetCommand::Remove { set } => {
            let set = find(&data, &local, &set)?;
            sets::remove(&data, &set.key)?;
            println!("Removed {}", set.name);
            Ok(())
        }
        SetCommand::Use { set } => {
            let set = find(&data, &local, &set)?;
            sets::activate(&data, &set.key)?;
            println!("Using {}", set.name);
            Ok(())
        }
        SetCommand::Run { set, yes, verbose } => {
            let key = match set {
                Some(name) => find(&data, &local, &name)?.key,
                None => sets::active(&data),
            };
            run_set(api, &key, yes, verbose)
        }
    }
}

fn mods(count: usize) -> String {
    crate::ui::plural(count, "mod", "mods")
}

/// "Competitive + 3 mods", "5 mods", "by textures.gg".
fn summary(set: &MySet, published: &[tgg_mod::Set]) -> String {
    let own = set
        .lines
        .iter()
        .filter(|line| matches!(line, sets::Line::Mod(m) if !m.left_out))
        .count();
    let on: Vec<&str> = set
        .builds_on()
        .map(|link| {
            published
                .iter()
                .find(|s| s.name == link)
                .map_or(link, |s| s.title.as_str())
        })
        .collect();
    if set.locked {
        return "by textures.gg".into();
    }
    match (on.is_empty(), own) {
        (true, _) => mods(own),
        (false, 0) => on.join(" + "),
        (false, _) => format!("{} + {}", on.join(" + "), mods(own)),
    }
}

fn show(api: &str, data: &Path, set: &MySet) -> Result<()> {
    println!("{}", crate::ui::heading(&set.name));
    if set.is_installed() {
        for line in &set.lines {
            if let sets::Line::Mod(m) = line {
                let state = if m.left_out { " (off)" } else { "" };
                println!("  {} {}{state}", m.id, m.version);
            }
        }
        println!("These are the mods in your mods folder. tgg mod list says more");
        return Ok(());
    }
    let net = crate::profiles::http(api);
    let plan = sets::plan(
        &net,
        data,
        &tgg_mod::default_mods_dir(),
        set,
        sets::skins_on(),
    )?;
    if let Some(offline) = &plan.offline {
        println!("{offline}. Showing the mods it used last");
    }
    for notice in &plan.notices {
        println!("{notice}");
    }
    let mut heading = String::new();
    for m in &plan.mods {
        let group = match &m.group {
            Group::FromSet { title, .. } => format!("From {title}"),
            Group::Added => "Added".into(),
            Group::LeftOut => "Left out".into(),
            Group::Skins => "For your skins".into(),
            Group::Needed => "Needed by the others".into(),
        };
        if group != heading {
            println!("{group}:");
            heading = group;
        }
        let detail = match (&m.skipped, &m.version, m.class) {
            (Some(why), ..) => crate::ui::yellow(format!("skipped: {why}")),
            (None, Some(version), Some(class)) => {
                crate::ui::dim(format!("{version}, {}", class_name(class)))
            }
            _ => String::new(),
        };
        println!("  {}  {detail}", m.id);
    }
    let queue = plan.gameplay();
    if queue.is_empty() {
        println!("No gameplay mods: it searches with players who have none");
    } else {
        let names: Vec<String> = queue
            .iter()
            .map(|m| format!("{} {}", m.id, m.version))
            .collect();
        println!("Its queue comes from {}", names.join(", "));
    }
    Ok(())
}

/// Run the set `key`: Installed mods as `tgg port run` always has, any
/// other through its own mods folder for this session.
pub fn run_set(api: &str, key: &str, yes: bool, verbose: bool) -> Result<()> {
    if key == INSTALLED {
        return crate::ports::run_installed(api, None, &[], verbose);
    }
    let net = crate::profiles::http(api);
    let signals = crate::signals::handle()?;
    let ctx = crate::profiles::ctx(&net, signals.stop_flag())?;
    let set = sets::load(&ctx.data, &local()?, key)?;
    let mut plan = sets::plan(ctx.net, &ctx.data, &ctx.mods, &set, sets::skins_on())?;
    if let Some(offline) = &plan.offline {
        println!("{offline}. Using the mods it used last time");
    }
    for notice in &plan.notices {
        println!("{notice}");
    }
    for m in plan.skipped() {
        println!(
            "Playing without {}: {}",
            m.id,
            m.skipped.as_deref().unwrap_or_default()
        );
    }
    crate::profiles::ask_trust(&ctx, &mut plan.plan, yes)?;
    let prepared = crate::profiles::with_progress(|progress| {
        sets::prepare(ctx.net, &plan, &ctx.data, &set, progress, ctx.cancel)
    })?;
    let iso = crate::config::iso_or_ask()?;
    let log = crate::paths::state()?.join(crate::ports::GAME_LOG);
    let log = (!verbose).then_some(log.as_path());
    let mut running = tgg_mod::launch_profile(&prepared.port, &iso, &prepared, None, log)?;
    println!(
        "Starting tgg-melee {} with {}",
        prepared.port.version, set.name
    );
    signals.set_game(Some(running.id()));
    let status = running.wait();
    signals.set_game(None);
    crate::profiles::finish(status, &signals, &prepared.port.executable(), log)
}
