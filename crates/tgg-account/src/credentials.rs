//! `credentials.json`: the saved sign-in for each textures.gg API.
//!
//! The file maps an API base URL (no trailing `/`) to what's saved for it:
//!
//! ```json
//! { "https://api.textures.gg": { "token": "…", "name": "someone", "id": "…" } }
//! ```
//!
//! `id` and `display_name` are optional. Entries for other APIs, and fields
//! this version doesn't know, are kept as they are when one entry changes.

use crate::{Account, Error};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The credentials file. Every call reads it afresh, so a sign-in made by
/// another program (the CLI while the app is open) shows up at once.
#[derive(Debug, Clone)]
pub struct Credentials {
    path: PathBuf,
}

/// What's saved for one API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    pub token: String,
    /// The account's handle.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl Credential {
    pub fn new(token: impl Into<String>, account: &Account) -> Self {
        Self {
            token: token.into(),
            name: account.handle.clone(),
            id: account.id.clone(),
            display_name: account.display_name.clone(),
        }
    }

    /// The account this was saved for, as it was then.
    pub fn account(&self) -> Account {
        Account {
            handle: self.name.clone(),
            display_name: self.display_name.clone(),
            id: self.id.clone(),
        }
    }
}

impl Credentials {
    /// `credentials.json` under `textures.gg` in the user's config folder
    /// (`~/.config` on Linux, Application Support on macOS, AppData\Roaming
    /// on Windows).
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|config| config.join("textures.gg").join("credentials.json"))
    }

    /// The file at [`Credentials::default_path`].
    pub fn open_default() -> Result<Self, Error> {
        Self::default_path()
            .map(Self::at)
            .ok_or(Error::NoConfigFolder)
    }

    /// The file at `path`, which needn't exist yet.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What's saved for `api`, if anything.
    pub fn get(&self, api: &str) -> Result<Option<Credential>, Error> {
        let Some(entry) = self.read()?.remove(key(api)) else {
            return Ok(None);
        };
        serde_json::from_value(entry)
            .map(Some)
            .map_err(|error| self.error(error))
    }

    /// Save `credential` for `api`, replacing what was there.
    pub fn set(&self, api: &str, credential: &Credential) -> Result<(), Error> {
        let mut saved = self.read()?;
        let entry = serde_json::to_value(credential).map_err(|error| self.error(error))?;
        saved.insert(key(api).to_owned(), entry);
        self.write(&saved)
    }

    /// Forget what's saved for `api`, returning it.
    pub fn remove(&self, api: &str) -> Result<Option<Credential>, Error> {
        let mut saved = self.read()?;
        let Some(entry) = saved.remove(key(api)) else {
            return Ok(None);
        };
        self.write(&saved)?;
        // An entry this version can't read is still removed.
        Ok(serde_json::from_value(entry).ok())
    }

    fn read(&self) -> Result<Map<String, Value>, Error> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| self.error(error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
            Err(error) => Err(self.error(error)),
        }
    }

    /// Replace the file in one step, readable only by this user: write a
    /// temporary file beside it, then rename it over the old one.
    fn write(&self, saved: &Map<String, Value>) -> Result<(), Error> {
        let folder = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(folder).map_err(|error| self.error(error))?;
        let name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temporary = folder.join(format!(".{name}.{}.tmp", std::process::id()));
        let written = (|| {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            serde_json::to_writer_pretty(&mut file, saved)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)
        })();
        written.map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            self.error(error)
        })
    }

    fn error(&self, error: impl std::fmt::Display) -> Error {
        Error::Credentials {
            path: self.path.clone(),
            reason: error.to_string(),
        }
    }
}

/// The key an API is saved under: its base URL without a trailing `/`.
pub(crate) fn key(api: &str) -> &str {
    api.trim_end_matches('/')
}
