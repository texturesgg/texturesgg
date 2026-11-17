//! The Replays page: the player's replays, newest first, filtered by a
//! search and where they came from; a row each: the two fighters as the
//! player's game draws them, who played and who won, where and how long,
//! whether it plays as is or what it needs, and Play. Pressing a row shows
//! its details beside the list.

use super::{Item, Replays, Status, actions};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, Tone, badge, button, chip, icon_tile, list_row, progress,
    search_box, search_field, skeleton, spacer, split, split_list, split_page, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};

/// The list reads best no wider than this.
const COLUMN: f32 = 860.0;
/// Placeholder rows while the replays are first read.
const PLACEHOLDERS: usize = 6;
/// A fighter's picture in a row.
const FIGHTER: f32 = 40.0;

pub fn view(app: &App, p: &Palette) -> El {
    let replays = &app.replays;
    let refresh = if replays.scanning || replays.refreshing {
        text_action("Reading…", false, p).key("replays/reading")
    } else {
        text_action("Read again", false, p)
            .key("replays/refresh")
            .on_press(|app: &mut App| {
                actions::scan(app);
            })
    };
    let head = row()
        .items_center()
        .child(filters(replays, p))
        .child(spacer())
        .child(row().mr(-TEXT_ACTION_INSET).child(refresh));
    let shown = replays.shown();
    let body: Vec<El> = if !replays.scanned {
        (0..PLACEHOLDERS)
            .map(|_| {
                col()
                    .py(space::SM)
                    .gap(space::XS)
                    .child(skeleton(0.4, p))
                    .child(skeleton(0.7, p))
            })
            .collect()
    } else if replays.items.is_empty() {
        vec![
            label(
                "No replays yet. Slippi and tgg-melee's recorder save them in your replay folders.",
            )
            .key("replays/empty")
            .font(text::BODY)
            .color(p.muted)
            .wrap(),
        ]
    } else if shown.is_empty() {
        vec![
            label("No replay matches that.")
                .key("replays/none")
                .font(text::BODY)
                .color(p.muted),
        ]
    } else {
        shown
            .into_iter()
            .map(|item| replay_row(app, replays, item, p))
            .collect()
    };
    let list = col().key("replays/list").children(body);
    // Wide, the list and the chosen replay's details sit side by side, each
    // scrolling on its own under the filters, which stay.
    if app.side_by_side() {
        let page = split_page().child(split(
            split_list("replays/scroll", Some(head), list),
            super::details::column(app, p),
        ));
        return col().grow(1.0).min_h(0.0).child(page);
    }
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::SM)
        .child(col().max_w(COLUMN).gap(space::SM).child(head).child(list));
    col()
        .key("replays/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// The top bar's end on Replays: the search.
pub fn top_bar(app: &App, p: &Palette) -> Vec<El> {
    vec![
        search_box(
            search_field(&app.replays.search, "Search players, fighters, stages", p)
                .key("replays/search")
                .on_edit(|app: &mut App| &mut app.replays.search),
            p,
        )
        .w(size::SEARCH),
    ]
}

/// Which folder's replays show.
fn filters(replays: &Replays, p: &Palette) -> El {
    let choice = |name: &str, source: Option<String>| {
        let chosen = replays.source == source;
        let key = format!(
            "replays/source/{}",
            source.as_deref().unwrap_or("all").to_lowercase()
        );
        chip(name, chosen, p)
            .key(key)
            .role("radio")
            .on_press(move |app: &mut App| actions::set_source(app, source.clone()))
    };
    row().gap(space::SNUG).child(choice("All", None)).children(
        replays
            .folders
            .iter()
            .map(|(name, _)| choice(name, Some(name.clone()))),
    )
}

/// A player's fighter as the player's game draws that costume, once its
/// picture has arrived; a quiet tile until then.
pub fn fighter_picture(
    app: &App,
    character: Option<u8>,
    costume: Option<u8>,
    side: f32,
    p: &Palette,
) -> El {
    let slot = character
        .and_then(tgg_app::replays::character)
        .and_then(|character| actions::costume_slot(app, character, costume.unwrap_or(0)));
    icon_tile(
        slot.and_then(|slot| app.renders.get(RenderKey::Slot(slot))),
        side,
        p,
    )
}

/// A replay: its fighters, who played as whom and who won, when, where and
/// how long, what it needs, and Play. Pressing it shows its details.
fn replay_row(app: &App, replays: &Replays, item: &Item, p: &Palette) -> El {
    let key = format!("replays/{}", item.path.display());
    let info = item.info.as_ref();
    let when = tgg_app::replays::played_at(&item.path)
        .or_else(|| {
            info.and_then(|info| info.started.as_ref())
                .map(|started| started.replace('T', " ").chars().take(16).collect())
        })
        .unwrap_or_default();
    let players: Vec<String> = info
        .map(|info| {
            info.players
                .iter()
                .map(|player| {
                    let name = player
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("P{}", player.port + 1));
                    let won = info.winner == Some(player.port);
                    format!("{name}{}", if won { " ★" } else { "" })
                })
                .collect()
        })
        .unwrap_or_default();
    let fighters = info.map(|info| {
        row().gap(space::TIGHT).children(
            info.players
                .iter()
                .take(2)
                .map(|player| fighter_picture(app, player.character, player.costume, FIGHTER, p)),
        )
    });
    let stage = info
        .and_then(|info| info.stage)
        .and_then(tgg_app::replays::stage_name);
    let length = info.and_then(|info| info.frames).map(|frames| {
        let seconds = frames / 60;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    });
    let facts: Vec<String> = stage
        .map(str::to_owned)
        .into_iter()
        .chain(length)
        .chain([item.source.clone()])
        .collect();
    let preparing = replays
        .preparing
        .as_ref()
        .filter(|preparing| preparing.path == item.path);
    let checking = replays.checking.as_ref() == Some(&item.path);
    let end: El = if let Some(preparing) = preparing {
        row()
            .key(format!("{key}/preparing"))
            .gap(space::XS)
            .items_center()
            .child(label("Getting its mods…").font(text::SMALL).color(p.muted))
            .child(
                col()
                    .w(size::INLINE_PROGRESS)
                    .child(progress(preparing.fraction(), p)),
            )
            .child(text_action("Stop", false, p).on_press(actions::cancel_preparing))
    } else if checking {
        label("Checking its mods…")
            .key(format!("{key}/checking"))
            .font(text::SMALL)
            .color(p.muted)
    } else {
        let path = item.path.clone();
        row().children(play_button(app, item, p).map(|button| {
            button
                .key(format!("{key}/play"))
                .on_press(move |app: &mut App| actions::play(app, &path))
        }))
    };
    let chosen = replays.selected.as_ref() == Some(&item.path);
    let pressed = item.path.clone();
    let unreadable = match &item.status {
        Status::Unreadable(why) => Some(why.clone()),
        _ => None,
    };
    list_row(p)
        .key(key)
        .mx(-space::SM)
        .fill(if chosen {
            p.selected
        } else {
            rune::Color::TRANSPARENT
        })
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| actions::select(app, &pressed))
        .children(fighters)
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    label(if players.is_empty() {
                        "A replay".to_owned()
                    } else {
                        players.join("  vs  ")
                    })
                    .font(text::STRONG)
                    .color(p.ink)
                    .truncate(),
                )
                .child(
                    label(if when.is_empty() {
                        facts.join(" · ")
                    } else {
                        format!("{when} · {}", facts.join(" · "))
                    })
                    .font(text::SMALL)
                    .color(p.muted)
                    .truncate(),
                )
                .children(
                    unreadable.map(|why| label(why).font(text::SMALL).color(p.danger).truncate()),
                ),
        )
        .child(status_badge(&item.status, p))
        .child(end)
}

/// Watch, while a replay can be played: not one that can't be read or needs
/// a game that can't be had, and not while a game runs or a replay's mods
/// are on their way. The caller keys it and makes pressing it play.
pub fn play_button(app: &App, item: &Item, p: &Palette) -> Option<El> {
    let playable = !matches!(item.status, Status::Unreadable(_) | Status::NoGame(_));
    let checking = app.replays.checking.as_ref() == Some(&item.path);
    (playable && !checking && app.mods.playing.is_none() && app.replays.preparing.is_none())
        .then(|| button("Watch", ButtonKind::Secondary, p))
}

/// What a replay needs, as a badge.
pub fn status_badge(status: &Status, p: &Palette) -> El {
    match status {
        Status::Unknown => badge("Checking…", Tone::Plain, p),
        Status::AsIs => badge("Plays as is", Tone::Mine, p),
        Status::Needs {
            mods: 0,
            game: Some(_),
            ..
        } => badge("Needs another tgg-melee", Tone::News, p),
        Status::Needs { mods, game, .. } => {
            let mods_words = if *mods == 1 {
                "1 mod".to_owned()
            } else {
                format!("{mods} mods")
            };
            let words = match game {
                Some(_) => format!("Needs {mods_words}, and another tgg-melee"),
                None if *mods == 0 => "Needs its own mods".to_owned(),
                None => format!("Needs {mods_words}"),
            };
            badge(words, Tone::News, p)
        }
        Status::NoGame(_) => badge("Its tgg-melee isn't available", Tone::Danger, p),
        Status::Unreadable(_) => badge("Can't be read", Tone::Danger, p),
    }
}
