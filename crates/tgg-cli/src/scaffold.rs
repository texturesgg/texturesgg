//! `tgg mod new`: a new mod from one of the SDK's templates or examples
//! ([`tgg_mod::new_mod`]). A script mod gets the API's types for its editor;
//! a C mod a first build, so its editor finds the game's headers.

use crate::build::{self, Options};
use crate::mods::Template;
use anyhow::{Result, anyhow};
use std::path::Path;
use tgg_mod::{ModId, NewMod, Repository};

/// Where to ask for what scripts can't reach yet.
const API_ISSUES: &str = "https://github.com/texturesgg/texturesgg/issues/new";

pub struct New<'a> {
    pub dir: &'a Path,
    pub id: Option<ModId>,
    pub name: Option<String>,
    /// `None`: asked on a terminal, else a rule.
    pub template: Option<Template>,
    /// An SDK example instead of a template.
    pub example: Option<String>,
    /// The SDK, or `None` to use the game in use's.
    pub sdk: Option<&'a Path>,
    pub cc: &'a Path,
}

fn example_of(template: Template) -> &'static str {
    match template {
        Template::Rule => tgg_mod::SCRIPT_TEMPLATES[0],
        Template::Hud => tgg_mod::SCRIPT_TEMPLATES[1],
        Template::Screen => tgg_mod::SCRIPT_TEMPLATES[2],
        Template::Native => tgg_mod::TEMPLATE,
    }
}

/// What the mod is for, asked once.
fn ask() -> Result<Template> {
    let items = [
        (Template::Rule, "change how the game plays (a rule)"),
        (Template::Hud, "show something on screen (HUD, trainer)"),
        (Template::Screen, "add a menu or screen"),
        (
            Template::Native,
            "native C mod (advanced: needs your own builds and a manual review)",
        ),
    ];
    let items = items
        .into_iter()
        .map(|(template, label)| (template, label.to_owned(), String::new()))
        .collect();
    crate::ui::pick("What's it for?", items, Some(Template::Rule))?
        .ok_or_else(|| anyhow!("nothing made"))
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
    let example = match (new.example, new.template) {
        (Some(example), _) => example,
        (None, Some(template)) => example_of(template).to_owned(),
        (None, None) if crate::ui::interactive() => example_of(ask()?).to_owned(),
        (None, None) => example_of(Template::Rule).to_owned(),
    };
    if example == tgg_mod::TEMPLATE {
        println!(
            "Native mods need a build for each platform and a manual review. If it's for \
             something scripts can't reach, asking for it in the API may be the better fix: \
             {API_ISSUES}"
        );
    }
    let created = tgg_mod::new_mod(
        &sdk,
        &NewMod {
            dir: dir.to_owned(),
            id,
            name: new.name.unwrap_or(folder_name),
            example: Some(example),
        },
    )?;
    build::read_manifest(dir)?;

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

    let script = tgg_mod::is_script(dir);
    if script {
        tgg_mod::write_script_types(&sdk, dir)?;
    } else {
        // A first build writes build/compile_commands.json, so an editor
        // finds the game's headers right away.
        let options = Options {
            sdk: Some(&sdk.root),
            cc: new.cc,
            debug: false,
        };
        if let Err(error) = build::build(dir, &options) {
            eprintln!("It doesn't build yet: {error:#}");
        }
    }
    next_steps(dir, script);
    Ok(())
}

/// The three things to do next.
fn next_steps(dir: &Path, script: bool) {
    let here = dir == Path::new(".");
    let folder = if here {
        "this folder".to_owned()
    } else {
        crate::ui::path(dir.display())
    };
    let arg = if here {
        String::new()
    } else {
        format!(" {}", dir.display())
    };
    let (editor, dev) = if script {
        (
            "completion and docs come from build/tgg.luau",
            "hot reload on save",
        )
    } else {
        (
            "headers come from build/compile_commands.json",
            "rebuilds on save",
        )
    };
    let dev_cmd = if script {
        format!("tgg mod dev{arg}")
    } else {
        format!("tgg mod dev --watch{arg}")
    };
    println!("1. Open {folder} in your editor ({editor})");
    println!("2. {} ({dev})", crate::ui::cmd(dev_cmd));
    println!("3. {}", crate::ui::cmd(format!("tgg mod build{arg}")));
}
