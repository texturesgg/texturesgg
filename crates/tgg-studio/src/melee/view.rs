//! Skins' page: the site's skins, the player's that are on, and the ones
//! they made, as plates with a picture each; kind chips over them, and Make
//! a skin at their end. A skin opens its sheet; Make a skin opens the
//! picker.

use super::{Kind, Melee, Tab, actions};
use crate::jobs::renders::RenderKey;
use crate::library::site::{self, Pack};
use crate::library::view::{self as skins, card};
use crate::state::App;
use crate::ui::tokens::radius;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{ButtonKind, button, chip, spacer, tab, text_action};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_app::costumes::Fighter;
use tgg_app::library::Skin;

/// How many placeholder plates stand in while the site's listing is read.
const PLACEHOLDERS: usize = 10;
/// A placeholder plate's name line.
const STUB_LINE: f32 = 10.0;

pub fn view(app: &App, p: &Palette) -> El {
    if let Some(document) = &app.editor {
        return crate::editor::view::view(app, document, p);
    }
    let melee = &app.melee;
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::MD)
        .children(melee.problem.as_ref().map(|problem| {
            label(problem.clone())
                .key("melee/problem")
                .font(text::BODY)
                .color(p.danger)
        }))
        .child(tabs(app, p))
        .child(over(app, p))
        .child(body(app, p));
    col()
        .key("melee/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
        .children(super::sheet::view(app, p))
}

/// Browse, Using and Made by you, with what each has.
fn tabs(app: &App, p: &Palette) -> El {
    let melee = &app.melee;
    let tab_for = |name: &str, count: usize, which: Tab, key: &str| {
        tab(name, count, melee.tab == which, p)
            .key(key.to_owned())
            .on_press(move |app: &mut App| actions::show_tab(app, which))
    };
    let using = app.library.skins().len();
    let made = app
        .library
        .skins()
        .iter()
        .filter(|skin| skins::made(skin))
        .count();
    row()
        .gap(space::LG)
        .child(tab_for(
            "Browse",
            listed(app).map_or(0, Vec::len),
            Tab::Browse,
            "skins/tab/browse",
        ))
        .child(tab_for("Using", using, Tab::Using, "skins/tab/using"))
        .child(tab_for("Made by you", made, Tab::Made, "skins/tab/made"))
}

/// The kinds the tab has skins of, and Make a skin at the row's end.
fn over(app: &App, p: &Palette) -> El {
    let melee = &app.melee;
    let kinds = kinds(app);
    let chips = kinds.into_iter().map(|kind| {
        let chosen = melee.kind == Some(kind);
        chip(kind.label(), chosen, p)
            .key(format!("skins/kind/{}", kind.label()))
            .role("radio")
            .checked(chosen)
            .on_press(move |app: &mut App| actions::show_kind(app, (!chosen).then_some(kind)))
    });
    row()
        .flex_wrap()
        .items_center()
        .gap(space::SNUG)
        .children(chips)
        .child(spacer())
        .child(
            crate::ui::text_action("Add a file…", false, p)
                .key("skins/add")
                .on_press_tx(crate::library::actions::choose_file),
        )
        .child(
            button("Make a skin", ButtonKind::Primary, p)
                .key("skins/make")
                .on_press(actions::make),
        )
}

/// The site's skins as last read.
fn listed(app: &App) -> Option<&Vec<Pack>> {
    app.library_page
        .browse
        .items
        .as_ref()
        .and_then(|items| items.as_ref().ok())
}

/// The kinds the tab has content for, in the chips' order.
fn kinds(app: &App) -> Vec<Kind> {
    let mut kinds: Vec<Kind> = Vec::new();
    match app.melee.tab {
        Tab::Browse => {
            if let Some(packs) = listed(app) {
                for pack in packs {
                    let kind = pack.kind();
                    if !kinds.contains(&kind) {
                        kinds.push(kind);
                    }
                }
            }
        }
        Tab::Using | Tab::Made => {
            for skin in local(app) {
                if let Some(kind) = skins::kind(skin)
                    && !kinds.contains(&kind)
                {
                    kinds.push(kind);
                }
            }
        }
    }
    Kind::ALL
        .into_iter()
        .filter(|kind| kinds.contains(kind))
        .collect()
}

/// The tab's body: the site's plates, the player's that are on, or the ones
/// they made; or what to do while there's nothing.
fn body(app: &App, p: &Palette) -> El {
    match app.melee.tab {
        Tab::Browse => browse(app, p),
        Tab::Using => {
            let skins = local(app);
            if skins.is_empty() {
                return quiet(
                    "skins/using/empty",
                    if app.library.skins().is_empty() {
                        "You have no skins yet. Browse textures.gg, or make one."
                    } else {
                        "No skins match that."
                    },
                    None,
                    p,
                );
            }
            skins::grid(skins.into_iter().map(|skin| plate(app, skin, p)).collect())
        }
        Tab::Made => {
            let skins = matching(app, skins::made);
            if skins.is_empty() {
                return quiet(
                    "skins/made/empty",
                    "Skins you make and save show here.",
                    Some(
                        text_action("Make one", false, p)
                            .key("skins/made/make")
                            .on_press(actions::make),
                    ),
                    p,
                );
            }
            skins::grid(skins.into_iter().map(|skin| plate(app, skin, p)).collect())
        }
    }
}

/// The site's skins as plates, with what to do while they're on their way,
/// out of reach, or none match.
fn browse(app: &App, p: &Palette) -> El {
    let Some(items) = &app.library_page.browse.items else {
        return row()
            .flex_wrap()
            .gap(space::MD)
            .children((0..PLACEHOLDERS).map(|_| {
                col()
                    .w(size::CARD)
                    .gap(space::SNUG)
                    .child(
                        el().w(size::CARD)
                            .aspect_ratio(4.0 / 5.0)
                            .radius(radius::CARD)
                            .fill(p.surface),
                    )
                    .child(
                        el().w_frac(0.6)
                            .h(STUB_LINE)
                            .radius(radius::PILL)
                            .fill(p.fill),
                    )
            }));
    };
    let packs: Vec<&Pack> = match items {
        Err(problem) => {
            return quiet(
                "skins/browse/problem",
                &format!("Couldn't reach textures.gg: {problem}"),
                Some(
                    text_action("Try again", false, p)
                        .key("skins/browse/again")
                        .on_press(|app: &mut App| site::load_again(app, true)),
                ),
                p,
            );
        }
        Ok(packs) => packs
            .iter()
            .filter(|pack| app.melee.kind.is_none_or(|kind| pack.kind() == kind))
            .filter(|pack| sought(app, std::slice::from_ref(pack)))
            .collect(),
    };
    if packs.is_empty() {
        return quiet("skins/browse/empty", "No skins match that.", None, p);
    }
    skins::grid(
        packs
            .into_iter()
            .map(|pack| site_plate(app, pack, p))
            .collect(),
    )
}

/// A skin on the site: its preview, its name, and what it's for.
fn site_plate(app: &App, pack: &Pack, p: &Palette) -> El {
    let mut picture = crate::ui::card_plate(p);
    if let Some(pixels) = pack
        .preview()
        .and_then(|url| app.library_page.browse.pictures.get(&url))
    {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let using = site::kept(app, &pack.id)
        .iter()
        .filter_map(|id| app.library.skins().iter().find(|skin| skin.id == *id))
        .any(|skin| skins::in_game(app, skin));
    let picture = if using {
        picture.child(
            crate::ui::badge("Using", crate::ui::Tone::Mine, p).absolute(space::XS, space::XS),
        )
    } else {
        picture
    };
    let mut line: Vec<String> = Vec::new();
    line.extend(pack.target_name.clone());
    if let Some(creator) = &pack.creator_name {
        line.push(format!("by {creator}"));
    }
    let opened = pack.clone();
    col()
        .w(size::CARD)
        .gap(space::SNUG)
        .child(picture)
        .child(
            col()
                .gap(space::TIGHT)
                .child(
                    label(pack.title.clone())
                        .font(text::STRONG)
                        .color(p.ink)
                        .truncate(),
                )
                .child(
                    label(line.join(" · "))
                        .font(text::SMALL)
                        .color(p.muted)
                        .truncate(),
                ),
        )
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(pack.title.clone())
        .focusable(true)
        .on_press(move |app: &mut App| actions::open_site_skin(app, opened.clone()))
}

/// One of the player's skins, opening its sheet.
fn plate(app: &App, skin: &Skin, p: &Palette) -> El {
    let id = skin.id;
    card(app, skin, p)
        .key(format!("skins/skin/{id}"))
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(skins::name(skin))
        .focusable(true)
        .on_press(move |app: &mut App| actions::open_skin(app, id))
}

/// The player's skins the current kind shows, newest first.
fn local(app: &App) -> Vec<&Skin> {
    app.library
        .skins()
        .iter()
        .rev()
        .filter(|skin| {
            app.melee
                .kind
                .is_none_or(|kind| skins::kind(skin) == Some(kind))
        })
        .filter(|skin| sought_skin(app, skin))
        .collect()
}

/// The local skins that `show` and the current kind both take.
fn matching(app: &App, show: impl Fn(&Skin) -> bool) -> Vec<&Skin> {
    local(app).into_iter().filter(|skin| show(skin)).collect()
}

/// Whether `pack` matches what the player looks for.
fn sought(app: &App, packs: &[&Pack]) -> bool {
    let words = words(app);
    packs.iter().any(|pack| {
        words.is_empty()
            || words.iter().all(|word| {
                pack.title.to_lowercase().contains(word)
                    || pack
                        .creator_name
                        .as_deref()
                        .is_some_and(|creator| creator.to_lowercase().contains(word))
                    || pack
                        .target_name
                        .as_deref()
                        .is_some_and(|target| target.to_lowercase().contains(word))
            })
    })
}

fn sought_skin(app: &App, skin: &Skin) -> bool {
    let words = words(app);
    words.is_empty()
        || words.iter().all(|word| {
            skin.name.to_lowercase().contains(word)
                || skins::place(skin).to_lowercase().contains(word)
        })
}

fn words(app: &App) -> Vec<String> {
    app.melee
        .search
        .text
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// What a place has to say while it has nothing to show.
fn quiet(key: &str, words: &str, action: Option<El>, p: &Palette) -> El {
    col()
        .key(key.to_owned())
        .gap(space::XS)
        .padding(space::MD)
        .child(
            label(words.to_owned())
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        )
        .children(action)
}

/// The search the top bar holds on Skins.
pub fn search(app: &App, p: &Palette) -> El {
    crate::ui::search_box(
        crate::ui::search_field(&app.melee.search, "Search skins", p)
            .key("skins/search")
            .on_edit(|app: &mut App| &mut app.melee.search),
        p,
    )
    .w(crate::ui::tokens::size::SEARCH)
}

/// The picker's plate for a fighter, with its picture and what's changed.
pub fn fighter_plate(app: &App, fighter: &Fighter, p: &Palette) -> El {
    let Some(slot) = Melee::shown_slot(fighter) else {
        return col();
    };
    let mut picture = crate::ui::card_plate(p);
    if let Some(pixels) = app.renders.get(RenderKey::Slot(slot)) {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let changed = app.melee.changed_count(fighter);
    let status = if app.melee.states.is_none() {
        " ".to_owned()
    } else if changed == 0 {
        "Vanilla".to_owned()
    } else {
        format!("{changed} custom")
    };
    let name = fighter.character.name();
    let character = fighter.character;
    col()
        .w(size::CARD)
        .gap(space::SNUG)
        .child(picture)
        .child(
            col()
                .gap(space::TIGHT)
                .child(label(name).font(text::STRONG).color(p.ink).truncate())
                .child(label(status).font(text::SMALL).color(if changed > 0 {
                    p.accent_text
                } else {
                    p.muted
                })),
        )
        .key(format!("melee/fighter/{name}"))
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(name)
        .focusable(true)
        .on_press(move |app: &mut App| actions::pick_fighter(app, character))
}

/// The picker's plate for a stage.
pub fn stage_plate(app: &App, stage: melee_dat::Stage, p: &Palette) -> El {
    let mut picture = crate::ui::card_plate(p);
    let slot = MeleeSlot::Stage(stage);
    if let Some(pixels) = app.renders.get(RenderKey::Slot(slot)) {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let changed = app.melee.changed(slot);
    let status = if app.melee.states.is_none() {
        " ".to_owned()
    } else if changed {
        "Custom".to_owned()
    } else {
        "Vanilla".to_owned()
    };
    let name = stage.name();
    col()
        .w(size::CARD)
        .gap(space::SNUG)
        .child(picture)
        .child(
            col()
                .gap(space::TIGHT)
                .child(label(name).font(text::STRONG).color(p.ink).truncate())
                .child(label(status).font(text::SMALL).color(if changed {
                    p.accent_text
                } else {
                    p.muted
                })),
        )
        .key(format!("melee/stage/{}", stage.file_name()))
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(name)
        .focusable(true)
        .on_press(move |app: &mut App| actions::pick_stage(app, stage))
}

/// The picker's tab: the game's fighters or its stages.
pub fn roster_tab(name: &str, count: usize, chosen: bool, which: super::Roster, p: &Palette) -> El {
    tab(name, count, chosen, p)
        .key(format!("skins/picker/{}", name.to_lowercase()))
        .on_press(move |app: &mut App| actions::pick_roster(app, which))
}

/// The sheet of the skin open, when it's one of the site's and the site's
/// own preview shows its picture.
pub fn site_picture<'a>(app: &'a App, pack: &Pack) -> Option<&'a rune::Pixels> {
    pack.preview()
        .and_then(|url| app.library_page.browse.pictures.get(&url))
}
