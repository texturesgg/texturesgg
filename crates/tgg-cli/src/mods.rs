//! `tgg mod`: build and pack mods, look inside packages, libraries and game
//! builds, write catalogs, publish to textures.gg, and manage the mods
//! installed in the game's mods folder.

use crate::account::Site;
use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::{Args, Subcommand};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use tgg_mod::port::PortError;
use tgg_mod::sdk::SdkError;
use tgg_mod::{
    Catalog, CatalogEntry, Files, Hooks, Manifest, ModId, ModsDir, Package, PackageRef, Port, Sdk,
    Symbols, catalog, conflicts, decls, files, links, netplay, overlaps, package, sdk, unmet,
};

#[derive(Subcommand)]
pub enum ModCommand {
    /// Build a mod's source against the game's SDK and pack it: DIR holds
    /// manifest.json, the C sources under src/, and any of files/, assets/
    /// and include/. A mod without C sources packs without compiling.
    Build {
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// First unpack the source from this zip ("-" for stdin) into DIR,
        /// which must be empty or missing. Only manifest.json, src/,
        /// include/, files/ and assets/ are taken from it.
        #[arg(long)]
        source_zip: Option<PathBuf>,
        /// The game SDK's folder or tgg-game-sdk.json; needed to compile C.
        #[arg(long, env = "TGG_GAME_SDK")]
        sdk: Option<PathBuf>,
        /// The C compiler; it must be GCC.
        #[arg(long, env = "CC", default_value = "gcc")]
        cc: PathBuf,
        /// The package zip to write [default: <id>-<version>.zip]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the package's path, SHA-256, size, manifest, canonical hooks
        /// and netplay class as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Create the mod on textures.gg from DIR's manifest.json, under your
    /// account, and make DIR a git repository if it isn't one.
    New {
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
pub struct PortArg {
    /// The game's executable, to refuse packages built for another game
    /// layout.
    #[arg(long, env = "TGG_PORT")]
    port: Option<PathBuf>,
}

impl PortArg {
    fn open(&self) -> Result<Option<Port>> {
        self.port
            .as_deref()
            .map(|path| Port::open(path).with_context(|| path.display().to_string()))
            .transpose()
    }
}

pub fn run(command: ModCommand, api: &str) -> Result<()> {
    match command {
        ModCommand::Build {
            dir,
            source_zip,
            sdk,
            cc,
            output,
            json,
        } => {
            if let Some(zip) = source_zip {
                unpack_source(&zip, &dir)?;
            }
            build(&dir, sdk.as_deref(), &cc, output, json)
        }
        ModCommand::New { dir } => new(Site::new(api)?.signed_in()?, &dir),
        ModCommand::Publish { dir } => publish(Site::new(api)?.signed_in()?, &dir),
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

/// The folders a mod ships beside its library, read from `dir`.
struct Folders {
    files: Files,
    assets: Files,
    include: Files,
}

impl Folders {
    fn read(dir: &Path) -> Result<Self> {
        Ok(Self {
            files: files::read_dir(dir, "files")?,
            assets: files::read_dir(dir, "assets")?,
            include: files::read_dir(dir, "include")?,
        })
    }

    fn pack(self, manifest: Manifest, library: Option<Vec<u8>>) -> Result<Package> {
        Ok(Package::pack(
            manifest,
            library,
            self.files,
            self.assets,
            self.include,
        )?)
    }
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
    write_package(package, None, output, json)
}

/// The largest source zip `build --source-zip` takes. Game files make mods
/// megabytes, a whole fighter tens of them.
const SOURCE_ZIP_LIMIT: u64 = 256 * 1024 * 1024;

/// The parts of a mod's folder that are its source.
const SOURCE: &[&str] = &["src", "include", "files", "assets"];

/// Unpack a mod's source (`manifest.json` and the [`SOURCE`] folders) from
/// a zip into `dir`.
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
        let wanted = relative == Path::new("manifest.json")
            || SOURCE.iter().any(|folder| relative.starts_with(folder));
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

/// The C sources under `dir`'s src/ and its other folders; a mod needs
/// sources, files or assets.
fn mod_source(dir: &Path) -> Result<(Vec<PathBuf>, Folders)> {
    let folders = Folders::read(dir)?;
    match sdk::mod_sources(dir) {
        Ok(sources) => Ok((sources, folders)),
        Err(SdkError::NoSources) if !folders.files.is_empty() || !folders.assets.is_empty() => {
            Ok((Vec::new(), folders))
        }
        Err(SdkError::NoSources) => bail!(
            "{} has no C sources under src/ and nothing under files/ or assets/",
            dir.display()
        ),
        Err(error) => Err(error.into()),
    }
}

/// Compile the mod in `dir` with `cc` against `sdk`, check the library as the
/// game will check it, and pack it with the mod's folders. The compiler runs
/// in `dir` on relative paths, writing to a scratch folder inside it, so the
/// library carries no path of this machine. A mod without sources packs as it
/// is.
fn build(
    dir: &Path,
    sdk: Option<&Path>,
    cc: &Path,
    output: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let manifest = read_manifest(dir)?;
    let (sources, folders) = mod_source(dir)?;
    if sources.is_empty() {
        return write_package(folders.pack(manifest, None)?, None, output, json);
    }
    let sdk =
        sdk.ok_or_else(|| anyhow!("compiling src/ needs the game SDK: --sdk or TGG_GAME_SDK"))?;
    let sdk = Sdk::open(sdk)?;
    ensure!(
        sdk.compiler == "GNU",
        "{} was built with {}; mods build with GCC",
        sdk.name,
        sdk.compiler
    );
    let symbols = Symbols::open(&sdk.symbols)?;
    let scratch = Path::new(".tgg-build");
    let library = scratch.join(manifest.library_name());
    std::fs::create_dir_all(dir.join(scratch))?;
    let compiled = std::process::Command::new(cc)
        .args(sdk.compile_args(&manifest.id, &sources, &library, false))
        .current_dir(dir)
        .status()
        .with_context(|| format!("running {}", cc.display()))
        .and_then(|status| {
            ensure!(status.success(), "{} failed: {status}", cc.display());
            read(&dir.join(&library))
        });
    std::fs::remove_dir_all(dir.join(scratch))?;
    let bytes = compiled?;
    let canonical = check_library(&bytes, &sdk, &symbols)?;
    let package = folders.pack(manifest, Some(bytes))?;
    write_package(package, Some(canonical), output, json)
}

/// Check a built library as the game checks it at load: its records against
/// the SDK, every symbol it names against the game's, and what it links
/// against. Returns its hooks by canonical name.
fn check_library(library: &[u8], sdk: &Sdk, symbols: &Symbols) -> Result<Hooks> {
    let declared = decls::read(library)?;
    ensure!(
        declared.game_abi.as_deref() == Some(sdk.game_abi.as_str())
            && declared.target.as_deref() == Some(sdk.target.as_str())
            && declared.api_version.as_deref() == Some(sdk.api_version.as_str()),
        "the library declares game layout {}, target {} and mod API {}; the SDK is {}, {} and {}",
        declared.game_abi.as_deref().unwrap_or("none"),
        declared.target.as_deref().unwrap_or("none"),
        declared.api_version.as_deref().unwrap_or("none"),
        sdk.game_abi,
        sdk.target,
        sdk.api_version
    );
    let mut problems: Vec<String> = Vec::new();
    let canonical = match symbols.canonical_hooks(&declared.hooks) {
        Ok(hooks) => hooks,
        Err(errors) => {
            problems.extend(errors.iter().map(ToString::to_string));
            Hooks::default()
        }
    };
    for name in &declared.symbols {
        if let Err(error) = symbols.resolve(name) {
            problems.push(error.to_string());
        }
    }
    problems.extend(
        links::check(library, symbols, &sdk.glibc)?
            .iter()
            .map(ToString::to_string),
    );
    if !problems.is_empty() {
        bail!(
            "the game would refuse this mod:\n  {}",
            problems.join("\n  ")
        );
    }
    Ok(canonical)
}

/// Run git in `dir`, returning its trimmed output, or failing with its error.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("running git")?;
    ensure!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn read_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join("manifest.json");
    Manifest::parse(&read(&path)?).with_context(|| path.display().to_string())
}

/// Create the mod on textures.gg and make `dir` a git repository to publish
/// from.
fn new(site: &Site, dir: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct Created {
        slug: String,
    }
    let manifest = read_manifest(dir)?;
    mod_source(dir)?;
    let mut body = json!({ "slug": manifest.id, "name": manifest.name });
    if let Some(description) = &manifest.description {
        body["description"] = json!(description);
    }
    let created: Created = site.post("/api/code-mods", &body)?;
    if git(dir, &["rev-parse", "--git-dir"]).is_err() {
        git(dir, &["init", "--initial-branch=main"])?;
    }
    println!(
        "Created {} on textures.gg. Commit your source, then run tgg mod publish.",
        created.slug
    );
    Ok(())
}

/// Tag `v<version>` at HEAD (or reuse that tag if it is already there) and
/// push HEAD and the tag. The registry builds every tag pushed to it.
fn publish(site: &Site, dir: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct PushAccess {
        remote: String,
        token: String,
    }
    let manifest = read_manifest(dir)?;
    mod_source(dir)?;
    ensure!(
        git(dir, &["status", "--porcelain"])?.is_empty(),
        "commit your changes first; the registry builds what is committed"
    );
    let head = git(dir, &["rev-parse", "HEAD"])?;
    let tag = format!("v{}", manifest.version);
    match git(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag}^{{commit}}"),
        ],
    ) {
        Ok(commit) if commit == head => {}
        Ok(_) => bail!(
            "{tag} already tags another commit; bump the version in manifest.json to release again"
        ),
        Err(_) => {
            git(dir, &["tag", "-a", &tag, "-m", &tag])?;
            println!("Tagged {tag}");
        }
    }
    let access: PushAccess = site.post(
        &format!("/api/code-mods/{}/push-token", manifest.id),
        &json!({}),
    )?;
    let (remote, token) = (access.remote, access.token);
    // The token goes through git's environment, not its command line.
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "push",
            &remote,
            "HEAD:refs/heads/main",
            &format!("refs/tags/{tag}"),
        ])
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.extraHeader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!("Authorization: Bearer {token}"),
        )
        .status()
        .context("running git")?;
    ensure!(status.success(), "git push failed");
    println!(
        "Pushed {} {}. It builds in a few seconds; its page on textures.gg shows the result.",
        manifest.id, manifest.version
    );
    Ok(())
}

/// Write `package` to `output`, or `<id>-<version>.zip`, and report it.
/// `canonical` is the library's hooks under their canonical names, when it
/// was checked against the game's symbols.
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
