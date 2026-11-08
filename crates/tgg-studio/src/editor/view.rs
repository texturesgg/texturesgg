//! The editor's screen: the stage with the textures beside it, the title
//! row's breadcrumbs, saved state and saves, and the unsaved-changes prompt.

use super::{Document, actions, colors_view, external, files};
use crate::keys::{Command, hint};
use crate::stage::view as stage_view;
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{ButtonKind, button, pane, pane_header, pane_row, plate, rule};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_app::textures::texture_detail;

/// The selected texture's preview, in points a side at most.
const PREVIEW: f32 = 240.0;

pub fn view(app: &App, document: &Document, p: &Palette) -> El {
    let matched = app.melee.subject.map(|subject| subject.plate_id()).unwrap_or_default();
    let stage = col().grow(1.0).min_w(0.0).h_full().gap(space::SM)
        .child(stage_view::plate(app, &matched, p))
        .child(stage_view::timeline(app, p));
    let panes = col().key("editor/panes").w(size::PANE_COLUMN).h_full().gap(space::SM)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(textures(document, p))
        .children(texture(document, p))
        .children(colors_view::pane_view(document, p));
    row().key("editor").grow(1.0).min_h(0.0).w_full().px(space::MD).pb(space::MD).gap(space::MD)
        .child(stage)
        .child(panes)
}

/// The title row while editing: where the costume is from, and whether
/// it's saved.
pub fn crumbs(document: &Document, p: &Palette) -> El {
    let fighter = document.slot.character().map_or_else(|| tgg_app::costumes::slot_label(Some(document.slot)), |character| character.name().to_owned());
    let color = document.slot.color().map(|color| color.name());
    let mut crumbs = row().gap(space::XS).items_center()
        .child(label("Melee").font(text::STRONG).color(p.muted))
        .child(label("/").font(text::BODY).color(p.faint))
        .child(label(fighter).font(text::STRONG).color(if color.is_some() { p.muted } else { p.ink }));
    if let Some(color) = color {
        crumbs = crumbs.child(label("/").font(text::BODY).color(p.faint)).child(label(color).font(text::STRONG).color(p.ink));
    }
    if document.modified() {
        crumbs = crumbs.child(row().key("editor/edited").ml(space::XS).gap(6.0).items_center()
            .child(el().size(6.0, 6.0).radius(3.0).fill(p.accent))
            .child(label("Edited").font(text::LABEL).color(p.accent_text)));
    }
    crumbs
}

/// Done, Save, and Save and install.
pub fn actions(document: &Document, p: &Palette) -> El {
    let saved = !document.modified();
    row().gap(space::XS)
        .child(button("Done", ButtonKind::Secondary, p).key("editor/done").help(hint("Back to the fighter", Command::Back))
            .on_press_tx(actions::leave))
        .child(button("Save", ButtonKind::Secondary, p).key("editor/save").opacity(if saved { 0.5 } else { 1.0 })
            .help(hint("Keep it in your library", Command::Save))
            .on_press_tx(|app: &mut App, tx| actions::save(app, false, None, tx)))
        .child(button("Save and install", ButtonKind::Primary, p).key("editor/install")
            .help(hint("Keep it in your library and write it into its slot", Command::SaveAndInstall))
            .on_press_tx(|app: &mut App, tx| actions::save(app, true, None, tx)))
}

/// Every texture, a row each: its picture, name and what it is.
fn textures(document: &Document, p: &Palette) -> El {
    let Ok(open) = &document.listing.document else { return el() };
    let rows = open.textures().iter().enumerate().map(|(index, texture)| {
        let mut thumb = plate(p).size(32.0, 32.0);
        if let Some(pixels) = document.thumbnails.get(index) {
            thumb = thumb.pixels_fit(pixels, rune::SurfaceFit::Fit);
        }
        let left = row().gap(space::SM).items_center()
            .child(thumb)
            .child(col().min_w(0.0)
                .child(label(document.listing.name(index)).font(text::STRONG).color(p.ink).truncate())
                .child(label(texture_detail(texture)).font(text::MONO_SMALL).color(p.faint).truncate()));
        pane_row(left, None, document.selected == Some(index), p)
            .key(format!("editor/texture/{index}"))
            .role("button").label(document.listing.name(index)).focusable(true).cursor(rune::Cursor::Pointer)
            .on_press(move |app: &mut App| actions::select(app, Some(index)))
    });
    let header = pane_header("Textures", Some(open.textures().len()), false, p).key("editor/pane/textures");
    pane(header, Some(col().key("editor/textures").max_h(360.0).scroll(ScrollSpec { stick_bottom: false }).children(rows)), p)
}

/// The selected texture: what it is and what can be done with it, then the
/// picture large, so the actions stay in view on a short window.
fn texture(document: &Document, p: &Palette) -> Option<El> {
    let index = document.selected?;
    let Ok(open) = &document.listing.document else { return None };
    let entry = open.textures().get(index)?;
    let (width, height) = (f32::from(entry.width), f32::from(entry.height));
    let scale = (PREVIEW / width.max(height)).min(8.0);
    let mut picture = plate(p).size(width * scale, height * scale);
    if let Some(pixels) = document.thumbnails.get(index) {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fit);
    }
    let body = col().px(space::SM).py(space::SM).gap(space::SM)
        .child(col().gap(2.0)
            .child(label(document.listing.name(index)).font(text::STRONG).color(p.ink))
            .child(label(texture_detail(entry)).font(text::MONO_SMALL).color(p.faint)))
        .child(row().gap(space::XS).flex_wrap()
            .child(button("Edit in your image editor", ButtonKind::Secondary, p).key("editor/external")
                .help(hint("Opens it there; each save there updates it here", Command::EditExternally))
                .on_press(external::edit))
            .child(button("Replace with a PNG…", ButtonKind::Secondary, p).key("editor/import").on_press_tx(files::choose_import))
            .child(button("Export…", ButtonKind::Secondary, p).key("editor/export").on_press_tx(files::choose_export)))
        .child(row().w_full().justify_center().child(picture));
    Some(pane(pane_header("Texture", None, false, p).key("editor/pane/texture"), Some(body), p))
}

/// Asks before unsaved edits go: save them, don't, or stay.
pub fn prompt(app: &App, p: &Palette) -> Option<El> {
    let document = app.editor.as_ref()?;
    document.pending?;
    let place = tgg_app::costumes::slot_label(Some(document.slot));
    let dialog = col().key("editor/prompt").w(440.0).fill(p.raise).border(1.0, p.line_strong).radius(radius::CONTROL)
        .shadow(24.0, 60.0, rune::Color::BLACK.alpha(0.55))
        .role("alertdialog").label("Unsaved changes")
        .modal(true, actions::answer_cancel)
        .child(col().px(space::LG).pt(space::LG).pb(space::MD).gap(6.0)
            .child(label("Save your changes?").font(text::H3).color(p.ink))
            .child(label(format!("Your edits to {place} go to your library as a skin. Without saving, they're lost.")).font(text::BODY).color(p.muted).wrap()))
        .child(rule(p))
        .child(row().px(space::LG).py(space::MD).gap(space::XS).items_center()
            .child(button("Cancel", ButtonKind::Secondary, p).key("prompt/cancel").on_press(actions::answer_cancel))
            .child(el().grow(1.0))
            .child(button("Don't save", ButtonKind::Secondary, p).key("prompt/discard").on_press_tx(actions::answer_discard))
            .child(button("Save", ButtonKind::Primary, p).key("prompt/save").on_press_tx(actions::answer_save)));
    Some(el().key("prompt/scrim").absolute(0.0, 0.0).w_full().h_full().fill(rune::Color::BLACK.alpha(0.6)).center().child(dialog))
}
