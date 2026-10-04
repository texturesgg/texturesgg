//! A port's game SDK, and the compiler command that builds a mod against it.
//!
//! tgg-mod-runtime's `tgg_prepare_game` writes the SDK beside a port build:
//! the headers a mod compiles against and `tgg-game-sdk.json`, which names
//! them by paths relative to itself, so the folder can move. A mod built
//! with the SDK's include path, definitions and options sees every game
//! struct as the game does.
//!
//! A mod's source follows one layout: `manifest.json` at its root and its C
//! sources under `src/`.

use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The SDK's file name inside its folder.
pub const FILE: &str = "tgg-game-sdk.json";

/// Flags every mod gets on top of the SDK's own, so a mod builds the same way
/// wherever it is built.
const MOD_FLAGS: &[&str] = &["-shared", "-fPIC", "-fvisibility=hidden", "-O2"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sdk {
    /// The port's name, such as `melee-pc`.
    pub name: String,
    /// The game layout mods built with this SDK load into.
    pub game_abi: String,
    /// The compiler family the game was built with, such as `GNU`.
    pub compiler: String,
    pub processor: String,
    /// The target triple the game was built for, such as `x86_64-linux-gnu`.
    pub target: String,
    pub include_dirs: Vec<PathBuf>,
    pub force_includes: Vec<PathBuf>,
    pub definitions: Vec<String>,
    pub options: Vec<String>,
}

#[derive(Deserialize)]
struct SdkFile {
    api: String,
    name: String,
    game_abi: String,
    compiler: String,
    processor: String,
    target: String,
    include_dirs: Vec<String>,
    #[serde(default)]
    force_includes: Vec<String>,
    #[serde(default)]
    definitions: Vec<String>,
    #[serde(default)]
    options: Vec<String>,
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
        let inside = |field: &'static str, paths: Vec<String>| {
            paths
                .into_iter()
                .map(|path| {
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
                })
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Self {
            include_dirs: inside("include_dirs", file.include_dirs)?,
            force_includes: inside("force_includes", file.force_includes)?,
            name: file.name,
            game_abi: file.game_abi,
            compiler: file.compiler,
            processor: file.processor,
            target: file.target,
            definitions: file.definitions,
            options: file.options,
        })
    }

    /// The arguments, after the compiler's own name, that build `sources`
    /// into the library `output`. Run it in the mod's folder, with `sources`
    /// relative to it, so no path of the build machine reaches the library.
    pub fn compile_args(&self, sources: &[PathBuf], output: &Path) -> Vec<OsString> {
        let mut args: Vec<OsString> = MOD_FLAGS.iter().map(OsString::from).collect();
        args.extend(self.options.iter().map(OsString::from));
        for dir in &self.include_dirs {
            args.push("-I".into());
            args.push(dir.into());
        }
        for file in &self.force_includes {
            args.push("-include".into());
            args.push(file.into());
        }
        args.extend(self.definitions.iter().map(|d| format!("-D{d}").into()));
        args.push(format!("-DTGG_GAME_ABI=\"{}\"", self.game_abi).into());
        args.push(format!("-DTGG_GAME_TARGET=\"{}\"", self.target).into());
        args.push("-o".into());
        args.push(output.into());
        args.extend(sources.iter().map(OsString::from));
        args
    }
}

/// Every `.c` file under `dir/src`, relative to `dir` and sorted, so the
/// compiler sees them in the same order everywhere.
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
        "api": "tgg/1", "name": "melee-pc", "game_abi": "888b9c012ddb068d",
        "compiler": "GNU", "processor": "x86_64", "target": "x86_64-linux-gnu",
        "include_dirs": ["include/runtime", "include/game/0"],
        "force_includes": ["include/game/0/pc/compat.h"],
        "definitions": ["TARGET_PC=1"],
        "options": ["-fsigned-char", "-fexec-charset=CP932"]
    }"#;

    #[test]
    fn paths_resolve_inside_the_sdk_and_never_leave_it() {
        let sdk = Sdk::parse(SDK.as_bytes(), Path::new("/opt/sdk")).expect("parse");
        assert_eq!(sdk.include_dirs[1], Path::new("/opt/sdk/include/game/0"));
        let args = sdk.compile_args(&["src/mod.c".into()], Path::new("out/mod.so"));
        let args: Vec<_> = args.iter().map(|a| a.to_str().expect("utf-8")).collect();
        assert!(args.contains(&"-DTGG_GAME_ABI=\"888b9c012ddb068d\""));
        assert_eq!(args.last(), Some(&"src/mod.c"));

        let escaping = SDK.replace("include/runtime", "../outside");
        assert!(matches!(
            Sdk::parse(escaping.as_bytes(), Path::new("/opt/sdk")),
            Err(SdkError::Path("include_dirs", _))
        ));
        let path_in_option = SDK.replace("-fsigned-char", "-include");
        assert!(matches!(
            Sdk::parse(path_in_option.as_bytes(), Path::new("/opt/sdk")),
            Err(SdkError::Option(_))
        ));
    }
}
