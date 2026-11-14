//! New mods: a mod started from one of the SDK's examples, the SDK of a
//! release (installed under `<data>/tgg/sdks/<game layout>/` when missing),
//! the build tools a mod needs, and the mod docs' address.
//!
//! [`new_mod`] only creates files and the mod's git repository; building is
//! the caller's next step.

use crate::cache::tgg_dir;
use crate::error::{Error, OnProgress, Progress};
use crate::manifest::ModId;
use crate::net::Net;
use crate::ports::{self, PortRelease};
use crate::sdk::{self, Sdk};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

/// The example a new mod starts from unless told otherwise.
pub const TEMPLATE: &str = "template";
/// The mod id the template's own files name.
const TEMPLATE_ID: &str = "my-mod";
/// Where the mod docs live, by the game's `major.minor`.
const DOCS: &str = "https://textures.gg/docs/mods";
/// A mod's build folder, which isn't copied from an example.
const BUILD: &str = "build";

/// A mod to start.
#[derive(Clone, Debug)]
pub struct NewMod {
    /// The folder: new, or holding only what [`may_already_be_there`] allows.
    pub dir: PathBuf,
    pub id: ModId,
    pub name: String,
    /// One of [`sdk_examples`]; [`TEMPLATE`] when `None`.
    pub example: Option<String>,
}

/// A mod [`new_mod`] started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedMod {
    pub dir: PathBuf,
    pub id: ModId,
    pub example: String,
    /// The game version of the SDK it started from.
    pub sdk_version: String,
    pub repository: Repository,
}

/// What became of the mod's git repository. A mod builds without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Repository {
    /// A new repository whose first commit is the example.
    Committed,
    /// git isn't installed.
    NoGit,
    /// A new repository with nothing committed: git has no user.name and
    /// user.email to commit with.
    NoIdentity,
    /// The folder was already in a repository, which is left as it was.
    AlreadyThere,
    /// git failed; the files are there.
    Failed(String),
}

/// The SDK of `release`, unpacked to `<data>/tgg/sdks/<game layout>/` from
/// the mirror when it isn't there yet. Releases sharing a layout share it.
pub fn release_sdk(
    net: &dyn Net,
    data: &Path,
    release: &PortRelease,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<Sdk, Error> {
    let root = tgg_dir(data).join("sdks");
    let folder = root.join(&release.game_abi);
    if !folder.join(sdk::FILE).is_file() {
        std::fs::create_dir_all(&root).map_err(Error::io(&root))?;
        let file = &release.files.sdk;
        let label = format!("Downloading the SDK for tgg-melee {}", release.version);
        let mut on_bytes = |bytes: u64, total: Option<u64>| {
            progress(Progress {
                step: 0,
                steps: 1,
                label: label.clone(),
                bytes,
                total,
            })
        };
        let bytes = ports::fetch_release_file(net, release, file, None, &mut on_bytes, cancel)?;
        ports::install_archive(&bytes, file, &folder)?;
    }
    let sdk = Sdk::open(&folder).map_err(|error| Error::BadSdk {
        path: folder.clone(),
        why: error.to_string(),
    })?;
    if sdk.game_abi != release.game_abi || sdk.target != release.target {
        return Err(Error::BadSdk {
            path: folder,
            why: format!(
                "it is game layout {} on {}, not {} on {}",
                sdk.game_abi, sdk.target, release.game_abi, release.target
            ),
        });
    }
    Ok(sdk)
}

/// The examples `sdk` ships that a mod can start from, by name.
pub fn sdk_examples(sdk: &Sdk) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(sdk.root.join("examples"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().join("manifest.json").is_file())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Start the mod `new` from one of `sdk`'s examples: copy it (no `build/`,
/// and files the folder already has stay), set the manifest's id and name,
/// point its docs lines at the docs for the SDK's version, give the
/// template's README and CMakeLists.txt the mod's name, and make the folder
/// a git repository whose first commit is the example.
pub fn new_mod(sdk: &Sdk, new: &NewMod) -> Result<CreatedMod, Error> {
    let dir = &new.dir;
    check_folder(dir)?;
    let example = new.example.as_deref().unwrap_or(TEMPLATE);
    let source = sdk.root.join("examples").join(example);
    if !source.join("manifest.json").is_file() {
        return Err(Error::NoExample {
            example: example.to_owned(),
            have: sdk_examples(sdk).join(", "),
        });
    }
    let created = copy_dir(&source, dir, Path::new(""))?;

    // The id and name go in as JSON values, so nothing else in the manifest
    // changes.
    let path = dir.join("manifest.json");
    let text = std::fs::read(&path).map_err(Error::io(&path))?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&text).map_err(|error| Error::BadSdk {
            path: source.join("manifest.json"),
            why: error.to_string(),
        })?;
    manifest["id"] = new.id.as_str().into();
    manifest["name"] = new.name.as_str().into();
    write(
        &path,
        serde_json::to_string_pretty(&manifest).expect("json") + "\n",
    )?;
    link_docs(dir, &sdk.version)?;
    if example == TEMPLATE {
        // The template's own files name its mod; a file that was already
        // there is left alone.
        for file in ["CMakeLists.txt", "README.md"] {
            let path = dir.join(file);
            if created.iter().any(|c| c == Path::new(file))
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                let text = if file == "README.md" {
                    // The template's README describes the template; a new
                    // mod gets its own, which link_docs points at the docs.
                    let readme = readme(&new.name, new.id.as_str());
                    let url = |page: &str| docs_url(&sdk.version, page);
                    readme.replace(SDK_DOCS_LINE, &docs_line(&url))
                } else {
                    text.replace(TEMPLATE_ID, new.id.as_str())
                };
                write(&path, text)?;
            }
        }
    }
    let repository = start_repository(dir, &format!("Start {} from the SDK's {example}", new.id));
    Ok(CreatedMod {
        dir: dir.clone(),
        id: new.id.clone(),
        example: example.to_owned(),
        sdk_version: sdk.version.clone(),
        repository,
    })
}

/// The build tools a mod needs, where they are on `PATH`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tools {
    /// The C compiler: `$CC` when set, else `gcc` (Homebrew's `gcc-15` on
    /// macOS). GCC 12 or later builds mods.
    pub cc: Option<PathBuf>,
    pub cmake: Option<PathBuf>,
    /// Optional: builds use make without it, more slowly.
    pub ninja: Option<PathBuf>,
    /// Optional: a mod builds without a repository, but publishing needs one.
    pub git: Option<PathBuf>,
}

/// Which of the build tools are on `PATH`. It doesn't run them; `tgg
/// doctor` checks versions and says how to install what's missing.
pub fn tools() -> Tools {
    let cc = match std::env::var_os("CC").filter(|cc| !cc.is_empty()) {
        Some(cc) => find(Path::new(&cc)),
        None if cfg!(target_os = "macos") => find(Path::new("gcc-15")),
        None => find(Path::new("gcc")),
    };
    Tools {
        cc,
        cmake: find(Path::new("cmake")),
        ninja: find(Path::new("ninja")),
        git: find(Path::new("git")),
    }
}

/// `program` as a path: itself when it names a file, else the first match
/// on `PATH` (with `.exe` on Windows).
fn find(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 {
        return program.is_file().then(|| program.to_owned());
    }
    let names: Vec<PathBuf> = if cfg!(windows) && program.extension().is_none() {
        vec![program.with_extension("exe"), program.to_owned()]
    } else {
        vec![program.to_owned()]
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// `major.minor` of a game version such as `0.3.2`, which the docs are
/// kept by; anything else is `latest`.
pub fn docs_minor(version: &str) -> String {
    match semver::Version::parse(version) {
        Ok(version) => format!("{}.{}", version.major, version.minor),
        Err(_) => "latest".to_owned(),
    }
}

/// The mod docs page `page` (a file of the SDK's `docs/` without `.md`)
/// for the game version `version`.
pub fn docs_url(version: &str, page: &str) -> String {
    format!("{DOCS}/{}/{page}", docs_minor(version))
}

fn write(path: &Path, text: impl AsRef<[u8]>) -> Result<(), Error> {
    std::fs::write(path, text).map_err(Error::io(path))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Make `dir` a git repository whose first commit is what's in it. A
/// repository that was already there is left alone.
fn start_repository(dir: &Path, message: &str) -> Repository {
    if git(dir, &["--version"]).is_err() {
        return Repository::NoGit;
    }
    if git(dir, &["rev-parse", "--git-dir"]).is_ok() {
        return Repository::AlreadyThere;
    }
    if let Err(error) = git(dir, &["init", "--quiet", "--initial-branch=main"]) {
        return Repository::Failed(error);
    }
    let who = ["user.name", "user.email"]
        .iter()
        .all(|key| git(dir, &["config", key]).is_ok_and(|value| !value.is_empty()));
    if !who {
        return Repository::NoIdentity;
    }
    match git(dir, &["add", "--all"])
        .and_then(|_| git(dir, &["commit", "--quiet", "--message", message]))
    {
        Ok(_) => Repository::Committed,
        Err(error) => Repository::Failed(error),
    }
}

/// The template's README line that says where the docs are.
const SDK_DOCS_LINE: &str =
    "- The docs came with the SDK: `$(tgg sdk path)/docs`. Start with `writing-mods.md`.";

/// The script template's README line that says where the docs are.
const SCRIPT_DOCS_LINE: &str =
    "- The docs came with the SDK: `$(tgg sdk path)/docs`. Start with `scripts.md`.";

fn docs_line(url: &dyn Fn(&str) -> String) -> String {
    format!(
        "- The docs: {}. Start with [writing mods]({}); `tgg docs` opens them, and\n  the SDK has a copy in `$(tgg sdk path)/docs`.",
        url("README"),
        url("writing-mods")
    )
}

/// A new mod's README: its name and id, and how to build and run it.
fn readme(name: &str, id: &str) -> String {
    format!(
        "# {name}\n\n`{id}`, a mod for tgg-melee.\n\n## Build and run\n\n```sh\n\
         tgg mod build        # build and pack it into build/\n\
         tgg mod dev --watch  # run it in the game, rebuilding on every change\n\
         ```\n\n{SDK_DOCS_LINE}\n"
    )
}

/// Point an example's "the docs are in your SDK" lines at the docs for the
/// SDK's version on textures.gg. A file without them is left as it is.
fn link_docs(dir: &Path, version: &str) -> Result<(), Error> {
    let url = |page: &str| docs_url(version, page);
    let edits = [
        ("README.md", SDK_DOCS_LINE.to_owned(), docs_line(&url)),
        (
            "README.md",
            SCRIPT_DOCS_LINE.to_owned(),
            format!(
                "- The docs: {}. Start with [scripts]({}); `tgg docs` opens them, and\n  the SDK has a copy in `$(tgg sdk path)/docs`.",
                url("README"),
                url("scripts")
            ),
        ),
        (
            "src/mod.c",
            " * swap what's below for your own code. The docs are in your SDK: run\n * `tgg sdk path` and open docs/writing-mods.md there.".to_owned(),
            format!(" * swap what's below for your own code. The docs:\n * {}", url("writing-mods")),
        ),
    ];
    for (file, old, new) in edits {
        let path = dir.join(file);
        if let Ok(text) = std::fs::read_to_string(&path)
            && text.contains(&old)
        {
            write(&path, text.replace(&old, &new))?;
        }
    }
    Ok(())
}

/// Files a folder may already hold when a mod starts in it: dotfiles (git,
/// direnv), a README, a license and a Nix flake.
pub fn may_already_be_there(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    name.starts_with('.')
        || ["README", "LICENSE", "LICENCE", "COPYING"]
            .iter()
            .any(|prefix| upper.starts_with(prefix))
        || ["flake.nix", "flake.lock", "shell.nix", "default.nix"].contains(&name)
}

/// Refuse a folder that holds anything but what [`may_already_be_there`]
/// allows.
fn check_folder(dir: &Path) -> Result<(), Error> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::Io {
                path: dir.to_owned(),
                source,
            });
        }
    };
    let mut others: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !may_already_be_there(name))
        .collect();
    others.sort();
    if others.is_empty() {
        Ok(())
    } else {
        Err(Error::FolderInUse {
            dir: dir.to_owned(),
            others: others.join(", "),
        })
    }
}

/// Copy the folder `from` into `to`, leaving out any `build/` folder and
/// keeping files `to` already has; an existing `.gitignore` gets the lines it
/// lacks. Returns the files it wrote, relative to `to`.
fn copy_dir(from: &Path, to: &Path, relative: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut created = Vec::new();
    std::fs::create_dir_all(to).map_err(Error::io(to))?;
    for entry in std::fs::read_dir(from).map_err(Error::io(from))? {
        let entry = entry.map_err(Error::io(from))?;
        let target = to.join(entry.file_name());
        let path = relative.join(entry.file_name());
        let kind = entry.file_type().map_err(Error::io(entry.path()))?;
        if kind.is_dir() {
            if entry.file_name() != BUILD {
                created.extend(copy_dir(&entry.path(), &target, &path)?);
            }
        } else if kind.is_file() {
            if !target.exists() {
                std::fs::copy(entry.path(), &target).map_err(Error::io(&target))?;
                created.push(path);
            } else if entry.file_name() == ".gitignore" {
                merge_lines(&entry.path(), &target)?;
            }
        }
    }
    Ok(created)
}

/// Append to `into` each line of `from` it doesn't have.
fn merge_lines(from: &Path, into: &Path) -> Result<(), Error> {
    let ours = std::fs::read_to_string(from).map_err(Error::io(from))?;
    let mut theirs = std::fs::read_to_string(into).map_err(Error::io(into))?;
    let missing: Vec<&str> = ours
        .lines()
        .filter(|line| !line.trim().is_empty() && !theirs.lines().any(|t| t == *line))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    if !theirs.is_empty() && !theirs.ends_with('\n') {
        theirs.push('\n');
    }
    for line in missing {
        theirs.push_str(line);
        theirs.push('\n');
    }
    write(into, theirs)
}

/// The example a script mod starts from.
pub const SCRIPT_TEMPLATE: &str = "script-template";

/// Whether the mod in `dir` is a script mod: its manifest says `"script": true`.
pub fn is_script(dir: &Path) -> bool {
    std::fs::read(dir.join("manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|manifest| manifest["script"] == serde_json::Value::Bool(true))
}

/// Where a script mod's editor finds the API's types: `build/tgg.luau`, which
/// the template's `.luaurc` names as `@tgg`.
pub const SCRIPT_TYPES: &str = "build/tgg.luau";

/// Copy the SDK's script API types to the mod's [`SCRIPT_TYPES`], for its
/// editor. An SDK without them (from before scripts) writes nothing.
pub fn write_script_types(sdk: &Sdk, dir: &Path) -> Result<bool, Error> {
    let from = sdk.root.join("luau").join("tgg.luau");
    if !from.is_file() {
        return Ok(false);
    }
    let to = dir.join(SCRIPT_TYPES);
    let build = dir.join(BUILD);
    std::fs::create_dir_all(&build).map_err(Error::io(&build))?;
    std::fs::copy(&from, &to).map_err(Error::io(&to))?;
    Ok(true)
}
