//! The window's frame: the sidebar of places beside the place showing, and
//! the title row above it, which moves the window. Welcome takes the whole
//! window until the player has a game. The sidebar hides whole; its toggle
//! keeps one place at the top left whether it shows or not. Drawing only: what
//! it shows comes from the app's state, and pressing calls `nav::actions`.

use crate::nav::{Place, actions as nav};
use crate::state::App;
use crate::ui::tokens::{Palette, motion, size, space, text};
use crate::ui::{close_glyph, icon_button, place_row, sidebar_glyph};
use crate::{library, melee, mods, welcome, window};
use rune::{Cx, El, col, el, row, text as label};

/// Where the sidebar toggle sits, from the window's top left.
const TOGGLE_AT: (f32, f32) = (12.0, (size::TITLE_ROW - size::CONTROL) / 2.0);
/// How far the title row's content starts in while the sidebar is hidden,
/// to clear the toggle.
const TITLE_CLEARANCE: f32 = 52.0;

pub fn view(app: &App, _: &mut Cx<App>) -> El {
    let p = Palette::of(app.settings.appearance());
    if app.game.is_none() {
        return welcome::view::view(app, &p);
    }
    let shown = app.nav.sidebar;
    let toggle = icon_button(sidebar_glyph(&p), if shown { "Hide sidebar" } else { "Show sidebar" }, &p)
        .key("shell/toggle")
        .on_press(nav::toggle_sidebar)
        .absolute(TOGGLE_AT.0, TOGGLE_AT.1);
    row().w_full().h_full().fill(p.bg)
        .child(sidebar(app, &p))
        .child(col().grow(1.0).min_w(0.0).h_full().child(title_row(app, &p)).child(place(app, &p)))
        .child(toggle)
        // Dialogs, over everything.
        .children(library::review::view(app, &p))
}

/// The places, then the player's game at the foot. Hidden, it narrows to
/// nothing on the default spring rather than vanishing.
fn sidebar(app: &App, p: &Palette) -> El {
    let shown = app.nav.sidebar;
    let places = Place::ALL.into_iter().map(|place| {
        place_row(place.label(), None, app.nav.place == place, p)
            .key(place.key())
            .on_press(move |app: &mut App| nav::go(app, place))
    });
    // One marker glides between places, so the move from one to the next shows.
    let index = Place::ALL.iter().position(|place| *place == app.nav.place).unwrap_or(0);
    let marker = el().key("sidebar/marker").w(size::SIDEBAR - 16.0).h(size::ROW).radius(4.0)
        .fill(p.hover).motion(motion::SNAPPY)
        .absolute(0.0, index as f32 * (size::ROW + 2.0));
    let foot = app.game.as_ref().map(|references| {
        let file = references.game().path().file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        col().px(18.0).py(space::SM).gap(2.0)
            .child(label("Melee NTSC 1.02").font(text::LABEL).color(p.muted))
            .child(label(file).font(text::MONO_SMALL).color(p.faint).truncate_middle())
    });
    let inner = col().w(size::SIDEBAR).h_full()
        .child(row().h(size::TITLE_ROW).pl(TITLE_CLEARANCE).items_center().window_drag()
            .child(label("textures.gg").font(text::WORDMARK).color(p.ink)))
        .child(col().mx(8.0).my(6.0).gap(2.0).child(marker).children(places))
        .child(el().grow(1.0))
        .child(el().h(1.0).w_full().fill(p.line))
        .children(foot);
    row().key("shell/sidebar").w(if shown { size::SIDEBAR + 1.0 } else { 0.0 }).h_full()
        .motion(motion::DEFAULT).clip_rounded(0.0).fill(p.sunk)
        .child(inner)
        .child(el().w(1.0).h_full().fill(p.line))
}

/// Where the player is and what the place can do; empty on a top-level
/// place, which says its own name. Its empty stretch moves the window.
fn title_row(app: &App, p: &Palette) -> El {
    let start = if app.nav.sidebar { space::LG } else { TITLE_CLEARANCE };
    let mut row = row().key("shell/title").h(size::TITLE_ROW).pl(start).pr(space::XS).gap(space::XS)
        .items_center().motion(motion::DEFAULT).window_drag()
        .children(crumbs(app, p))
        .children(notice(app, p))
        .child(el().grow(1.0));
    if let Some(controls) = window_controls(p) {
        row = row.child(controls);
    }
    row
}

/// The window's own controls, where the app draws them (not on macOS, which
/// keeps its own): close.
pub fn window_controls(p: &Palette) -> Option<El> {
    cfg!(not(target_os = "macos"))
        .then(|| icon_button(close_glyph(p), "Close", p).key("window/close").on_press_tx(window::close))
}

fn place(app: &App, p: &Palette) -> El {
    let page = match app.nav.place {
        Place::Melee => melee::view::view(app, p),
        Place::Library => library::view::view(app, p),
        Place::Mods => mods::view::view(app, p),
    };
    col().key("shell/place").grow(1.0).min_h(0.0).w_full().child(page)
}

/// Where the player is, when it's below a top-level place.
fn crumbs(app: &App, p: &Palette) -> Option<El> {
    match (app.nav.place, app.melee.subject) {
        (Place::Melee, Some(subject)) => Some(melee::subject::crumbs(subject, p)),
        _ => None,
    }
}

/// What just happened, said in the title row.
fn notice(app: &App, p: &Palette) -> Option<El> {
    let notice = app.notice.as_ref()?;
    Some(label(notice.text.clone()).key("shell/notice").font(text::BODY).color(if notice.error { p.danger } else { p.muted })
        .truncate().min_w(0.0).shrink().ml(space::SM))
}
