//! What the player does in New mod, and what comes back from its jobs.

use super::library::{self, CreatedMod, Spec, Tools};
use super::{NewMod, Stage, TEMPLATE};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::{FileDialogOptions, Tx};

/// What a New mod job found.
pub enum Done {
    /// The SDK's examples (or why they couldn't be read), and the tools.
    Ready {
        examples: Result<Vec<String>, String>,
        tools: Tools,
    },
    /// The mod made, or why not.
    Created(Result<CreatedMod, String>),
}

/// Open the dialog afresh, reading the SDK's examples and looking for the
/// tools on a job.
pub fn open(app: &mut App) {
    if app.mods.port().is_none() {
        return;
    }
    let parent = if app.new_mod.parent.as_os_str().is_empty() {
        super::default_parent()
    } else {
        app.new_mod.parent.clone()
    };
    app.new_mod = NewMod {
        open: true,
        parent,
        example: TEMPLATE.to_owned(),
        ..NewMod::default()
    };
    let data = app.data.tgg.clone();
    app.jobs.spawn("new mod: examples", move || {
        Job::NewMod(Box::new(Done::Ready {
            examples: library::sdk_examples(&data),
            tools: library::tools(),
        }))
    });
}

pub fn close(app: &mut App) {
    if !matches!(app.new_mod.stage, Stage::Creating) {
        app.new_mod.open = false;
    }
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

/// Whether the mod can be made as filled in: a valid id, while nothing's
/// being made.
pub fn can_create(new_mod: &NewMod) -> bool {
    matches!(new_mod.stage, Stage::Editing | Stage::Failed(_)) && new_mod.mod_id().is_some()
}

/// Make the mod, on a job.
pub fn create(app: &mut App) {
    let new_mod = &app.new_mod;
    let (true, Some(id)) = (can_create(new_mod), new_mod.mod_id()) else {
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
    app.new_mod.stage = Stage::Creating;
    app.new_mod.menu = false;
    let data = app.data.tgg.clone();
    app.jobs.spawn("new mod", move || {
        Job::NewMod(Box::new(Done::Created(library::new_mod(&data, &spec))))
    });
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Ready { examples, tools } => {
            if let Ok(examples) = &examples
                && !examples
                    .iter()
                    .any(|example| *example == app.new_mod.example)
                && let Some(first) = examples.first()
            {
                app.new_mod.example = first.clone();
            }
            app.new_mod.examples = Some(examples);
            app.new_mod.tools = Some(tools);
        }
        Done::Created(Ok(created)) => app.new_mod.stage = Stage::Created(created),
        Done::Created(Err(why)) => app.new_mod.stage = Stage::Failed(why),
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
