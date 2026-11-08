//! The Replays page: the player's replays, newest first, a row each: when,
//! who played as whom, where, who won, where it came from, and whether it
//! plays as is or what it needs; Play.

use super::{Item, Replays, Status, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Tone, badge, button, page_head, progress, rule, skeleton, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};

/// The list reads best no wider than this.
const COLUMN: f32 = 860.0;
/// A replay's preparing progress, in its row.
const INLINE_PROGRESS: f32 = 80.0;
/// Placeholder rows while the replays are first read.
const PLACEHOLDERS: usize = 6;

pub fn view(app: &App, p: &Palette) -> El {
    let replays = &app.replays;
    let folders: Vec<String> = replays
        .folders
        .iter()
        .map(|(name, path)| format!("{name}: {}", path.display()))
        .collect();
    let refresh = if replays.scanning || replays.refreshing {
        text_action("Reading…", false, p).key("replays/reading")
    } else {
        text_action("Read again", false, p)
            .key("replays/refresh")
            .on_press(|app: &mut App| {
                actions::scan(app);
                actions::refresh(app);
            })
    };
    let head = row()
        .items_end()
        .gap(space::LG)
        .child(col().grow(1.0).min_w(0.0).child(page_head(
            "Replays",
            "Your Slippi and tgg-melee replays, each played with the mods its game had.",
            p,
        )))
        .child(
            col()
                .items_end()
                .gap(space::TIGHT)
                .children(
                    folders
                        .into_iter()
                        .map(|folder| label(folder).font(text::MONO_SMALL).color(p.faint)),
                )
                .child(refresh),
        );
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
                "No replays yet. Slippi and tgg-melee's recorder save them in the folders above.",
            )
            .key("replays/empty")
            .font(text::BODY)
            .color(p.muted),
        ]
    } else {
        replays
            .items
            .iter()
            .map(|item| replay_row(app, replays, item, p))
            .collect()
    };
    let list = col()
        .key("replays/list")
        .max_w(COLUMN)
        .child(rule(p))
        .children(body.into_iter().flat_map(|row| [row, rule(p)]));
    let page = col()
        .px(space::LG + 4.0)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::LG)
        .child(head)
        .child(list);
    col()
        .key("replays/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// A replay: when and where it was played, who played as whom and who won,
/// what it needs, and Play.
fn replay_row(app: &App, replays: &Replays, item: &Item, p: &Palette) -> El {
    let key = format!("replays/{}", item.path.display());
    let info = item.info.as_ref();
    let when = tgg_app::replays::played_at(&item.path)
        .or_else(|| {
            info.and_then(|info| info.started.as_ref())
                .map(|started| started.replace('T', " ").chars().take(16).collect())
        })
        .unwrap_or_else(|| {
            item.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let players: Vec<String> = info
        .map(|info| {
            info.players
                .iter()
                .map(|player| {
                    let fighter = player
                        .character
                        .and_then(tgg_app::replays::character)
                        .map_or("?", |fighter| fighter.name());
                    let name = player
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("P{}", player.port + 1));
                    let won = info.winner == Some(player.port);
                    format!("{name} ({fighter}){}", if won { " ★" } else { "" })
                })
                .collect()
        })
        .unwrap_or_default();
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
    let end: El = if let Some(preparing) = preparing {
        row()
            .key(format!("{key}/preparing"))
            .gap(space::XS)
            .items_center()
            .child(label("Getting its mods…").font(text::SMALL).color(p.muted))
            .child(
                col()
                    .w(INLINE_PROGRESS)
                    .child(progress(preparing.fraction(), p)),
            )
            .child(text_action("Stop", false, p).on_press(actions::cancel_preparing))
    } else {
        let path = item.path.clone();
        let playable = !matches!(item.status, Status::Unreadable(_) | Status::NoGame(_));
        row().children(
            (playable && app.mods.playing.is_none() && replays.preparing.is_none()).then(|| {
                button("Play", ButtonKind::Secondary, p)
                    .key(format!("{key}/play"))
                    .on_press(move |app: &mut App| actions::play(app, &path))
            }),
        )
    };
    row()
        .key(key)
        .py(space::XS)
        .gap(space::SM)
        .items_center()
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    row()
                        .gap(space::XS)
                        .items_end()
                        .child(
                            label(players.join("  vs  "))
                                .font(text::STRONG)
                                .color(p.ink)
                                .truncate(),
                        )
                        .children(
                            players
                                .is_empty()
                                .then(|| label("A replay").font(text::STRONG).color(p.muted)),
                        ),
                )
                .child(
                    label(format!("{when} · {}", facts.join(" · ")))
                        .font(text::SMALL)
                        .color(p.muted)
                        .truncate(),
                ),
        )
        .child(status_badge(&item.status, p))
        .child(end)
}

/// What a replay needs, as a badge.
fn status_badge(status: &Status, p: &Palette) -> El {
    match status {
        Status::Unknown => badge("Checking…", Tone::Plain, p),
        Status::AsIs => badge("Plays as is", Tone::Mine, p),
        Status::Needs {
            mods: 0,
            game: Some(version),
            ..
        } => badge(format!("Needs tgg-melee {version}"), Tone::News, p),
        Status::Needs { mods, game, .. } => {
            let mods_words = if *mods == 1 {
                "1 mod".to_owned()
            } else {
                format!("{mods} mods")
            };
            let words = match game {
                Some(version) => format!("Needs {mods_words}, tgg-melee {version}"),
                None if *mods == 0 => "Needs its own setup".to_owned(),
                None => format!("Needs {mods_words}"),
            };
            badge(words, Tone::News, p)
        }
        Status::NoGame(version) => badge(
            format!("tgg-melee {version} isn't available"),
            Tone::Danger,
            p,
        ),
        Status::Unreadable(_) => badge("Can't be read", Tone::Danger, p),
    }
}
