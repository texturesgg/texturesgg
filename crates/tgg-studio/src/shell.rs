//! The window's frame: the sidebar beside the content, and the top bar
//! inside the content. First run takes the whole window until the player has
//! a disc and tgg-melee. The sidebar hides completely; the button that brings
//! it back is the first thing in the top bar, and the same button in the
//! sidebar hides it. No borders between regions. Drawing only: what it shows
//! comes from the app's state, and pressing calls `nav::actions`.

use crate::nav::{Place, actions as nav};
use crate::state::{App, NoticeAction};
use crate::ui::tokens::{Palette, elevation, motion, radius, size, space, text};
use crate::ui::{
    Icon, close_glyph, dot, icon, icon_button, place_row, sidebar_glyph, spacer, text_action,
    title_bar,
};
use crate::{
    editor, first_run, keys, library, melee, mods, newmod, play, preferences, profile, replays,
    report, signin,
};
use rune::{Cx, El, col, el, row, text as label};

/// How far in the sidebar's head starts on macOS, to clear the window's
/// traffic lights, which sit over it.
const TRAFFIC_LIGHTS: f32 = 72.0;

pub fn view(app: &App, _: &mut Cx<App>) -> El {
    // Dark only for launch.
    let p = Palette::gallery();
    // First run takes the whole window until there's a disc and tgg-melee;
    // signing in is still asked in its dialog over it.
    if app.first_run.is_some() {
        return col()
            .w_full()
            .h_full()
            .child(first_run::view::view(app, &p))
            .children(signin::view::sign_in(app, &p));
    }
    // No fill of its own: the sidebar and the content each fill theirs, so
    // where the window is see-through the sidebar's glass shows.
    row()
        .w_full()
        .h_full()
        // Its width decides whether lists and details sit side by side.
        .on_layout(|app: &mut App, rect: [f32; 4]| {
            if (app.window_width - rect[2]).abs() >= 0.5 {
                app.window_width = rect[2];
            }
            if (app.window_height - rect[3]).abs() >= 0.5 {
                app.window_height = rect[3];
            }
        })
        .child(sidebar(app, &p))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .fill(p.bg)
                .child(top_bar(app, &p))
                .child(place(app, &p)),
        )
        // A mod's details, the whole window tall at its right, as the sidebar is at its left.
        .child(mods::details::view(app, &p))
        .child(replays::details::view(app, &p))
        .children(notice(app, &p))
        // Dialogs, over everything.
        .children(library::review::view(app, &p))
        .children(editor::view::prompt(app, &p))
        .children(mods::review::view(app, &p))
        .children(mods::sheet::view(app, &p))
        .children(mods::versions::view(app, &p))
        .children(preferences::view::view(app, &p))
        // A select's menu, over Settings.
        .children(preferences::view::menu(app, &p))
        .children(report::view::view(app, &p))
        .children(signin::view::sign_in(app, &p))
        .children(replays::review::view(app, &p))
        .children(newmod::view::view(app, &p))
        .children(crate::sets::view::trust(app, &p))
        // Play's set picker's menu, over Settings, and its friend sheet.
        .children(play::view::menu(app, &p))
        .children(play::view::friend(app, &p))
}

/// The button that shows or hides the sidebar.
fn toggle(app: &App, p: &Palette) -> El {
    let does = if app.nav.sidebar {
        "Hide sidebar"
    } else {
        "Show sidebar"
    };
    icon_button(sidebar_glyph(p), does, p)
        .key("shell/toggle")
        .help(keys::hint(does, keys::Command::ToggleSidebar))
        .on_press(nav::toggle_sidebar)
}

/// The place's glyph in the sidebar.
fn glyph(place: Place) -> Icon {
    match place {
        Place::Play => Icon::Play,
        Place::Skins => Icon::Skins,
        Place::Mods => Icon::Mods,
        Place::Replays | Place::Profile => Icon::Replays,
    }
}

/// The app's name with the hide button, the four places, and the account
/// row with the settings gear at the foot. Nothing else until friends exist.
/// Hidden, it narrows to nothing.
fn sidebar(app: &App, p: &Palette) -> El {
    let shown = app.nav.sidebar;
    let places = Place::ALL.into_iter().map(|place| {
        let current = app.nav.place == place;
        let ink = if current { p.ink } else { p.faint };
        place_row(Some(icon(glyph(place), ink)), place.label(), current, p)
            .key(place.key())
            .on_press_tx(move |app: &mut App, tx| editor::actions::go(app, place, tx))
    });
    let head_inset = if cfg!(target_os = "macos") {
        TRAFFIC_LIGHTS
    } else {
        space::XS
    };
    let inner = col()
        .w(size::SIDEBAR)
        .h_full()
        .px(space::XS)
        .pb(space::XS)
        .child(
            title_bar(row())
                .h(size::TOP_BAR)
                .pl(head_inset - space::XS)
                .gap(space::XS)
                .items_center()
                .child(toggle(app, p))
                .child(label("textures.gg").font(text::WORDMARK).color(p.ink)),
        )
        .child(col().gap(space::TIGHT).children(places))
        .child(spacer())
        .child(signin::view::foot(app, p));
    let base = col()
        .key("shell/sidebar")
        .w(if shown { size::SIDEBAR } else { 0.0 })
        .h_full()
        .motion(motion::SIDEBAR)
        .clip_rounded(0.0)
        .fill(p.sunk);
    // Smoked glass where the window is see-through (`tokens::GLASS`); solid
    // everywhere else. The material is macOS's blur; elsewhere it's a no-op.
    let base = if crate::ui::tokens::GLASS {
        base.system_material(rune::SystemMaterial::Popover)
            .fill(crate::ui::tokens::glass())
    } else {
        base
    };
    base.child(inner)
}

/// The top bar: the sidebar's button while it's hidden, the place's name
/// (or where the player is under it), and at its end the place's search and
/// its one action. Its empty stretch moves the window on macOS.
fn top_bar(app: &App, p: &Palette) -> El {
    let start = if app.nav.sidebar {
        space::PAGE
    } else if cfg!(target_os = "macos") {
        TRAFFIC_LIGHTS
    } else {
        space::SM
    };
    let name = crumbs(app, p).unwrap_or_else(|| {
        label(app.nav.place.label())
            .key("shell/title/name")
            .font(text::TITLE)
            .color(p.ink)
    });
    title_bar(row())
        .key("shell/title")
        .h(size::TOP_BAR)
        .pl(start)
        .pr(space::PAGE - space::XXS)
        .gap(space::SM)
        .items_center()
        .motion(motion::SIDEBAR)
        .children((!app.nav.sidebar).then(|| toggle(app, p)))
        .child(name)
        .child(el().grow(1.0))
        .children(update(app, p))
        .children(place_top(app, p))
}

/// The place showing, coming in with a fade and a rise when it changes.
fn place(app: &App, p: &Palette) -> El {
    let page = match app.nav.place {
        Place::Play => play::view::view(app, p),
        Place::Skins => melee::view::view(app, p),
        Place::Mods => mods::view::view(app, p),
        Place::Replays => replays::view::view(app, p),
        Place::Profile => profile::view::view(app, p),
    };
    col()
        .key(format!("shell/place/{}", app.nav.place.label()))
        .transition(motion::enter())
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
        (Place::Skins, Some(subject)) => Some(melee::subject::crumbs(subject, p)),
        _ => None,
    }
}

/// The place's search and its one action, at the top bar's end.
fn place_top(app: &App, p: &Palette) -> Vec<El> {
    if let Some(document) = &app.editor {
        return vec![editor::view::actions(document, p)];
    }
    match app.nav.place {
        Place::Skins => vec![melee::view::search(app, p)],
        Place::Mods => mods::view::top_bar(app, p),
        Place::Replays => {
            let mut top = replays::view::top_bar(app, p);
            top.push(
                crate::ui::button_small("Open a replay file", crate::ui::ButtonKind::Secondary, p)
                    .key("replays/open")
                    .on_press_tx(replays::actions::open_file),
            );
            top
        }
        Place::Play => play::view::top_bar(app, p),
        Place::Profile => Vec::new(),
    }
}

/// What just happened, in a toast at the foot of the place: its words, Undo
/// after an install, and dismissing it. A failure is marked in red.
fn notice(app: &App, p: &Palette) -> Option<El> {
    let notice = app.notice.as_ref()?;
    if app.window_height <= 0.0 {
        return None;
    }
    let left = if app.nav.sidebar { size::SIDEBAR } else { 0.0 } + space::PAGE;
    let top = app.window_height - size::TOAST_HEIGHT - space::LG;
    let actions: Vec<El> = match notice.action {
        Some(NoticeAction::Undo(slot)) => vec![
            text_action("Undo", false, p)
                .key("shell/notice/undo")
                .on_press(move |app: &mut App| library::actions::undo(app, slot)),
        ],
        Some(NoticeAction::UndoPort) => vec![
            text_action("Undo", false, p)
                .key("shell/notice/undo")
                .on_press(library::port::undo),
        ],
        Some(NoticeAction::Reopen(_)) => vec![
            text_action("Reopen", false, p)
                .key("shell/notice/reopen")
                .on_press(editor::draft::reopen),
            text_action("Discard", false, p)
                .key("shell/notice/discard")
                .on_press(editor::draft::discard),
        ],
        None => Vec::new(),
    };
    let (y, blur) = elevation::TOAST;
    Some(
        row()
            .key("shell/notice")
            .w(size::TOAST)
            .h(size::TOAST_HEIGHT)
            .pl(space::MD)
            .pr(space::XS)
            .gap(space::XS)
            .items_center()
            .fill(p.raise)
            .border(size::LINE, p.line)
            .radius(radius::CARD)
            .shadow(y, blur, p.shadow)
            .transition(motion::enter())
            .role("status")
            .label(notice.text.clone())
            .children(notice.error.then(|| dot(p.danger)))
            .child(
                label(notice.text.clone())
                    .font(text::BODY)
                    .color(p.ink)
                    .truncate()
                    .min_w(0.0)
                    .grow(1.0),
            )
            .children(actions)
            .child(
                icon_button(close_glyph(p), "Dismiss", p)
                    .key("shell/notice/dismiss")
                    .on_press(|app: &mut App| app.notice = None),
            )
            .absolute(left, top),
    )
}

/// A newer version of the app, when the launch check found one: a link to
/// where the player gets it.
fn update(app: &App, p: &Palette) -> Option<El> {
    let version = app.update.as_ref()?;
    Some(
        text_action(&format!("textures.gg {version} is out"), false, p)
            .key("shell/update")
            .on_press_tx(|app: &mut App, tx| {
                if let Err(error) = tx.open_url(tgg_app::update::DOWNLOAD_URL) {
                    app.tell(format!("Couldn't open the download page: {error}"), true);
                }
            }),
    )
}
