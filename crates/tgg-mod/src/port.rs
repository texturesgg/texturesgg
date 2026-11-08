//! A tgg-melee executable: which game layout and version it is.
//!
//! tgg-melee records the manifest API, its game layout, its name, its target
//! triple and its version in the executable's `tgg_port` section, each
//! NUL-terminated, so a tool can tell which packages fit a build without
//! running it. A build without the section has no mod loader.

use object::{Object, ObjectSection};
use std::path::{Path, PathBuf};

/// A game build that loads mods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub executable: PathBuf,
    /// The game layout mods must be built for.
    pub game_abi: String,
    /// The game's name: `tgg-melee`.
    pub name: String,
    /// The target triple it was built for, such as `x86_64-linux-gnu`.
    pub target: String,
    /// The game's version, such as `0.1.0`.
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("{0} is not a file")]
    NoExecutable(PathBuf),
    #[error("this build has no mod loader")]
    NoRuntime,
    #[error("this build loads {0} mods; this tool handles {api} mods", api = crate::API)]
    Api(String),
    #[error("the mod loader section of this build is malformed")]
    Malformed,
    #[error("{0} is not a program this tool reads: {1}")]
    Object(PathBuf, object::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Port {
    /// The game whose executable is `executable`.
    pub fn open(executable: &Path) -> Result<Self, PortError> {
        let executable = executable.to_owned();
        if !executable.is_file() {
            return Err(PortError::NoExecutable(executable));
        }
        let cache = object::ReadCache::new(std::fs::File::open(&executable)?);
        let file =
            object::File::parse(&cache).map_err(|e| PortError::Object(executable.clone(), e))?;
        let section = file
            .section_by_name("tgg_port")
            .ok_or(PortError::NoRuntime)?;
        let data = section
            .data()
            .map_err(|e| PortError::Object(executable.clone(), e))?;
        let mut fields = data
            .split(|&b| b == 0)
            .map(|field| std::str::from_utf8(field).map_err(|_| PortError::Malformed));
        let mut field = || match fields.next() {
            Some(Ok(field)) if !field.is_empty() => Ok(field.to_owned()),
            _ => Err(PortError::Malformed),
        };
        let api = field()?;
        if api != crate::API {
            return Err(PortError::Api(api));
        }
        Ok(Self {
            game_abi: field()?,
            name: field()?,
            target: field()?,
            version: field()?,
            executable,
        })
    }

    /// The folder the executable is in.
    pub fn folder(&self) -> &Path {
        self.executable.parent().unwrap_or(Path::new("."))
    }
}
