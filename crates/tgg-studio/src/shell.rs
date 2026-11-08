//! The window's frame: the sidebar of places beside the place showing, and
//! the title row above it, which moves the window. Welcome takes the whole
//! window until the player has a game. The sidebar hides whole; its toggle
//! keeps one place at the top left whether it shows or not. Drawing only: what
//! it shows comes from the app's state, and pressing calls `nav::actions`.

use crate::mods::Disc;
use crate::nav::{Group, Place, actions as nav};
use crate::state::App;
use crate::ui::tokens::{Palette, motion, radius, size, space, text};
use crate::ui::{
    PLACE_INSET, close_glyph, icon_button, place_row, play_action, rule, sidebar_glyph, spacer,
    text_action,
};
use crate::{
    editor, keys, library, melee, mods, newmod, preferences, replays, report, signin, slippi,
    welcome, window,
};
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
    let does = if shown {
        "Hide sidebar"
    } else {
        "Show sidebar"
    };
    let toggle = icon_button(sidebar_glyph(&p), does, &p)
        .key("shell/toggle")
        .help(keys::hint(does, keys::Command::ToggleSidebar))
        .on_press(nav::toggle_sidebar)
        .absolute(TOGGLE_AT.0, TOGGLE_AT.1);
    row()
        .w_full()
        .h_full()
        .fill(p.bg)
        // Its width decides whether lists and details sit side by side.
        .on_layout(|app: &mut App, rect: [f32; 4]| {
            if (app.window_width - rect[2]).abs() >= 0.5 {
                app.window_width = rect[2];
            }
        })
        .child(sidebar(app, &p))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .child(title_row(app, &p))
                .child(place(app, &p)),
        )
        // A mod's details, the whole window tall at its right, as the sidebar is at its left.
        .child(mods::details::view(app, &p))
        .child(replays::details::view(app, &p))
        .child(toggle)
        // Dialogs, over everything.
        .children(library::review::view(app, &p))
        .children(editor::view::prompt(app, &p))
        .children(mods::review::view(app, &p))
        .children(mods::versions::view(app, &p))
        .children(preferences::view::view(app, &p))
        // A select's menu, over Settings.
        .children(preferences::view::menu(app, &p))
        .children(report::view::view(app, &p))
        .children(signin::view::sign_in(app, &p))
        .children(signin::view::account(app, &p))
        .children(replays::review::view(app, &p))
        .children(newmod::view::view(app, &p))
        // Its examples' menu, over it.
        .children(newmod::view::menu(app, &p))
}

/// The places, recent edits, and Settings at the foot. Hidden, it narrows to
/// nothing on the default spring rather than vanishing.
fn sidebar(app: &App, p: &Palette) -> El {
    let shown = app.nav.sidebar;
    // The places under their games' names (Slippi, tgg-melee), each name
    // with its game's Play, so which game a place is about, and what Play
    // starts, shows. Every row has a fixed height, so the marker's offset
    // is their sum.
    let mut places = Vec::new();
    let mut at = 0.0;
    let mut marked = 0.0;
    for (n, group) in Group::ALL.into_iter().enumerate() {
        let head = if n == 0 {
            size::GROUP_HEAD
        } else {
            size::GROUP_HEAD + space::SM
        };
        places.push(
            row()
                .key(format!("sidebar/group/{n}"))
                .h(head)
                .items_end()
                .pl(PLACE_INSET)
                .child(
                    row()
                        .h(size::GROUP_HEAD)
                        .w_full()
                        .items_center()
                        .child(label(group.label()).font(text::LABEL).color(p.faint))
                        .child(el().grow(1.0))
                        .children(game_action(app, group, p)),
                ),
        );
        at += head + space::TIGHT;
        for place in Place::ALL
            .into_iter()
            .filter(|place| place.group() == group)
        {
            if app.nav.place == place {
                marked = at;
            }
            places.push(
                place_row(place.label(), None, app.nav.place == place, p)
                    .key(place.key())
                    .on_press_tx(move |app: &mut App, tx| editor::actions::go(app, place, tx)),
            );
            at += size::ROW + space::TIGHT;
        }
    }
    // One marker glides between places, so the move from one to the next shows.
    let marker = el()
        .key("sidebar/marker")
        .w(size::SIDEBAR - 2.0 * space::XS)
        .h(size::ROW)
        .radius(radius::CONTROL)
        .fill(p.hover)
        .motion(motion::SNAPPY)
        .absolute(0.0, marked);
    // The foot: who's signed in (else Sign in), and Settings beside it.
    let foot = signin::view::foot(app, p);
    let inner = col()
        .w(size::SIDEBAR)
        .h_full()
        .child(
            row()
                .h(size::TITLE_ROW)
                .pl(TITLE_CLEARANCE)
                .items_center()
                .window_drag()
                .child(label("textures.gg").font(text::WORDMARK).color(p.ink)),
        )
        .child(
            col()
                .mx(space::XS)
                .my(space::SNUG)
                .gap(space::TIGHT)
                .child(marker)
                .children(places),
        )
        .children(recent(app, p))
        .child(spacer())
        .child(rule(p))
        .child(foot);
    row()
        .key("shell/sidebar")
        .w(if shown {
            size::SIDEBAR + size::LINE
        } else {
            0.0
        })
        .h_full()
        .motion(motion::DEFAULT)
        .clip_rounded(0.0)
        .fill(p.sunk)
        .child(inner)
        .child(el().w(size::LINE).h_full().fill(p.line))
}

/// What a game's name in the sidebar offers: Play, when it can start;
/// tgg-melee's Stop while it runs; Set up, while tgg-melee or its clean
/// disc is missing (Mods says what's needed). Slippi offers nothing until
/// Slippi Launcher has installed its Dolphin.
fn game_action(app: &App, group: Group, p: &Palette) -> Option<El> {
    let (action, key) = match group {
        Group::Iso if app.slippi.running => (
            label("Open").font(text::LABEL).color(p.faint),
            "sidebar/slippi/open",
        ),
        Group::Iso => {
            app.slippi.dolphin.as_ref()?;
            app.game.as_ref()?;
            (
                play_action("Play Slippi", p).on_press(slippi::actions::play),
                "sidebar/slippi/play",
            )
        }
        Group::Port if app.mods.playing.is_some() => (
            text_action("Stop", false, p).on_press(mods::actions::stop),
            "sidebar/tgg-melee/stop",
        ),
        Group::Port => match (app.mods.port(), &app.mods.disc) {
            (Some(_), Disc::Found(_)) => (
                play_action("Play tgg-melee", p).on_press(mods::actions::play),
                "sidebar/tgg-melee/play",
            ),
            (None, _) | (Some(_), Disc::Missing) => (
                text_action("Set up", false, p)
                    .on_press_tx(|app: &mut App, tx| editor::actions::go(app, Place::Mods, tx)),
                "sidebar/tgg-melee/set-up",
            ),
            // Still looking for the disc.
            (Some(_), _) => return None,
        },
    };
    Some(action.key(key))
}

/// The costumes and stages edited lately, the latest first: pressing one
/// opens it for editing again (through the unsaved-changes prompt).
fn recent(app: &App, p: &Palette) -> Option<El> {
    let slots = &app.settings.recent;
    if slots.is_empty() {
        return None;
    }
    let open = app.editor.as_ref().map(|document| document.slot);
    let rows = slots.iter().copied().map(|slot| {
        place_row(
            &tgg_app::costumes::slot_label(Some(slot)),
            None,
            open == Some(slot),
            p,
        )
        .key(format!("sidebar/recent/{}", slot.file_name()))
        .on_press_tx(move |app: &mut App, tx| editor::actions::edit_recent(app, slot, tx))
    });
    Some(
        col()
            .key("sidebar/recent")
            .mx(space::XS)
            .mt(space::MD)
            .gap(space::TIGHT)
            .child(
                label("Recent")
                    .font(text::LABEL)
                    .color(p.faint)
                    .px(PLACE_INSET)
                    .pb(space::XXS),
            )
            .children(rows),
    )
}

/// Where the player is and what the place can do; empty on a top-level
/// place, which says its own name. Its empty stretch moves the window.
fn title_row(app: &App, p: &Palette) -> El {
    let start = if app.nav.sidebar {
        space::LG
    } else {
        TITLE_CLEARANCE
    };
    let mut row = row()
        .key("shell/title")
        .h(size::TITLE_ROW)
        .pl(start)
        .pr(space::XS)
        .gap(space::XS)
        .items_center()
        .motion(motion::DEFAULT)
        .window_drag()
        .children(crumbs(app, p))
        .children(notice(app, p))
        .child(el().grow(1.0))
        .children(update(app, p))
        .children(title_actions(app, p));
    // While the details pane shows, the window's controls sit in its head, at the window's edge.
    if !mods::details::shown(app)
        && !replays::details::shown(app)
        && let Some(controls) = window_controls(p)
    {
        row = row.child(controls);
    }
    row
}

/// The window's own controls, where the app draws them (not on macOS, which
/// keeps its own): close.
pub fn window_controls(p: &Palette) -> Option<El> {
    cfg!(not(target_os = "macos")).then(|| {
        icon_button(close_glyph(p), "Close", p)
            .key("window/close")
            .on_press_tx(window::close)
    })
}

fn place(app: &App, p: &Palette) -> El {
    let page = match app.nav.place {
        Place::Melee => melee::view::view(app, p),
        Place::Library => library::view::view(app, p),
        Place::Mods => mods::view::view(app, p),
        Place::Replays => replays::view::view(app, p),
    };
    col()
        .key("shell/place")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .child(page)
}

/// Where the player is, when it's below a top-level place.
fn crumbs(app: &App, p: &Palette) -> Option<El> {
    if let Some(document) = &app.editor {
        return Some(editor::view::crumbs(document, p));
    }
    match (app.nav.place, app.melee.subject) {
        (Place::Melee, Some(subject)) => Some(melee::subject::crumbs(subject, p)),
        _ => None,
    }
}

/// What just happened, said in the title row.
fn notice(app: &App, p: &Palette) -> Option<El> {
    let notice = app.notice.as_ref()?;
    Some(
        label(notice.text.clone())
            .key("shell/notice")
            .font(text::BODY)
            .color(if notice.error { p.danger } else { p.muted })
            .truncate()
            .min_w(0.0)
            .shrink()
            .ml(space::SM),
    )
}

/// The one or few actions the place showing offers, at the title row's end.
fn title_actions(app: &App, p: &Palette) -> Option<El> {
    if let Some(document) = &app.editor {
        return Some(editor::view::actions(document, p));
    }
    match app.nav.place {
        Place::Melee => melee::subject::title_actions(app, p),
        // Each game's Play is beside its name in the sidebar.
        Place::Mods | Place::Replays | Place::Library => None,
    }
}

/// A newer version of the app, when the launch check found one: a link to
/// where the player gets it.
fn update(app: &App, p: &Palette) -> Option<El> {
    let version = app.update.as_ref()?;
    Some(
        text_action(&format!("tgg-studio {version} is out"), false, p)
            .key("shell/update")
            .on_press_tx(|app: &mut App, tx| {
                if let Err(error) = tx.open_url(tgg_app::update::DOWNLOAD_URL) {
                    app.tell(format!("Couldn't open the download page: {error}"), true);
                }
            }),
    )
}
