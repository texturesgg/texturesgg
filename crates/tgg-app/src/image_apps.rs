//! The apps on this computer that open PNGs, to offer as the image editor a
//! texture opens in: on Linux, from the desktop entries in the XDG data
//! folders (`applications/*.desktop` that list `image/png`), graphics apps
//! first, web browsers left out. Windows and macOS list none yet; the
//! player browses for an app there instead.

use std::path::{Path, PathBuf};

/// An app that opens PNGs: the name it shows, and the command that opens a
/// file with it (the file's path goes on its end).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageApp {
    pub name: String,
    pub command: String,
}

/// The apps that open PNGs, graphics apps first, then by name.
pub fn image_apps() -> Vec<ImageApp> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let mut seen = Vec::new();
    let mut apps: Vec<(bool, ImageApp)> = Vec::new();
    for folder in data_folders() {
        let Ok(entries) = std::fs::read_dir(folder.join("applications")) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            // The first folder with an entry of a name wins, as XDG says.
            let Some(id) = path.file_name().map(|name| name.to_owned()) else {
                continue;
            };
            if path
                .extension()
                .is_none_or(|extension| extension != "desktop")
                || seen.contains(&id)
            {
                continue;
            }
            seen.push(id);
            if let Some(entry) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| parse(&text))
            {
                apps.push(entry);
            }
        }
    }
    apps.sort_by(|(a_graphics, a), (b_graphics, b)| {
        b_graphics
            .cmp(a_graphics)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps.into_iter().map(|(_, app)| app).collect()
}

/// `$XDG_DATA_HOME` (else `~/.local/share`), then each of `$XDG_DATA_DIRS`
/// (else `/usr/local/share` and `/usr/share`).
fn data_folders() -> Vec<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let home = env("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|home| Path::new(&home).join(".local/share")));
    let dirs = env("XDG_DATA_DIRS").map_or_else(
        || {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        },
        |dirs| std::env::split_paths(&dirs).collect(),
    );
    home.into_iter().chain(dirs).collect()
}

/// A desktop entry's app, when it's one that opens PNGs and shows in menus,
/// and whether it's a graphics app.
fn parse(text: &str) -> Option<(bool, ImageApp)> {
    let mut in_entry = false;
    let (mut name, mut exec, mut opens_png, mut graphics, mut browser, mut hidden) =
        (None, None, false, false, false, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_owned()),
            "Exec" => exec = Some(value.trim().to_owned()),
            "MimeType" => opens_png = value.split(';').any(|kind| kind.trim() == "image/png"),
            "Categories" => {
                graphics = value.split(';').any(|category| category == "Graphics");
                browser = value.split(';').any(|category| category == "WebBrowser");
            }
            "NoDisplay" | "Hidden" => hidden |= value.trim() == "true",
            "Type" => hidden |= value.trim() != "Application",
            _ => {}
        }
    }
    if !opens_png || browser || hidden {
        return None;
    }
    let (name, command) = (name?, without_field_codes(&exec?));
    (!command.is_empty()).then_some((graphics, ImageApp { name, command }))
}

/// An `Exec` line without its field codes (`%f`, `%U` and the rest), `%%`
/// read as `%`: the command the file's path goes after.
fn without_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut characters = exec.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '%' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => out.push('%'),
            // A field code: dropped.
            Some(_) | None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{parse, without_field_codes};

    #[test]
    fn an_exec_line_loses_its_field_codes() {
        assert_eq!(without_field_codes("aseprite %U"), "aseprite");
        assert_eq!(
            without_field_codes("gimp-3.0 %U --new-instance"),
            "gimp-3.0 --new-instance"
        );
        assert_eq!(without_field_codes("app --rate 100%%"), "app --rate 100%");
    }

    #[test]
    fn only_apps_that_open_pngs_and_show_are_offered() {
        let aseprite = "[Desktop Entry]\nType=Application\nName=Aseprite\nExec=aseprite %U\nCategories=Graphics;2DGraphics;\nMimeType=image/gif;image/png;\n";
        let parsed = parse(aseprite).expect("an image app");
        assert_eq!(
            (parsed.0, parsed.1.name.as_str(), parsed.1.command.as_str()),
            (true, "Aseprite", "aseprite")
        );
        let browser = "[Desktop Entry]\nType=Application\nName=Helium\nExec=helium %U\nCategories=Network;WebBrowser;\nMimeType=image/png;\n";
        assert!(
            parse(browser).is_none(),
            "web browsers aren't image editors"
        );
        let hidden = "[Desktop Entry]\nType=Application\nName=Hidden\nExec=x %f\nNoDisplay=true\nMimeType=image/png;\n";
        assert!(parse(hidden).is_none());
        let other =
            "[Desktop Entry]\nType=Application\nName=Text\nExec=x %f\nMimeType=text/plain;\n";
        assert!(parse(other).is_none());
        // Keys in another group don't count.
        let action = "[Desktop Entry]\nType=Application\nName=Viewer\nExec=v %f\nMimeType=image/png;\n[Desktop Action new]\nName=New window\nExec=v --new\n";
        assert_eq!(
            parse(action).map(|(_, app)| app.name),
            Some("Viewer".into())
        );
    }
}
