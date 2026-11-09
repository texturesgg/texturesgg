//! Play's page: a card for each game, side by side, wrapping when narrow.

use crate::mods::Disc;
use crate::nav::Place;
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, button, button_off, page_head, panel, spacer, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};
use std::path::Path;

/// Where the player gets Slippi Launcher, which installs Slippi's Dolphin.
const SLIPPI_URL: &str = "https://slippi.gg";

pub fn view(app: &App, p: &Palette) -> El {
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::LG)
        .child(page_head(
            Place::Play.label(),
            "Your two games: Slippi, with your skins, and tgg-melee, with your mods.",
            p,
        ))
        .child(
            row()
                .flex_wrap()
                .gap(space::MD)
                .child(slippi(app, p))
                .child(tgg_melee(app, p)),
        );
    col()
        .key("play/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// A game's card: its name, what it is, facts about it, then its main
/// action and the quieter ones.
fn game_card(
    key: &str,
    name: &str,
    about: &str,
    facts: Vec<El>,
    main: El,
    more: Vec<El>,
    p: &Palette,
) -> El {
    panel(p)
        .key(key.to_owned())
        .w(size::GAME_CARD)
        .gap(space::MD)
        .child(
            col()
                .gap(space::SNUG)
                .child(label(name.to_owned()).font(text::H3).color(p.ink))
                .child(
                    label(about.to_owned())
                        .font(text::BODY)
                        .color(p.muted)
                        .wrap(),
                ),
        )
        // Cards in a row stand as tall as the tallest; the facts take the
        // room, so the actions keep to the foot.
        .child(col().grow(1.0).gap(space::TIGHT).children(facts))
        .child(
            row()
                .items_center()
                .gap(space::SM)
                .child(main)
                .child(spacer())
                .child(row().gap(space::XXS).mr(-TEXT_ACTION_INSET).children(more)),
        )
}

/// A fact on a card: a file, a version.
fn fact(words: String, p: &Palette) -> El {
    label(words)
        .font(text::MONO_SMALL)
        .color(p.muted)
        .truncate_middle()
        .min_w(0.0)
}

/// A line on a card saying what's missing.
fn needs(words: &str, p: &Palette) -> El {
    label(words.to_owned())
        .font(text::SMALL)
        .color(p.ink)
        .wrap()
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Slippi: the player's ISO in Slippi's Dolphin, skins and all. Play needs
/// the Dolphin Slippi Launcher installs.
fn slippi(app: &App, p: &Palette) -> El {
    let mut facts = Vec::new();
    if let Some(references) = &app.game {
        facts.push(fact(file_name(references.game().path()), p));
    }
    let main = if app.slippi.running {
        button_off("Slippi is open", p).key("play/slippi/open")
    } else if app.slippi.dolphin.is_some() && app.game.is_some() {
        button("Play", ButtonKind::Primary, p)
            .key("play/slippi/play")
            .on_press(crate::slippi::actions::play)
    } else {
        facts.push(needs(
            "Slippi Launcher isn't installed yet. It sets up the Dolphin Slippi plays in.",
            p,
        ));
        button("Get Slippi", ButtonKind::Secondary, p)
            .key("play/slippi/get")
            .on_press_tx(|app: &mut App, tx| {
                if let Err(error) = tx.open_url(SLIPPI_URL) {
                    app.tell(format!("Couldn't open slippi.gg: {error}"), true);
                }
            })
    };
    let more = vec![
        text_action("Choose another ISO…", false, p)
            .key("play/slippi/choose-iso")
            .on_press_tx(crate::preferences::actions::choose_game),
    ];
    game_card(
        "play/slippi",
        "Slippi",
        "Melee online through Slippi, with the skins in your ISO.",
        facts,
        main,
        more,
        p,
    )
}

/// tgg-melee: the port, with the mods turned on. Play needs it installed
/// and an unmodified disc for it.
fn tgg_melee(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let mut facts = Vec::new();
    let mut more = Vec::new();
    let set_up = |words: &str| {
        button(words, ButtonKind::Primary, p)
            .key("play/tgg-melee/set-up")
            .on_press_tx(|app: &mut App, tx| crate::editor::actions::go(app, Place::Mods, tx))
    };
    let main = match mods.port() {
        None => {
            facts.push(needs("Not installed yet.", p));
            set_up("Set up tgg-melee")
        }
        Some(port) => {
            let on = mods.local.mods.iter().filter(|m| m.enabled).count();
            facts.push(fact(
                format!("Version {} · {}", port.version, mods_on(on)),
                p,
            ));
            if let Some(update) = mods.port_update() {
                more.push(
                    text_action(&format!("{} is out", update.version), false, p)
                        .key("play/tgg-melee/update")
                        .on_press_tx(|app: &mut App, tx| {
                            crate::editor::actions::go(app, Place::Mods, tx)
                        }),
                );
            }
            more.push(
                text_action("Choose a disc…", false, p)
                    .key("play/tgg-melee/choose-disc")
                    .on_press_tx(crate::mods::actions::choose_disc),
            );
            match &mods.disc {
                _ if mods.playing.is_some() => button("Stop", ButtonKind::Secondary, p)
                    .key("play/tgg-melee/stop")
                    .on_press(crate::mods::actions::stop),
                Disc::Found(disc) => {
                    facts.push(fact(format!("Plays {}", file_name(disc)), p));
                    button("Play", ButtonKind::Primary, p)
                        .key("play/tgg-melee/play")
                        .on_press(crate::mods::actions::play)
                }
                Disc::Missing => {
                    facts.push(needs(
                        "It needs an unmodified Melee disc, and none was found. Your ISO has skins in it.",
                        p,
                    ));
                    more.pop();
                    button("Choose a disc…", ButtonKind::Primary, p)
                        .key("play/tgg-melee/choose-disc")
                        .on_press_tx(crate::mods::actions::choose_disc)
                }
                Disc::Unknown | Disc::Looking => {
                    facts.push(fact("Looking for an unmodified disc…".to_owned(), p));
                    button_off("Play", p).key("play/tgg-melee/waiting")
                }
            }
        }
    };
    game_card(
        "play/tgg-melee",
        "tgg-melee",
        "Melee rebuilt for your computer, with the mods you choose.",
        facts,
        main,
        more,
        p,
    )
}

fn mods_on(count: usize) -> String {
    match count {
        0 => "no mods on".to_owned(),
        1 => "1 mod on".to_owned(),
        n => format!("{n} mods on"),
    }
}
