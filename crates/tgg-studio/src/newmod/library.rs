//! What New mod asks tgg-mod: the SDK's examples, which tools are
//! installed, making the mod, and the docs' address. The one place the
//! dialog meets the library.
//!
//! WIP: tgg-mod's `new_mod`, `sdk_examples`, `tools` and `docs_url` are
//! being moved out of tgg-cli; until they land these say so instead.

use std::path::{Path, PathBuf};

/// Which of the tools that build mods are installed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tools {
    pub gcc: Option<PathBuf>,
    pub cmake: Option<PathBuf>,
    pub git: Option<PathBuf>,
}

/// What became of the mod's git repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Repository {
    Committed,
    NoGit,
    NoIdentity,
    AlreadyThere,
    Failed(String),
}

/// A mod made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedMod {
    pub dir: PathBuf,
    pub example: String,
    pub sdk_version: String,
    pub repository: Repository,
}

/// A mod to make.
pub struct Spec {
    pub dir: PathBuf,
    pub id: tgg_mod::ModId,
    pub name: String,
    pub example: Option<String>,
}

const NOT_YET: &str = "tgg-mod can't start mods yet";

pub fn sdk_examples(_data: &Path) -> Result<Vec<String>, String> {
    Err(NOT_YET.to_owned())
}

pub fn tools() -> Tools {
    Tools::default()
}

pub fn new_mod(_data: &Path, _spec: &Spec) -> Result<CreatedMod, String> {
    Err(NOT_YET.to_owned())
}

pub fn docs_url(_version: &str, page: &str) -> String {
    format!("https://textures.gg/docs/mods/latest/{page}")
}
