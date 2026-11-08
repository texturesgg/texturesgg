//! A replay's details, in a pane at the window's right, the whole window
//! tall as the Mods details are: the stage as the player's game draws it,
//! each player's fighter, name, code and stocks left, who won, their stats
//! side by side (read from the replay's frames once it's opened), what it
//! needs, and Play.

use super::{Item, actions, view};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, motion, size, space, text};
use crate::ui::{self, Tone, badge, rule, spacer, text_action};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_app::replay_stats::PlayerStats;

/// A player's fighter in the pane.
const FIGHTER: f32 = 72.0;

/// Whether the pane shows: on Replays, while a replay is chosen.
pub fn shown(app: &App) -> bool {
    app.nav.place == crate::nav::Place::Replays
        && app.replays.selected.is_some()
        && !app.side_by_side()
}

/// The pane: its head (moving the window, holding the window's controls
/// while it shows) over the replay's details; narrowing to nothing on the
/// default spring when no replay is chosen, its content pinned to the
/// window's edge so it slides.
pub fn view(app: &App, p: &Palette) -> El {
    let shown = shown(app);
    let head = row()
        .h(size::TITLE_ROW)
        .pl(space::MD)
        .pr(space::XS)
        .gap(space::XS)
        .items_center()
        .window_drag()
        .child(label("Replay").font(text::LABEL).color(p.faint))
        .child(spacer())
        .child(
            text_action("Hide", false, p)
                .key("replays/details/close")
                .on_press(actions::close_details),
        )
        .children(shown.then(|| crate::shell::window_controls(p)).flatten());
    row()
        .key("replays/details")
        .w(if shown {
            size::PANE_COLUMN + size::LINE
        } else {
            0.0
        })
        .h_full()
        .justify_end()
        .motion(motion::DEFAULT)
        .clip_rounded(0.0)
        .fill(p.sunk)
        .child(el().w(size::LINE).h_full().fill(p.line))
        .child(
            col().w(size::PANE_COLUMN).h_full().child(head).child(
                col()
                    .grow(1.0)
                    .min_h(0.0)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .children(body(app, p)),
            ),
        )
}

/// What the pane says about the chosen replay.
fn body(app: &App, p: &Palette) -> Option<El> {
    let path = app.replays.selected.as_ref()?;
    let item = app.replays.items.iter().find(|item| item.path == *path)?;
    let info = item.info.as_ref();
    let stage = info.and_then(|info| info.stage);
    let stage_slot = stage
        .and_then(tgg_app::replays::stage)
        .map(melee_dat::MeleeSlot::Stage);
    // Wide, as a stage is: its picture fills it, trimmed top and bottom.
    let mut plate = ui::plate(p).w_full().h(PLATE_HEIGHT);
    if let Some(pixels) = stage_slot.and_then(|slot| app.renders.get(RenderKey::Slot(slot))) {
        plate = plate.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let title = stage
        .and_then(tgg_app::replays::stage_name)
        .unwrap_or("A replay");
    let when = tgg_app::replays::played_at(&item.path).unwrap_or_default();
    let length = info
        .and_then(|info| info.frames)
        .map(|frames| format!("{}:{:02}", frames / 60 / 60, frames / 60 % 60));
    let line: Vec<String> = [Some(when), length, Some(item.source.clone())]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect();
    let stats = app.replays.stats.get(path);
    let play = view::play_button(app, item, p).map(|button| {
        let path = item.path.clone();
        button
            .key("replays/details/play")
            .on_press(move |app: &mut App| actions::play(app, &path))
    });
    Some(
        col()
            .key("replays/details/body")
            .px(space::MD)
            .pt(space::XS)
            .pb(space::MD)
            .gap(space::MD)
            .child(plate)
            .child(
                col()
                    .gap(space::TIGHT)
                    .child(label(title).font(text::H3).color(p.ink))
                    .child(
                        label(line.join(" · "))
                            .font(text::SMALL)
                            .color(p.muted)
                            .wrap(),
                    ),
            )
            .child(
                row()
                    .gap(space::XS)
                    .items_center()
                    .child(view::status_badge(&item.status, p))
                    .child(spacer())
                    .children(play),
            )
            .child(rule(p))
            .children(players(app, item, p))
            .child(rule(p))
            .child(stats_table(item, stats, p)),
    )
}

/// The stage's picture, at its tallest.
const PLATE_HEIGHT: f32 = 160.0;

/// The two players side by side: fighter, name, code, stocks left, winner.
fn players(app: &App, item: &Item, p: &Palette) -> Option<El> {
    let info = item.info.as_ref()?;
    let stats = app
        .replays
        .stats
        .get(&item.path)
        .and_then(|stats| stats.as_ref())
        .and_then(|stats| stats.as_ref().ok());
    let columns = info.players.iter().take(2).map(|player| {
        let name = player
            .name
            .clone()
            .unwrap_or_else(|| format!("Player {}", player.port + 1));
        let fighter = player
            .character
            .and_then(tgg_app::replays::character)
            .map(|fighter| fighter.name())
            .unwrap_or("?");
        let stocks = stats
            .and_then(|stats| stats.iter().find(|stat| stat.port == player.port))
            .map(|stat| {
                if stat.stocks_left == 1 {
                    "1 stock left".to_owned()
                } else {
                    format!("{} stocks left", stat.stocks_left)
                }
            });
        col()
            .key(format!("replays/details/player/{}", player.port))
            .grow(1.0)
            .min_w(0.0)
            .gap(space::TIGHT)
            .child(view::fighter_picture(
                app,
                player.character,
                player.costume,
                FIGHTER,
                p,
            ))
            .child(
                row()
                    .gap(space::XS)
                    .items_center()
                    .pt(space::XXS)
                    .child(label(name).font(text::STRONG).color(p.ink).truncate())
                    .children(
                        (info.winner == Some(player.port)).then(|| badge("Won", Tone::Mine, p)),
                    ),
            )
            .child(label(fighter).font(text::SMALL).color(p.muted))
            .children(
                player
                    .code
                    .clone()
                    .map(|code| label(code).font(text::MONO_SMALL).color(p.faint)),
            )
            .children(stocks.map(|stocks| label(stocks).font(text::SMALL).color(p.muted)))
    });
    Some(row().gap(space::MD).children(columns))
}

/// The players' stats in rows, a column each; reading, or why not.
fn stats_table(
    item: &Item,
    stats: Option<&Option<Result<Vec<PlayerStats>, String>>>,
    p: &Palette,
) -> El {
    let note = |words: &str| {
        label(words.to_owned())
            .key("replays/details/stats")
            .font(text::SMALL)
            .color(p.muted)
            .wrap()
    };
    let stats = match stats {
        None | Some(None) => return note("Reading the game…"),
        Some(Some(Err(why))) => return note(&format!("Its stats couldn't be read: {why}")),
        Some(Some(Ok(stats))) if stats.len() != 2 => {
            return note("Stats are for games of two players.");
        }
        Some(Some(Ok(stats))) => stats,
    };
    let _ = item;
    let percent = |rate: Option<f32>| {
        rate.map_or_else(|| "–".to_owned(), |rate| format!("{:.0}%", rate * 100.0))
    };
    let rows: [(&str, Stat); 5] = [
        ("Kills", Box::new(|stat| stat.kills.to_string())),
        (
            "Damage dealt",
            Box::new(|stat| format!("{:.0}%", stat.damage_dealt)),
        ),
        (
            "Openings per kill",
            Box::new(|stat| {
                stat.openings_per_kill()
                    .map_or_else(|| "–".to_owned(), |rate| format!("{rate:.1}"))
            }),
        ),
        (
            "L-cancels",
            Box::new(move |stat| percent(stat.l_cancel_rate())),
        ),
        (
            "Actions per minute",
            Box::new(|stat| format!("{:.0}", stat.actions_per_minute)),
        ),
    ];
    col()
        .key("replays/details/stats")
        .gap(space::XS)
        .children(rows.iter().map(|(name, value)| {
            row()
                .gap(space::SM)
                .items_center()
                .child(
                    label((*name).to_owned())
                        .font(text::SMALL)
                        .color(p.muted)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .children(stats.iter().map(|stat| {
                    label(value(stat))
                        .font(text::MONO_TEXT)
                        .color(p.ink)
                        .w(STAT)
                }))
        }))
}

/// A stat's column.
const STAT: f32 = 56.0;

/// How one stat reads for a player.
type Stat = Box<dyn Fn(&PlayerStats) -> String>;

/// The details as a column of the page, beside the list (side by side):
/// the chosen replay's, else a line saying to choose one. It scrolls on
/// its own.
pub fn column(app: &App, p: &Palette) -> El {
    let body = body(app, p).unwrap_or_else(|| {
        col()
            .key("replays/details/empty")
            .px(space::MD)
            .py(space::LG)
            .child(
                label("Choose a replay to see who played, where, and how it went.")
                    .font(text::BODY)
                    .color(p.muted)
                    .wrap(),
            )
    });
    col()
        .key("replays/details/column")
        .w(size::DETAILS_COLUMN)
        .h_full()
        .min_h(0.0)
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(crate::ui::tokens::radius::CONTROL)
        .clip_rounded(crate::ui::tokens::radius::CONTROL)
        .child(
            col()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}
