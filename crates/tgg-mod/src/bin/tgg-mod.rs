//! `tgg-mod`: build and pack mods, look inside packages, libraries and ports,
//! write catalogs, and manage the mods installed in a port.

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};
use tgg_mod::port::PortError;
use tgg_mod::{
    Catalog, CatalogEntry, Hooks, Layout, Manifest, ModId, Netplay, Package, PackageRef, Port, Sdk,
    Symbols, catalog, conflicts, decls, package, sdk, unmet_imports,
};

#[derive(Parser)]
#[command(version, about = "Mod packages for tgg-mod-runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a mod's source against a port's game SDK and pack it: DIR holds
    /// manifest.json and the C sources under src/.
    Build {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// First unpack the source from this zip ("-" for stdin) into DIR,
        /// which must be empty or missing. Only manifest.json and src/ are
        /// taken from it.
        #[arg(long)]
        source_zip: Option<PathBuf>,
        /// The game SDK's folder or tgg-game-sdk.json.
        #[arg(long, env = "TGG_GAME_SDK")]
        sdk: PathBuf,
        /// A layout file from `tgg-mod layout`: refuse hooks the game can't
        /// take, and report each hook's canonical name.
        #[arg(long)]
        layout: Option<PathBuf>,
        /// The C compiler; it must be GCC.
        #[arg(long, env = "CC", default_value = "gcc")]
        cc: PathBuf,
        /// The package zip to write [default: <id>-<version>.zip]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the package's path, SHA-256, size and manifest as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Read the functions a port build lets mods name, for a registry to check
    /// hooks against.
    Layout {
        /// The port's executable.
        executable: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
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
    /// The port's executable; mods install beside it in mods/.
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
        Command::Build {
            dir,
            source_zip,
            sdk,
            layout,
            cc,
            output,
            json,
        } => {
            if let Some(zip) = source_zip {
                unpack_source(&zip, &dir)?;
            }
            let layout = layout.map(|path| read_layout(&path)).transpose()?;
            build(&dir, &sdk, layout.as_ref(), &cc, output, json)
        }
        Command::Layout { executable, output } => write_layout(&executable, &output),
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
    write_package(Package::pack(library, manifest)?, None, output, json)
}

/// The largest source zip `build --source-zip` takes.
const SOURCE_ZIP_LIMIT: u64 = 8 * 1024 * 1024;

/// Unpack a mod's source (`manifest.json` and `src/`) from a zip into `dir`.
fn unpack_source(zip: &Path, dir: &Path) -> Result<()> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let reader: Box<dyn Read> = if zip == Path::new("-") {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(zip).with_context(|| zip.display().to_string())?)
    };
    reader.take(SOURCE_ZIP_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= SOURCE_ZIP_LIMIT,
        "the source zip is over {SOURCE_ZIP_LIMIT} bytes"
    );
    if dir.exists() {
        ensure!(
            std::fs::read_dir(dir)?.next().is_none(),
            "{} is not empty",
            dir.display()
        );
    }
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("the source zip")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        // enclosed_name refuses absolute paths and any `..`.
        let Some(relative) = entry.enclosed_name() else {
            bail!(
                "the source zip names {:?}, outside the source",
                entry.name()
            );
        };
        let wanted = relative == Path::new("manifest.json") || relative.starts_with("src");
        if !wanted || entry.is_dir() {
            continue;
        }
        let path = dir.join(&relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))?;
        let mut file = std::fs::File::create(&path).with_context(|| path.display().to_string())?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

/// Compile the mod in `dir` with `cc` against `sdk` and pack the library.
/// The compiler runs in `dir` on relative paths, writing to a scratch folder
/// inside it, so the library carries no path of this machine.
fn build(
    dir: &Path,
    sdk: &Path,
    layout: Option<&Layout>,
    cc: &Path,
    output: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let sdk = Sdk::open(sdk)?;
    ensure!(
        sdk.compiler == "GNU",
        "{} was built with {}; mods build with GCC",
        sdk.name,
        sdk.compiler
    );
    let manifest_path = dir.join("manifest.json");
    let manifest = Manifest::parse(&read(&manifest_path)?)
        .with_context(|| manifest_path.display().to_string())?;
    let sources = sdk::mod_sources(dir)?;
    let scratch = Path::new(".tgg-build");
    let library = scratch.join(&manifest.entry);
    std::fs::create_dir_all(dir.join(scratch))?;
    let status = std::process::Command::new(cc)
        .args(sdk.compile_args(&sources, &library))
        .current_dir(dir)
        .status()
        .with_context(|| format!("running {}", cc.display()))?;
    ensure!(status.success(), "{} failed: {status}", cc.display());
    let bytes = read(&dir.join(&library))?;
    std::fs::remove_dir_all(dir.join(scratch))?;
    let package = Package::pack(bytes, manifest)?;
    ensure!(
        package.manifest.game_abi.as_deref() == Some(sdk.game_abi.as_str())
            && package.manifest.target.as_deref() == Some(sdk.target.as_str()),
        "the library declares a game layout or target other than the SDK's {} {}",
        sdk.game_abi,
        sdk.target
    );
    let canonical = match layout {
        Some(layout) => {
            ensure!(
                layout.game_abi == sdk.game_abi && layout.target == sdk.target,
                "the layout file is for {} {}, the SDK for {} {}",
                layout.game_abi,
                layout.target,
                sdk.game_abi,
                sdk.target
            );
            let canonical = layout.symbols.canonical_hooks(&package.manifest.hooks);
            match canonical {
                Ok(hooks) => Some(hooks),
                Err(errors) => {
                    let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
                    bail!(
                        "the mod hooks functions the game can't take:\n  {}",
                        errors.join("\n  ")
                    );
                }
            }
        }
        None => None,
    };
    write_package(package, canonical, output, json)
}

fn read_layout(path: &Path) -> Result<Layout> {
    serde_json::from_slice(&read(path)?).with_context(|| path.display().to_string())
}

fn write_layout(executable: &Path, output: &Path) -> Result<()> {
    let port = Port::open(executable).with_context(|| executable.display().to_string())?;
    let symbols = Symbols::read(&read(executable)?).context("reading the symbol table")?;
    ensure!(
        !symbols.exported.is_empty(),
        "{} has no symbol table; ports ship with .symtab",
        executable.display()
    );
    let statics: usize = symbols.statics.values().map(|names| names.len()).sum();
    let layout = Layout {
        api: tgg_mod::API.to_owned(),
        game_abi: port.game_abi,
        target: port.target,
        port: port.name,
        symbols,
    };
    let json = serde_json::to_vec(&layout).expect("json");
    std::fs::write(output, json).with_context(|| output.display().to_string())?;
    println!(
        "{}: {} {} on {}, {} exported, {} static",
        output.display(),
        layout.port,
        layout.game_abi,
        layout.target,
        layout.symbols.exported.len(),
        statics
    );
    Ok(())
}

/// Write `package` to `output`, or `<id>-<version>.zip`, and report it.
/// `canonical` is the package's hooks under their canonical names, when a
/// layout was checked.
fn write_package(
    package: Package,
    canonical: Option<Hooks>,
    output: Option<PathBuf>,
    json: bool,
) -> Result<()> {
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
            "canonical_hooks": canonical,
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
                "target": port.target,
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
        "target": declared.target,
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
        if manifest.game_abi.as_deref() != Some(port.game_abi.as_str())
            || manifest.target.as_deref() != Some(port.target.as_str())
        {
            bail!(
                "{} is built for game layout {} on {}; {} is {} on {}",
                manifest.id,
                manifest.game_abi.as_deref().unwrap_or("none"),
                manifest.target.as_deref().unwrap_or("no target"),
                port.name,
                port.game_abi,
                port.target
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
