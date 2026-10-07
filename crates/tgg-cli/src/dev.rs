//! `tgg mod dev`: build a mod for debugging, put it in the game's mods
//! folder, and run the game, again on every change with `--watch`.
//!
//! The build is installed to `build/dev/<id>/` in the mod's folder, and the
//! mods folder gets a link `<id>` to it. A registry install of the same mod
//! there is moved aside to `.~dev-<id>` (the game skips dot folders) and put
//! back when the session ends. A dev build never matches a registry build's
//! hash, so it never matches a netplay peer's.

use crate::build::{self, Options};
use crate::ports::{self, Installed};
use crate::signals::Signals;
use anyhow::{Context, Result, bail, ensure};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime};
use tgg_mod::{ModId, ModsDir, conflicts, unmet};

pub struct Dev<'a> {
    pub dir: &'a Path,
    pub watch: bool,
    pub gdb: bool,
    pub version: Option<&'a str>,
    pub keep: bool,
    pub cc: &'a Path,
    pub args: &'a [String],
}

pub fn dev(dev: Dev) -> Result<()> {
    ensure!(
        !(dev.watch && dev.gdb),
        "--watch restarts the game on each change, which gdb can't follow; use one"
    );
    let installed = ports::resolve(dev.version)?;
    if dev.gdb {
        ensure!(
            installed.has_debug_info(),
            "gdb needs the game's debug info: tgg port install {} --debug",
            installed.version
        );
    }
    let iso = crate::config::iso()?;
    let sdk = crate::sdks::for_installed(&installed)?;
    let options = Options {
        sdk: Some(&sdk),
        cc: dev.cc,
        debug: true,
    };

    let id = build_dev(dev.dir, &options)?;
    let mods = ModsDir::game();
    let link = Link::make(
        &mods,
        &id,
        &dev.dir.join(build::BUILD).join("dev").join(id.as_str()),
    )?;
    warn_about(&mods, &id)?;

    // Ctrl-C and SIGTERM end the game (or gdb) too; tgg waits for it, then
    // puts the mods folder back.
    let signals = crate::signals::handle()?;

    let result = if dev.gdb {
        run_gdb(&installed, &iso, dev.args)
    } else if dev.watch {
        watch(&dev, &installed, &iso, &id, &options, &signals)
    } else {
        let mut game = Game::start(&installed, &iso, dev.args, &id)?;
        game.wait()
    };
    if dev.keep {
        link.keep();
    } else {
        link.restore()?;
    }
    result
}

/// Build the mod in `dir` and install it to `build/dev/<id>/`.
fn build_dev(dir: &Path, options: &Options) -> Result<ModId> {
    let built = build::build(dir, options)?;
    let id = built.package.manifest.id.clone();
    ModsDir::new(dir.join(build::BUILD).join("dev"))
        .install(&built.package)
        .context("installing the dev build")?;
    eprintln!("Built {id} {}", built.package.manifest.version);
    Ok(id)
}

/// Say before the game starts what would stop the mod from loading beside
/// the other installed mods.
fn warn_about(mods: &ModsDir, id: &ModId) -> Result<()> {
    let installed = mods.list()?;
    let Some(this) = installed.iter().find(|m| m.manifest.id == *id) else {
        return Ok(());
    };
    for conflict in conflicts(&this.manifest, &installed, None) {
        eprintln!(
            "{}",
            red(&format!(
                "{id} replaces {}, and so does {}; the one that loads later is refused",
                conflict.replaces, conflict.with
            ))
        );
    }
    let missing = unmet(&this.manifest, &installed);
    if !missing.is_empty() {
        eprintln!(
            "{}",
            red(&format!(
                "{id} needs these installed and on: {}",
                missing.join(", ")
            ))
        );
    }
    Ok(())
}

fn red(text: &str) -> String {
    if std::io::stderr().is_terminal() {
        format!("\x1b[31m{text}\x1b[0m")
    } else {
        text.to_owned()
    }
}

/// The link from the mods folder to the dev build, and the registry install
/// it moved aside.
struct Link {
    link: PathBuf,
    aside: PathBuf,
}

impl Link {
    fn make(mods: &ModsDir, id: &ModId, build: &Path) -> Result<Self> {
        let root = mods.root();
        std::fs::create_dir_all(root).with_context(|| root.display().to_string())?;
        let link = mods.folder(id, true);
        // `~` can't appear in an id, so this never names another mod.
        let aside = root.join(format!(".~dev-{id}"));
        match std::fs::symlink_metadata(&link) {
            // A link left by a session that didn't end cleanly.
            Ok(meta) if meta.file_type().is_symlink() => std::fs::remove_file(&link)?,
            Ok(_) => {
                ensure!(
                    !aside.exists(),
                    "{} and {} both exist; keep the one you want as {}",
                    link.display(),
                    aside.display(),
                    link.display()
                );
                std::fs::rename(&link, &aside)
                    .with_context(|| format!("moving {} aside", link.display()))?;
                eprintln!("Moved the installed {id} aside for this session");
            }
            Err(_) => {}
        }
        symlink(&std::path::absolute(build)?, &link)?;
        Ok(Self { link, aside })
    }

    /// Leave the link in place.
    fn keep(self) {
        eprintln!("Left {} linked to the dev build", self.link.display());
    }

    /// Remove the link and put back what it replaced.
    fn restore(self) -> Result<()> {
        if std::fs::symlink_metadata(&self.link).is_ok_and(|meta| meta.file_type().is_symlink()) {
            std::fs::remove_file(&self.link).with_context(|| self.link.display().to_string())?;
        }
        if self.aside.exists() && !self.link.exists() {
            std::fs::rename(&self.aside, &self.link)
                .with_context(|| format!("putting back {}", self.link.display()))?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).with_context(|| link.display().to_string())
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> Result<()> {
    bail!("tgg mod dev runs tgg-melee, which is for Linux")
}

/// A running game, its output passed through with this mod's refusals in
/// red.
struct Game {
    child: Child,
    readers: Vec<std::thread::JoinHandle<()>>,
}

impl Game {
    fn start(installed: &Installed, iso: &Path, args: &[String], id: &ModId) -> Result<Self> {
        let mut child = Command::new(installed.executable())
            .arg(iso)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("running {}", installed.executable().display()))?;
        let refused = format!("mods: {id}: refused");
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let readers = vec![
            pass_through(stdout, refused.clone(), false),
            pass_through(stderr, refused, true),
        ];
        crate::signals::handle()?.set_game(Some(child.id()));
        Ok(Self { child, readers })
    }

    fn finish(&mut self) {
        if let Ok(signals) = crate::signals::handle() {
            signals.set_game(None);
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }

    fn wait(&mut self) -> Result<()> {
        let status = self.child.wait()?;
        self.finish();
        if !status.success() {
            bail!("tgg-melee exited with {status}");
        }
        Ok(())
    }

    fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.finish();
    }
}

fn pass_through(
    from: impl std::io::Read + Send + 'static,
    refused: String,
    to_stderr: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let color = if to_stderr {
            std::io::stderr().is_terminal()
        } else {
            std::io::stdout().is_terminal()
        };
        for line in std::io::BufReader::new(from).lines() {
            let Ok(line) = line else { break };
            let line = if color && line.contains(&refused) {
                format!("\x1b[31m{line}\x1b[0m")
            } else {
                line
            };
            let _ = if to_stderr {
                writeln!(std::io::stderr(), "{line}")
            } else {
                writeln!(std::io::stdout(), "{line}")
            };
        }
    })
}

fn run_gdb(installed: &Installed, iso: &Path, args: &[String]) -> Result<()> {
    let status = crate::signals::run(
        Command::new("gdb")
            .args(["-ex", "set breakpoint pending on", "--args"])
            .arg(installed.executable())
            .arg(iso)
            .args(args),
    )
    .context("running gdb; is it installed?")?;
    ensure!(status.success(), "gdb exited with {status}");
    Ok(())
}

/// The state of every file a build reads, to notice a change.
fn snapshot(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let mut files = Vec::new();
    let mut pending: Vec<PathBuf> = build::SOURCE
        .iter()
        .map(|folder| dir.join(folder))
        .collect();
    pending.push(dir.join("manifest.json"));
    while let Some(path) = pending.pop() {
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                pending.extend(
                    entries
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.path()),
                );
            }
        } else if let Ok(modified) = meta.modified() {
            files.push((path, modified, meta.len()));
        }
    }
    files.sort();
    files
}

/// Run the game, and on each change to the mod rebuild it and, if it
/// builds, restart the game, until Ctrl-C.
fn watch(
    dev: &Dev,
    installed: &Installed,
    iso: &Path,
    id: &ModId,
    options: &Options,
    signals: &Signals,
) -> Result<()> {
    let mut seen = snapshot(dev.dir);
    let mut game = Some(Game::start(installed, iso, dev.args, id)?);
    eprintln!(
        "Watching {} for changes; Ctrl-C ends the session",
        dev.dir.display()
    );
    while !signals.stopping() {
        std::thread::sleep(Duration::from_millis(300));
        if let Some(running) = &mut game
            && !running.running()
        {
            running.finish();
            game = None;
            eprintln!("The game ended; it starts again on the next change");
        }
        let now = snapshot(dev.dir);
        if now == seen {
            continue;
        }
        seen = now;
        match build_dev(dev.dir, options) {
            Ok(_) => {
                if let Some(mut running) = game.take() {
                    running.stop();
                }
                game = Some(Game::start(installed, iso, dev.args, id)?);
            }
            Err(error) => eprintln!("{}", red(&format!("{error:#}"))),
        }
    }
    if let Some(mut running) = game.take() {
        if running.running() {
            // Ctrl-C reached it too; give it a moment to end on its own.
            std::thread::sleep(Duration::from_millis(500));
        }
        running.stop();
    }
    Ok(())
}
