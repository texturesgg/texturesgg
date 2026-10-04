//! A port's executable, and whether it carries the runtime.
//!
//! tgg-mod-runtime records its API, the game layout it was built for, and
//! the port's name in the executable's `tgg_port` section, each
//! NUL-terminated. A build without the section has no mod loader.

use crate::install::ModsDir;
use object::{Object, ObjectSection};
use std::path::{Path, PathBuf};

/// A port build that carries the runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub executable: PathBuf,
    /// The game layout mods must be built for.
    pub game_abi: String,
    /// The port's name, such as `melee-pc`.
    pub name: String,
    /// The target triple it was built for, such as `x86_64-linux-gnu`.
    pub target: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("{0} is not a file")]
    NoExecutable(PathBuf),
    #[error("this build has no textures.gg mod loader")]
    NoRuntime,
    #[error("this build's mod loader is {0}; the app installs {api} mods", api = crate::API)]
    Api(String),
    #[error("the mod loader section of this build is malformed")]
    Malformed,
    #[error("{0} is not a program this app reads: {1}")]
    Object(PathBuf, object::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Port {
    /// The port whose executable is `executable`. Ports name their
    /// executables differently, so callers keep the path, not a folder.
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
        let game_abi = field()?;
        let name = field()?;
        let target = field()?;
        Ok(Self {
            executable,
            game_abi,
            name,
            target,
        })
    }

    /// The folder the executable is in.
    pub fn folder(&self) -> &Path {
        self.executable.parent().unwrap_or(Path::new("."))
    }

    /// The `mods/` folder beside the executable, which the runtime loads.
    pub fn mods(&self) -> ModsDir {
        ModsDir::new(self.folder().join("mods"))
    }
}
