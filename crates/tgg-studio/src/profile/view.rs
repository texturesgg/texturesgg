//! You's page: who's signed in and signing out, then the skins they made and
//! the mods they're making, each with a way to more of them, or to make one.

use super::{Pack, Tab, actions};
use crate::nav::{Place, actions as nav};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    self, ButtonKind, TEXT_ACTION_INSET, button, card_plate, icon_tile, page_head, rule, spacer,
    status, tab, text_action,
};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_mod::LocalMod;

pub fn view(app: &App, p: &Palette) -> El {
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::XL)
        .children(head(app, p))
        .child(
            col()
                .gap(space::LG)
                .child(part("On this computer", p))
                .child(skins(app, p))
                .child(mods(app, p)),
        )
        .child(published(app, p));
    col()
        .key("profile/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// As the site's profile heads it: their picture and name, pronouns and
/// since when, their bio and numbers; then the profile on the site, editing
/// it there, and signing out.
fn head(app: &App, p: &Palette) -> Option<El> {
    let who = app.signin.who.as_ref()?;
    let user = app
        .profile
        .user
        .as_ref()
        .and_then(|user| user.as_ref().ok());
    let name = crate::signin::shown_name(&who.account);
    let mut facts: Vec<String> = Vec::new();
    if let Some(user) = user {
        facts.extend(
            user.pronouns
                .clone()
                .filter(|words| !words.trim().is_empty()),
        );
        facts.extend(member_since(&user.created_at));
    } else {
        facts.push(format!("@{} on textures.gg", who.account.handle));
    }
    if !who.verified {
        facts.push("The site couldn't be reached to check just now".to_owned());
    }
    let numbers = user.map(|user| {
        let stats = &user.stats;
        row()
            .key("profile/stats")
            .gap(space::MD)
            .child(number(stats.pack_count, "pack", p))
            .child(number(stats.download_count, "download", p))
            .child(number(stats.like_count, "like", p))
    });
    let bio = user
        .and_then(|user| user.bio.clone())
        .filter(|bio| !bio.trim().is_empty())
        .map(|bio| label(bio).font(text::BODY).color(p.ink).wrap());
    let handle = who.account.handle.clone();
    let picture = user
        .and_then(|user| user.image.as_ref())
        .and_then(|url| app.profile.pictures.get(url));
    Some(
        row()
            .gap(space::LG)
            .items_start()
            .child(portrait(&name, picture, p))
            .child(
                col()
                    .key("profile/name")
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(space::SM)
                    .child(page_head(&name, &facts.join(" · "), p))
                    .children(bio)
                    .children(numbers),
            )
            .child(
                row()
                    .gap(space::XS)
                    .child(
                        button("View on textures.gg", ButtonKind::Secondary, p)
                            .key("profile/view")
                            .on_press_tx(move |app: &mut App, tx| {
                                actions::open_site(app, &format!("/users/{handle}"), tx)
                            }),
                    )
                    .child(
                        button("Edit profile", ButtonKind::Secondary, p)
                            .key("profile/edit")
                            .on_press_tx(|app: &mut App, tx| {
                                actions::open_site(app, "/settings", tx)
                            }),
                    )
                    .child(
                        button("Sign out", ButtonKind::Secondary, p)
                            .key("profile/sign-out")
                            .on_press(crate::signin::actions::sign_out),
                    ),
            ),
    )
}

/// Their picture, else their initial, as large as the site shows it.
fn portrait(name: &str, picture: Option<&rune::Pixels>, p: &Palette) -> El {
    let initial: String = name
        .chars()
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_default();
    let tile = el()
        .key("profile/portrait")
        .size(size::PORTRAIT, size::PORTRAIT)
        .radius(radius::CONTROL)
        .clip_rounded(radius::CONTROL)
        .fill(p.selected)
        .border(size::LINE, p.accent_text)
        .center();
    match picture {
        Some(pixels) => tile.pixels_fit(pixels, rune::SurfaceFit::Fill),
        None => tile.child(label(initial).font(text::H2).color(p.accent_text)),
    }
}

/// "1,204 downloads" (`what`, counted), the number in mono as the site sets it.
fn number(count: u64, what: &str, p: &Palette) -> El {
    let what = if count == 1 {
        what.to_owned()
    } else {
        format!("{what}s")
    };
    row()
        .gap(space::SNUG)
        .items_end()
        .child(label(grouped(count)).font(text::MONO_TEXT).color(p.ink))
        .child(label(what).font(text::BODY).color(p.muted))
}

/// `count` with thousands set apart: 1,204.
pub(crate) fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// "Member since September 2026", from the site's timestamp.
pub(crate) fn member_since(created_at: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let year = created_at.get(..4)?;
    let month: usize = created_at.get(5..7)?.parse().ok()?;
    Some(format!(
        "Member since {} {year}",
        MONTHS.get(month.checked_sub(1)?)?
    ))
}

/// A part of the page: "On this computer", "Published".
fn part(title: &str, p: &Palette) -> El {
    label(title.to_owned()).font(text::LABEL).color(p.faint)
}

/// What they've published on textures.gg, and what they liked there, tabbed
/// as the site's profile tabs them. Code mods show once they have any.
fn published(app: &App, p: &Palette) -> El {
    let profile = &app.profile;
    let count = |list: &Option<Result<Vec<_>, String>>| {
        list.as_ref()
            .and_then(|list| list.as_ref().ok())
            .map_or(0, Vec::len)
    };
    let (packs, likes, mods) = (
        count(&profile.packs),
        count(&profile.likes),
        profile
            .mods
            .as_ref()
            .and_then(|mods| mods.as_ref().ok())
            .map_or(0, Vec::len),
    );
    let tab_for = |name: &str, count: usize, which: Tab, key: &str| {
        tab(name, count, profile.tab == which, p)
            .key(key.to_owned())
            .on_press(move |app: &mut App| actions::show_tab(app, which))
    };
    let tabs = row()
        .gap(space::LG)
        .child(tab_for("Packs", packs, Tab::Packs, "profile/tab/packs"))
        .children((mods > 0).then(|| tab_for("Mods", mods, Tab::Mods, "profile/tab/mods")))
        .child(tab_for("Likes", likes, Tab::Likes, "profile/tab/likes"));
    let shown = if profile.tab == Tab::Mods && mods == 0 {
        Tab::Packs
    } else {
        profile.tab
    };
    let body = match shown {
        Tab::Packs => pack_grid(
            app,
            &profile.packs,
            "Packs you upload to textures.gg show here.",
            p,
        ),
        Tab::Likes => pack_grid(
            app,
            &profile.likes,
            "Packs you like on textures.gg show here, to add from.",
            p,
        ),
        Tab::Mods => code_mod_list(app, p),
    };
    col()
        .key("profile/published")
        .gap(space::MD)
        .child(part("On textures.gg", p))
        .child(tabs)
        .child(body)
}

/// A list of packs: on its way, why it couldn't be read, empty, or a grid.
fn pack_grid(
    app: &App,
    list: &Option<Result<Vec<Pack>, String>>,
    empty_words: &str,
    p: &Palette,
) -> El {
    match list {
        None if app.signin.who.is_some() => quiet("Reading textures.gg…", p),
        None => quiet(empty_words, p),
        Some(Err(problem)) => problem_line(problem, p),
        Some(Ok(packs)) if packs.is_empty() => quiet(empty_words, p),
        Some(Ok(packs)) => row()
            .flex_wrap()
            .gap(space::MD)
            .children(packs.iter().map(|pack| pack_card(app, pack, p))),
    }
}

fn quiet(words: &str, p: &Palette) -> El {
    label(words.to_owned())
        .key("profile/published/note")
        .font(text::BODY)
        .color(p.muted)
        .wrap()
}

/// Why textures.gg couldn't be read, and trying again.
fn problem_line(problem: &str, p: &Palette) -> El {
    row()
        .key("profile/published/problem")
        .gap(space::XS)
        .items_center()
        .child(
            label(format!("Couldn't reach textures.gg: {problem}"))
                .font(text::BODY)
                .color(p.danger)
                .wrap()
                .shrink()
                .min_w(0.0),
        )
        .child(
            text_action("Try again", false, p)
                .key("profile/published/again")
                .on_press(actions::refresh),
        )
}

/// A pack: its picture, title, what it's for and its likes (or who made
/// it), and adding it to the library or seeing it on the site.
fn pack_card(app: &App, pack: &Pack, p: &Palette) -> El {
    let mut picture = card_plate(p);
    if let Some(pixels) = actions::thumbnail(pack).and_then(|url| app.profile.pictures.get(&url)) {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let mut about: Vec<String> = Vec::new();
    about.extend(pack.target().map(str::to_owned));
    match &pack.creator_name {
        Some(creator) => about.push(format!("by {creator}")),
        None => about.push(match pack.vote_count {
            1 => "1 like".to_owned(),
            likes => format!("{likes} likes"),
        }),
    }
    let fetching = app.profile.fetching.as_deref() == Some(pack.id.as_str());
    let add = pack.for_melee().then(|| {
        if fetching {
            text_action("Downloading…", false, p).key(format!("profile/pack/{}/add", pack.id))
        } else {
            let added = pack.clone();
            text_action("Add", false, p)
                .key(format!("profile/pack/{}/add", pack.id))
                .on_press(move |app: &mut App| actions::add_pack(app, &added))
        }
    });
    let site = actions::pack_path(pack);
    ui::card(picture, &pack.title, status(about.join(" · "), false, p), p)
        .key(format!("profile/pack/{}", pack.id))
        .child(
            row()
                .gap(space::XXS)
                .ml(-TEXT_ACTION_INSET)
                .children(add)
                .child(
                    text_action("View", false, p)
                        .key(format!("profile/pack/{}/view", pack.id))
                        .on_press_tx(move |app: &mut App, tx| actions::open_site(app, &site, tx)),
                ),
        )
}

/// Their code mods on textures.gg, each opening its page there.
fn code_mod_list(app: &App, p: &Palette) -> El {
    let mods = app
        .profile
        .mods
        .as_ref()
        .and_then(|mods| mods.as_ref().ok())
        .map(Vec::as_slice)
        .unwrap_or_default();
    col().child(rule(p)).children(mods.iter().flat_map(|m| {
        let path = format!(
            "/games/{}/code-mods/{}",
            m.game.as_deref().unwrap_or(super::MELEE),
            m.slug
        );
        let line = row()
            .key(format!("profile/code-mod/{}", m.slug))
            .py(space::XS)
            .gap(space::SM)
            .items_center()
            .hover(p.hover)
            .cursor(rune::Cursor::Pointer)
            .role("link")
            .label(m.name.clone())
            .focusable(true)
            .on_press_tx(move |app: &mut App, tx| actions::open_site(app, &path, tx))
            .child(
                col()
                    .gap(space::TIGHT)
                    .grow(1.0)
                    .min_w(0.0)
                    .child(label(m.name.clone()).font(text::STRONG).color(p.ink))
                    .children(
                        m.description
                            .clone()
                            .map(|words| label(words).font(text::SMALL).color(p.muted).truncate()),
                    ),
            );
        [line, rule(p)]
    }))
}

/// A section: its heading and count, a way to see all of them, and what's in it.
fn section(title: &str, count: usize, more: Option<El>, body: El, p: &Palette) -> El {
    col()
        .gap(space::SM)
        .child(
            row()
                .gap(space::XS)
                .items_end()
                .child(label(title.to_owned()).font(text::H3).color(p.ink))
                .child(
                    label(count.to_string())
                        .font(text::MONO_SMALL)
                        .color(p.faint),
                )
                .child(spacer())
                .children(more.map(|more| more.mr(-TEXT_ACTION_INSET))),
        )
        .child(body)
}

/// Words for an empty section, and the place to go to make one.
fn empty(key: &str, words: &str, action: &str, place: Place, p: &Palette) -> El {
    col()
        .key(key.to_owned())
        .gap(space::XS)
        .child(
            label(words.to_owned())
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        )
        .child(
            row().ml(-TEXT_ACTION_INSET).child(
                text_action(action, false, p)
                    .key(format!("{key}/go"))
                    .on_press(move |app: &mut App| nav::go(app, place)),
            ),
        )
}

/// The skins they saved from the editor, newest first.
fn skins(app: &App, p: &Palette) -> El {
    let made: Vec<_> = app
        .library
        .skins()
        .iter()
        .rev()
        .filter(|skin| crate::library::view::made(skin))
        .collect();
    let more = (!made.is_empty()).then(|| {
        text_action("Show on Skins", false, p)
            .key("profile/skins/all")
            .on_press(actions::open_skins)
    });
    let body = if made.is_empty() {
        empty(
            "profile/skins/empty",
            "Skins you edit and save show here.",
            "Pick a fighter on Skins",
            Place::Skins,
            p,
        )
    } else {
        row().flex_wrap().gap(space::MD).children(
            made.iter()
                .map(|skin| crate::library::view::card(app, skin, p)),
        )
    };
    section("Skins you made", made.len(), more, body, p).key("profile/skins")
}

/// The mods they're making: the ones that load from their own folders.
fn mods(app: &App, p: &Palette) -> El {
    let making: Vec<&LocalMod> = app.mods.local.mods.iter().filter(|m| m.dev).collect();
    let body = if making.is_empty() {
        empty(
            "profile/mods/empty",
            "Mods you start with New mod… show here while you make them.",
            "Start one on Mods",
            Place::Mods,
            p,
        )
    } else {
        col()
            .child(rule(p))
            .children(making.iter().flat_map(|m| [mod_row(app, m, p), rule(p)]))
    };
    section("Mods you're making", making.len(), None, body, p).key("profile/mods")
}

/// A mod they're making: its icon, name and version; pressing it shows it on Mods.
fn mod_row(app: &App, m: &LocalMod, p: &Palette) -> El {
    let id = m.id().clone();
    let key = format!("profile/mod/{id}");
    row()
        .key(key)
        .py(space::XS)
        .px(space::XS)
        .mx(-space::XS)
        .gap(space::SM)
        .items_center()
        .radius(radius::CONTROL)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(m.manifest.name.clone())
        .focusable(true)
        .on_press(move |app: &mut App| actions::open_mod(app, &id))
        .child(icon_tile(app.mods.icon(m.id()), size::THUMBNAIL, p))
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
                            label(m.manifest.name.clone())
                                .font(text::STRONG)
                                .color(p.ink)
                                .truncate(),
                        )
                        .child(
                            label(m.manifest.version.to_string())
                                .font(text::MONO_SMALL)
                                .color(p.faint),
                        ),
                )
                .child(status(
                    if m.enabled {
                        "A dev build · on"
                    } else {
                        "A dev build · off"
                    },
                    m.enabled,
                    p,
                )),
        )
}
