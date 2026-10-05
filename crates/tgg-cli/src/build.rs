//! Building a mod: its source, compiling it against the game's SDK, checking
//! the library as the game will, and packing it.

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::json;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tgg_mod::sdk::SdkError;
use tgg_mod::{Files, Hooks, Manifest, Package, Sdk, Symbols, decls, files, links, sdk};

/// The parts of a mod's folder that are its source, beside `manifest.json`.
pub const SOURCE: &[&str] = &["src", "include", "files", "assets"];

/// Where a build writes what it makes, inside the mod's folder.
pub const BUILD: &str = "build";

pub fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| path.display().to_string())
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    let path = dir.join("manifest.json");
    Manifest::parse(&read(&path)?).with_context(|| path.display().to_string())
}

/// The folders a mod ships beside its library, read from `dir`.
pub struct Folders {
    pub files: Files,
    pub assets: Files,
    pub include: Files,
}

impl Folders {
    pub fn read(dir: &Path) -> Result<Self> {
        Ok(Self {
            files: files::read_dir(dir, "files")?,
            assets: files::read_dir(dir, "assets")?,
            include: files::read_dir(dir, "include")?,
        })
    }

    pub fn pack(self, manifest: Manifest, library: Option<Vec<u8>>) -> Result<Package> {
        Ok(Package::pack(
            manifest,
            library,
            self.files,
            self.assets,
            self.include,
        )?)
    }
}

/// The C sources under `dir`'s src/ and its other folders; a mod needs
/// sources, files or assets.
pub fn mod_source(dir: &Path) -> Result<(Vec<PathBuf>, Folders)> {
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

/// The SDK at `path`, or else the one for the game in use, installed if it's
/// missing.
pub fn resolve_sdk(path: Option<&Path>) -> Result<Sdk> {
    let folder = match path {
        Some(path) => path.to_owned(),
        None => {
            if crate::ports::current_version()?.is_none() {
                bail!(
                    "building C needs the game's SDK: install the game with tgg port install, or pass --sdk"
                );
            }
            crate::sdks::for_installed(&crate::ports::resolve(None)?)?
        }
    };
    Sdk::open(&folder).with_context(|| folder.display().to_string())
}

/// A built mod.
pub struct Built {
    pub package: Package,
    /// The library's hooks by canonical name, when it has a library.
    pub canonical: Option<Hooks>,
}

/// How to build a mod.
pub struct Options<'a> {
    /// The SDK, or `None` to use the game in use's.
    pub sdk: Option<&'a Path>,
    pub cc: &'a Path,
    /// `-O0 -g` instead of `-O2`.
    pub debug: bool,
}

/// Compile the mod in `dir` against the SDK, check the library as the game
/// will check it, and pack it with the mod's folders. The compiler runs in
/// `dir` on relative paths, writing to a scratch folder inside it, so the
/// library carries no path of this machine. A mod without sources packs as
/// it is. Writes `build/compile_commands.json` for editors on the way.
pub fn build(dir: &Path, options: &Options) -> Result<Built> {
    let manifest = read_manifest(dir)?;
    let (sources, folders) = mod_source(dir)?;
    if sources.is_empty() {
        return Ok(Built {
            package: folders.pack(manifest, None)?,
            canonical: None,
        });
    }
    let sdk = resolve_sdk(options.sdk)?;
    ensure!(
        sdk.compiler == "GNU",
        "{} was built with {}; mods build with GCC",
        sdk.name,
        sdk.compiler
    );
    let symbols = Symbols::open(&sdk.symbols)?;
    write_compile_commands(dir, options, &sdk, &manifest, &sources)?;
    let scratch = Path::new(".tgg-build");
    let library = scratch.join(manifest.library_name());
    std::fs::create_dir_all(dir.join(scratch))?;
    let compiled = std::process::Command::new(options.cc)
        .args(sdk.compile_args(&manifest.id, &sources, &library, options.debug))
        .current_dir(dir)
        .status()
        .map_err(|error| anyhow!("couldn't run {}: {error}", options.cc.display()))
        .and_then(|status| {
            ensure!(
                status.success(),
                "{} failed: {status}",
                options.cc.display()
            );
            read(&dir.join(&library))
        });
    std::fs::remove_dir_all(dir.join(scratch))?;
    let bytes = compiled?;
    let canonical = check_library(&bytes, &sdk, &symbols)?;
    Ok(Built {
        package: folders.pack(manifest, Some(bytes))?,
        canonical: Some(canonical),
    })
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

/// `build/compile_commands.json`: each source's compile command, with
/// absolute paths, so clangd and other editors see the mod as GCC does.
fn write_compile_commands(
    dir: &Path,
    options: &Options,
    sdk: &Sdk,
    manifest: &Manifest,
    sources: &[PathBuf],
) -> Result<()> {
    let root = std::path::absolute(dir)?;
    let flags = sdk.compile_flags(&manifest.id, options.debug);
    let text = |arg: &OsString| arg.to_string_lossy().into_owned();
    let entries: Vec<_> = sources
        .iter()
        .map(|source| {
            let mut arguments = vec![options.cc.to_string_lossy().into_owned()];
            arguments.extend(flags.iter().map(text));
            arguments.extend(["-c".to_owned(), source.to_string_lossy().into_owned()]);
            json!({
                "directory": root,
                "file": root.join(source),
                "arguments": arguments,
            })
        })
        .collect();
    let build = dir.join(BUILD);
    std::fs::create_dir_all(&build).with_context(|| build.display().to_string())?;
    let path = build.join("compile_commands.json");
    let json = serde_json::to_string_pretty(&entries).expect("json") + "\n";
    std::fs::write(&path, json).with_context(|| path.display().to_string())
}

/// The largest source zip `build --source-zip` takes. Game files make mods
/// megabytes, a whole fighter tens of them.
const SOURCE_ZIP_LIMIT: u64 = 256 * 1024 * 1024;

/// Unpack a mod's source (`manifest.json` and the [`SOURCE`] folders) from
/// a zip into `dir`.
pub fn unpack_source(zip: &Path, dir: &Path) -> Result<()> {
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
