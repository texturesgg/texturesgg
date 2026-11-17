//! A skin's sheet: its picture, its name, who made it, and the two switches
//! that put it in tgg-melee or into the player's Melee disc. Make a skin's
//! picker: the game's fighters and stages, opening the editor on the one
//! picked. Both sit over the page.

use super::view::{fighter_plate, roster_tab, site_picture, stage_plate};
use super::{Roster, actions};
use crate::jobs::renders::RenderKey;
use crate::library::view as skins;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, dialog, spacer, switch, text_action,
};
use rune::{El, col, row, text as label};

/// The sheet's picture, a side.
const PICTURE: f32 = 260.0;

/// A skin's sheet, or Make a skin's picker, when one shows.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    if app.melee.making {
        return picker(app, p);
    }
    sheet(app, p)
}

/// One skin: what it is, what it changes, and where it can go.
fn sheet(app: &App, p: &Palette) -> Option<El> {
    let (name, maker, goes, picture) = if let Some(skin) = actions::sheet_skin(app) {
        let pixels = app.renders.get(RenderKey::Skin(skin.id));
        (
            skins::name(skin),
            skins::maker(skin).to_owned(),
            skins::goes(skin),
            pixels.cloned(),
        )
    } else {
        let pack = actions::sheet_pack(app)?;
        (
            pack.title.clone(),
            match &pack.creator_name {
                Some(creator) => format!("by {creator}"),
                None => "Someone on textures.gg".to_owned(),
            },
            pack.target_name.clone().unwrap_or_default(),
            site_picture(app, pack).cloned(),
        )
    };
    let mut plate = skins::sheet_plate(p, PICTURE);
    if let Some(pixels) = &picture {
        plate = plate.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let about = [goes, maker]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let netplay = actions::sheet_skin(app)
        .and_then(|skin| {
            let slot = skin.slot?;
            crate::library::port::verdict(app, skin.id, slot)
        })
        .and_then(|verdict| match verdict {
            tgg_mod::Verdict::Counts(_) => {
                Some("In a netplay match this shows the normal file, on your screen only.")
            }
            _ => None,
        });
    let fetching = app
        .library_page
        .browse
        .fetching
        .as_deref()
        .is_some_and(|id| actions::sheet_pack(app).is_some_and(|pack| pack.id == id));
    let use_it = actions::sheet_using(app);
    let in_slippi = actions::sheet_in_slippi(app);
    let body = row()
        .key("skins/sheet/body")
        .gap(space::LG)
        .items_start()
        .child(plate)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::SM)
                .child(
                    col()
                        .gap(space::TIGHT)
                        .child(label(name.clone()).font(text::H3).color(p.ink))
                        .child(label(about).font(text::SMALL).color(p.muted)),
                )
                .children(netplay.map(|words| label(words).font(text::SMALL).color(p.faint).wrap()))
                .children(fetching.then(|| {
                    label("Getting it from textures.gg…")
                        .font(text::SMALL)
                        .color(p.muted)
                }))
                .child(toggle(
                    "Use it",
                    "",
                    "skins/sheet/use",
                    use_it,
                    true,
                    p,
                    actions::turn_use,
                ))
                .child(toggle(
                    "Also in Slippi",
                    "Writes it into your Melee disc. Undo any time.",
                    "skins/sheet/slippi",
                    in_slippi,
                    false,
                    p,
                    actions::turn_slippi,
                ))
                .children(original(app, in_slippi, p)),
        );
    let mut buttons = Vec::new();
    if actions::sheet_skin(app).is_some() {
        buttons.push(
            button("Remove", ButtonKind::Secondary, p)
                .key("skins/sheet/remove")
                .on_press(actions::remove_skin),
        );
    } else {
        buttons.push(
            button("View on textures.gg", ButtonKind::Secondary, p)
                .key("skins/sheet/site")
                .on_press_tx(actions::open_page),
        );
    }
    buttons.push(spacer());
    buttons.push(
        button("Close", ButtonKind::Secondary, p)
            .key("skins/sheet/close")
            .on_press(actions::close_sheet),
    );
    let parts = Dialog {
        title: String::new(),
        line: String::new(),
        body: Some(body),
        buttons,
    };
    Some(dialog(
        "skins/sheet",
        "dialog",
        DialogWidth::Sheet,
        parts,
        actions::close_sheet,
        p,
    ))
}

/// The game's own file back in the slot "Also in Slippi" wrote, once it has.
fn original(app: &App, in_slippi: bool, p: &Palette) -> Option<El> {
    if !in_slippi {
        return None;
    }
    let slot = actions::sheet_skin(app)?.slot?;
    Some(
        row().ml(-TEXT_ACTION_INSET).child(
            text_action("Put the original back", false, p)
                .key("skins/sheet/original")
                .on_press(move |app: &mut App| crate::library::actions::restore(app, slot)),
        ),
    )
}

/// A row of a sheet: what it does on the left, its switch on the right.
/// `lead` is the one the sheet is for; the other is quieter.
fn toggle(
    name: &str,
    line: &str,
    key: &str,
    on: bool,
    lead: bool,
    p: &Palette,
    change: impl Fn(&mut App, bool) + 'static,
) -> El {
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(space::TIGHT)
        .child(
            label(name.to_owned())
                .font(if lead { text::STRONG } else { text::BODY })
                .color(if lead { p.ink } else { p.muted }),
        )
        .children((!line.is_empty()).then(|| {
            label(line.to_owned())
                .font(text::SMALL)
                .color(p.muted)
                .wrap()
        }));
    row()
        .key(format!("{key}/row"))
        .py(space::XS)
        .gap(space::MD)
        .items_center()
        .child(words)
        .child(
            switch(key, on, p)
                .label(name)
                .on_press(move |app: &mut App| change(app, !on)),
        )
}

/// Make a skin: the game's fighters or stages to pick from.
fn picker(app: &App, p: &Palette) -> Option<El> {
    let melee = &app.melee;
    let fighters = melee.picker == Roster::Fighters;
    let tabs = row()
        .gap(space::LG)
        .child(roster_tab(
            "Fighters",
            melee.fighters.len(),
            fighters,
            Roster::Fighters,
            p,
        ))
        .child(roster_tab(
            "Stages",
            melee.stages.len(),
            !fighters,
            Roster::Stages,
            p,
        ));
    let plates: Vec<El> = if fighters {
        melee
            .fighters
            .iter()
            .map(|fighter| fighter_plate(app, fighter, p))
            .collect()
    } else {
        melee
            .stages
            .iter()
            .map(|stage| stage_plate(app, *stage, p))
            .collect()
    };
    let body = col()
        .key("skins/picker/body")
        .px(space::LG)
        .pb(space::MD)
        .gap(space::MD)
        .child(tabs)
        .child(row().flex_wrap().gap(space::MD).children(plates));
    let parts = Dialog {
        title: "Make a skin".into(),
        line: "Pick the fighter or stage you'll change.".into(),
        body: Some(body),
        buttons: vec![
            button("Cancel", ButtonKind::Secondary, p)
                .key("skins/picker/cancel")
                .on_press(actions::close_making),
        ],
    };
    Some(dialog(
        "skins/picker",
        "dialog",
        DialogWidth::Sheet,
        parts,
        actions::close_making,
        p,
    ))
}
