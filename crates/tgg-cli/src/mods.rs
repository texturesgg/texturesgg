//! `tgg mod`: create, build and pack mods, look inside packages, libraries and
//! game builds, write catalogs, publish to textures.gg, and manage the mods
//! installed in the game's mods folder.

use crate::account::Site;
use crate::build::{self, Options, read};
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};
use tgg_mod::port::PortError;
use tgg_mod::{
    Catalog, CatalogEntry, Hooks, ModId, ModsDir, Package, PackageRef, Port, catalog, conflicts,
    decls, netplay, overlaps, package, unmet,
};

#[derive(Subcommand)]
pub enum ModCommand {
    /// Start a mod in DIR from the SDK's template (or another example), and
    /// build it once so editors find the game's headers.
    New {
        #[arg(default_value = ".", value_parser = dir_arg)]
        dir: PathBuf,
        /// The mod's id [default: DIR's name]
        #[arg(long)]
        id: Option<ModId>,
        /// The name players see [default: DIR's name]
        #[arg(long)]
        name: Option<String>,
        /// The SDK example to start from, such as lcancel-trainer.
        #[arg(long)]
        example: Option<String>,
        /// The SDK's folder or tgg-game-sdk.json [default: the SDK of the
        /// game in use]
        #[arg(long, env = "TGG_GAME_SDK")]
        sdk: Option<PathBuf>,
        #[command(flatten)]
        cc: Cc,
    },
    /// Build a mod's source against the game's SDK, check it as the game
    /// will, and pack it: DIR holds manifest.json, the C sources under src/,
    /// and any of include/, files/ and assets/. A mod without C sources packs
    /// without compiling.
    Build {
        #[arg(default_value = ".", value_parser = dir_arg)]
        dir: PathBuf,
        /// First unpack the source from this zip ("-" for stdin) into DIR,
        /// which must be empty or missing. Only manifest.json, src/,
        /// include/, files/ and assets/ are taken from it.
        #[arg(long)]
        source_zip: Option<PathBuf>,
        /// The commit of the reviewed source this is built from, which the
        /// package records: for the registry's builder.
        #[arg(long, value_parser = commit_arg)]
        source_commit: Option<String>,
        /// The class the registry's review gave it (display), which the
        /// package records: for the registry's builder.
        #[arg(long, value_enum)]
        class: Option<ClassArg>,
        /// The SDK's folder or tgg-game-sdk.json [default: the SDK of the
        /// game in use]
        #[arg(long, env = "TGG_GAME_SDK")]
        sdk: Option<PathBuf>,
        /// Build with -O0 -g instead of -O2, for gdb.
        #[arg(long)]
        debug: bool,
        #[command(flatten)]
        cc: Cc,
        /// The package zip to write [default: DIR/build/<id>-<version>.zip]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the package's path, SHA-256, size, manifest, canonical hooks
        /// and netplay class as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Bring a mod's source up to the SDK you have: rename what the decomp
    /// renamed (the SDK's rename maps), at exactly the spots the compiler
    /// points at, and build again until nothing changes. Reports what it
    /// renamed, and what's left for you.
    Fix {
        #[arg(default_value = ".", value_parser = dir_arg)]
        dir: PathBuf,
        /// Say what it would rename, without writing.
        #[arg(long)]
        check: bool,
        /// The SDK's folder or tgg-game-sdk.json [default: the SDK of the
        /// game in use]
        #[arg(long, env = "TGG_GAME_SDK")]
        sdk: Option<PathBuf>,
        #[command(flatten)]
        cc: Cc,
    },
    /// Build the mod for debugging, put it in the game's mods folder, and run
    /// the game with your disc image.
    Dev {
        #[arg(default_value = ".", value_parser = dir_arg)]
        dir: PathBuf,
        /// Rebuild on each change, and restart the game when it builds.
        #[arg(long)]
        watch: bool,
        /// Run the game under gdb (needs tgg port install --debug).
        #[arg(long)]
        gdb: bool,
        /// The installed tgg-melee version to run [default: the one in use]
        #[arg(long)]
        port: Option<String>,
        /// A local build of the port to run instead of an installed version:
        /// the folder with the tgg-melee executable and tgg-melee-sdk
        #[arg(long, env = "TGG_GAME", conflicts_with = "port")]
        game: Option<PathBuf>,
        /// Leave the dev build linked in the mods folder afterwards.
        #[arg(long)]
        keep: bool,
        #[command(flatten)]
        cc: Cc,
        /// Arguments for the game.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Create the mod on textures.gg from DIR's manifest.json, under your
    /// account. tgg mod publish does this the first time too.
    Register {
        #[arg(default_value = ".", value_parser = dir_arg)]
        dir: PathBuf,
    },
    /// Build the mod as tgg mod build does, then tag its version and push it
    /// to its repository on textures.gg, which builds the tag: DIR is the
    /// mod's git checkout. A mod that doesn't build isn't pushed.
    Publish {
        #[arg(default_value = ".", value_parser = dir_arg)]
        dir: PathBuf,
        /// Push without building here first; textures.gg still builds it.
        #[arg(long)]
        skip_build: bool,
        /// The SDK's folder or tgg-game-sdk.json to build with [default: the
        /// SDK of the game in use]
        #[arg(long, env = "TGG_GAME_SDK")]
        sdk: Option<PathBuf>,
        #[command(flatten)]
        cc: Cc,
    },
    /// Pack a built mod: DIR is the mod's installed folder, as the SDK's
    /// tgg_add_mod writes it (manifest.json, the library, and its folders).
    Pack {
        #[arg(value_parser = dir_arg)]
        dir: PathBuf,
        /// The package zip to write [default: <id>-<version>.zip]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the package's path, SHA-256, size, manifest and netplay
        /// class as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print what a package zip, a mod library, or a game executable declares,
    /// as JSON.
    Inspect { file: PathBuf },
    /// Write a catalog of package zips, with URLs relative to the catalog.
    Catalog {
        #[arg(short, long)]
        output: PathBuf,
        #[arg(required = true)]
        packages: Vec<PathBuf>,
    },
    /// List the installed mods and sets. Works offline.
    List {
        #[arg(long)]
        json: bool,
    },
    /// The sets of mods on textures.gg.
    Sets,
    /// Install mods from textures.gg (ID, or ID@RANGE such as
    /// tgg.netplay@^0.2), a set of them (--set), or package zips. With
    /// nothing named, pick a set.
    Install {
        /// A set from tgg mod sets, such as competitive.
        #[arg(long, conflicts_with = "mods")]
        set: Option<String>,
        /// Don't ask before installing.
        #[arg(short, long)]
        yes: bool,
        #[command(flatten)]
        port: PortArg,
        #[arg(value_name = "ID[@RANGE]|ZIP", value_parser = word_arg)]
        mods: Vec<String>,
    },
    /// Update the sets and mods installed from textures.gg, and say when
    /// there's a newer tgg-melee.
    Update {
        /// Only this installed set.
        #[arg(long)]
        set: Option<String>,
        /// Don't ask before updating.
        #[arg(short, long)]
        yes: bool,
    },
    /// Turn installed mods on; with none named, pick them.
    #[command(visible_alias = "enable")]
    On {
        #[arg(value_parser = id_arg)]
        ids: Vec<ModId>,
    },
    /// Turn installed mods off (the game skips them); with none named, pick
    /// them.
    #[command(visible_alias = "disable")]
    Off {
        #[arg(value_parser = id_arg)]
        ids: Vec<ModId>,
    },
    /// Remove installed mods, or a set and the mods only it installed.
    Remove {
        #[arg(long, conflicts_with = "ids")]
        set: Option<String>,
        #[arg(required_unless_present = "set", value_parser = id_arg)]
        ids: Vec<ModId>,
    },
}

#[derive(Args)]
pub struct Cc {
    /// The C compiler: GCC 12 or later [default: gcc; on macOS Homebrew's
    /// gcc-15]
    #[arg(long, env = "CC")]
    pub cc: Option<PathBuf>,
}

impl Cc {
    pub fn resolve(&self) -> PathBuf {
        self.cc
            .clone()
            .filter(|cc| !cc.as_os_str().is_empty())
            .unwrap_or_else(crate::toolchain::default_cc)
    }
}

#[derive(Args, Default)]
pub struct PortArg {
    /// The game's executable, to refuse packages built for another game
    /// layout [default: the tgg-melee in use, if one is installed]
    #[arg(long, env = "TGG_PORT")]
    port: Option<PathBuf>,
}

impl PortArg {
    fn open(&self) -> Result<Option<Port>> {
        match &self.port {
            Some(path) => Ok(Some(
                Port::open(path).with_context(|| path.display().to_string())?,
            )),
            None if crate::ports::current_version()?.is_some() => {
                Ok(Some(crate::ports::resolve(None)?.port()?))
            }
            None => Ok(None),
        }
    }

    /// The game mods are fetched for: `--port`'s executable, else the
    /// tgg-melee in use as its release names it, if one is installed.
    fn game(&self) -> Result<Option<Port>> {
        if self.port.is_some() {
            return self.open();
        }
        let ports = crate::paths::ports()?;
        match tgg_mod::ports::current_port(&ports)? {
            Some(version) => Ok(Some(
                tgg_mod::ports::installed_port(&ports, &version)?.port(),
            )),
            None => Ok(None),
        }
    }
}

pub fn run(command: ModCommand, api: &str) -> Result<()> {
    match command {
        ModCommand::New {
            dir,
            id,
            name,
            example,
            sdk,
            cc,
        } => crate::scaffold::new(crate::scaffold::New {
            dir: &dir,
            id,
            name,
            example,
            sdk: sdk.as_deref(),
            cc: &cc.resolve(),
        }),
        ModCommand::Build {
            dir,
            source_zip,
            source_commit,
            class,
            sdk,
            debug,
            cc,
            output,
            json,
        } => {
            if let Some(zip) = source_zip {
                build::unpack_source(&zip, &dir)?;
            }
            let options = Options {
                sdk: sdk.as_deref(),
                cc: &cc.resolve(),
                debug,
            };
            let mut built = build::build(&dir, &options)?;
            built.package.manifest.source = source_commit;
            if let Some(ClassArg::Display) = class {
                tgg_mod::netplay::display_check(&built.package.manifest)
                    .map_err(|why| anyhow::anyhow!("it can't be a display mod: {why}"))?;
                built.package.manifest.class = Some(tgg_mod::ModClass::Display);
            }
            write_package(
                built.package,
                built.canonical,
                output,
                &dir.join(build::BUILD),
                json,
            )
        }
        ModCommand::Fix {
            dir,
            check,
            sdk,
            cc,
        } => crate::fix::fix(
            &dir,
            &Options {
                sdk: sdk.as_deref(),
                cc: &cc.resolve(),
                debug: false,
            },
            check,
        ),
        ModCommand::Dev {
            dir,
            watch,
            gdb,
            port,
            game,
            keep,
            cc,
            args,
        } => crate::dev::dev(crate::dev::Dev {
            dir: &dir,
            watch,
            gdb,
            version: port.as_deref(),
            game: game.as_deref(),
            keep,
            cc: &cc.resolve(),
            args: &args,
        }),
        ModCommand::Register { dir } => {
            crate::publish::register(Site::new(api)?.signed_in()?, &dir)
        }
        ModCommand::Publish {
            dir,
            skip_build,
            sdk,
            cc,
        } => {
            let cc = cc.resolve();
            let options = Options {
                sdk: sdk.as_deref(),
                cc: &cc,
                debug: false,
            };
            let manifest = crate::publish::check(&dir, (!skip_build).then_some(&options))?;
            crate::publish::publish(Site::new(api)?.signed_in()?, &dir, &manifest)
        }
        ModCommand::Pack { dir, output, json } => pack(&dir, output, json),
        ModCommand::Inspect { file } => inspect(&file),
        ModCommand::Catalog { output, packages } => write_catalog(&output, &packages),
        ModCommand::List { json } => {
            // Offline and without a game too.
            let port = PortArg::default().game().ok().flatten();
            with_player(api, port, true, |player| player.list(json))
        }
        ModCommand::Sets => {
            let port = PortArg::default().game().ok().flatten();
            with_player(api, port, true, |player| player.sets())
        }
        ModCommand::Install {
            set,
            yes,
            port,
            mods,
        } => {
            let packages: Vec<PathBuf> = mods
                .iter()
                .filter(|arg| is_package(arg))
                .map(PathBuf::from)
                .collect();
            if !packages.is_empty() {
                ensure!(
                    packages.len() == mods.len(),
                    "install package zips and mods from textures.gg in separate commands"
                );
                return install(&ModsDir::game(), port.open()?.as_ref(), &packages);
            }
            with_player(api, port.game()?, yes, |player| {
                if set.is_none() && mods.is_empty() {
                    if !crate::ui::interactive() {
                        bail!("name mods, or a set with --set; tgg mod sets lists them");
                    }
                    return match player.pick_set()? {
                        Some(set) => player.install(Some(&set), &[]),
                        None => Ok(()),
                    };
                }
                player.install(set.as_deref(), &mods)
            })
        }
        ModCommand::Update { set, yes } => with_player(api, PortArg::default().game()?, yes, |p| {
            p.update(set.as_deref())
        }),
        ModCommand::On { ids } => toggle(api, ids, true),
        ModCommand::Off { ids } => toggle(api, ids, false),
        ModCommand::Remove { set, ids } => {
            let port = PortArg::default().game().ok().flatten();
            with_player(api, port, true, |p| p.remove(set.as_deref(), &ids))
        }
    }
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ClassArg {
    Display,
}

fn commit_arg(arg: &str) -> Result<String, String> {
    if tgg_mod::manifest::is_commit(arg) {
        Ok(arg.to_owned())
    } else {
        Err("a commit is 40 lowercase hex digits".into())
    }
}

/// Whether `arg` of `tgg mod install` names a package zip rather than a mod
/// on textures.gg.
fn is_package(arg: &str) -> bool {
    arg.to_ascii_lowercase().ends_with(".zip") || Path::new(arg).is_file()
}

/// Run `work` as the player, in the game's mods folder, against the API
/// `api`. Ctrl-C cancels a download between mods, leaving each mod old or
/// new.
fn with_player(
    api: &str,
    port: Option<Port>,
    yes: bool,
    work: impl FnOnce(&crate::player::Player) -> Result<()>,
) -> Result<()> {
    let releases = std::env::var("TGG_RELEASES")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| tgg_mod::net::DEFAULT_RELEASES.to_owned());
    let net =
        tgg_mod::HttpNet::with_urls(concat!("tgg/", env!("CARGO_PKG_VERSION")), api, &releases);
    let signals = crate::signals::handle()?;
    let player = crate::player::Player {
        net: &net,
        mods: tgg_mod::default_mods_dir(),
        ports: crate::paths::ports()?,
        packages: tgg_mod::packages_dir(&crate::paths::data()?),
        port,
        out: crate::player::Out::stdout(),
        ask: !yes && std::io::IsTerminal::is_terminal(&std::io::stdin()),
        cancel: signals.stop_flag(),
    };
    work(&player)
}

fn open_package(path: &Path) -> Result<(Package, Vec<u8>)> {
    let bytes = read(path)?;
    let package = Package::from_zip(&bytes).with_context(|| path.display().to_string())?;
    Ok((package, bytes))
}

fn print_json(value: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(value).expect("json"));
}

fn pack(dir: &Path, output: Option<PathBuf>, json: bool) -> Result<()> {
    write_package(build::pack_folder(dir)?, None, output, Path::new(""), json)
}

/// Write `package` to `output`, or `<id>-<version>.zip` in `folder`, and
/// report it.
/// `canonical` is the library's hooks under their canonical names, when it
/// was checked against the game's symbols.
fn write_package(
    package: Package,
    canonical: Option<Hooks>,
    output: Option<PathBuf>,
    folder: &Path,
    json: bool,
) -> Result<()> {
    let manifest = &package.manifest;
    let output =
        output.unwrap_or_else(|| folder.join(format!("{}-{}.zip", manifest.id, manifest.version)));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| parent.display().to_string())?;
    }
    let zip = package.to_zip();
    std::fs::write(&output, &zip).with_context(|| output.display().to_string())?;
    if json {
        print_json(&json!({
            "path": output,
            "sha256": package::sha256_hex(&zip),
            "size": zip.len(),
            "manifest": manifest,
            "canonical_hooks": canonical,
            "netplay": netplay::classify(manifest),
        }));
    } else {
        let hooks = &manifest.hooks;
        println!(
            "{}: {} {} ({} before, {} after, {} replaced, {} events)",
            output.display(),
            manifest.id,
            manifest.version,
            hooks.before.len(),
            hooks.after.len(),
            hooks.replaces.len(),
            manifest.events.len()
        );
    }
    Ok(())
}

fn inspect(path: &Path) -> Result<()> {
    let bytes = read(path)?;
    if bytes.starts_with(b"PK\x03\x04") {
        let package = Package::from_zip(&bytes).with_context(|| path.display().to_string())?;
        print_json(&json!({
            "sha256": package::sha256_hex(&bytes),
            "size": bytes.len(),
            "manifest": package.manifest,
            "netplay": netplay::classify(&package.manifest),
        }));
        return Ok(());
    }
    match Port::open(path) {
        Ok(port) => {
            print_json(&json!({
                "api": tgg_mod::API,
                "game_abi": port.game_abi,
                "name": port.name,
                "target": port.target,
                "version": port.version,
            }));
            return Ok(());
        }
        Err(PortError::NoRuntime) => {}
        Err(error) => return Err(error).with_context(|| path.display().to_string()),
    }
    let declared = decls::read(&bytes).with_context(|| path.display().to_string())?;
    ensure!(
        declared.game_abi.is_some(),
        "{} is neither a mod library nor a game build with the mod loader",
        path.display()
    );
    print_json(&json!({
        "game_abi": declared.game_abi,
        "target": declared.target,
        "api_version": declared.api_version,
        "state": declared.state,
        "init": declared.init,
        "hooks": declared.hooks,
        "events": declared.events,
        "content": declared.content,
        "settings": declared.settings,
        "symbols": declared.symbols,
        "exports": declared.exports,
        "imports": declared.imports,
    }));
    Ok(())
}

fn write_catalog(output: &Path, packages: &[PathBuf]) -> Result<()> {
    let base = output.parent().unwrap_or(Path::new(""));
    let mut mods = Vec::new();
    for path in packages {
        let (package, bytes) = open_package(path)?;
        let url = path
            .strip_prefix(base)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        mods.push(CatalogEntry {
            netplay: netplay::classify(&package.manifest),
            manifest: package.manifest,
            package: PackageRef {
                url,
                sha256: package::sha256_hex(&bytes),
                size: bytes.len() as u64,
                signature: None,
            },
            publisher: None,
            changes_game: true,
        });
    }
    mods.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    let catalog = Catalog {
        schema: catalog::SCHEMA,
        snapshot: None,
        mods,
        ids: Vec::new(),
    };
    std::fs::write(output, catalog.to_json()).with_context(|| output.display().to_string())?;
    println!("{}: {} mods", output.display(), catalog.mods.len());
    Ok(())
}

/// Install each package in order, refusing one built for another game layout
/// than `port`'s or one that replaces a function a turned-on mod already
/// replaces. A file another mod also ships is a warning naming the mod whose
/// copy the game serves.
fn install(mods: &ModsDir, port: Option<&Port>, packages: &[PathBuf]) -> Result<()> {
    for path in packages {
        let (package, _) = open_package(path)?;
        let manifest = &package.manifest;
        // A mod of files only fits the game, not one build of it.
        if let Some(port) = port
            && manifest.game_abi.is_some()
            && (manifest.game_abi.as_deref() != Some(port.game_abi.as_str())
                || manifest.target.as_deref() != Some(port.target.as_str()))
        {
            bail!(
                "{} is built for game layout {} on {}; tgg-melee {} is {} on {}",
                manifest.id,
                manifest.game_abi.as_deref().unwrap_or("none"),
                manifest.target.as_deref().unwrap_or("no target"),
                port.version,
                port.game_abi,
                port.target
            );
        }
        let installed = mods.list()?;
        let clashes = conflicts(manifest, &installed, None);
        if !clashes.is_empty() {
            let clashes: Vec<_> = clashes
                .iter()
                .map(|c| format!("replaces {} (so does {})", c.replaces, c.with))
                .collect();
            bail!("{} conflicts: {}", manifest.id, clashes.join(", "));
        }
        mods.install(&package)
            .with_context(|| format!("installing {}", manifest.id))?;
        println!("{} {}", manifest.id, manifest.version);
        for overlap in overlaps(manifest, &installed) {
            eprintln!(
                "warning: {} and {} both ship {}; the game uses {}'s",
                manifest.id, overlap.with, overlap.path, overlap.wins
            );
        }
        let missing = unmet(manifest, &installed);
        if !missing.is_empty() {
            eprintln!(
                "warning: {} won't load until these are installed and on: {}",
                manifest.id,
                missing.join(", ")
            );
        }
    }
    Ok(())
}

/// `help` where a folder or mod goes is a request for the command's help,
/// not a folder named help, unless one exists.
fn not_help(text: &str) -> std::result::Result<(), String> {
    if text.eq_ignore_ascii_case("help") && !Path::new(text).exists() {
        return Err("for this command's help, use --help".into());
    }
    Ok(())
}

fn dir_arg(text: &str) -> std::result::Result<PathBuf, String> {
    not_help(text).map(|()| PathBuf::from(text))
}

fn word_arg(text: &str) -> std::result::Result<String, String> {
    not_help(text).map(|()| text.to_owned())
}

fn id_arg(text: &str) -> std::result::Result<ModId, String> {
    not_help(text)?;
    text.parse()
        .map_err(|error: tgg_mod::manifest::ManifestError| error.to_string())
}

/// `tgg mod on|off`: the mods `ids`, or with none, the ones picked from a
/// checklist in an interactive terminal.
fn toggle(api: &str, ids: Vec<ModId>, on: bool) -> Result<()> {
    with_player(api, None, true, |player| {
        let ids = if ids.is_empty() {
            if !crate::ui::interactive() {
                bail!(
                    "name the mods: tgg mod {} ID...",
                    if on { "on" } else { "off" }
                );
            }
            player.pick_mods(on)?
        } else {
            ids
        };
        player.set_enabled(&ids, on)
    })
}
