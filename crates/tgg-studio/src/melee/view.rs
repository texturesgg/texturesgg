//! Skins' page: the site's skins, the player's that are on, and the ones
//! they made, as plates with a picture each; Browse filters, sort, and
//! pagination over `/api/packs`, and Make a skin at the row's end.

use super::{BrowseSort, Kind, Melee, Menu, Tab, actions};
use crate::jobs::renders::RenderKey;
use crate::library::site::{self, Pack, Target};
use crate::library::view::{self as skins, card};
use crate::nav::Place;
use crate::state::App;
use crate::ui::tokens::radius;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{
    ButtonKind, button, button_off, chip, select, select_item, select_menu, select_rule, spacer,
    tab, text_action,
};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_app::costumes::Fighter;
use tgg_app::library::Skin;

/// How many placeholder plates stand in while the site's listing is read.
const PLACEHOLDERS: usize = 10;
/// A placeholder plate's name line.
const STUB_LINE: f32 = 10.0;
/// Width of Browse's Sort select.
const SORT_WIDTH: f32 = 128.0;
/// Width of Browse's Fighter/Stage target select.
const TARGET_WIDTH: f32 = 172.0;

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
        .children(over(app, p))
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

/// Browse, Using and Made by you on the left, page actions on the right.
fn tabs(app: &App, p: &Palette) -> El {
    let melee = &app.melee;
    let tab_for = |name: &str, count: usize, which: Tab, key: &str| {
        tab(name, count, melee.tab == which, p)
            .key(key.to_owned())
            .on_press(move |app: &mut App| actions::show_tab(app, which))
    };
    let browse_count = {
        let total = app.library_page.browse.total;
        if total > 0 {
            total
        } else {
            listed(app).map_or(0, Vec::len)
        }
    };
    let using = app.library.skins().len();
    let made = app
        .library
        .skins()
        .iter()
        .filter(|skin| skins::made(skin))
        .count();
    let mut header = row()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .child(tab_for(
            "Browse",
            browse_count,
            Tab::Browse,
            "skins/tab/browse",
        ))
        .child(tab_for("Using", using, Tab::Using, "skins/tab/using"))
        .child(tab_for("Made by you", made, Tab::Made, "skins/tab/made"))
        .child(spacer());
    let mut actions_row = row().items_center().gap(space::MD);
    if melee.tab == Tab::Browse {
        actions_row = actions_row.child(
            text_action("Open on textures.gg", false, p)
                .key("skins/browse/site")
                .on_press_tx(actions::open_browse_on_site),
        );
    }
    header = header.child(
        actions_row
            .child(
                text_action("Add a file…", false, p)
                    .key("skins/add")
                    .on_press_tx(crate::library::actions::choose_file),
            )
            .child(
                button("Make a skin", ButtonKind::Primary, p)
                    .key("skins/make")
                    .on_press(actions::make),
            ),
    );
    header
}

/// Kind chips and, on Browse, fighter/stage target and sort selects.
fn over(app: &App, p: &Palette) -> Option<El> {
    let melee = &app.melee;
    let kinds = kinds(app);
    if melee.tab != Tab::Browse && kinds.is_empty() {
        return None;
    }
    let chips = kinds.into_iter().map(|kind| {
        let chosen = melee.kind == Some(kind);
        chip(kind.label(), chosen, p)
            .key(format!("skins/kind/{}", kind.label()))
            .role("radio")
            .checked(chosen)
            .on_press(move |app: &mut App| actions::show_kind(app, (!chosen).then_some(kind)))
    });
    let mut bar = row()
        .flex_wrap()
        .items_center()
        .gap(space::SNUG)
        .children(chips);
    if melee.tab == Tab::Browse {
        bar = bar.child(spacer());
        if let Some(target_picker) = target_select(app, p) {
            bar = bar.child(target_picker);
        }
        bar = bar.child(
            select(melee.sort.label(), melee.menu == Some(Menu::Sort), p)
                .key("skins/sort")
                .min_w(SORT_WIDTH)
                .label(format!("Sort by {}", melee.sort.label().to_lowercase()))
                .on_press(|app: &mut App| actions::toggle_menu(app, Menu::Sort)),
        );
    }
    Some(bar)
}

/// The Browse fighter/stage target select, shown when targets for the active
/// kind are available.
fn target_select(app: &App, p: &Palette) -> Option<El> {
    let (characters, stages) = available_targets(app);
    if characters.is_empty() && stages.is_empty() {
        return None;
    }
    let current = app.melee.target.as_deref().map(|slug| {
        app.library_page
            .browse
            .target_name(slug)
            .unwrap_or(slug)
            .to_owned()
    });
    let shown = current.unwrap_or_else(|| target_all_label(app.melee.kind).to_owned());
    Some(
        select(&shown, app.melee.menu == Some(Menu::Target), p)
            .key("skins/target")
            .min_w(TARGET_WIDTH)
            .label(format!("Filter by {shown}"))
            .on_press(|app: &mut App| actions::toggle_menu(app, Menu::Target)),
    )
}

fn target_all_label(kind: Option<Kind>) -> &'static str {
    match kind {
        Some(Kind::Fighters) => "All fighters",
        Some(Kind::Stages) => "All stages",
        _ => "Fighter or stage",
    }
}

/// Open Browse select menu (Sort or Fighter/Stage target), mounted at root.
pub fn menu(app: &App, p: &Palette) -> Option<El> {
    if app.nav.place != Place::Skins || app.melee.tab != Tab::Browse || app.editor.is_some() {
        return None;
    }
    let (anchor, items) = match app.melee.menu? {
        Menu::Sort => ("skins/sort", sort_menu(app, p)),
        Menu::Target => ("skins/target", target_menu(app, p)),
    };
    Some(select_menu(anchor, items, actions::close_menu, p))
}

fn sort_menu(app: &App, p: &Palette) -> Vec<El> {
    BrowseSort::ALL
        .into_iter()
        .map(|sort| {
            select_item(sort.label(), app.melee.sort == sort, p)
                .key(format!("skins/sort/{}", sort.api_value()))
                .on_press(move |app: &mut App| actions::set_sort(app, sort))
        })
        .collect()
}

fn target_menu(app: &App, p: &Palette) -> Vec<El> {
    let (characters, stages) = available_targets(app);
    let mut items = vec![
        select_item(
            target_all_label(app.melee.kind),
            app.melee.target.is_none(),
            p,
        )
        .key("skins/target/all")
        .on_press(|app: &mut App| actions::set_target(app, None)),
    ];
    let has_both = !characters.is_empty() && !stages.is_empty();
    if !characters.is_empty() {
        items.push(select_rule(p));
        for target in &characters {
            items.push(target_menu_item(app, target, p));
        }
    }
    if has_both || (characters.is_empty() && !stages.is_empty()) {
        items.push(select_rule(p));
    }
    for target in stages {
        items.push(target_menu_item(app, target, p));
    }
    items
}

fn target_menu_item(app: &App, target: &Target, p: &Palette) -> El {
    let slug = target.slug.clone();
    let chosen = app.melee.target.as_deref() == Some(slug.as_str());
    select_item(&target.name, chosen, p)
        .key(format!("skins/target/{slug}"))
        .on_press(move |app: &mut App| actions::set_target(app, Some(slug.clone())))
}

/// Fighter and stage targets allowed by the current kind chip.
fn available_targets(app: &App) -> (Vec<&Target>, Vec<&Target>) {
    let targets = &app.library_page.browse.targets;
    let has_counts = targets.iter().any(|target| target.pack_count > 0);
    let active = |target: &&Target| !has_counts || target.pack_count > 0;
    let characters = if matches!(app.melee.kind, None | Some(Kind::Fighters)) {
        targets
            .iter()
            .filter(|target| target.category == "character")
            .filter(active)
            .collect()
    } else {
        Vec::new()
    };
    let stages = if matches!(app.melee.kind, None | Some(Kind::Stages)) {
        targets
            .iter()
            .filter(|target| target.category == "stage")
            .filter(active)
            .collect()
    } else {
        Vec::new()
    };
    (characters, stages)
}

/// The site's skins as last read.
fn listed(app: &App) -> Option<&Vec<Pack>> {
    app.library_page
        .browse
        .items
        .as_ref()
        .and_then(|items| items.as_ref().ok())
}

/// The kinds the tab shows chips for.
fn kinds(app: &App) -> Vec<Kind> {
    match app.melee.tab {
        Tab::Browse => Kind::BROWSE.to_vec(),
        Tab::Using | Tab::Made => {
            let mut found: Vec<Kind> = Vec::new();
            for skin in local(app) {
                if let Some(kind) = skins::kind(skin)
                    && !found.contains(&kind)
                {
                    found.push(kind);
                }
            }
            Kind::ALL
                .into_iter()
                .filter(|kind| found.contains(kind))
                .collect()
        }
    }
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

/// The site's skins as plates, with pagination, retry, and filter reset.
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
    let packs = match items {
        Err(problem) => {
            return quiet(
                "skins/browse/problem",
                &format!("Couldn't reach textures.gg: {problem}"),
                Some(
                    text_action("Try again", false, p)
                        .key("skins/browse/again")
                        .on_press(site::query),
                ),
                p,
            );
        }
        Ok(packs) => packs,
    };
    if packs.is_empty() {
        let filtered = app.melee.kind.is_some()
            || app.melee.target.is_some()
            || !app.melee.search.text.trim().is_empty();
        return quiet(
            "skins/browse/empty",
            if filtered {
                "No skins match those filters."
            } else {
                "No skins found on textures.gg."
            },
            filtered.then(|| {
                text_action("Clear filters", false, p)
                    .key("skins/browse/clear")
                    .on_press(actions::clear_browse_filters)
            }),
            p,
        );
    }
    col()
        .gap(space::LG)
        .child(skins::grid(
            packs.iter().map(|pack| site_plate(app, pack, p)).collect(),
        ))
        .child(browse_footer(app, packs.len(), p))
}

fn browse_footer(app: &App, shown: usize, p: &Palette) -> El {
    let browse = &app.library_page.browse;
    let total = browse.total.max(shown);
    let summary = if total > shown {
        format!("Showing {shown} of {total} skins")
    } else if total == 1 {
        "1 skin".to_owned()
    } else {
        format!("{total} skins")
    };
    let more = browse.has_more().then(|| {
        if browse.loading_more {
            button_off("Loading…", p).key("skins/browse/more")
        } else {
            button("Load more", ButtonKind::Secondary, p)
                .key("skins/browse/more")
                .on_press(actions::load_more)
        }
    });
    let error = browse.more_error.as_ref().map(|problem| {
        row()
            .items_center()
            .gap(space::SM)
            .child(
                label(format!("Couldn't load more: {problem}"))
                    .key("skins/browse/more-error")
                    .font(text::SMALL)
                    .color(p.danger),
            )
            .child(
                text_action("Try again", false, p)
                    .key("skins/browse/more-again")
                    .on_press(actions::load_more),
            )
    });
    col().gap(space::XS).children(error).child(
        row()
            .items_center()
            .gap(space::MD)
            .child(
                label(summary)
                    .key("skins/browse/summary")
                    .font(text::SMALL)
                    .color(p.muted),
            )
            .children(more),
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
        .key(format!("skins/site/{}", pack.id))
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
            .on_edit(|app: &mut App| &mut app.melee.search)
            .on_submit(|app: &mut App, _| actions::submit_search(app)),
        p,
    )
    .w(size::SEARCH)
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
