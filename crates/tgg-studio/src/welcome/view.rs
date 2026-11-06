//! The Welcome screen: what the app needs, the ISOs found, and a way to
//! choose one.

use super::actions;
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{ButtonKind, button};
use rune::{El, col, el, row, text as label};
use tgg_app::GameChoice;

pub fn view(app: &App, p: &Palette) -> El {
    let welcome = &app.welcome;
    let mut body = col().w(520.0).gap(space::MD)
        .child(label("textures.gg").font(text::WORDMARK).color(p.faint))
        .child(label("Find your Melee").font(text::H1).color(p.ink))
        .child(label("textures.gg works on your own copy of Super Smash Bros. Melee, NTSC 1.02. Pick one found in your game folders, or choose its ISO.")
            .font(text::BODY).color(p.muted).wrap());
    if !welcome.games.is_empty() {
        body = body.child(games(&welcome.games, p));
    }
    body = body.child(row().pt(space::XS)
        .child(button("Choose an ISO…", ButtonKind::Primary, p).key("welcome/choose").on_press_tx(actions::choose)));
    if let Some(problem) = &welcome.problem {
        body = body.child(label(problem.clone()).key("welcome/problem").font(text::BODY).color(p.danger).wrap());
    }
    col().w_full().h_full().fill(p.bg).center()
        .child(title_band(p))
        .child(body)
}

/// The ISOs found, one row each: its file name, its folder, and whether
/// Slippi plays it.
fn games(games: &[GameChoice], p: &Palette) -> El {
    let rows = games.iter().enumerate().map(|(index, game)| {
        let path = game.path.clone();
        let name = game.path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let folder = game.path.parent().map_or_else(String::new, |folder| folder.display().to_string());
        let note = if game.slippi_plays { "Slippi plays this one" } else { "" };
        row().key(format!("welcome/game/{index}")).h(48.0).px(space::SM).gap(space::SM).items_center()
            .fill(p.surface).hover(p.hover).role("button").label(name.clone()).focusable(true)
            .child(col().grow(1.0).min_w(0.0)
                .child(label(name).font(text::STRONG).color(p.ink).truncate())
                .child(label(folder).font(text::MONO_SMALL).color(p.faint).truncate_middle()))
            .child(label(note).font(text::SMALL).color(p.muted))
            .on_press(move |app: &mut App| actions::use_game_at(app, &path))
    });
    col().border(1.0, p.line_strong).radius(4.0).clip_rounded(4.0).children(rows)
}

/// The band across the top that moves the window, with its controls.
fn title_band(p: &Palette) -> El {
    let mut band = row().h(size::TITLE_ROW).w_full().pr(space::XS).items_center().window_drag()
        .child(el().grow(1.0));
    if let Some(controls) = crate::shell::window_controls(p) {
        band = band.child(controls);
    }
    band.absolute(0.0, 0.0)
}
