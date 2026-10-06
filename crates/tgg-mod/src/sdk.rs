//! tgg-melee's mod SDK: what a mod is built with.
//!
//! Every tgg-melee build writes the SDK, and each release ships it as an
//! archive: the headers a mod compiles against, the game's symbol list, and
//! `tgg-game-sdk.json`, which names them by paths relative to itself, so the
//! folder can move. A mod built with the SDK's include path, definitions and
//! options sees every game struct as the game does.
//!
//! A mod's source follows one layout: `manifest.json` at its root and its C
//! sources under `src/`.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The SDK's file name inside its folder.
pub const FILE: &str = "tgg-game-sdk.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sdk {
    /// The SDK's folder.
    pub root: PathBuf,
    /// The game's name: `tgg-melee`.
    pub name: String,
    /// The game's version, such as `0.1.0` (`dev` for a build that isn't a
    /// release).
    pub version: String,
    /// The mod API `major.minor` mods built with it declare.
    pub api_version: String,
    /// The game layout mods built with this SDK load into.
    pub game_abi: String,
    /// The compiler family the game was built with, such as `GNU`.
    pub compiler: String,
    /// The version of that compiler. Any GCC from 12 on builds mods that fit.
    pub compiler_version: String,
    pub processor: String,
    /// The target triple the game was built for, such as `x86_64-linux-gnu`.
    pub target: String,
    pub include_dirs: Vec<PathBuf>,
    pub force_includes: Vec<PathBuf>,
    pub definitions: Vec<String>,
    pub options: Vec<String>,
    /// Libraries every mod links, as `-l` names.
    pub libraries: Vec<String>,
    /// The oldest glibc the game runs on: no mod may need a newer symbol
    /// version.
    pub glibc: String,
    /// The game's symbol list.
    pub symbols: PathBuf,
}

#[derive(Deserialize)]
struct SdkFile {
    api: String,
    api_version: String,
    name: String,
    version: String,
    game_abi: String,
    compiler: String,
    compiler_version: String,
    processor: String,
    target: String,
    include_dirs: Vec<String>,
    #[serde(default)]
    force_includes: Vec<String>,
    #[serde(default)]
    definitions: Vec<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    libraries: Vec<String>,
    glibc: String,
    symbols: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0} is not a game SDK: {1}")]
    Json(PathBuf, serde_json::Error),
    #[error("the SDK targets {0}; this tool builds {api} mods", api = crate::API)]
    Api(String),
    #[error("the SDK's {0} {1:?} must be a relative path inside the SDK")]
    Path(&'static str, String),
    #[error("the SDK's game layout {0:?} is not 16 hex digits")]
    GameAbi(String),
    #[error("the SDK's option {0:?} names a file; files go in force_includes")]
    Option(String),
    #[error("the SDK's library {0:?} is not a plain library name")]
    Library(String),
    #[error("the mod has no C sources under src/")]
    NoSources,
}

impl Sdk {
    /// Read the SDK at `path`: its folder, or its `tgg-game-sdk.json`.
    pub fn open(path: &Path) -> Result<Self, SdkError> {
        let file = if path.is_dir() {
            path.join(FILE)
        } else {
            path.to_owned()
        };
        let json = std::fs::read(&file).map_err(|e| SdkError::Io(file.clone(), e))?;
        let root = file.parent().unwrap_or(Path::new("."));
        Self::parse(&json, root).map_err(|error| match error {
            SdkError::Json(_, e) => SdkError::Json(file.clone(), e),
            other => other,
        })
    }

    /// Parse an SDK file whose relative paths start at `root`.
    pub fn parse(json: &[u8], root: &Path) -> Result<Self, SdkError> {
        let file: SdkFile =
            serde_json::from_slice(json).map_err(|e| SdkError::Json(PathBuf::new(), e))?;
        if file.api != crate::API {
            return Err(SdkError::Api(file.api));
        }
        if file.game_abi.len() != 16 || !file.game_abi.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(SdkError::GameAbi(file.game_abi));
        }
        // Options are flags only. A file an option names would escape the
        // SDK, and its path would make a build depend on where it ran.
        if let Some(option) = file.options.iter().find(|option| {
            option.as_str() == "-include" || option.contains('/') || option.starts_with("-I")
        }) {
            return Err(SdkError::Option(option.clone()));
        }
        if let Some(library) = file.libraries.iter().find(|library| {
            library.is_empty()
                || !library
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.'))
        }) {
            return Err(SdkError::Library(library.clone()));
        }
        let inside = |field: &'static str, path: String| {
            let relative = Path::new(&path);
            let escapes = relative.is_absolute()
                || relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)));
            if escapes {
                Err(SdkError::Path(field, path))
            } else {
                Ok(root.join(relative))
            }
        };
        let all_inside = |field: &'static str, paths: Vec<String>| {
            paths
                .into_iter()
                .map(|path| inside(field, path))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Self {
            root: root.to_owned(),
            include_dirs: all_inside("include_dirs", file.include_dirs)?,
            force_includes: all_inside("force_includes", file.force_includes)?,
            symbols: inside("symbols", file.symbols)?,
            name: file.name,
            version: file.version,
            api_version: file.api_version,
            game_abi: file.game_abi,
            compiler: file.compiler,
            compiler_version: file.compiler_version,
            processor: file.processor,
            target: file.target,
            definitions: file.definitions,
            options: file.options,
            libraries: file.libraries,
            glibc: file.glibc,
        })
    }
}

/// Every `.c` file under `dir/src`, but names starting with a dot, relative
/// to `dir` and sorted, so the compiler sees them in the same order
/// everywhere.
pub fn mod_sources(dir: &Path) -> Result<Vec<PathBuf>, SdkError> {
    let mut sources = Vec::new();
    let mut pending = vec![PathBuf::from("src")];
    while let Some(relative) = pending.pop() {
        let entries = match std::fs::read_dir(dir.join(&relative)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && relative == Path::new("src") => {
                break;
            }
            Err(e) => return Err(SdkError::Io(dir.join(&relative), e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| SdkError::Io(dir.join(&relative), e))?;
            // Names starting with a dot are left out, as from a package.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = relative.join(entry.file_name());
            let kind = entry
                .file_type()
                .map_err(|e| SdkError::Io(dir.join(&path), e))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "c") {
                sources.push(path);
            }
        }
    }
    if sources.is_empty() {
        return Err(SdkError::NoSources);
    }
    sources.sort();
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDK: &str = r#"{
        "api": "tgg-melee/0", "api_version": "0.1", "name": "tgg-melee", "version": "0.1.0",
        "game_abi": "6a0e926ca3e90452",
        "compiler": "GNU", "compiler_version": "15.3.0", "processor": "x86_64",
        "target": "x86_64-linux-gnu",
        "include_dirs": ["include/game/0", "game", "include"],
        "definitions": ["TARGET_PC", "bool=int"],
        "options": ["-std=gnu17", "-fwrapv"],
        "force_includes": ["include/tgg/glibc.h"],
        "libraries": ["m"],
        "glibc": "2.34",
        "symbols": "symbols.txt"
    }"#;

    #[test]
    fn paths_resolve_inside_the_sdk_and_never_leave_it() {
        let sdk = Sdk::parse(SDK.as_bytes(), Path::new("/opt/sdk")).expect("parse");
        assert_eq!(sdk.include_dirs[1], Path::new("/opt/sdk/game"));

        let escaping = SDK.replace("\"game\"", "\"../outside\"");
        assert!(matches!(
            Sdk::parse(escaping.as_bytes(), Path::new("/opt/sdk")),
            Err(SdkError::Path("include_dirs", _))
        ));
        let path_in_option = SDK.replace("-fwrapv", "-include");
        assert!(matches!(
            Sdk::parse(path_in_option.as_bytes(), Path::new("/opt/sdk")),
            Err(SdkError::Option(_))
        ));
    }
}
