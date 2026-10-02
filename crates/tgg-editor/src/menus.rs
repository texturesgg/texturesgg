//! The app's menus: the menu bar on macOS, and elsewhere the menu button
//! at the top row's left. They hold the app, the file and its textures,
//! history, the view, and help. Each item dispatches the same action as its key
//! binding, and shows as disabled where nothing handles it (the editor's
//! items outside the editor).

use crate::editor::{
    EditExternally, ExportPng, ImportPng, OpenDat, Redo, ResetCamera, Save, SaveAndInstall, SaveAs,
    ToggleHighlight, ToggleMovesPane, TogglePlayback, ToggleTexturePane, ToggleTexturesPane, Undo,
};
use crate::{OpenSettings, Quit, ReportProblem, ToggleSidebar};
use gpui::{Menu, MenuItem};

pub(crate) fn menus() -> Vec<Menu> {
    // macOS keeps Settings and Quit in the app's own menu; elsewhere they
    // close the File menu, as they do in most apps there.
    let app = [
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::action("Quit textures.gg", Quit),
    ];
    let mac = cfg!(target_os = "macos");
    let mut file = vec![
        MenuItem::action("Open DAT…", OpenDat),
        MenuItem::separator(),
        MenuItem::action("Save", Save),
        MenuItem::action("Save and Install", SaveAndInstall),
        MenuItem::action("Save As…", SaveAs),
        MenuItem::separator(),
        MenuItem::action("Import PNG…", ImportPng),
        MenuItem::action("Export PNG…", ExportPng),
        MenuItem::action("Edit in Image Editor", EditExternally),
    ];
    let mut menus = Vec::new();
    if mac {
        menus.push(Menu::new("textures.gg").items(app));
    } else {
        file.push(MenuItem::separator());
        file.extend(app);
    }
    menus.extend([
        Menu::new("File").items(file),
        Menu::new("Edit").items([
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
        ]),
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::separator(),
            MenuItem::action("Textures", ToggleTexturesPane),
            MenuItem::action("Texture", ToggleTexturePane),
            MenuItem::action("Moves", ToggleMovesPane),
            MenuItem::separator(),
            MenuItem::action("Play or Pause", TogglePlayback),
            MenuItem::action("Reset Camera", ResetCamera),
            MenuItem::action("Highlight Selection", ToggleHighlight),
        ]),
        Menu::new("Help").items([MenuItem::action("Report a Problem…", ReportProblem)]),
    ]);
    menus
}
