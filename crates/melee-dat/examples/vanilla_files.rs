//! Write `data/vanilla-files.json`: the size and SHA-256 of every costume,
//! fighter data, effects and versus stage file on a clean Melee NTSC 1.02
//! disc, and the fingerprint of every model fighters' costumes share.
//!
//! ```text
//! cargo run --release -p melee-dat --example vanilla_files -- MELEE.iso [OUTPUT]
//! ```
//!
//! Any ISO but the one the reference catalog records as its source is
//! refused, so the table can only describe the game as shipped.

use melee_dat::{Character, MeleeSlot, SharedModel, catalog::CATALOG_JSON};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    schema_version: u32,
    source: serde_json::Value,
    files: Vec<Record>,
    shared: Vec<SharedRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    name: String,
    byte_length: usize,
    sha256: String,
}

#[derive(Serialize)]
struct SharedRecord {
    fighter: &'static str,
    name: &'static str,
    fingerprint: String,
}

/// A slot's file: a costume (`PlFcRe.dat`), a fighter's data (`PlFc.dat`),
/// an effects file (`EfFxData.dat`) or a versus stage (`GrNLa.dat`).
fn belongs(name: &str) -> bool {
    MeleeSlot::from_file_name(name).is_some()
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let iso: PathBuf = args
        .next()
        .ok_or("usage: vanilla_files MELEE.iso [OUTPUT]")?
        .into();
    let output: PathBuf = args.next().map_or_else(
        || concat!(env!("CARGO_MANIFEST_DIR"), "/data/vanilla-files.json").into(),
        Into::into,
    );

    let catalog: serde_json::Value = serde_json::from_str(CATALOG_JSON)?;
    let source = &catalog["source"];
    let expected = source["isoSha256"]
        .as_str()
        .ok_or("the reference catalog records no source ISO")?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut File::open(&iso)?, &mut hasher)?;
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(format!(
            "{} is not the catalog's clean source ISO (sha256 {actual}, expected {expected})",
            iso.display()
        )
        .into());
    }

    let mut disc = gc_iso::Disc::open(&iso)?;
    let mut names: Vec<String> = disc
        .files()
        .iter()
        .filter(|entry| !entry.is_dir && belongs(&entry.name))
        .map(|entry| entry.name.clone())
        .collect();
    names.sort();
    let files = names
        .into_iter()
        .map(|name| {
            let bytes = disc.read(&name)?;
            Ok(Record {
                byte_length: bytes.len(),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                name,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let shared = Character::all()
        .flat_map(SharedModel::of)
        .map(|model| {
            let bytes = disc.read(&model.slot().file_name())?;
            Ok(SharedRecord {
                fighter: model.character().code(),
                name: model.name(),
                fingerprint: model.fingerprint(&bytes)?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let table = Table {
        schema_version: 2,
        source: serde_json::json!({
            "gameId": source["gameId"],
            "discRevision": source["discRevision"],
            "isoSha256": source["isoSha256"],
        }),
        files,
        shared,
    };
    let mut json = serde_json::to_string_pretty(&table)?;
    json.push('\n');
    std::fs::write(&output, json)?;
    println!(
        "{} files and {} shared models -> {}",
        table.files.len(),
        table.shared.len(),
        output.display()
    );
    Ok(())
}
