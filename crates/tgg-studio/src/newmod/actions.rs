//! What the player does on the Start a mod sheet, and what comes back from
//! its jobs.

use super::library::{self, CreatedMod, Sdk, Spec, Tools};
use super::{Fetching, Kind, NewMod, Stage};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::Tx;

/// What a Start a mod job found.
pub enum Done {
    /// The installed tgg-melee's SDK, or why it couldn't be had; and the
    /// build tools found.
    Ready(Box<Ready>),
    /// The mod made, or why not.
    Created(Box<Result<CreatedMod, String>>),
}

/// The SDK, or why it couldn't be had; the build tools.
pub struct Ready {
    pub sdk: Result<Sdk, String>,
    pub tools: Tools,
}

/// Open the sheet afresh, and on a job get the installed tgg-melee's SDK
/// (downloading it the first time, its progress shown) and look for the
/// tools that build mods.
pub fn open(app: &mut App) {
    let Some(release) = app.mods.port().map(|port| port.release.clone()) else {
        return;
    };
    let parent = if app.new_mod.parent.as_os_str().is_empty() {
        super::default_parent()
    } else {
        app.new_mod.parent.clone()
    };
    let fetching = Fetching::default();
    app.new_mod = NewMod {
        open: true,
        parent,
        fetching: Some(fetching.clone()),
        ..NewMod::default()
    };
    let (net, data, waker) = (app.mods.net.clone(), app.data.tgg.clone(), app.jobs.waker());
    app.jobs.spawn("start a mod: the SDK", move || {
        let report = move |step: library::Progress| {
            if let Ok(mut latest) = fetching.progress.lock() {
                *latest = Some(step);
            }
            if let Some(waker) = &waker {
                waker();
            }
        };
        let sdk = library::release_sdk(&*net, &data, &release, &report, &fetching.cancel)
            .map_err(|error| error.to_string());
        Job::NewMod(Box::new(Done::Ready(Box::new(Ready {
            sdk,
            tools: library::tools(),
        }))))
    });
}

/// Close the sheet (not while the mod's being made), stopping the SDK's
/// download if it's on its way.
pub fn close(app: &mut App) {
    if matches!(app.new_mod.stage, Stage::Creating) {
        return;
    }
    if let Some(fetching) = app.new_mod.fetching.take() {
        fetching
            .cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    app.new_mod.open = false;
}

/// Choose what to make.
pub fn pick_kind(app: &mut App, kind: Kind) {
    app.new_mod.kind = kind;
}

/// Make the mod, on a job.
pub fn create(app: &mut App) {
    let new_mod = &app.new_mod;
    let (true, Some(id), Some(Ok(sdk))) = (new_mod.ready(), new_mod.mod_id(), &new_mod.sdk) else {
        return;
    };
    let spec = Spec {
        dir: new_mod.folder(),
        id,
        name: new_mod.title(),
        example: Some(new_mod.kind.template().to_owned()),
    };
    let sdk = sdk.clone();
    app.new_mod.stage = Stage::Creating;
    app.jobs.spawn("start a mod", move || {
        let created = library::new_mod(&sdk, &spec).map_err(|error| error.to_string());
        Job::NewMod(Box::new(Done::Created(Box::new(created))))
    });
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Ready(ready) => {
            let Ready { sdk, tools } = *ready;
            app.new_mod.fetching = None;
            app.new_mod.sdk = Some(sdk);
            app.new_mod.tools = Some(tools);
        }
        Done::Created(created) => {
            app.new_mod.stage = match *created {
                Ok(created) => Stage::Created(created),
                Err(why) => Stage::Failed(why),
            };
        }
    }
    true
}

/// Whether the mod was made from one of the SDK's script examples.
pub fn is_script(created: &CreatedMod) -> bool {
    tgg_mod::SCRIPT_TEMPLATES.contains(&created.example.as_str())
}

/// The command that runs the mod in the game, from anywhere: it needs no
/// build, where a script's own files are what the game reads.
pub fn dev_command(created: &CreatedMod) -> String {
    let dir = shell_quoted(&created.dir.display().to_string());
    if is_script(created) {
        format!("tgg mod dev {dir}")
    } else {
        format!("tgg mod dev --watch {dir}")
    }
}

/// `path` as one word for a shell: quoted when it has a space or a quote.
fn shell_quoted(path: &str) -> String {
    if path.contains([' ', '\'', '"']) {
        format!("'{}'", path.replace('\'', r"'\''"))
    } else {
        path.to_owned()
    }
}

pub fn copy_dev(app: &mut App, tx: &mut Tx) {
    if let Stage::Created(created) = &app.new_mod.stage {
        tx.copy(dev_command(created));
        app.tell("Copied the command.", false);
    }
}

/// Open the mod's folder, where the player's editor takes it from.
pub fn open_in_editor(app: &mut App, tx: &mut Tx) {
    let Stage::Created(created) = &app.new_mod.stage else {
        return;
    };
    let dir = created.dir.display().to_string();
    if let Err(error) = tx.open_url(&dir) {
        app.tell(format!("Couldn't open {dir}: {error}"), true);
    }
}

/// The guide to writing a mod, for the SDK it started from: scripts first
/// for a script, the whole API for C.
pub fn open_guide(app: &mut App, tx: &mut Tx) {
    let Stage::Created(created) = &app.new_mod.stage else {
        return;
    };
    let page = if is_script(created) {
        "scripts"
    } else {
        "writing-mods"
    };
    let url = library::docs_url(&created.sdk_version, page);
    if let Err(error) = tx.open_url(&url) {
        app.tell(format!("Couldn't open the guide: {error}"), true);
    }
}

#[cfg(test)]
mod tests {
    use super::shell_quoted;

    #[test]
    fn a_folder_with_a_space_is_one_word_for_the_shell() {
        assert_eq!(shell_quoted("/home/a/mods/x"), "/home/a/mods/x");
        assert_eq!(shell_quoted("/home/a b/x"), "'/home/a b/x'");
        assert_eq!(shell_quoted("/it's"), r"'/it'\''s'");
    }
}
