//! `tgg mod dev`: build a mod for debugging, put it in the game's mods
//! folder, and run the game, again on every change with `--watch`.
//!
//! The build is installed to `build/dev/<id>/` in the mod's folder, and the
//! mods folder gets a link `<id>` to it, or on Windows a copy of it (Windows
//! won't follow a junction a standard user made). A registry install of the same mod
//! there is moved aside to `.~dev-<id>` (the game skips dot folders) and put
//! back when the session ends. A dev build never matches a registry build's
//! hash, so it never matches a netplay peer's.

use crate::build::{self, Options};
use crate::ports::{self, Installed};
use crate::signals::Signals;
use anyhow::{Context, Result, ensure};
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
    /// A local build of the port (its build folder) to run instead
    pub game: Option<&'a Path>,
    pub keep: bool,
    pub cc: &'a Path,
    pub args: &'a [String],
}

pub fn dev(dev: Dev) -> Result<()> {
    ensure!(
        !(dev.watch && dev.gdb),
        "--watch restarts the game on each change, which gdb can't follow; use one"
    );
    let (installed, sdk) = match dev.game {
        // A local build carries its debug info and its SDK in the build folder
        Some(folder) => {
            let installed = Installed {
                version: format!("the build in {}", folder.display()),
                folder: folder.to_owned(),
            };
            ensure!(
                installed.executable().is_file(),
                "{} has no {}: --game wants the port's build folder",
                folder.display(),
                crate::ports::EXECUTABLE
            );
            let sdk = folder.join("tgg-melee-sdk");
            ensure!(
                sdk.join("tgg-game-sdk.json").is_file(),
                "{} has no tgg-melee-sdk folder: build the port first",
                folder.display()
            );
            (installed, sdk)
        }
        None => {
            let installed = ports::resolve(dev.version)?;
            if dev.gdb {
                ensure!(
                    installed.has_debug_info(),
                    "gdb needs the game's debug info: tgg port install {} --debug",
                    installed.version
                );
            }
            let sdk = crate::sdks::for_installed(&installed)?;
            (installed, sdk)
        }
    };
    crate::ports::check_runtime(&installed)?;
    let iso = crate::config::iso_or_ask()?;
    if tgg_mod::is_script(dev.dir) {
        return script_dev(&dev, &installed, &iso, &sdk);
    }
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
        run_gdb(&installed, &iso, dev.args, &[])
    } else if dev.watch {
        watch(&dev, &installed, &iso, &id, &options, &signals, &link)
    } else {
        let mut game = Game::start(&installed, &iso, dev.args, &id, &[])?;
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

/// The parts of a script mod's folder the game reads.
const SCRIPT_PARTS: &[&str] = &["manifest.json", "scripts", "assets", "files"];

/// A script mod: nothing to build. The mods folder's `<id>` is a link to the
/// mod's own folder (on Windows a copy kept in step with it), and the game
/// runs with `TGG_SCRIPT_DEV=<id>`, so it reloads a script on the frame after
/// it's saved.
fn script_dev(dev: &Dev, installed: &Installed, iso: &Path, sdk: &Path) -> Result<()> {
    let id = build::read_manifest(dev.dir)?.id;
    let sdk = tgg_mod::Sdk::open(sdk).with_context(|| sdk.display().to_string())?;
    tgg_mod::write_script_types(&sdk, dev.dir)?;
    let mods = ModsDir::game();
    let target = if cfg!(windows) {
        let staged = dev.dir.join(build::BUILD).join("dev").join(id.as_str());
        if staged.exists() {
            std::fs::remove_dir_all(&staged).with_context(|| staged.display().to_string())?;
        }
        sync_script(dev.dir, &staged)?;
        staged
    } else {
        std::path::absolute(dev.dir)?
    };
    let link = Link::make(&mods, &id, &target)?;
    warn_about(&mods, &id)?;
    let signals = crate::signals::handle()?;
    let env = [("TGG_SCRIPT_DEV", id.as_str())];
    let result = if dev.gdb {
        run_gdb(installed, iso, dev.args, &env)
    } else {
        let mut game = Game::start(installed, iso, dev.args, &id, &env)?;
        eprintln!("Save a script and the game runs it on the next frame; Ctrl-C ends the session");
        let mut seen = snapshot(dev.dir);
        while game.running() && !signals.stopping() {
            std::thread::sleep(Duration::from_millis(100));
            if cfg!(windows) {
                let now = snapshot(dev.dir);
                if now != seen {
                    seen = now;
                    if let Err(error) = sync_script(dev.dir, &link.link) {
                        eprintln!("{}", red(&format!("{error:#}")));
                    }
                }
            }
        }
        if game.running() {
            std::thread::sleep(Duration::from_millis(500));
            game.stop();
            Ok(())
        } else {
            game.wait()
        }
    };
    if dev.keep {
        link.keep();
    } else {
        link.restore()?;
    }
    result
}

/// Bring `to` in step with the script mod in `from`: its [`SCRIPT_PARTS`],
/// each changed file written beside and renamed over the old one, so the
/// game never reads half a file, and files no longer in `from` removed.
fn sync_script(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| to.display().to_string())?;
    for part in SCRIPT_PARTS {
        sync_path(&from.join(part), &to.join(part))?;
    }
    Ok(())
}

fn sync_path(from: &Path, to: &Path) -> Result<()> {
    let context = || to.display().to_string();
    let Ok(meta) = std::fs::metadata(from) else {
        if to.is_dir() {
            std::fs::remove_dir_all(to).with_context(context)?;
        } else if to.exists() {
            std::fs::remove_file(to).with_context(context)?;
        }
        return Ok(());
    };
    if meta.is_dir() {
        std::fs::create_dir_all(to).with_context(context)?;
        for entry in std::fs::read_dir(from).with_context(|| from.display().to_string())? {
            let name = entry?.file_name();
            sync_path(&from.join(&name), &to.join(&name))?;
        }
        for entry in std::fs::read_dir(to).with_context(context)? {
            let name = entry?.file_name();
            if !from.join(&name).exists() {
                sync_path(&from.join(&name), &to.join(&name))?;
            }
        }
    } else if std::fs::read(from).ok() != std::fs::read(to).ok() {
        let mut new = to.as_os_str().to_owned();
        new.push(".~new");
        let new = PathBuf::from(new);
        std::fs::copy(from, &new).with_context(|| new.display().to_string())?;
        std::fs::rename(&new, to).with_context(context)?;
    }
    Ok(())
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

/// The mods folder's `<id>`, a link to the dev build (on Windows a copy of
/// it), and the registry install it moved aside.
struct Link {
    link: PathBuf,
    build: PathBuf,
    aside: PathBuf,
}

/// The file that marks a mods folder's `<id>` as a copy of a dev build.
const DEV_COPY: &str = ".tgg-dev";

/// Whether `link` is what a dev session put there: a link, or a marked copy.
fn is_dev(link: &Path) -> bool {
    std::fs::symlink_metadata(link).is_ok_and(|meta| meta.file_type().is_symlink())
        || link.join(DEV_COPY).is_file()
}

impl Link {
    fn make(mods: &ModsDir, id: &ModId, build: &Path) -> Result<Self> {
        let root = mods.root();
        std::fs::create_dir_all(root).with_context(|| root.display().to_string())?;
        let link = mods.folder(id, true);
        // `~` can't appear in an id, so this never names another mod.
        let aside = root.join(format!(".~dev-{id}"));
        if is_dev(&link) {
            // Left by a session that didn't end cleanly.
            remove_dev(&link)?;
        } else if std::fs::symlink_metadata(&link).is_ok() {
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
        let made = Self {
            link,
            build: std::path::absolute(build)?,
            aside,
        };
        made.place()?;
        Ok(made)
    }

    #[cfg(unix)]
    fn place(&self) -> Result<()> {
        std::os::unix::fs::symlink(&self.build, &self.link)
            .with_context(|| self.link.display().to_string())
    }

    #[cfg(windows)]
    fn place(&self) -> Result<()> {
        copy_dir(&self.build, &self.link)?;
        let marker = self.link.join(DEV_COPY);
        std::fs::write(&marker, b"").with_context(|| marker.display().to_string())
    }

    /// After a rebuild: a copy is made again; a link already shows it.
    fn refresh(&self) -> Result<()> {
        if cfg!(windows) {
            remove_dev(&self.link)?;
            self.place()?;
        }
        Ok(())
    }

    /// Leave the link in place.
    fn keep(self) {
        if cfg!(windows) {
            eprintln!("Left a copy of the dev build in {}", self.link.display());
        } else {
            eprintln!("Left {} linked to the dev build", self.link.display());
        }
    }

    /// Remove the link and put back what it replaced.
    fn restore(self) -> Result<()> {
        if is_dev(&self.link) {
            remove_dev(&self.link)?;
        }
        if self.aside.exists() && !self.link.exists() {
            std::fs::rename(&self.aside, &self.link)
                .with_context(|| format!("putting back {}", self.link.display()))?;
        }
        Ok(())
    }
}

/// Copy the folder `from` to `to`, which doesn't exist yet.
#[cfg(windows)]
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir(to).with_context(|| to.display().to_string())?;
    for entry in std::fs::read_dir(from).with_context(|| from.display().to_string())? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).with_context(|| target.display().to_string())?;
        }
    }
    Ok(())
}

/// Remove a dev session's link, not what it points to, or its copy.
fn remove_dev(link: &Path) -> Result<()> {
    let removed = if link.join(DEV_COPY).is_file()
        && !std::fs::symlink_metadata(link).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        std::fs::remove_dir_all(link)
    } else {
        // A link to a folder is a folder to Windows.
        std::fs::remove_file(link).or_else(|_| std::fs::remove_dir(link))
    };
    removed.with_context(|| link.display().to_string())
}

/// A running game, its output passed through with this mod's refusals in
/// red.
struct Game {
    child: Child,
    readers: Vec<std::thread::JoinHandle<()>>,
}

impl Game {
    fn start(
        installed: &Installed,
        iso: &Path,
        args: &[String],
        id: &ModId,
        env: &[(&str, &str)],
    ) -> Result<Self> {
        let mut child = Command::new(installed.executable())
            .arg(iso)
            .args(args)
            .envs(env.iter().copied())
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
        crate::ports::check_exit(status)
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

fn run_gdb(installed: &Installed, iso: &Path, args: &[String], env: &[(&str, &str)]) -> Result<()> {
    let status = crate::signals::run(
        Command::new("gdb")
            .envs(env.iter().copied())
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
    link: &Link,
) -> Result<()> {
    let mut seen = snapshot(dev.dir);
    let mut game = Some(Game::start(installed, iso, dev.args, id, &[])?);
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
                link.refresh()?;
                game = Some(Game::start(installed, iso, dev.args, id, &[])?);
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
