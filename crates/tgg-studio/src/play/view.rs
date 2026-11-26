//! Play's page: notices, then the hero, a dark panel with the active set in
//! big type, the three ways to play, and the fighter the player last used
//! standing in a warm spotlight beside them; under it the queues open right
//! now and the player's last matches, each on a card. The light is drawn
//! with gradients, so it's sharp at any size and needs nothing measured.
//! The set picker's menu and the friend sheet sit over the page.

use super::actions;
use super::{Queue, QueueMod, Room, State, gameplay, matching, searching};
use crate::jobs::renders::RenderKey;
use crate::mods::Disc;
use crate::nav::Place;
use crate::sets::{How, Launching, Session};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, Icon, button, button_main, button_off, button_small, callout,
    dialog, icon, list_row, progress, section, select_item, select_menu, select_rule, text_action,
    text_field,
};
use rune::{Color, El, LoopSpec, ScrollSpec, col, el, row, text as label};

/// The fighter's side: in a wide place, and in a narrow one.
const HERO: (f32, f32) = (300.0, 220.0);
/// How far down its square a fighter's feet are, as a fraction of its side
/// (the renderer frames every fighter's idle the same way): where the pool
/// of light is.
const FEET: f32 = 0.8;
/// The fighter's room beside the words, in fighter sides.
const STAGE: f32 = 1.4;
/// The pool of light on the floor, in fighter sides across and down: a flat
/// ellipse, bright at its heart, with two faint rings around it.
const POOL: (f32, f32) = (0.6, 0.1);
const POOL_STOPS: [(f32, f32); 9] = [
    (0.0, 0.42),
    (0.35, 0.16),
    (0.62, 0.0),
    (0.7, 0.0),
    (0.72, 0.22),
    (0.75, 0.0),
    (0.9, 0.0),
    (0.92, 0.12),
    (0.95, 0.0),
];
/// The warm glow behind the fighter: where its heart is, as a fraction of
/// the room down, and its brightness out to the room's edges, where it's
/// gone.
const GLOW_AT: f32 = 0.55;
const GLOW_STOPS: [(f32, f32); 3] = [(0.0, 0.13), (0.5, 0.04), (1.0, 0.0)];
/// The widest the line explaining Find a match runs.
const EXPLAINER: f32 = 440.0;
/// The live dot: its side, and its slow pulse.
const LIVE_DOT: f32 = 7.0;
const LIVE_PULSE: LoopSpec = LoopSpec::opacity(1.8, 0.0, 0.3, 1.0);
/// How faint the main button is while another game is got ready.
const BUSY: f32 = 0.4;
/// The width of the last matches, beside the queues.
const MATCHES: f32 = 384.0;
/// How many of a queue's mods are named before "+ N more".
const NAMED_MODS: usize = 3;
/// A fighter's picture in a match row.
const FIGHTER: f32 = 40.0;

pub fn view(app: &App, p: &Palette) -> El {
    let body = col()
        .w_full()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::LG);
    let body = match needs_setup(app) {
        Some(words) => body.child(quiet(words, p).key("play/setup")),
        None => body
            .children(notices(app, p))
            .child(hero(app, p))
            .child(below(app, p)),
    };
    // The menu and the sheet are drawn by the shell, over everything.
    col().grow(1.0).min_h(0.0).child(
        col()
            .key("play/scroll")
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(body),
    )
}

/// Whether the place is wide enough for the big fighter and the cards side
/// by side. Before the first layout, wide.
fn wide(app: &App) -> bool {
    app.window_width <= 0.0 || app.side_by_side()
}

/// A card on the page: the queues, the last matches.
fn tile(p: &Palette) -> El {
    col()
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CARD)
        .padding(space::LG)
}

/// `alphas` as stops of `color`, for a gradient that fades to nothing in
/// the same color (no gray fringe).
fn stops<const N: usize>(color: Color, alphas: [(f32, f32); N]) -> [(f32, Color); N] {
    alphas.map(|(at, alpha)| (at, color.alpha(alpha)))
}

/// A card's heading: its name, and a line under it saying what it's for.
fn tile_head(name: &str, line: &str, p: &Palette) -> El {
    col()
        .gap(space::TIGHT)
        .mb(space::XS)
        .child(label(name.to_owned()).font(text::TITLE).color(p.ink))
        .child(
            label(line.to_owned())
                .font(text::SMALL)
                .color(p.muted)
                .wrap(),
        )
}

/// The top bar's end on Play: how many are searching, once the queues are
/// read. (The brief asked for "N playing now"; the platform keeps no playing
/// count anywhere, so this is what there is.)
pub fn top_bar(app: &App, p: &Palette) -> Vec<El> {
    let Some(queues) = actions::queues_of(app) else {
        return Vec::new();
    };
    let total: u32 = queues.iter().map(|queue| queue.searching).sum();
    vec![
        label(super::searching_now(total))
            .font(text::SMALL)
            .color(p.muted),
    ]
}

/// What Play needs before it can play, in one quiet line; nothing when it
/// has both.
fn needs_setup(app: &App) -> Option<String> {
    let playing = app.mods.port().is_some() && matches!(app.mods.disc, Disc::Found(_));
    (!playing).then(|| {
        "Play needs tgg-melee and your Melee disc. You can set both up on Mods.".to_owned()
    })
}

fn quiet(words: impl Into<String>, p: &Palette) -> El {
    label(words.into()).font(text::BODY).color(p.muted).wrap()
}

/// A line under what the sheet is doing, smaller and dimmer than `quiet`:
/// what happens next.
fn faint(words: &str, p: &Palette) -> El {
    label(words.to_owned())
        .font(text::SMALL)
        .color(p.faint)
        .wrap()
}

/// What the set says it plays without, above the card: each its own slim
/// callout. They never block playing.
fn notices(app: &App, p: &Palette) -> Vec<El> {
    crate::sets::actions::notices(app, &app.sets.active)
        .into_iter()
        .enumerate()
        .map(|(index, words)| {
            callout(label(words).font(text::BODY).color(p.ink).wrap(), None, p)
                .key(format!("play/notice/{index}"))
        })
        .collect()
}

/// The hero, on the page itself so its words line up with the title: the
/// set, its queue, the three ways to play and what Find a match does,
/// beside the fighter standing in a pool of light.
fn hero(app: &App, p: &Palette) -> El {
    let side = if wide(app) { HERO.0 } else { HERO.1 };
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::LG)
        .children(status(app, p))
        .child(row().w_full().child(picker(app, p)))
        .child(
            col()
                .gap(space::SM)
                .mt(space::XS)
                .child(buttons(app, p))
                .children(match &app.sets.launching {
                    Some(launching) => Some(launching_line(launching, p)),
                    None => explainer(app, p),
                }),
        );
    row()
        .key("play/card")
        .w_full()
        .items_center()
        .gap(space::LG)
        .child(words)
        .child(spotlight(app, side, p))
}

/// The fighter in a pool of light, with a warm glow behind: in 3D, idling,
/// and turned by a drag; its picture when its model couldn't be read.
fn spotlight(app: &App, side: f32, p: &Palette) -> El {
    let (w, h) = (side * STAGE, side);
    let feet = side * FEET;
    let pool = (side * POOL.0, side * POOL.1);
    let fighter = match &app.play.fighter {
        Some(fighter) => rune::effect(crate::stage::FIGHTER)
            .key("play/fighter/model")
            .size(side, side)
            .cursor(if fighter.gesture.is_some() {
                rune::Cursor::Grabbing
            } else {
                rune::Cursor::Grab
            })
            .on_drag(actions::spin),
        None => picture(app, side),
    };
    col()
        .key("play/fighter")
        .w(w)
        .h(h)
        .child(el().absolute(0.0, 0.0).w(w).h(h).radial_gradient(
            [0.5, GLOW_AT],
            [w / 2.0, h / 2.0],
            stops(p.accent, GLOW_STOPS),
        ))
        .child(
            el().absolute(w / 2.0 - pool.0, feet - pool.1)
                .size(pool.0 * 2.0, pool.1 * 2.0)
                .radial_gradient([0.5, 0.5], [pool.0, pool.1], stops(p.accent, POOL_STOPS)),
        )
        .child(fighter.absolute((w - side) / 2.0, 0.0))
}

/// Under the buttons, what Find a match does with this set: which players
/// it pairs the player with, by the gameplay mods they run.
fn explainer(app: &App, p: &Palette) -> Option<El> {
    let plan = app.sets.plan(&app.sets.active)?;
    let mods = gameplay(plan);
    let queues = actions::queues_of(app);
    let queue =
        queues.and_then(|queues| matching(queues, &mods).and_then(|index| queues.get(index)));
    let words = match queue {
        Some(queue) if queue.standing() => format!(
            "Find a match pairs you with anyone in {}, the always-open queue for these mods.",
            queue.label
        ),
        _ if mods.is_empty() => {
            "Find a match pairs you with players who run no gameplay mods.".to_owned()
        }
        _ => {
            let mut words = format!(
                "Find a match pairs you only with players running these exact gameplay mods ({}).",
                mod_names(app, &mods)
            );
            let alone = queue.is_none_or(|queue| queue.searching == 0);
            let standing = queues.is_some_and(|queues| queues.iter().any(Queue::standing));
            if alone && standing {
                words.push_str(" An always-open queue below finds a game sooner.");
            }
            words
        }
    };
    Some(
        label(words)
            .key("play/explainer")
            .font(text::SMALL)
            .color(p.muted)
            .wrap()
            .max_w(EXPLAINER),
    )
}

/// The active set, as the card's heading and the button opening the
/// picker: its name, and under it what it plays (the sets it builds on, then
/// its own mods).
fn picker(app: &App, p: &Palette) -> El {
    let set = app.sets.active();
    let name = set.map_or_else(
        || tgg_mod::sets::INSTALLED_NAME.to_owned(),
        |set| set.name.clone(),
    );
    let summary = set
        .map(|set| crate::sets::summary(app, set))
        .unwrap_or_default();
    col()
        .key("play/set")
        .min_w(0.0)
        .gap(space::XXS)
        .px(space::XS)
        .py(space::XXS)
        .ml(-space::XS)
        .radius(radius::CONTROL)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("combobox")
        .checked(app.play.menu)
        .focusable(true)
        .label(name.clone())
        .on_press(actions::toggle_menu)
        .child(
            row()
                .gap(space::SM)
                .items_center()
                .child(
                    label(name)
                        .font(text::DISPLAY)
                        .color(p.ink)
                        .truncate()
                        .min_w(0.0),
                )
                .child(icon(Icon::Chevron, p.muted)),
        )
        .children((!summary.is_empty()).then(|| {
            label(summary)
                .font(text::BODY)
                .color(p.muted)
                .truncate()
                .min_w(0.0)
        }))
}

/// Find a match, Play with a friend, Play offline, all one height; while a
/// game runs, what it's doing instead of Find a match, with Cancel.
fn buttons(app: &App, p: &Palette) -> El {
    let main = match &app.sets.session {
        Some(session) => live(app, session, p),
        None if app.mods.playing.is_some() => running(p),
        None if app.sets.busy() => button_main("Find a match", ButtonKind::Primary, p)
            .disabled(true)
            .opacity(BUSY),
        None => button_main("Find a match", ButtonKind::Primary, p)
            .key("play/find")
            .on_press(actions::find),
    };
    // In a narrow window they wrap rather than run under the fighter.
    row()
        .flex_wrap()
        .gap(space::XS)
        .items_center()
        .child(main)
        .child(
            button_main("Play with a friend", ButtonKind::Secondary, p)
                .key("play/friend")
                .on_press(actions::open_friend),
        )
        .child(
            button_main("Play offline", ButtonKind::Secondary, p)
                .key("play/offline")
                .on_press(actions::play_offline),
        )
}

/// The main button while a game runs: what it's doing, how long it has been
/// (or who it found), and Cancel, which ends it.
fn live(app: &App, session: &Session, p: &Palette) -> El {
    let words = match &session.how {
        How::Search => format!("Searching {}", searching_in(app, session)),
        How::Queue { label, .. } => format!("Searching {label}"),
        How::Host => "In a room".to_owned(),
        How::Join(_) => "Joining a room".to_owned(),
        How::Offline => "Playing offline".to_owned(),
    };
    let searching = matches!(session.how, How::Search | How::Queue { .. });
    let next = searching.then(|| match &app.play.log.found {
        Some(name) => label(format!("Found {name}. Starting the game"))
            .font(text::SMALL)
            .color(p.on_ink),
        None => label(clock(session)).font(text::MONO_TEXT).color(p.on_ink),
    });
    session_pill(words, next, p)
}

/// The main button while a game runs that Play didn't start (a replay): it
/// can still be ended here.
fn running(p: &Palette) -> El {
    session_pill("A game is running".to_owned(), None, p)
}

/// A main button that says what's running, with Cancel.
fn session_pill(words: String, next: Option<El>, p: &Palette) -> El {
    row()
        .key("play/session")
        .h(size::CONTROL_MAIN)
        .px(space::LG - space::XXS)
        .gap(space::SM)
        .items_center()
        .justify_center()
        .radius(radius::PILL)
        .fill(p.ink)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(format!("{words} · Cancel"))
        .on_press(crate::mods::actions::stop)
        .child(label(words).font(text::STRONG).color(p.on_ink))
        .children(next)
        .child(label("·").font(text::SMALL).color(p.on_ink.alpha(0.35)))
        .child(
            label("Cancel")
                .font(text::STRONG)
                .color(p.on_ink.alpha(0.6)),
        )
}

/// How long the game has been searching, as `0:07`.
fn clock(session: &Session) -> String {
    let seconds = session.started.elapsed().as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The queue the pill names: the queue the card's line matches, else the
/// name the game was started under (it doesn't change when the player
/// switches sets mid-search).
fn searching_in(app: &App, session: &Session) -> String {
    line_queue(app).map_or_else(|| session.name.clone(), |queue| queue.label.clone())
}

/// The queue the card's line and pill name: the one in the list whose mods
/// are the set's gameplay mods.
fn line_queue(app: &App) -> Option<&Queue> {
    let queues = actions::queues_of(app)?;
    let plan = app.sets.plan(&app.sets.active)?;
    matching(queues, &gameplay(plan)).and_then(|index| queues.get(index))
}

/// Where the queue the card covers sits in `queues`, so the list doesn't
/// repeat it.
fn covered_queue(app: &App, queues: &[Queue]) -> Option<usize> {
    let plan = app.sets.plan(&app.sets.active)?;
    matching(queues, &gameplay(plan))
}

/// Over the set's name: which queue it searches in and how many are there,
/// with a dot that breathes while anyone is. Nothing until the queues are
/// read, or while a game is got ready (that shows under the buttons).
fn status(app: &App, p: &Palette) -> Option<El> {
    if app.sets.launching.is_some() {
        return None;
    }
    let queues = actions::queues_of(app)?;
    let plan = app.sets.plan(&app.sets.active)?;
    let queue = matching(queues, &gameplay(plan)).and_then(|index| queues.get(index));
    let words = match queue {
        Some(queue) if queue.standing() => {
            format!("{} queue · {}", queue.label, searching(queue.searching))
        }
        Some(queue) => format!("Its own queue · {}", searching(queue.searching)),
        None => "Its own queue · nobody searching yet".to_owned(),
    };
    let live = queue.is_some_and(|queue| queue.searching > 0);
    let dot = el()
        .size(LIVE_DOT, LIVE_DOT)
        .radius(LIVE_DOT / 2.0)
        .fill(if live { p.success } else { p.faint });
    Some(
        row()
            .key("play/status")
            .gap(space::XS)
            .items_center()
            .child(if live { dot.looping(LIVE_PULSE) } else { dot })
            .child(label(words).font(text::LABEL).color(p.muted)),
    )
}

fn launching_line(launching: &Launching, p: &Palette) -> El {
    col()
        .gap(space::SNUG)
        .child(
            row()
                .gap(space::SM)
                .items_center()
                .child(
                    label(launching.doing.clone())
                        .font(text::SMALL)
                        .color(p.muted),
                )
                .child(
                    text_action("Cancel", false, p)
                        .key("play/launching/cancel")
                        .on_press(crate::sets::actions::cancel),
                ),
        )
        .child(
            col()
                .w(size::INLINE_PROGRESS)
                .child(progress(fraction(launching), p)),
        )
}

/// How far along getting the game ready is, from 0 to 1.
fn fraction(launching: &Launching) -> f32 {
    let Some(progress) = launching
        .progress
        .lock()
        .ok()
        .and_then(|progress| progress.clone())
    else {
        return 0.0;
    };
    let within = progress
        .total
        .filter(|total| *total > 0)
        .map_or(0.0, |total| progress.bytes as f32 / total as f32);
    ((progress.step as f32 + within.min(1.0)) / progress.steps.max(1) as f32).clamp(0.0, 1.0)
}

/// The fighter the player had in their latest match, drawn with nothing
/// behind it; Fox before there's a match.
fn picture(app: &App, side: f32) -> El {
    let slot = app.play.hero.or_else(|| actions::fox_slot(app));
    let tile = el().size(side, side);
    match slot.and_then(|slot| app.renders.get(RenderKey::Fighter(slot))) {
        Some(pixels) => tile.pixels_fit(pixels, rune::SurfaceFit::Fill),
        None => tile,
    }
}

/// Under the hero, each on a card: the queues open right now (the ones
/// textures.gg keeps open, then the players' own, each with what it needs
/// and how many are there; the one the hero covers isn't listed again) and
/// the last matches. Side by side when the place is wide, else one over the
/// other.
fn below(app: &App, p: &Palette) -> El {
    let queues = tile(p)
        .key("play/queues")
        .min_w(0.0)
        .gap(space::XS)
        .child(tile_head(
            "Other queues",
            "Join one to play its mods for that match. Your set stays as it is.",
            p,
        ))
        .children(queues(app, p));
    let matches = matches(app, p);
    if wide(app) {
        row()
            .w_full()
            .gap(space::LG)
            .items_start()
            .child(queues.grow(1.0))
            .children(matches.map(|matches| matches.w(MATCHES)))
    } else {
        col()
            .w_full()
            .gap(space::LG)
            .child(queues)
            .children(matches)
    }
}

fn queues(app: &App, p: &Palette) -> Vec<El> {
    let queues = match &app.play.queues {
        None => return vec![faint("Asking textures.gg what's open…", p)],
        Some(Err(_)) => {
            return vec![quiet("Couldn't read what's open right now.", p)];
        }
        Some(Ok(queues)) => queues,
    };
    let covered = covered_queue(app, queues);
    let mut items = Vec::new();
    let standing: Vec<&Queue> = queues
        .iter()
        .enumerate()
        .filter(|(index, queue)| Some(*index) != covered && queue.standing())
        .map(|(_, queue)| queue)
        .collect();
    if !standing.is_empty() {
        items.push(section("Always open", p));
        items.extend(standing.into_iter().map(|queue| queue_row(app, queue, p)));
    }
    let open: Vec<&Queue> = queues
        .iter()
        .enumerate()
        .filter(|(index, queue)| Some(*index) != covered && !queue.standing())
        .map(|(_, queue)| queue)
        .collect();
    if !open.is_empty() {
        items.push(section("Open right now", p));
        items.extend(open.into_iter().map(|queue| queue_row(app, queue, p)));
    }
    if items.is_empty() {
        items.push(quiet("Nothing else is open right now.", p));
    }
    items
}

fn queue_row(app: &App, queue: &Queue, p: &Palette) -> El {
    let mods = mod_names(app, &queue.mods);
    let key = format!("play/queue/{}", queue_key(&queue.label));
    let mods_here = queue.mods.clone();
    let label_here = queue.label.clone();
    list_row(p)
        .key(key.clone())
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(
                    label(queue.label.clone())
                        .font(text::STRONG)
                        .color(p.ink)
                        .truncate(),
                )
                .children(
                    (!mods.is_empty())
                        .then(|| label(mods).font(text::SMALL).color(p.muted).truncate()),
                ),
        )
        .child(
            label(searching(queue.searching))
                .font(text::MONO_SMALL)
                .color(p.faint),
        )
        .child(
            button_small("Join", ButtonKind::Secondary, p)
                .key(format!("{key}/join"))
                .on_press(move |app: &mut App| {
                    crate::sets::actions::launch(
                        app,
                        How::Queue {
                            label: label_here.clone(),
                            mods: mods_here.clone(),
                        },
                    );
                }),
        )
}

/// A queue's name as a key: its letters and digits, the rest as dashes.
fn queue_key(label: &str) -> String {
    let mut key = String::new();
    for letter in label.trim().chars() {
        if letter.is_ascii_alphanumeric() {
            key.push(letter.to_ascii_lowercase());
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    let key = key.trim_end_matches('-').to_owned();
    if key.is_empty() {
        "queue".to_owned()
    } else {
        key
    }
}

/// The mods a queue needs, named: the first few, then how many more.
fn mod_names(app: &App, mods: &[QueueMod]) -> String {
    let names: Vec<String> = mods
        .iter()
        .take(NAMED_MODS)
        .map(|mod_| crate::sets::actions::mod_name(app, &mod_.id))
        .collect();
    let rest = mods.len().saturating_sub(names.len());
    match rest {
        0 => names.join(", "),
        n => format!("{} + {n} more", names.join(", ")),
    }
}

/// The player's last three matches, from their replays: both fighters,
/// won or lost, and the stage. Pressing one watches it.
fn matches(app: &App, p: &Palette) -> Option<El> {
    let items: Vec<&crate::replays::Item> = app
        .replays
        .items
        .iter()
        .filter(|item| {
            item.info
                .as_ref()
                .is_some_and(|info| info.players.len() == 2)
        })
        .take(3)
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(
        tile(p)
            .key("play/matches")
            .gap(space::XS)
            .child(tile_head("Your last matches", "Press one to watch it.", p))
            .children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| match_row(app, item, index, p)),
            ),
    )
}

fn match_row(app: &App, item: &crate::replays::Item, index: usize, p: &Palette) -> El {
    let info = item.info.as_ref().expect("a match's two players");
    let fighters = row()
        .gap(space::TIGHT)
        .children(info.players.iter().take(2).map(|player| {
            crate::replays::view::fighter_picture(app, player.character, player.costume, FIGHTER, p)
        }));
    let detail = detail_of(app, info);
    let path = item.path.clone();
    // Green for a win, red for a loss, faint when the replay doesn't say
    // which player was this one.
    let won = actions::player_of(app, info).map(|me| info.winner == Some(me.port));
    let outcome_dot = el()
        .size(LIVE_DOT, LIVE_DOT)
        .radius(LIVE_DOT / 2.0)
        .fill(match won {
            Some(true) => p.success,
            Some(false) => p.danger,
            None => p.faint,
        });
    list_row(p)
        .key(format!("play/match/{index}"))
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| crate::replays::actions::play(app, &path))
        .child(fighters)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(
                    row()
                        .gap(space::XS)
                        .items_center()
                        .child(outcome_dot)
                        .child(
                            label(result(app, info))
                                .font(text::STRONG)
                                .color(p.ink)
                                .truncate()
                                .min_w(0.0),
                        ),
                )
                .children(
                    (!detail.is_empty())
                        .then(|| label(detail).font(text::SMALL).color(p.muted).truncate()),
                ),
        )
}

/// Won or lost, against whom, when the replay says who the player is; else
/// who won.
fn result(app: &App, info: &tgg_mod::ReplayInfo) -> String {
    if let Some(me) = actions::player_of(app, info) {
        let who = info
            .players
            .iter()
            .find(|player| player.port != me.port)
            .map_or_else(|| "them".to_owned(), name_of);
        return if info.winner == Some(me.port) {
            format!("Won vs {who}")
        } else {
            format!("Lost to {who}")
        };
    }
    match info
        .winner
        .and_then(|port| info.players.iter().find(|player| player.port == port))
    {
        Some(winner) => format!("{} won", name_of(winner)),
        None => "A match".to_owned(),
    }
}

/// A replay player's name, else their port.
fn name_of(player: &tgg_mod::ReplayPlayer) -> String {
    player
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("P{}", player.port + 1))
}

/// A match's line under its result: the other fighter, and the stage.
fn detail_of(app: &App, info: &tgg_mod::ReplayInfo) -> String {
    let me = actions::player_of(app, info);
    let other = info
        .players
        .iter()
        .find(|player| me.is_none_or(|me| player.port != me.port));
    let fighter = other
        .and_then(|player| player.character)
        .and_then(tgg_app::replays::character)
        .map(|character| character.name().to_owned());
    let stage = info
        .stage
        .and_then(tgg_app::replays::stage_name)
        .map(str::to_owned);
    [fighter, stage]
        .into_iter()
        .flatten()
        .collect::<Vec<String>>()
        .join(" · ")
}

/// The set picker's menu: the player's sets, choosing one starts it; then
/// where to change them.
pub fn menu(app: &App, p: &Palette) -> Option<El> {
    if !app.play.menu || app.nav.place != Place::Play {
        return None;
    }
    let mut items: Vec<El> = app
        .sets
        .list
        .iter()
        .filter(|set| !set.is_installed() || !set.lines.is_empty())
        .map(|set| {
            let key = set.key.clone();
            select_item(&set.name, app.sets.active == set.key, p)
                .key(format!("play/set/{}", set.key))
                .on_press(move |app: &mut App| actions::choose(app, &key))
        })
        .collect();
    items.push(select_rule(p));
    items.push(
        select_item("Change sets on Mods", false, p)
            .key("play/set/mods")
            .on_press_tx(|app: &mut App, tx| crate::editor::actions::go(app, Place::Mods, tx)),
    );
    Some(select_menu("play/set", items, actions::close_menu, p))
}

/// The friend sheet: the room as the game reports it, and Join by code.
/// Hosting shows the invite link above the code, who's in and how full it
/// is; the sign-in, getting-mods, playing and ended states come from the
/// same report.
pub fn friend(app: &App, p: &Palette) -> Option<El> {
    if !app.play.friend || app.nav.place != Place::Play {
        return None;
    }
    let busy = app.sets.busy() || app.mods.playing.is_some();
    let room: El = match &app.sets.session {
        Some(session) => room_state(&app.play.room, &session.how, p).key("play/friend/room"),
        _ if busy => button_off("Host a room", p),
        _ => col()
            .gap(space::XS)
            .child(
                row().child(
                    button("Host a room", ButtonKind::Secondary, p)
                        .key("play/friend/host")
                        .on_press(actions::host),
                ),
            )
            .child(faint("You get a code and a link to share.", p)),
    };
    let empty = app.play.join.text.trim().is_empty();
    let join = row()
        .gap(space::SM)
        .items_center()
        .child(
            col().grow(1.0).min_w(0.0).child(
                text_field(&app.play.join, "Room code", p)
                    .key("play/friend/code")
                    .on_edit(|app: &mut App| &mut app.play.join),
            ),
        )
        .child(if empty || busy {
            button_off("Join", p)
        } else {
            button("Join", ButtonKind::Primary, p)
                .key("play/friend/join")
                .on_press(actions::join)
        });
    let body = col()
        .px(space::LG)
        .pb(space::SM)
        .gap(space::MD)
        .child(room)
        .child(section("Join a room", p))
        .child(join);
    Some(dialog(
        "play/friend/sheet",
        "dialog",
        DialogWidth::Narrow,
        Dialog {
            title: "Play with a friend".to_owned(),
            line: "Rooms take 2 to 4 players.".to_owned(),
            body: Some(body),
            buttons: vec![
                button("Done", ButtonKind::Secondary, p)
                    .key("play/friend/done")
                    .on_press(actions::close_friend),
            ],
        },
        actions::close_friend,
        p,
    ))
}

/// The room, as the report's state has it: signing in, waiting with the
/// link and code out, getting the host's mods, playing, or ended and why.
fn room_state(report: &Room, how: &How, p: &Palette) -> El {
    let joining = matches!(how, How::Join(_));
    let mut body = col().gap(space::SM);
    match report.state {
        State::None => {
            let words = if joining {
                "Joining the room"
            } else {
                "Making the room"
            };
            body = body
                .child(quiet(words, p))
                .child(faint("This takes a few seconds.", p));
        }
        State::Starting => {
            body = body
                .child(quiet("Signing you in", p))
                .child(faint("This takes a few seconds.", p));
        }
        State::SigningIn => body = body.children(sign_in(report, p)),
        State::Waiting => {
            body = body
                .children(shared(report, p))
                .children(players(report, p))
                .child(quiet("Your friends join with the code or the link.", p));
        }
        State::GettingMods => {
            let whose = match report.host.as_str() {
                "" => "the host's mods".to_owned(),
                host => format!("{host}'s mods"),
            };
            body = body
                .child(quiet(format!("Getting {whose}"), p))
                .child(faint("The game restarts with them and joins again.", p));
        }
        State::Playing => {
            body = body.children(players(report, p)).child(quiet(
                "The host started the game, so nobody else can join.",
                p,
            ));
        }
        State::Ended => {
            body = body.child(quiet(
                match report.why.as_str() {
                    "" => "The room ended.",
                    why => why,
                },
                p,
            ));
        }
    }
    body
}

/// What's shareable: the invite link with Copy, and the code large with its
/// own Copy under it.
fn shared(report: &Room, p: &Palette) -> Vec<El> {
    let mut parts = Vec::new();
    if let Some(invite) = report.invite() {
        parts.push(
            col()
                .gap(space::XXS)
                .child(section("Invite link", p))
                .child(
                    row()
                        .gap(space::SM)
                        .items_center()
                        .child(
                            label(invite)
                                .key("play/friend/link")
                                .font(text::MONO_TEXT)
                                .color(p.muted)
                                .truncate()
                                .grow(1.0)
                                .min_w(0.0),
                        )
                        .child(
                            button_small("Copy", ButtonKind::Secondary, p)
                                .key("play/friend/copy-link")
                                .on_press_tx(actions::copy_link),
                        ),
                ),
        );
    }
    if !report.code.is_empty() {
        parts.push(code_block(&report.code, p));
    }
    parts
}

/// Who's in the room, the host first, and how full it is.
fn players(report: &Room, p: &Palette) -> Option<El> {
    (!report.players.is_empty()).then(|| {
        row()
            .key("play/friend/players")
            .gap(space::SM)
            .items_center()
            .child(
                label(report.players.join(", "))
                    .font(text::BODY)
                    .color(p.ink)
                    .wrap()
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(
                label(report.count())
                    .font(text::SMALL)
                    .color(p.muted)
                    .truncate(),
            )
    })
}

/// Nobody's signed in: the code to confirm, and the page to confirm it on.
fn sign_in(report: &Room, p: &Palette) -> Vec<El> {
    let mut parts = vec![quiet("Confirm the code on textures.gg to sign in.", p)];
    if !report.signin.code.is_empty() {
        parts.push(
            row()
                .gap(space::SM)
                .items_center()
                .px(space::SM)
                .py(space::XS)
                .fill(p.surface)
                .border(size::LINE, p.line)
                .radius(radius::CONTROL + 1.0)
                .child(
                    label(report.signin.code.clone())
                        .key("play/friend/signin/code")
                        .font(text::MONO_CODE)
                        .color(p.ink)
                        .truncate()
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    button_small("Copy", ButtonKind::Secondary, p)
                        .key("play/friend/signin/copy")
                        .on_press_tx(actions::copy_sign_in_code),
                ),
        );
    }
    if !report.signin.url.is_empty() {
        parts.push(
            button("Open textures.gg", ButtonKind::Primary, p)
                .key("play/friend/signin/open")
                .on_press_tx(actions::open_sign_in),
        );
        parts.push(
            label(report.signin.url.clone())
                .key("play/friend/signin/url")
                .font(text::MONO_SMALL)
                .color(p.muted)
                .truncate(),
        );
    }
    parts
}

/// A room's code, large, with Copy.
fn code_block(code: &str, p: &Palette) -> El {
    row()
        .gap(space::SM)
        .items_center()
        .px(space::SM)
        .py(space::XS)
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL + 1.0)
        .child(
            label(code.to_owned())
                .key("play/friend/code-shown")
                .font(text::MONO_CODE)
                .color(p.ink)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            button("Copy", ButtonKind::Secondary, p)
                .key("play/friend/copy")
                .on_press_tx(actions::copy_code),
        )
}
