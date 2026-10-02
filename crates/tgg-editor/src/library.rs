//! The player's skins: every costume or stage file they've added, kept once
//! each (by SHA-256) in the app's data folder, with a record of its name,
//! the slot it was made for, and where it came from. The library is the
//! source of truth; the player's ISO holds copies of what they install.
//!
//! ```text
//! <data dir>/textures.gg/library/library.json
//! <data dir>/textures.gg/library/skins/<sha256>.dat
//! ```

use crate::Error;
use dat_parser::DatFile;
use dat_parser::hsd::scene::HSD_SCENE_MAX_DAT_BYTES;
use melee_dat::MeleeReferenceCatalog;
use melee_dat::{ParsedFilename, parse_filename};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Where a skin came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SkinSource {
    /// A file the player added (dropped or chosen).
    Imported,
    /// Saved from the editor, from the skin `from` when it had one.
    Edited { from: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    /// The file's SHA-256, which also names its copy in the library.
    pub id: String,
    /// The name shown for it, from its file name.
    pub name: String,
    /// The file name it was added with.
    pub file_name: String,
    pub byte_length: usize,
    /// The game file it was made for (`PlFcRe.dat`, `GrNLa.dat`), when it
    /// says: from its root names, else its file name.
    pub slot: Option<String>,
    pub source: SkinSource,
    /// When it was added, in seconds since the Unix epoch.
    pub added: u64,
}

/// A file to add, looked at but not yet kept: what it is and where it
/// seems to go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub file_name: String,
    pub name: String,
    /// Its SHA-256, the id it gets in the library.
    pub id: String,
    /// The slot it says it was made for, if any.
    pub slot: Option<String>,
    /// The library's record of this exact file, when it already has it.
    pub known: Option<Skin>,
    pub bytes: Rc<Vec<u8>>,
}

/// A file the library turned away, and why, in the player's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    pub file_name: String,
    pub reason: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Index {
    schema_version: u32,
    skins: Vec<Skin>,
}

pub struct Library {
    root: PathBuf,
    skins: Vec<Skin>,
}

impl Library {
    /// The library in the app's data folder.
    pub fn open() -> Self {
        let root = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("textures.gg")
            .join("library");
        Self::open_at(root)
    }

    /// The library kept in `root`. An unreadable index is set aside, not
    /// overwritten, so its skins can be recovered.
    pub fn open_at(root: PathBuf) -> Self {
        let index_path = root.join("library.json");
        let skins = match std::fs::read_to_string(&index_path) {
            Ok(text) => match serde_json::from_str::<Index>(&text) {
                Ok(index) => index.skins,
                Err(error) => {
                    let aside = root.join(format!("library.unreadable-{}.json", now()));
                    crate::log(&format!(
                        "library index unreadable ({error}); kept as {}",
                        aside.display()
                    ));
                    let _ = std::fs::rename(&index_path, aside);
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        Self { root, skins }
    }

    /// Every skin, oldest first.
    pub fn skins(&self) -> &[Skin] {
        &self.skins
    }

    /// Look at the file `file_name` holding `bytes` before adding it: it must
    /// be a DAT the app can read. The slot it was made for comes from its
    /// root names, else its file name. Nothing is saved yet.
    pub fn inspect(
        &self,
        file_name: &str,
        bytes: Vec<u8>,
        catalog: &MeleeReferenceCatalog,
    ) -> Result<Candidate, Rejected> {
        let reject = |reason: String| Rejected {
            file_name: file_name.to_owned(),
            reason,
        };
        if bytes.len() > HSD_SCENE_MAX_DAT_BYTES {
            return Err(reject(format!(
                "it's larger than any Melee file ({} MB)",
                bytes.len() / 1_000_000
            )));
        }
        let dat = DatFile::parse(&bytes)
            .map_err(|error| reject(format!("it isn't a DAT file the app can read ({error})")))?;
        let slot = catalog
            .costume_slot(dat.roots.iter().map(|root| root.name.as_str()))
            .or_else(|| slot_from_file_name(file_name));
        let id = format!("{:x}", Sha256::digest(&bytes));
        Ok(Candidate {
            known: self.skins.iter().find(|skin| skin.id == id).cloned(),
            name: display_name(file_name),
            file_name: file_name.to_owned(),
            slot,
            id,
            bytes: Rc::new(bytes),
        })
    }

    /// Look at every DAT in `paths`, and every DAT inside a `.zip` among
    /// them, before adding them.
    pub fn inspect_files(
        &self,
        paths: &[PathBuf],
        catalog: &MeleeReferenceCatalog,
    ) -> Vec<Result<Candidate, Rejected>> {
        let mut results = Vec::new();
        for path in paths {
            let file_name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let bytes = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    results.push(Err(Rejected {
                        file_name,
                        reason: format!("it couldn't be read ({error})"),
                    }));
                    continue;
                }
            };
            if has_extension(&file_name, "zip") {
                match dats_in_zip(&bytes) {
                    Ok(dats) if dats.is_empty() => results.push(Err(Rejected {
                        file_name,
                        reason: "it has no DAT files inside".into(),
                    })),
                    Ok(dats) => results.extend(
                        dats.into_iter()
                            .map(|(name, bytes)| self.inspect(&name, bytes, catalog)),
                    ),
                    Err(reason) => results.push(Err(Rejected { file_name, reason })),
                }
            } else {
                results.push(self.inspect(&file_name, bytes, catalog));
            }
        }
        results
    }

    /// Keep `candidate` in the library for `slot`, the one the player
    /// confirmed. A file the library already has keeps its record, with the
    /// new slot.
    pub fn store(
        &mut self,
        candidate: &Candidate,
        slot: Option<String>,
        source: SkinSource,
    ) -> Result<Skin, Error> {
        if let Some(known) = self.skins.iter_mut().find(|skin| skin.id == candidate.id) {
            if known.slot != slot {
                known.slot = slot;
                let known = known.clone();
                self.save().map_err(Error::LibrarySave)?;
                return Ok(known);
            }
            return Ok(known.clone());
        }
        let blob = self.blob_path(&candidate.id);
        std::fs::create_dir_all(blob.parent().expect("blobs live in a folder"))
            .and_then(|()| write_atomically(&blob, &candidate.bytes))
            .map_err(Error::LibrarySave)?;
        let skin = Skin {
            id: candidate.id.clone(),
            name: candidate.name.clone(),
            file_name: candidate.file_name.clone(),
            byte_length: candidate.bytes.len(),
            slot,
            source,
            added: now(),
        };
        self.skins.push(skin.clone());
        self.save().map_err(Error::LibrarySave)?;
        Ok(skin)
    }

    /// Take skin `id` out of the library's list. Its file stays kept, since
    /// an install's history may still need it to undo.
    pub fn remove(&mut self, id: &str) -> Result<Skin, Error> {
        let index = self
            .skins
            .iter()
            .position(|skin| skin.id == id)
            .ok_or(Error::NotInLibrary)?;
        let skin = self.skins.remove(index);
        self.save().map_err(Error::LibrarySave)?;
        Ok(skin)
    }

    /// Keep `bytes` (a slot's file before an install replaced it) without
    /// listing it as a skin, and return its id.
    pub fn keep(&self, bytes: &[u8]) -> Result<String, Error> {
        let id = format!("{:x}", Sha256::digest(bytes));
        let blob = self.blob_path(&id);
        if !blob.exists() {
            std::fs::create_dir_all(blob.parent().expect("blobs live in a folder"))
                .and_then(|()| write_atomically(&blob, bytes))
                .map_err(Error::LibraryKeep)?;
        }
        Ok(id)
    }

    /// The bytes kept as `id`, checked against their hash.
    pub fn read(&self, id: &str) -> Result<Vec<u8>, Error> {
        let bytes = std::fs::read(self.blob_path(id)).map_err(Error::LibraryMissing)?;
        if format!("{:x}", Sha256::digest(&bytes)) != id {
            return Err(Error::LibraryChanged);
        }
        Ok(bytes)
    }

    /// The skin whose file is `bytes`, if the library has it.
    pub fn skin_for(&self, bytes: &[u8]) -> Option<&Skin> {
        let id = format!("{:x}", Sha256::digest(bytes));
        self.skins.iter().find(|skin| skin.id == id)
    }

    /// Where the library keeps skin `id`'s file.
    pub fn blob_path(&self, id: &str) -> PathBuf {
        self.root.join("skins").join(format!("{id}.dat"))
    }

    fn save(&self) -> std::io::Result<()> {
        let index = Index {
            schema_version: 1,
            skins: self.skins.clone(),
        };
        let mut json = serde_json::to_string_pretty(&index).expect("the index serializes");
        json.push('\n');
        std::fs::create_dir_all(&self.root)?;
        write_atomically(&self.root.join("library.json"), json.as_bytes())
    }
}

/// The slot a costume or stage file names (`PlFcRe-waffle.dat` is Falco's
/// Red slot).
fn slot_from_file_name(file_name: &str) -> Option<String> {
    match parse_filename(file_name)? {
        ParsedFilename::Character {
            character_code,
            costume_code,
        } => Some(format!("Pl{character_code}{costume_code}.dat")),
        ParsedFilename::Stage { filename } => Some(filename.to_owned()),
    }
}

/// A readable name from a file name, without the slot code it starts with:
/// `PlFxOr-Asymm_Jacket.dat` reads "Asymm Jacket", `GrNLaWaffle.dat` reads
/// "Waffle", and a bare `PlFcRe.dat` keeps its stem.
fn display_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let without_slot = slot_from_file_name(file_name)
        .and_then(|slot| stem.strip_prefix(slot.trim_end_matches(".dat")))
        .unwrap_or(stem);
    let words: Vec<&str> = without_slot
        .split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        stem.to_owned()
    } else {
        words.join(" ")
    }
}

/// Every DAT inside a zip archive, by file name, each within the largest
/// size a Melee file can have.
fn dats_in_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("it isn't a zip the app can open ({error})"))?;
    let mut dats = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("its contents couldn't be read ({error})"))?;
        let path = entry.name().to_owned();
        let Some(name) = Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if entry.is_dir() || path.starts_with("__MACOSX") || !has_extension(&name, "dat") {
            continue;
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(HSD_SCENE_MAX_DAT_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|error| format!("{name} couldn't be read ({error})"))?;
        if data.len() <= HSD_SCENE_MAX_DAT_BYTES {
            dats.push((name, data));
        }
    }
    Ok(dats)
}

fn has_extension(file_name: &str, extension: &str) -> bool {
    Path::new(file_name)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Write `bytes` beside `path`, then rename it over, so a crash never
/// leaves a half-written file.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::{dats_in_zip, display_name, slot_from_file_name};
    use std::io::Write;

    #[test]
    fn names_read_without_the_slot_code() {
        assert_eq!(display_name("PlFxOr-Asymm_Jacket.dat"), "Asymm Jacket");
        assert_eq!(display_name("PlFcRe.dat"), "PlFcRe");
        assert_eq!(display_name("waffle falco.dat"), "waffle falco");
        assert_eq!(display_name("GrNLaWaffle.dat"), "Waffle");
    }

    #[test]
    fn file_names_give_a_slot_when_the_contents_dont() {
        assert_eq!(
            slot_from_file_name("PlFcRe-waffle.dat").as_deref(),
            Some("PlFcRe.dat")
        );
        assert_eq!(
            slot_from_file_name("GrNLaWaffle.dat").as_deref(),
            Some("GrNLa.dat")
        );
        assert_eq!(slot_from_file_name("notes.dat"), None);
    }

    #[test]
    fn zips_give_up_their_dats_and_nothing_else() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("skins/PlFcRe-waffle.dat", &b"dat"[..]),
            ("__MACOSX/skins/._PlFcRe-waffle.dat", b"junk"),
            ("readme.txt", b"hi"),
        ] {
            zip.start_file(name, options).expect("start");
            zip.write_all(data).expect("write");
        }
        let bytes = zip.finish().expect("finish").into_inner();
        let dats = dats_in_zip(&bytes).expect("zip opens");
        assert_eq!(dats, [("PlFcRe-waffle.dat".to_owned(), b"dat".to_vec())]);
    }

    /// A costume: identified by its root name, kept once, its slot changed
    /// when the player overrides it, and read back intact.
    #[test]
    fn a_costume_is_identified_kept_once_and_read_back() {
        use super::{Library, SkinSource};
        let bytes = crate::test_dat::model_named("PlyFalco5KRe_Share_joint");
        let catalog = melee_dat::MeleeReferenceCatalog::checked_in().expect("catalog");
        let folder = tempfile::tempdir().expect("temp folder");
        let mut library = Library::open_at(folder.path().to_owned());
        // A name that says nothing: the slot comes from the contents.
        let candidate = library
            .inspect("falco final v3.dat", bytes.clone(), &catalog)
            .expect("readable");
        assert_eq!(candidate.slot.as_deref(), Some("PlFcRe.dat"));
        assert_eq!(candidate.name, "falco final v3");
        assert!(library.skins().is_empty(), "inspecting keeps nothing");
        let skin = library
            .store(&candidate, candidate.slot.clone(), SkinSource::Imported)
            .expect("stored");

        let again = library
            .inspect("copy.dat", bytes.clone(), &catalog)
            .expect("readable");
        assert_eq!(again.known.as_ref(), Some(&skin));
        let moved = library
            .store(&again, Some("PlFcBu.dat".into()), SkinSource::Imported)
            .expect("stored again");
        assert_eq!(moved.slot.as_deref(), Some("PlFcBu.dat"));
        let reopened = Library::open_at(folder.path().to_owned());
        assert_eq!(reopened.skins().len(), 1);
        assert_eq!(reopened.skins()[0].slot.as_deref(), Some("PlFcBu.dat"));
        assert_eq!(library.read(&skin.id).expect("read"), bytes);
        assert!(
            library
                .inspect("notes.dat", b"hello".to_vec(), &catalog)
                .is_err()
        );
    }
}
