//! Every vanilla Melee NTSC 1.02 costume and versus stage file, by size and
//! SHA-256 (`data/vanilla-files.json`, written by the
//! `vanilla_files` example from the clean ISO the reference catalog
//! records). Tells whether a slot in a player's ISO still holds its original
//! file, without any game content.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;

pub const VANILLA_FILES_JSON: &str = include_str!("../data/vanilla-files.json");

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VanillaFile {
    pub name: String,
    pub byte_length: usize,
    pub sha256: String,
}

#[derive(Deserialize)]
struct Table {
    files: Vec<VanillaFile>,
}

fn table() -> &'static HashMap<String, VanillaFile> {
    static TABLE: OnceLock<HashMap<String, VanillaFile>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let table: Table =
            serde_json::from_str(VANILLA_FILES_JSON).expect("the checked-in vanilla table parses");
        table
            .files
            .into_iter()
            .map(|file| (file.name.clone(), file))
            .collect()
    })
}

/// The vanilla file called `name` (`PlFcRe.dat`, `GrNLa.dat`), if the table
/// has one.
pub fn vanilla_file(name: &str) -> Option<&'static VanillaFile> {
    table().get(name)
}

/// Every vanilla costume and versus stage file.
pub fn vanilla_files() -> impl Iterator<Item = &'static VanillaFile> {
    table().values()
}

/// Whether `bytes` are exactly the vanilla file called `name`.
pub fn is_vanilla(name: &str, bytes: &[u8]) -> bool {
    vanilla_file(name).is_some_and(|file| {
        file.byte_length == bytes.len() && format!("{:x}", Sha256::digest(bytes)) == file.sha256
    })
}

#[cfg(test)]
mod tests {
    use super::{is_vanilla, vanilla_file};
    use crate::{CHARACTERS, STAGES};

    #[test]
    fn every_stage_and_fighter_has_vanilla_files() {
        for (stage, _) in STAGES {
            assert!(vanilla_file(stage).is_some(), "{stage}");
        }
        for (code, name) in CHARACTERS {
            assert!(
                vanilla_file(&format!("Pl{code}Nr.dat")).is_some(),
                "{name}'s neutral costume"
            );
        }
    }

    #[test]
    fn only_the_exact_bytes_are_vanilla() {
        assert!(!is_vanilla("PlFcRe.dat", b"not a costume"));
        assert!(!is_vanilla("PlXxYy.dat", b""));
        // The right length is not enough: the hash decides.
        let length = vanilla_file("PlFcRe.dat")
            .expect("in the table")
            .byte_length;
        assert!(!is_vanilla("PlFcRe.dat", &vec![0; length]));
    }
}
