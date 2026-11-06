//! The app's keyboard shortcuts, in one table: each binding is a chord, the
//! command it runs, and where it applies. A key the window gets, when
//! nothing focused takes it, runs the first binding that matches in the
//! most particular scope that's active, so Escape leaves the editor while a
//! costume is open and goes back to the roster otherwise. The same table
//! names each shortcut for buttons and tooltips.

use crate::state::App;
use rune::{Key, KeyEvent, Tx};

/// What a shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Up a level: out of the editor, or from a fighter back to the roster.
    Back,
    ToggleSidebar,
    TogglePlayback,
    Undo,
    Redo,
    Save,
    SaveAndInstall,
}

/// Where a binding applies, most particular last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// Anywhere there's a game.
    Window,
    /// While a model is on the stage.
    Stage,
    /// While a costume is open for editing.
    Editor,
}

/// The key of a chord: a character with Ctrl (Cmd on macOS) or a named key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Ctrl plus this character (Cmd on macOS).
    Command(char),
    Escape,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub press: Press,
    pub shift: bool,
    pub alt: bool,
}

pub struct Binding {
    pub chord: Chord,
    pub command: Command,
    pub scope: Scope,
}

const fn bind(press: Press, shift: bool, alt: bool, command: Command, scope: Scope) -> Binding {
    Binding { chord: Chord { press, shift, alt }, command, scope }
}

/// Every shortcut. A chord may appear in several scopes; the most particular
/// active one wins.
pub const BINDINGS: &[Binding] = &[
    bind(Press::Escape, false, false, Command::Back, Scope::Window),
    bind(Press::Command('\\'), false, false, Command::ToggleSidebar, Scope::Window),
    bind(Press::Space, false, false, Command::TogglePlayback, Scope::Stage),
    bind(Press::Command('z'), false, false, Command::Undo, Scope::Editor),
    bind(Press::Command('z'), true, false, Command::Redo, Scope::Editor),
    bind(Press::Command('s'), false, false, Command::Save, Scope::Editor),
    bind(Press::Command('s'), false, true, Command::SaveAndInstall, Scope::Editor),
];

impl Chord {
    fn matches(self, key: &KeyEvent) -> bool {
        let pressed = match self.press {
            Press::Command(character) => key.is_command(character),
            Press::Escape => key.key == Key::Escape && !key.mods.cmd,
            Press::Space => matches!(&key.key, Key::Text(text) if text == " ") && !key.mods.cmd,
        };
        pressed && key.mods.shift == self.shift && key.mods.alt == self.alt
    }

    /// How the chord reads on this platform: "Ctrl Shift Z", "⌘⇧Z".
    pub fn label(self) -> String {
        let mac = cfg!(target_os = "macos");
        let mut parts: Vec<String> = Vec::new();
        if matches!(self.press, Press::Command(_)) {
            parts.push(if mac { "⌘" } else { "Ctrl" }.into());
        }
        if self.alt {
            parts.push(if mac { "⌥" } else { "Alt" }.into());
        }
        if self.shift {
            parts.push(if mac { "⇧" } else { "Shift" }.into());
        }
        parts.push(match self.press {
            Press::Command(character) => character.to_ascii_uppercase().to_string(),
            Press::Escape => "Esc".into(),
            Press::Space => "Space".into(),
        });
        parts.join(if mac { "" } else { " " })
    }
}

/// Whether `scope` applies now.
fn active(app: &App, scope: Scope) -> bool {
    match scope {
        Scope::Window => app.game.is_some(),
        Scope::Stage => app.stage.shown.is_some(),
        Scope::Editor => app.editor.is_some(),
    }
}

/// The command `key` runs now, if any: the matching binding in the most
/// particular active scope.
pub fn command_for(app: &App, key: &KeyEvent) -> Option<Command> {
    BINDINGS
        .iter()
        .filter(|binding| active(app, binding.scope) && binding.chord.matches(key))
        .max_by_key(|binding| binding.scope)
        .map(|binding| binding.command)
}

/// The shortcut for `command`, as it reads, for buttons and tooltips.
pub fn shortcut(command: Command) -> Option<String> {
    BINDINGS.iter().find(|binding| binding.command == command).map(|binding| binding.chord.label())
}

/// A tooltip saying what an action does and its shortcut: "Save (Ctrl S)".
pub fn hint(does: &str, command: Command) -> String {
    match shortcut(command) {
        Some(keys) => format!("{does} ({keys})"),
        None => does.to_owned(),
    }
}

/// Run `command`: each goes to the actions of what it belongs to.
pub fn run(app: &mut App, command: Command, tx: &mut Tx) -> bool {
    use crate::{editor::actions as editor, nav::actions as nav, stage::actions as stage};
    match command {
        Command::Back if app.editor.is_some() => editor::leave(app, tx),
        Command::Back => return nav::up(app),
        Command::ToggleSidebar => nav::toggle_sidebar(app),
        Command::TogglePlayback => stage::toggle_playback(app),
        Command::Undo => editor::undo(app),
        Command::Redo => editor::redo(app),
        Command::Save => editor::save(app, false, None, tx),
        Command::SaveAndInstall => editor::save(app, true, None, tx),
    }
    true
}

/// The window's keys, when nothing focused takes them.
pub fn on_key(app: &mut App, key: &KeyEvent, tx: &mut Tx) -> bool {
    command_for(app, key).is_some_and(|command| run(app, command, tx))
}

#[cfg(test)]
mod tests {
    use super::{BINDINGS, Chord, Press};

    #[test]
    fn no_chord_is_bound_twice_in_one_scope() {
        for (index, binding) in BINDINGS.iter().enumerate() {
            let twice = BINDINGS[index + 1..].iter().any(|other| other.scope == binding.scope && other.chord == binding.chord);
            assert!(!twice, "{:?} is bound twice in {:?}", binding.chord, binding.scope);
        }
    }

    #[test]
    fn chords_read_as_the_platform_writes_them() {
        let redo = Chord { press: Press::Command('z'), shift: true, alt: false };
        let expected = if cfg!(target_os = "macos") { "⌘⇧Z" } else { "Ctrl Shift Z" };
        assert_eq!(redo.label(), expected);
    }
}
