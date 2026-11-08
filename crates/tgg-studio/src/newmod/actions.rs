//! What the player does in New mod, and what comes back from its jobs.

use super::library::{self, CreatedMod, Sdk, Spec, Tools};
use super::{Fetching, NewMod, Stage, TEMPLATE};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::{FileDialogOptions, Tx};

/// What a New mod job found.
pub enum Done {
    /// The installed tgg-melee's SDK and its examples, or why it couldn't
    /// be had; and the build tools found.
    Ready(Box<Ready>),
    /// The mod made, or why not.
    Created(Box<Result<CreatedMod, String>>),
}

/// The SDK and its examples, or why it couldn't be had; the build tools.
pub struct Ready {
    pub sdk: Result<(Sdk, Vec<String>), String>,
    pub tools: Tools,
}

/// Open the dialog afresh: on a job, get the installed tgg-melee's SDK
/// (downloading it the first time, its progress shown), read its examples
/// and look for the build tools.
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
        example: TEMPLATE.to_owned(),
        fetching: Some(fetching.clone()),
        ..NewMod::default()
    };
    let (net, data, waker) = (app.mods.net.clone(), app.data.tgg.clone(), app.jobs.waker());
    app.jobs.spawn("new mod: the SDK", move || {
        let report = move |step: library::Progress| {
            if let Ok(mut latest) = fetching.progress.lock() {
                *latest = Some(step);
            }
            if let Some(waker) = &waker {
                waker();
            }
        };
        let sdk = library::release_sdk(&*net, &data, &release, &report, &fetching.cancel)
            .map(|sdk| {
                let examples = library::sdk_examples(&sdk);
                (sdk, examples)
            })
            .map_err(|error| error.to_string());
        Job::NewMod(Box::new(Done::Ready(Box::new(Ready {
            sdk,
            tools: library::tools(),
        }))))
    });
}

/// Close the dialog (not while the mod's being made), stopping the SDK's
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

pub fn toggle_menu(app: &mut App) {
    app.new_mod.menu = !app.new_mod.menu;
}

pub fn close_menu(app: &mut App) {
    app.new_mod.menu = false;
}

pub fn pick_example(app: &mut App, example: &str) {
    app.new_mod.example = example.to_owned();
    app.new_mod.menu = false;
}

/// Choose the folder the mod's folder goes in.
pub fn choose_folder(app: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Where the mod's folder goes".into(),
        directory: Some(app.new_mod.parent.clone()),
        ..Default::default()
    };
    tx.open_folder(options, |app: &mut App, answer, _| match answer {
        Ok(Some(folder)) => app.new_mod.parent = folder,
        Ok(None) => {}
        Err(error) => app.tell(format!("The folder dialog couldn't open: {error}"), true),
    });
}

/// Whether the mod can be made as filled in: the SDK here, a valid id,
/// while nothing's being made.
pub fn can_create(new_mod: &NewMod) -> bool {
    matches!(new_mod.stage, Stage::Editing | Stage::Failed(_))
        && new_mod.examples().is_some()
        && new_mod.mod_id().is_some()
}

/// Make the mod, on a job.
pub fn create(app: &mut App) {
    let new_mod = &app.new_mod;
    let (true, Some(id), Some(Ok((sdk, _)))) =
        (can_create(new_mod), new_mod.mod_id(), &new_mod.sdk)
    else {
        return;
    };
    let name = match new_mod.name.text.trim() {
        "" => id.to_string(),
        name => name.to_owned(),
    };
    let spec = Spec {
        dir: new_mod.folder(),
        id,
        name,
        example: Some(new_mod.example.clone()),
    };
    let sdk = sdk.clone();
    app.new_mod.stage = Stage::Creating;
    app.new_mod.menu = false;
    app.jobs.spawn("new mod", move || {
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
            if let Ok((_, examples)) = &sdk
                && !examples.contains(&app.new_mod.example)
                && let Some(first) = examples.first()
            {
                app.new_mod.example = first.clone();
            }
            app.new_mod.sdk = Some(sdk);
            app.new_mod.tools = Some(tools);
        }
        Done::Created(created) => {
            app.new_mod.stage = match *created {
                Ok(created) => Stage::Created(created),
                Err(why) => Stage::Failed(why),
            }
        }
    }
    true
}

/// The commands that build the mod and run it in the game, from its folder.
pub fn commands(created: &CreatedMod) -> String {
    format!(
        "cd {}\ntgg mod build\ntgg mod dev --watch",
        shell_quoted(&created.dir.display().to_string())
    )
}

/// `path` as one word for a shell: quoted when it has a space or a quote.
fn shell_quoted(path: &str) -> String {
    if path.contains([' ', '\'', '"']) {
        format!("'{}'", path.replace('\'', r"'\''"))
    } else {
        path.to_owned()
    }
}

pub fn copy_commands(app: &mut App, tx: &mut Tx) {
    if let Stage::Created(created) = &app.new_mod.stage {
        tx.copy(commands(created));
        app.tell("Copied the commands.", false);
    }
}

/// Show the mod's folder in the system's file manager.
pub fn open_folder(app: &mut App, tx: &mut Tx) {
    let Stage::Created(created) = &app.new_mod.stage else {
        return;
    };
    let dir = created.dir.display().to_string();
    if let Err(error) = tx.open_url(&dir) {
        app.tell(format!("Couldn't open {dir}: {error}"), true);
    }
}

/// The guide to writing a mod, for the SDK it started from.
pub fn open_guide(app: &mut App, tx: &mut Tx) {
    let Stage::Created(created) = &app.new_mod.stage else {
        return;
    };
    let url = library::docs_url(&created.sdk_version, "writing-mods");
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
