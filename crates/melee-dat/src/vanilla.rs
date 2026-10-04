//! Every vanilla Melee NTSC 1.02 costume, fighter data, effects and versus
//! stage file, by size and SHA-256, and the fingerprint of each model
//! fighters' costumes share (`data/vanilla-files.json`, written by the
//! `vanilla_files` example from the clean ISO the reference catalog
//! records). Tells whether a slot in a player's ISO still holds its original
//! file, and whether a shared model still draws as shipped, without any game
//! content.

use crate::file_names::Character;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;

const VANILLA_FILES_JSON: &str = include_str!("../data/vanilla-files.json");

/// One file of the game as shipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VanillaFile {
    pub name: String,
    pub byte_length: usize,
    pub sha256: String,
}

#[derive(Deserialize)]
struct Table {
    files: Vec<Record>,
    shared: Vec<SharedRecord>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    name: String,
    byte_length: usize,
    sha256: String,
}

/// A shared model's fingerprint ([`crate::SharedModel::fingerprint`]), by
/// its fighter's code and its name.
#[derive(Deserialize)]
struct SharedRecord {
    fighter: String,
    name: String,
    fingerprint: String,
}

struct Tables {
    files: HashMap<String, VanillaFile>,
    /// Fingerprints by fighter code and model name.
    shared: HashMap<(String, String), String>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let table: Table =
            serde_json::from_str(VANILLA_FILES_JSON).expect("the checked-in vanilla table parses");
        Tables {
            files: table
                .files
                .into_iter()
                .map(|file| {
                    let file = VanillaFile {
                        name: file.name,
                        byte_length: file.byte_length,
                        sha256: file.sha256,
                    };
                    (file.name.clone(), file)
                })
                .collect(),
            shared: table
                .shared
                .into_iter()
                .map(|model| ((model.fighter, model.name), model.fingerprint))
                .collect(),
        }
    })
}

/// The vanilla file called `name` (`PlFcRe.dat`, `PlFc.dat`, `GrNLa.dat`),
/// if the table has one.
pub fn vanilla_file(name: &str) -> Option<&'static VanillaFile> {
    tables().files.get(name)
}

/// Every vanilla costume, fighter data, effects and versus stage file.
pub fn vanilla_files() -> impl Iterator<Item = &'static VanillaFile> {
    tables().files.values()
}

/// The fingerprint of `character`'s shared model `name` as shipped.
pub fn vanilla_shared(character: Character, name: &str) -> Option<&'static str> {
    tables()
        .shared
        .get(&(character.code().to_owned(), name.to_owned()))
        .map(String::as_str)
}

/// Whether `bytes` are exactly the vanilla file called `name`.
pub fn is_vanilla(name: &str, bytes: &[u8]) -> bool {
    vanilla_file(name).is_some_and(|file| {
        file.byte_length == bytes.len() && format!("{:x}", Sha256::digest(bytes)) == file.sha256
    })
}

#[cfg(test)]
mod tests {
    use super::{is_vanilla, vanilla_file, vanilla_shared};
    use crate::{Character, Stage};

    #[test]
    fn every_stage_and_fighter_has_vanilla_files() {
        for stage in Stage::all() {
            assert!(
                vanilla_file(stage.file_name()).is_some(),
                "{}",
                stage.name()
            );
        }
        for character in Character::all() {
            assert!(
                vanilla_file(&format!("Pl{}Nr.dat", character.code())).is_some(),
                "{}'s neutral costume",
                character.name()
            );
            assert!(
                vanilla_file(&format!("Pl{}.dat", character.code())).is_some(),
                "{}'s data file",
                character.name()
            );
            for model in crate::SharedModel::of(character) {
                assert!(
                    vanilla_shared(character, model.name()).is_some(),
                    "{}'s {}",
                    character.name(),
                    model.name()
                );
            }
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
