//! `tgg-mod`: pack built mods, look inside packages, libraries and ports,
//! write catalogs, and manage the mods installed in a port.

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};
use tgg_mod::port::PortError;
use tgg_mod::{
    Catalog, CatalogEntry, Manifest, ModId, Netplay, Package, PackageRef, Port, catalog, conflicts,
    decls, package, unmet_imports,
};

#[derive(Parser)]
#[command(version, about = "Mod packages for tgg-mod-runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pack a built mod: DIR holds manifest.json and the library it names,
    /// as tgg_add_mod writes them.
    Pack {
        dir: PathBuf,
        /// The package zip to write [default: <id>-<version>.zip]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the package's path, SHA-256, size and manifest as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print what a package zip, a mod library, or a port executable declares,
    /// as JSON.
    Inspect { file: PathBuf },
    /// Write a catalog of package zips, with URLs relative to the catalog.
    Catalog {
        #[arg(short, long)]
        output: PathBuf,
        #[arg(required = true)]
        packages: Vec<PathBuf>,
    },
    /// List the mods installed in a port, in load order.
    List {
        #[command(flatten)]
        port: PortArg,
        #[arg(long)]
        json: bool,
    },
    /// Install package zips into a port, replacing installed versions.
    Install {
        #[command(flatten)]
        port: PortArg,
        #[arg(required = true)]
        packages: Vec<PathBuf>,
    },
    /// Turn installed mods on.
    Enable {
        #[command(flatten)]
        port: PortArg,
        #[arg(required = true)]
        ids: Vec<ModId>,
    },
    /// Turn installed mods off; the runtime skips them.
    Disable {
        #[command(flatten)]
        port: PortArg,
        #[arg(required = true)]
        ids: Vec<ModId>,
    },
    /// Remove installed mods.
    Remove {
        #[command(flatten)]
        port: PortArg,
        #[arg(required = true)]
        ids: Vec<ModId>,
    },
}

#[derive(Args)]
struct PortArg {
    /// The port's folder or executable.
    #[arg(long, env = "TGG_PORT")]
    port: PathBuf,
}

impl PortArg {
    fn open(&self) -> Result<Port> {
        Port::open(&self.port).with_context(|| self.port.display().to_string())
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Pack { dir, output, json } => pack(&dir, output, json),
        Command::Inspect { file } => inspect(&file),
        Command::Catalog { output, packages } => write_catalog(&output, &packages),
        Command::List { port, json } => list(&port.open()?, json),
        Command::Install { port, packages } => install(&port.open()?, &packages),
        Command::Enable { port, ids } => set_enabled(&port.open()?, &ids, true),
        Command::Disable { port, ids } => set_enabled(&port.open()?, &ids, false),
        Command::Remove { port, ids } => {
            let mods = port.open()?.mods();
            for id in &ids {
                mods.remove(id).with_context(|| format!("removing {id}"))?;
            }
            Ok(())
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| path.display().to_string())
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
    let manifest_path = dir.join("manifest.json");
    let manifest = Manifest::parse(&read(&manifest_path)?)
        .with_context(|| manifest_path.display().to_string())?;
    let library = read(&dir.join(&manifest.entry))?;
    let package = Package::pack(library, manifest)?;
    let manifest = &package.manifest;
    let output = output
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}.zip", manifest.id, manifest.version)));
    let zip = package.to_zip();
    std::fs::write(&output, &zip).with_context(|| output.display().to_string())?;
    if json {
        print_json(&json!({
            "path": output,
            "sha256": package::sha256_hex(&zip),
            "size": zip.len(),
            "manifest": manifest,
        }));
    } else {
        let hooks = &manifest.hooks;
        println!(
            "{}: {} {} ({} before, {} after, {} replaced)",
            output.display(),
            manifest.id,
            manifest.version,
            hooks.before.len(),
            hooks.after.len(),
            hooks.replaces.len()
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
        }));
        return Ok(());
    }
    match Port::open(path) {
        Ok(port) => {
            print_json(&json!({
                "runtime": tgg_mod::API,
                "game_abi": port.game_abi,
                "port": port.name,
            }));
            return Ok(());
        }
        Err(PortError::NoRuntime) => {}
        Err(error) => return Err(error).with_context(|| path.display().to_string()),
    }
    let declared = decls::read(&bytes).with_context(|| path.display().to_string())?;
    ensure!(
        declared.game_abi.is_some(),
        "{} is neither a mod library nor a port build with the mod loader",
        path.display()
    );
    print_json(&json!({
        "game_abi": declared.game_abi,
        "hooks": declared.hooks,
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
            manifest: package.manifest,
            package: PackageRef {
                url,
                sha256: package::sha256_hex(&bytes),
                size: bytes.len() as u64,
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

fn list(port: &Port, json: bool) -> Result<()> {
    let installed = port.mods().list()?;
    if json {
        print_json(&json!(
            installed
                .iter()
                .map(|mod_| json!({ "enabled": mod_.enabled, "manifest": mod_.manifest }))
                .collect::<Vec<_>>()
        ));
        return Ok(());
    }
    for mod_ in &installed {
        let manifest = &mod_.manifest;
        let netplay = match manifest.netplay {
            Netplay::Cosmetic => "cosmetic",
            Netplay::Gameplay => "gameplay",
        };
        let off = if mod_.enabled { "" } else { " (off)" };
        println!("{} {} {netplay}{off}", manifest.id, manifest.version);
    }
    Ok(())
}

/// Install each package in order, refusing one built for another game layout
/// or one that replaces a function a turned-on mod already replaces.
fn install(port: &Port, packages: &[PathBuf]) -> Result<()> {
    let mods = port.mods();
    for path in packages {
        let (package, _) = open_package(path)?;
        let manifest = &package.manifest;
        if manifest.game_abi.as_deref() != Some(port.game_abi.as_str()) {
            bail!(
                "{} is built for game layout {}; {} is {}",
                manifest.id,
                manifest.game_abi.as_deref().unwrap_or("none"),
                port.name,
                port.game_abi
            );
        }
        let installed = mods.list()?;
        let clashes = conflicts(manifest, &installed);
        if !clashes.is_empty() {
            let clashes: Vec<_> = clashes
                .iter()
                .map(|c| format!("{} (also replaced by {})", c.symbol, c.with))
                .collect();
            bail!("{} conflicts: {}", manifest.id, clashes.join(", "));
        }
        mods.install(&package)
            .with_context(|| format!("installing {}", manifest.id))?;
        println!("{} {}", manifest.id, manifest.version);
        let missing = unmet_imports(manifest, &installed);
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

fn set_enabled(port: &Port, ids: &[ModId], enabled: bool) -> Result<()> {
    let mods = port.mods();
    for id in ids {
        mods.set_enabled(id, enabled)
            .with_context(|| format!("turning {id} {}", if enabled { "on" } else { "off" }))?;
    }
    Ok(())
}
