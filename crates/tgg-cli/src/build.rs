//! Building a mod: its source, building it with the game's SDK, checking the
//! library's records, and packing it.

use anyhow::{Context, Result, anyhow, bail, ensure};
use std::path::{Path, PathBuf};
use std::process::Command;
use tgg_mod::sdk::SdkError;
use tgg_mod::{Files, Hooks, Manifest, Package, Sdk, Symbols, decls, files, sdk};

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

/// Build the mod in `dir` with the SDK's own CMake (`TggMod.cmake`), check
/// the library's records against the SDK, and pack it with the mod's
/// folders. A mod without sources packs as it is.
///
/// Every build goes through a CMake project tgg writes in `build/tgg-project/`,
/// whatever CMakeLists.txt the mod has, so a mod builds here as it does on
/// textures.gg, and each SDK compiles, links and checks mods its own way.
/// CMake's `compile_commands.json` is copied to `build/` for editors.
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
    let root = std::path::absolute(dir)?;
    let build = root.join(BUILD);
    let project = build.join("tgg-project");
    let binary = build.join("tgg");
    write_project(&project, &sdk, &root, &sources)?;

    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(&project)
        .arg("-B")
        .arg(&binary)
        .arg(format!("-DCMAKE_C_COMPILER={}", options.cc.display()))
        .arg(format!(
            "-DTGG_MOD_DEBUG={}",
            if options.debug { "ON" } else { "OFF" }
        ));
    // Ninja when it's there; a new build folder only, since CMake keeps the
    // generator it started with.
    if !binary.join("CMakeCache.txt").exists() && on_path("ninja") {
        configure.args(["-G", "Ninja"]);
    }
    run(configure.arg("--log-level=WARNING"), "cmake")?;
    run(
        Command::new("cmake").arg("--build").arg(&binary),
        "the build",
    )?;

    let commands = binary.join("compile_commands.json");
    if commands.is_file() {
        std::fs::copy(&commands, build.join("compile_commands.json"))?;
    }
    let library = binary
        .join("mods")
        .join(manifest.id.as_str())
        .join(manifest.library_name());
    let bytes = read(&library)?;
    let symbols = Symbols::open(&sdk.symbols)?;
    let canonical = check_library(&bytes, &sdk, &symbols)?;
    Ok(Built {
        package: folders.pack(manifest, Some(bytes))?,
        canonical: Some(canonical),
    })
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// Run `command`, failing with `what` if it can't start or doesn't succeed.
fn run(command: &mut Command, what: &str) -> Result<()> {
    // Their output goes to stderr: stdout is for what tgg reports (--json).
    let status = command
        .stdout(std::io::stderr())
        .status()
        .map_err(|error| anyhow!("couldn't run {what}: {error}; tgg doctor says what's missing"))?;
    ensure!(status.success(), "{what} failed: {status}");
    Ok(())
}

/// The CMake project that builds the mod at `root` with the SDK: its sources
/// and manifest by absolute path, so the project lives apart from the mod.
fn write_project(project: &Path, sdk: &Sdk, root: &Path, sources: &[PathBuf]) -> Result<()> {
    // CMake reads `\` as an escape, and quotes end a string.
    let quote = |path: &Path| format!("\"{}\"", path.display().to_string().replace('\\', "/"));
    let sources: Vec<String> = sources.iter().map(|s| quote(&root.join(s))).collect();
    let text = format!(
        "# Written by tgg mod build: the mod in {root}, built with the SDK's TggMod.cmake.\n\
         cmake_minimum_required(VERSION 3.25)\n\
         project(tgg-mod LANGUAGES C)\n\
         set(TGG_SDK {sdk})\n\
         set(TGG_MODS_DIR \"${{CMAKE_BINARY_DIR}}/mods\")\n\
         include(\"${{TGG_SDK}}/TggMod.cmake\")\n\
         tgg_use_sdk(\"${{TGG_SDK}}\")\n\
         tgg_add_mod(mod\n    MANIFEST {manifest}\n    SOURCES\n        {sources})\n",
        root = root.display(),
        sdk = quote(&sdk.root),
        manifest = quote(&root.join("manifest.json")),
        sources = sources.join("\n        "),
    );
    std::fs::create_dir_all(project).with_context(|| project.display().to_string())?;
    let path = project.join("CMakeLists.txt");
    // Rewritten only when it changes, so CMake doesn't reconfigure for nothing.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text).with_context(|| path.display().to_string())?;
    }
    Ok(())
}

/// Check a built library's records against the SDK and every symbol it names
/// against the game's, as the game does at load (the SDK's own build checks
/// what it links against). Returns its hooks by canonical name.
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
    if !problems.is_empty() {
        bail!(
            "the game would refuse this mod:\n  {}",
            problems.join("\n  ")
        );
    }
    Ok(canonical)
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
