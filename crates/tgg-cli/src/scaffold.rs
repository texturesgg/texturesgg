//! `tgg mod new`: a new mod from one of the SDK's examples
//! ([`tgg_mod::new_mod`]), then a first build, or for a script mod the API's
//! types for its editor.

use crate::build::{self, Options};
use anyhow::{Result, anyhow};
use std::path::Path;
use tgg_mod::{ModId, NewMod, Repository};

pub struct New<'a> {
    pub dir: &'a Path,
    pub id: Option<ModId>,
    pub name: Option<String>,
    pub example: Option<String>,
    /// From the SDK's script template
    pub script: bool,
    /// The SDK, or `None` to use the game in use's.
    pub sdk: Option<&'a Path>,
    pub cc: &'a Path,
}

pub fn new(new: New) -> Result<()> {
    let dir = new.dir;
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
    let sdk = build::resolve_sdk(new.sdk)?;
    let created = tgg_mod::new_mod(
        &sdk,
        &NewMod {
            dir: dir.to_owned(),
            id,
            name: new.name.unwrap_or(folder_name),
            example: if new.script {
                Some(tgg_mod::SCRIPT_TEMPLATE.to_owned())
            } else {
                new.example
            },
        },
    )?;
    build::read_manifest(dir)?;

    let (id, example) = (&created.id, &created.example);
    if dir.file_name().is_some_and(|folder| folder == id.as_str()) {
        println!("Created {id} from the SDK's {example}");
    } else {
        println!("Created {id} in {} from the SDK's {example}", dir.display());
    }
    match created.repository {
        Repository::Committed | Repository::AlreadyThere => {}
        Repository::NoGit => println!(
            "git isn't installed, so there's no repository yet (tgg doctor says how to get it)"
        ),
        Repository::NoIdentity => println!(
            "Made a git repository. To commit, tell git who you are:\n  \
             git config --global user.name \"Your Name\"\n  \
             git config --global user.email you@example.com"
        ),
        Repository::Failed(error) => eprintln!("Couldn't start its git repository: {error}"),
    }

    if tgg_mod::is_script(dir) {
        // A script mod isn't compiled; its editor wants the API's types.
        tgg_mod::write_script_types(&sdk, dir)?;
        if dir == Path::new(".") {
            println!("tgg mod dev runs it in the game");
        } else {
            println!(
                "To run it in the game: cd {}, then tgg mod dev",
                dir.display()
            );
        }
        return Ok(());
    }

    // A first build writes build/compile_commands.json, so an editor finds
    // the game's headers right away.
    let options = Options {
        sdk: Some(&sdk.root),
        cc: new.cc,
        debug: false,
    };
    match build::build(dir, &options) {
        Ok(_) if dir == Path::new(".") => println!("Built it; tgg mod dev runs it in the game"),
        Ok(_) => println!(
            "Built it. To run it in the game: cd {}, then tgg mod dev",
            dir.display()
        ),
        Err(error) => eprintln!("It doesn't build yet: {error:#}"),
    }
    Ok(())
}
