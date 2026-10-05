//! `tgg mod`: create, build and pack mods, look inside packages, libraries and
//! game builds, write catalogs, publish to textures.gg, and manage the mods
//! installed in the game's mods folder.

use crate::account::Site;
use crate::build::{self, Folders, Options, read, read_manifest};
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
        #[arg(default_value = ".")]
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
        #[command(flatten)]
        cc: Cc,
    },
    /// Build a mod's source against the game's SDK, check it as the game
    /// will, and pack it: DIR holds manifest.json, the C sources under src/,
    /// and any of include/, files/ and assets/. A mod without C sources packs
    /// without compiling.
    Build {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// First unpack the source from this zip ("-" for stdin) into DIR,
        /// which must be empty or missing. Only manifest.json, src/,
        /// include/, files/ and assets/ are taken from it.
        #[arg(long)]
        source_zip: Option<PathBuf>,
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
    /// Build the mod for debugging, put it in the game's mods folder, and run
    /// the game with your disc image.
    Dev {
        #[arg(default_value = ".")]
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
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Tag the mod's version and push it to its repository on textures.gg,
    /// which builds the tag: DIR is the mod's git checkout.
    Publish {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Pack a built mod: DIR is the mod's installed folder, as the SDK's
    /// tgg_add_mod writes it (manifest.json, the library, and its folders).
    Pack {
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
    /// List the installed mods, by id.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Install package zips, replacing installed versions.
    Install {
        #[command(flatten)]
        port: PortArg,
        #[arg(required = true)]
        packages: Vec<PathBuf>,
    },
    /// Turn installed mods on.
    Enable {
        #[arg(required = true)]
        ids: Vec<ModId>,
    },
    /// Turn installed mods off; the game skips them.
    Disable {
        #[arg(required = true)]
        ids: Vec<ModId>,
    },
    /// Remove installed mods.
    Remove {
        #[arg(required = true)]
        ids: Vec<ModId>,
    },
}

#[derive(Args)]
pub struct Cc {
    /// The C compiler: GCC 12 or later.
    #[arg(long, env = "CC", default_value = "gcc")]
    pub cc: PathBuf,
}

#[derive(Args)]
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
}

pub fn run(command: ModCommand, api: &str) -> Result<()> {
    match command {
        ModCommand::New {
            dir,
            id,
            name,
            example,
            cc,
        } => crate::scaffold::new(crate::scaffold::New {
            dir: &dir,
            id,
            name,
            example,
            cc: &cc.cc,
        }),
        ModCommand::Build {
            dir,
            source_zip,
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
                cc: &cc.cc,
                debug,
            };
            let built = build::build(&dir, &options)?;
            write_package(
                built.package,
                built.canonical,
                output,
                &dir.join(build::BUILD),
                json,
            )
        }
        ModCommand::Dev {
            dir,
            watch,
            gdb,
            port,
            keep,
            cc,
            args,
        } => crate::dev::dev(crate::dev::Dev {
            dir: &dir,
            watch,
            gdb,
            version: port.as_deref(),
            keep,
            cc: &cc.cc,
            args: &args,
        }),
        ModCommand::Register { dir } => {
            crate::publish::register(Site::new(api)?.signed_in()?, &dir)
        }
        ModCommand::Publish { dir } => crate::publish::publish(Site::new(api)?.signed_in()?, &dir),
        ModCommand::Pack { dir, output, json } => pack(&dir, output, json),
        ModCommand::Inspect { file } => inspect(&file),
        ModCommand::Catalog { output, packages } => write_catalog(&output, &packages),
        ModCommand::List { json } => list(&ModsDir::game(), json),
        ModCommand::Install { port, packages } => {
            install(&ModsDir::game(), port.open()?.as_ref(), &packages)
        }
        ModCommand::Enable { ids } => set_enabled(&ModsDir::game(), &ids, true),
        ModCommand::Disable { ids } => set_enabled(&ModsDir::game(), &ids, false),
        ModCommand::Remove { ids } => {
            let mods = ModsDir::game();
            for id in &ids {
                mods.remove(id).with_context(|| format!("removing {id}"))?;
            }
            Ok(())
        }
    }
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
    let manifest = read_manifest(dir)?;
    let library_path = dir.join(manifest.library_name());
    // Without a named entry, a folder with no library is a mod of files only.
    let library = if manifest.entry.is_none() && !library_path.exists() {
        None
    } else {
        Some(read(&library_path)?)
    };
    let package = Folders::read(dir)?.pack(manifest, library)?;
    write_package(package, None, output, Path::new(""), json)
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
        });
    }
    mods.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    let catalog = Catalog {
        schema: catalog::SCHEMA,
        mods,
    };
    std::fs::write(output, catalog.to_json()).with_context(|| output.display().to_string())?;
    println!("{}: {} mods", output.display(), catalog.mods.len());
    Ok(())
}

/// How `tgg mod list` names a netplay class.
fn netplay_label(class: tgg_mod::Netplay) -> &'static str {
    match class {
        tgg_mod::Netplay::Code => "counts for netplay: code",
        tgg_mod::Netplay::Files => "counts for netplay: files",
        tgg_mod::Netplay::Costumes => "counts for netplay unless its costumes only change looks",
        tgg_mod::Netplay::Data => "free for netplay",
    }
}

fn list(mods: &ModsDir, json: bool) -> Result<()> {
    let installed = mods.list()?;
    if json {
        print_json(&json!(
            installed
                .iter()
                .map(|mod_| json!({
                    "enabled": mod_.enabled,
                    "netplay": netplay::classify(&mod_.manifest),
                    "manifest": mod_.manifest,
                }))
                .collect::<Vec<_>>()
        ));
        return Ok(());
    }
    if installed.is_empty() {
        println!("No mods in {}", mods.root().display());
    }
    for mod_ in &installed {
        let manifest = &mod_.manifest;
        let off = if mod_.enabled { "" } else { ", off" };
        println!(
            "{} {} ({}{off})",
            manifest.id,
            manifest.version,
            netplay_label(netplay::classify(manifest))
        );
    }
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

fn set_enabled(mods: &ModsDir, ids: &[ModId], enabled: bool) -> Result<()> {
    for id in ids {
        mods.set_enabled(id, enabled)
            .with_context(|| format!("turning {id} {}", if enabled { "on" } else { "off" }))?;
    }
    Ok(())
}
