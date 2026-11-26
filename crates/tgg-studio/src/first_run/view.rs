//! First run's screen: one column, four numbered steps, and the one button.
//! No sidebar and nothing else: until the player has a disc and tgg-melee,
//! this is the app.

use super::{FirstRun, LINKS, Picks, Stage, actions};
use crate::mods::{Disc, Registry, Work, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Icon, button, button_main, icon, page_head, progress, spacer, text_action,
    title_bar,
};
use rune::{El, ScrollSpec, col, el, row, text as label};

/// How wide the steps read.
const COLUMN: f32 = 700.0;
/// A step's number, or its check.
const MARK: f32 = 22.0;
/// How far down the column starts, clear of the window's traffic lights.
const TOP: f32 = space::XXL + space::XL;
/// What dims the steps that need a disc.
const DIMMED: f32 = 0.4;
/// A choice's shortest height, so the three line up.
const PICK_MIN_H: f32 = 92.0;

pub fn view(app: &App, p: &Palette) -> El {
    let first_run = app.first_run.as_ref();
    let stage = first_run.map_or(Stage::Done, |first| first.stage);
    let done = stage == Stage::Done;
    let has_disc = disc_found(app);
    let picked = first_run.map_or(0, |first| first.picks.count());

    let line = if done {
        "Everything is installed."
    } else if !has_disc {
        "Start with your Melee disc."
    } else {
        "Nothing downloads until you press the button."
    };
    let mut body = col()
        .w(COLUMN)
        .pt(TOP)
        .pb(space::XL)
        .gap(space::LG)
        .child(page_head(
            if done {
                "Ready to play"
            } else {
                "Set up Melee"
            },
            line,
            p,
        ));
    if let Some(problem) = first_run.and_then(|first| first.problem.as_ref()) {
        body = body.child(
            label(problem.clone())
                .key("first-run/problem")
                .font(text::BODY)
                .color(p.danger)
                .wrap(),
        );
    }
    body = body
        .child(
            col()
                .w_full()
                .gap(space::TIGHT)
                .child(disc_row(app, p))
                .child(game_row(app, stage, has_disc, p))
                .child(mods_row(app, first_run, stage, has_disc, p))
                .child(sign_in_row(app, has_disc, p)),
        )
        .child(
            row()
                .pl(MARK + space::MD)
                .child(main_button(app, stage, has_disc, picked, p)),
        );
    col()
        .key("first-run")
        .w_full()
        .h_full()
        .fill(p.bg)
        .child(title_band())
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .items_center()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}

/// Your Melee disc: found, or why not and a way to choose it.
fn disc_row(app: &App, p: &Palette) -> El {
    let found = disc_found(app);
    let body = col()
        .gap(space::SNUG)
        .child(step_title("Your Melee disc", p))
        .child(
            label(if found {
                "Found on this computer."
            } else {
                "We didn't find one on this computer. It's a file that ends in .iso, about 1.4 GB. Slippi uses the same one."
            })
            .font(text::BODY)
            .color(p.muted)
            .wrap(),
        );
    let control = if found {
        Some(
            text_action("Choose another", false, p)
                .key("first-run/choose-disc")
                .on_press_tx(actions::choose_disc),
        )
    } else {
        Some(
            button("Choose your disc", ButtonKind::Primary, p)
                .key("first-run/choose-disc")
                .on_press_tx(actions::choose_disc),
        )
    };
    step_row(1, found, body, control, false, p)
}

/// The game: what tgg-melee is and its size, or the download while it runs.
fn game_row(app: &App, stage: Stage, has_disc: bool, p: &Palette) -> El {
    let installed = app.mods.port().is_some() || stage == Stage::Done;
    let mut control = None;
    let mut body = col()
        .w_full()
        .gap(space::SNUG)
        .child(step_title("The game", p));
    if stage == Stage::Downloading {
        if let Some(work) = &app.mods.work {
            body = body
                .child(label(downloading(work)).font(text::BODY).color(p.muted))
                .child(progress(work.fraction(), p).key("first-run/progress"));
        } else {
            body = body.child(label("Downloading.").font(text::BODY).color(p.muted));
        }
    } else {
        let release = app
            .mods
            .releases
            .first()
            .or_else(|| app.mods.port().map(|port| &port.release));
        let words = release.map_or_else(
            || "tgg-melee runs Melee on your computer, with mods and skins.".to_owned(),
            |release| {
                format!(
                    "tgg-melee runs Melee on your computer, with mods and skins. {}.",
                    size_text(release.size())
                )
            },
        );
        body = body.child(label(words).font(text::BODY).color(p.muted).wrap());
        // Without a release to download, the button can't do anything: say
        // why, and offer to ask again.
        if release.is_none() {
            let why = match &app.mods.registry {
                Registry::Offline => Some("Can't reach textures.gg. Check your connection."),
                Registry::Failed(why) => Some(why.as_str()),
                Registry::NotAsked | Registry::Asking | Registry::Answered => None,
            };
            if let Some(why) = why {
                body = body.child(
                    label(why)
                        .font(text::BODY)
                        .color(p.danger)
                        .wrap()
                        .key("first-run/why"),
                );
                control = Some(
                    text_action("Try again", false, p)
                        .key("first-run/try-again")
                        .on_press(crate::mods::actions::fetch),
                );
            }
        }
    }
    step_row(2, installed, body, control, !has_disc, p)
}

/// The mods to start with: three choices before setup, what they made after.
fn mods_row(
    app: &App,
    first_run: Option<&FirstRun>,
    stage: Stage,
    has_disc: bool,
    p: &Palette,
) -> El {
    let picks = first_run.map_or_else(Picks::default, |first| first.picks);
    let mut body = col()
        .w_full()
        .gap(space::SNUG)
        .child(step_title("Mods to start with", p));
    match stage {
        Stage::Ready => {
            body = body.child(
                label("Pick any. They go into one set, and you can change it on Mods.")
                    .font(text::BODY)
                    .color(p.muted)
                    .wrap(),
            );
            let offered: Vec<_> = LINKS
                .into_iter()
                .filter_map(|link| {
                    app.mods
                        .sets
                        .iter()
                        .find(|set| set.name == link)
                        .map(|set| (link, set))
                })
                .collect();
            body = if offered.is_empty() {
                body.child(
                    label("Getting what textures.gg has…")
                        .font(text::SMALL)
                        .color(p.faint),
                )
            } else {
                body.child(
                    row().w_full().gap(space::XS).children(
                        offered
                            .into_iter()
                            .map(|(link, set)| pick(link, set, picks.on(link), has_disc, p)),
                    ),
                )
            };
        }
        Stage::Downloading => {
            body = body.child(picked_label(app, &picks, p));
        }
        Stage::SettingUp => {
            body = body.child(label("Making your sets.").font(text::BODY).color(p.muted));
        }
        Stage::Done => {
            let count = picks.count();
            body = body.child(
                label(if count == 0 {
                    "None. Add some on Mods when you want them.".to_owned()
                } else if count == 1 {
                    format!("{}. It's the set Play starts.", picked_titles(app, &picks))
                } else {
                    format!(
                        "{}, in one set called My set. It's the set Play starts.",
                        picked_titles(app, &picks)
                    )
                })
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
            );
        }
    }
    step_row(3, stage == Stage::Done, body, None, !has_disc, p)
}

/// Signing in, optional and only needed to find matches.
fn sign_in_row(app: &App, has_disc: bool, p: &Palette) -> El {
    let done = app.signin.who.is_some();
    let who = app
        .signin
        .who
        .as_ref()
        .map(|signed| crate::signin::shown_name(&signed.account));
    let body = col()
        .gap(space::SNUG)
        .child(step_title("Sign in to textures.gg", p))
        .child(
            label(match &who {
                Some(who) => format!("Signed in as {who}."),
                None => "You need it to find matches. You can do it later.".to_owned(),
            })
            .font(text::BODY)
            .color(p.muted)
            .wrap(),
        );
    let control = (!done).then(|| {
        button("Sign in", ButtonKind::Secondary, p)
            .key("first-run/signin")
            .on_press(crate::signin::actions::begin)
    });
    step_row(4, done, body, control, !has_disc, p)
}

/// One choice among the mods to start with: its name, what it does, and how
/// many mods it has.
fn pick(link: &str, set: &tgg_mod::Set, on: bool, enabled: bool, p: &Palette) -> El {
    let link = link.to_owned();
    let title = set.title.clone();
    let mods = count(set.mods.len());
    col()
        .key(format!("first-run/pick/{}", set.name))
        .grow(1.0)
        .min_w(0.0)
        .min_h(PICK_MIN_H)
        .gap(space::TIGHT)
        .padding(space::SM)
        .radius(radius::CARD)
        .fill(if on { p.selected } else { p.surface })
        .border(
            if on { size::LINE_STRONG } else { size::LINE },
            if on { p.accent } else { p.line },
        )
        .cursor(rune::Cursor::Pointer)
        .role("checkbox")
        .checked(on)
        .disabled(!enabled)
        .label(title.clone())
        .focusable(true)
        .on_press(move |app: &mut App| actions::toggle(app, &link))
        .child(
            row()
                .gap(space::XS)
                .items_center()
                .child(
                    label(title)
                        .font(text::STRONG)
                        .color(p.ink)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .children(on.then(|| icon(Icon::Check, p.accent))),
        )
        .child(
            col()
                .grow(1.0)
                .gap(space::TIGHT)
                .children((!set.description.is_empty()).then(|| {
                    label(set.description.clone())
                        .font(text::SMALL)
                        .color(p.muted)
                        .wrap()
                        .max_lines(3)
                }))
                .child(spacer())
                .child(label(mods).font(text::SMALL).color(p.faint)),
        )
}

/// The one button: Download and set up, Download the game, Cancel, or Play.
fn main_button(app: &App, stage: Stage, has_disc: bool, picked: usize, p: &Palette) -> El {
    match stage {
        Stage::Done => button_main("Play", ButtonKind::Primary, p)
            .key("first-run/play")
            .on_press(actions::play),
        Stage::Downloading => button_main("Cancel", ButtonKind::Secondary, p)
            .key("first-run/cancel")
            .on_press(actions::cancel),
        Stage::SettingUp => button_main("Setting up", ButtonKind::Secondary, p)
            .disabled(true)
            .opacity(DIMMED),
        Stage::Ready => {
            let ready = has_disc && (app.mods.port().is_some() || !app.mods.releases.is_empty());
            let button = button_main(
                if picked == 0 {
                    "Download the game"
                } else {
                    "Download and set up"
                },
                ButtonKind::Primary,
                p,
            )
            .key("first-run/download");
            if ready {
                button.on_press(actions::download)
            } else {
                button.disabled(true).opacity(DIMMED)
            }
        }
    }
}

/// A step: its number or its check, what it says, and what the player can do
/// about it.
fn step_row(
    number: usize,
    done: bool,
    body: El,
    control: Option<El>,
    dim: bool,
    p: &Palette,
) -> El {
    row()
        .w_full()
        .py(space::SM)
        .gap(space::MD)
        .items_center()
        .opacity(if dim { DIMMED } else { 1.0 })
        .child(mark(number, done, p))
        .child(body.grow(1.0).min_w(0.0))
        .children(control)
}

fn mark(number: usize, done: bool, p: &Palette) -> El {
    el().size(MARK, MARK)
        .radius(radius::PILL)
        .center()
        .fill(if done { p.success } else { p.fill })
        .child(if done {
            icon(Icon::Check, p.on_ink)
        } else {
            label(number.to_string()).font(text::LABEL).color(p.muted)
        })
}

fn step_title(title: &str, p: &Palette) -> El {
    label(title).font(text::TITLE).color(p.ink)
}

/// Whether a disc is ready for both the app and tgg-melee.
fn disc_found(app: &App) -> bool {
    app.game.is_some() || matches!(app.mods.disc, Disc::Found(_))
}

/// While tgg-melee downloads: how many megabytes so far, and of how many.
fn downloading(work: &Work) -> String {
    let progress = work
        .progress
        .lock()
        .ok()
        .and_then(|progress| progress.clone());
    let Some(progress) = progress else {
        return work.label();
    };
    let total = progress.total.map(size_text);
    match (progress.bytes, total) {
        (0, Some(total)) => format!("Downloading, 0 of {total}"),
        (0, None) => "Downloading".to_owned(),
        (bytes, Some(total)) => {
            format!("Downloading, {} of {total}", size_text(bytes))
        }
        (bytes, None) => format!("Downloading, {}", size_text(bytes)),
    }
}

/// The picks by the names textures.gg gives them.
fn picked_titles(app: &App, picks: &Picks) -> String {
    picks
        .links()
        .iter()
        .map(|link| {
            app.mods
                .sets
                .iter()
                .find(|set| set.name == *link)
                .map_or_else(|| title_of(link), |set| set.title.clone())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn picked_label(app: &App, picks: &Picks, p: &Palette) -> El {
    label(if picks.count() == 0 {
        "None".to_owned()
    } else {
        picked_titles(app, picks)
    })
    .font(text::BODY)
    .color(if picks.count() == 0 { p.faint } else { p.muted })
    .wrap()
}

fn title_of(link: &str) -> String {
    match link {
        "competitive" => "Competitive".to_owned(),
        "practice" => "Practice".to_owned(),
        "replays" => "Replays".to_owned(),
        other => other.to_owned(),
    }
}

/// "1 mod", "3 mods".
fn count(n: usize) -> String {
    if n == 1 {
        "1 mod".to_owned()
    } else {
        format!("{n} mods")
    }
}

/// The band across the top: on macOS it moves the window, as the title row
/// does once there is a game.
fn title_band() -> El {
    title_bar(row())
        .h(size::TOP_BAR)
        .w_full()
        .absolute(0.0, 0.0)
}
