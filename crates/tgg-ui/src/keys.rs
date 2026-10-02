//! Key hints for menus and tooltips, written from the same binding string the
//! app binds, so a hint can't drift from its key.
//!
//! Bindings use gpui's syntax (`secondary-shift-s`), where `secondary` is Cmd
//! on macOS and Ctrl elsewhere.

use gpui::SharedString;

/// How `binding` reads on this platform: "Ctrl+Shift+S", or "⇧⌘S" on macOS.
pub fn shortcut(binding: &str) -> SharedString {
    label(binding, cfg!(target_os = "macos")).into()
}

fn label(binding: &str, mac: bool) -> String {
    let mut parts: Vec<&str> = binding.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    let key = match key {
        "enter" => "Enter".to_owned(),
        "escape" => "Esc".to_owned(),
        "space" => "Space".to_owned(),
        "tab" => "Tab".to_owned(),
        key => key.to_uppercase(),
    };
    if mac {
        // macOS lists modifiers in a fixed order: Control, Option, Shift,
        // Command.
        let order = ["ctrl", "alt", "shift", "secondary", "cmd"];
        let mut modifiers: Vec<&str> = parts.clone();
        modifiers.sort_by_key(|modifier| order.iter().position(|known| known == modifier));
        let symbols: String = modifiers
            .iter()
            .map(|modifier| match *modifier {
                "ctrl" => "⌃",
                "alt" => "⌥",
                "shift" => "⇧",
                "secondary" | "cmd" => "⌘",
                other => other,
            })
            .collect();
        format!("{symbols}{key}")
    } else {
        let mut words: Vec<&str> = parts
            .iter()
            .map(|modifier| match *modifier {
                "secondary" | "ctrl" => "Ctrl",
                "alt" => "Alt",
                "shift" => "Shift",
                "cmd" => "Super",
                other => other,
            })
            .collect();
        words.push(&key);
        words.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn hints_follow_the_platform() {
        assert_eq!(label("secondary-o", false), "Ctrl+O");
        assert_eq!(label("secondary-shift-s", false), "Ctrl+Shift+S");
        assert_eq!(label("secondary-o", true), "⌘O");
        assert_eq!(label("secondary-shift-s", true), "⇧⌘S");
        assert_eq!(label("escape", false), "Esc");
    }
}
