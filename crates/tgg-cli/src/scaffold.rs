//! `tgg mod new`: a new mod from one of the SDK's examples.

use crate::build::{self, Options};
use crate::publish::git;
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::path::Path;
use tgg_mod::ModId;

/// The example a new mod starts from unless told otherwise.
const TEMPLATE: &str = "template";
/// The name the template's files give its mod.
const TEMPLATE_ID: &str = "my-mod";

pub struct New<'a> {
    pub dir: &'a Path,
    pub id: Option<ModId>,
    pub name: Option<String>,
    pub example: Option<String>,
    pub cc: &'a Path,
}

pub fn new(new: New) -> Result<()> {
    let dir = new.dir;
    if dir.exists() {
        ensure!(
            std::fs::read_dir(dir)
                .with_context(|| dir.display().to_string())?
                .next()
                .is_none(),
            "{} isn't empty; tgg mod new starts a mod in a new or empty folder",
            dir.display()
        );
    }
    let folder_name = std::path::absolute(dir)?
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = match new.id {
        Some(id) => id,
        None => folder_name.parse().map_err(|_| {
            anyhow!(
                "{folder_name:?} isn't a mod id; pass --id (1 to 64 of a-z, 0-9, '.', '-' and '_')"
            )
        })?,
    };
    let sdk = build::resolve_sdk(None)?;
    let example = new.example.as_deref().unwrap_or(TEMPLATE);
    let source = sdk.root.join("examples").join(example);
    if !source.join("manifest.json").is_file() {
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
        bail!(
            "the SDK has no example {example:?}; it has {}",
            names.join(", ")
        );
    }
    copy_dir(&source, dir)?;

    // The id and name go in as JSON values, so nothing else in the manifest
    // changes.
    let path = dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&build::read(&path)?).with_context(|| path.display().to_string())?;
    manifest["id"] = id.as_str().into();
    manifest["name"] = new.name.unwrap_or(folder_name).into();
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&manifest).expect("json") + "\n",
    )?;
    link_docs(dir, &sdk.version)?;
    if example == TEMPLATE {
        let cmake = dir.join("CMakeLists.txt");
        if let Ok(text) = std::fs::read_to_string(&cmake) {
            std::fs::write(&cmake, text.replace(TEMPLATE_ID, id.as_str()))?;
        }
    }
    build::read_manifest(dir)?;

    if git(dir, &["rev-parse", "--git-dir"]).is_err() {
        git(dir, &["init", "--quiet", "--initial-branch=main"])?;
    }
    println!("Created {id} in {} from the SDK's {example}", dir.display());

    // A first build writes build/compile_commands.json, so an editor finds
    // the game's headers right away.
    let options = Options {
        sdk: Some(&sdk.root),
        cc: new.cc,
        debug: false,
    };
    match build::build(dir, &options) {
        Ok(_) => println!("Built it; tgg mod dev runs it in the game"),
        Err(error) => eprintln!("It doesn't build yet: {error:#}"),
    }
    Ok(())
}

/// Point the template's "the docs are in your SDK" lines at the docs for
/// the SDK's version on textures.gg. A file without them is left as it is.
fn link_docs(dir: &Path, version: &str) -> Result<()> {
    let url = |page: &str| crate::docs::url(version, page);
    let edits = [
        (
            "README.md",
            "- The docs came with the SDK: `$(tgg sdk path)/docs`. Start with `writing-mods.md`.".to_owned(),
            format!(
                "- The docs: {}. Start with [writing mods]({}); `tgg docs` opens them, and\n  the SDK has a copy in `$(tgg sdk path)/docs`.",
                url("README"),
                url("writing-mods")
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
            std::fs::write(&path, text.replace(&old, &new))
                .with_context(|| path.display().to_string())?;
        }
    }
    Ok(())
}

/// Copy the folder `from` into `to`, leaving out any `build/` folder.
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| to.display().to_string())?;
    for entry in std::fs::read_dir(from).with_context(|| from.display().to_string())? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if entry.file_name() != build::BUILD {
                copy_dir(&entry.path(), &target)?;
            }
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target).with_context(|| target.display().to_string())?;
        }
    }
    Ok(())
}
