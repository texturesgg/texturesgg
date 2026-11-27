//! First run's screen: one column, four numbered steps, and the one button.
//! No sidebar and nothing else: until the player finishes setup once, this is
//! the app.

use super::{DEFAULT_SET, Stage, actions};
use crate::mods::{Disc, Registry, Work, disc_warning, mod_count, size_text};
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

pub fn view(app: &App, p: &Palette) -> El {
    let first_run = app.first_run.as_ref();
    let stage = first_run.map_or(Stage::Done, |first| first.stage);
    let done = stage == Stage::Done;
    let has_disc = disc_found(app);
    let use_default = first_run.is_none_or(|first| first.use_default_set);
    let has_port = app.mods.port().is_some();
    let has_update = app.mods.port_update().is_some();

    let line = if done {
        "Everything is set up."
    } else if !has_disc {
        "Start with your Melee disc."
    } else if has_port && !has_update {
        "Confirm your starting set to get started."
    } else if has_update {
        "Confirm your starting set, then update tgg-melee."
    } else {
        "Confirm your starting set, then download."
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
                .child(mods_row(app, use_default, stage, has_disc, p))
                .child(sign_in_row(app, has_disc, p)),
        )
        .child(
            row()
                .pl(MARK + space::MD)
                .child(main_button(app, stage, has_disc, use_default, p)),
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

/// Your Melee disc: found (with a warning if counted files are modified), or
/// why not and a way to choose it.
fn disc_row(app: &App, p: &Palette) -> El {
    let found = disc_found(app);
    let warning = found
        .then(|| disc_warning(&app.mods.disc_modified))
        .flatten();
    let body = col()
        .gap(space::SNUG)
        .child(step_title("Your Melee disc", p))
        .child(
            label(if found {
                "Found on this computer."
            } else {
                "None found on this computer. Pick your 1.4 GB Melee .iso file (the same one Slippi uses)."
            })
            .font(text::BODY)
            .color(p.muted)
            .wrap(),
        )
        .children(warning.map(|text| {
            label(text)
                .key("first-run/disc-warning")
                .font(text::SMALL)
                .color(p.accent_text)
                .wrap()
        }));
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

/// The game: already installed, update available, what tgg-melee is and its
/// size, or the download while it runs.
fn game_row(app: &App, stage: Stage, has_disc: bool, p: &Palette) -> El {
    let current = app.mods.port();
    let update = app.mods.port_update();
    let installed_up_to_date = (current.is_some() && update.is_none()) || stage == Stage::Done;
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
    } else if let (Some(port), Some(next)) = (current, update) {
        let words = if app.mods.has_port(&next.version) {
            format!(
                "You have tgg-melee {}. Version {} is already installed and ready to use.",
                port.version, next.version
            )
        } else {
            format!(
                "You have tgg-melee {}. Version {} is out ({}).",
                port.version,
                next.version,
                size_text(next.size())
            )
        };
        body = body.child(
            label(words)
                .key("first-run/port-update")
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        );
    } else if let Some(port) = current {
        body = body.child(
            label(format!("tgg-melee {} is already installed.", port.version))
                .key("first-run/port-installed")
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        );
    } else {
        let release = app.mods.releases.first();
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
    step_row(2, installed_up_to_date, body, control, !has_disc, p)
}

/// The mods to start with: the single recommended default set pre-checked, or
/// unchecked for plain Melee.
fn mods_row(app: &App, use_default: bool, stage: Stage, has_disc: bool, p: &Palette) -> El {
    let default_set = app.mods.sets.iter().find(|set| set.name == DEFAULT_SET);
    let title = default_set.map_or("Competitive", |set| set.title.as_str());
    let mut body = col()
        .w_full()
        .gap(space::SNUG)
        .child(step_title("Mods to start with", p));
    match stage {
        Stage::Ready => {
            body = body
                .child(
                    label("Turn it off if you want plain Melee. You can change your sets later on Mods.")
                        .font(text::BODY)
                        .color(p.muted)
                        .wrap(),
                )
                .child(default_set_card(
                    title,
                    default_set.map(|set| set.mods.len()),
                    use_default,
                    has_disc,
                    p,
                ));
        }
        Stage::Downloading => {
            body = body.child(
                label(if use_default {
                    title.to_owned()
                } else {
                    "Plain Melee (no mods)".to_owned()
                })
                .font(text::BODY)
                .color(if use_default { p.muted } else { p.faint })
                .wrap(),
            );
        }
        Stage::SettingUp => {
            body = body.child(
                label(if use_default {
                    format!("Setting up {title}.")
                } else {
                    "Setting up plain Melee.".to_owned()
                })
                .font(text::BODY)
                .color(p.muted),
            );
        }
        Stage::Done => {
            body = body.child(
                label(if use_default {
                    format!("{title}. It's the set Play starts.")
                } else {
                    "Plain Melee with no mods. Add mods or sets on Mods whenever you want."
                        .to_owned()
                })
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
            );
        }
    }
    step_row(3, stage == Stage::Done, body, None, !has_disc, p)
}

/// The single pre-checked default set card: Competitive, covering online
/// matchmaking, tournament rules, replays, and solo practice drills/trainers.
fn default_set_card(title: &str, mods: Option<usize>, on: bool, enabled: bool, p: &Palette) -> El {
    col()
        .key(format!("first-run/pick/{DEFAULT_SET}"))
        .w_full()
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
        .label(title.to_owned())
        .focusable(true)
        .on_press(actions::toggle_default_set)
        .child(
            row()
                .gap(space::XS)
                .items_center()
                .child(label(title.to_owned()).font(text::STRONG).color(p.ink))
                .children(mods.map(|n| label(mod_count(n)).font(text::SMALL).color(p.faint)))
                .child(spacer())
                .children(on.then(|| icon(Icon::Check, p.accent))),
        )
        .child(
            label(
                "Online matchmaking, UCF and tournament rules, replays, and solo practice drills and trainers.",
            )
            .font(text::SMALL)
            .color(p.muted)
            .wrap(),
        )
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

/// The one button: adapts to whether tgg-melee is installed, updatable, or
/// needs a fresh download, and whether the default set is checked.
fn main_button(app: &App, stage: Stage, has_disc: bool, use_default: bool, p: &Palette) -> El {
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
            let has_port = app.mods.port().is_some();
            let has_update = app.mods.port_update().is_some();
            let ready = has_disc && (has_port || !app.mods.releases.is_empty());
            let title = if has_port && !has_update {
                if use_default {
                    "Set up Competitive"
                } else {
                    "Continue"
                }
            } else if has_update {
                if use_default {
                    "Update and set up"
                } else {
                    "Update tgg-melee"
                }
            } else if use_default {
                "Download and set up"
            } else {
                "Download the game"
            };
            let button = button_main(title, ButtonKind::Primary, p).key("first-run/download");
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

/// The band across the top: on macOS and Windows it drags the window, as the
/// title row does once there is a game.
fn title_band() -> El {
    title_bar(row())
        .h(size::TOP_BAR)
        .w_full()
        .absolute(0.0, 0.0)
}
