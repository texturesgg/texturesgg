//! The Colors pane: each group's swatches, the selected texture's groups
//! when it has any, and the sliders for the color being changed.

use super::colors::{self, ColorPick};
use super::Document;
use crate::state::App;
use crate::ui::tokens::{Palette, radius, space, text};
use crate::ui::{ButtonKind, button, pane, pane_header, slider, srgb, text_action};
use rune::{Drag, El, ScrollSpec, Tx, col, el, row, text as label};
use tgg_app::colors::{Change, ColorGroup, Source, hex};

/// The most swatches a group shows; the rest are reached by a hue shift.
const SHOWN: usize = 24;
/// A swatch's side, in points.
const SWATCH: f32 = 20.0;
/// The pane's tallest list, in points.
const LIST_HEIGHT: f32 = 300.0;

/// The pane, when the costume has colors outside its textures.
pub fn pane_view(document: &Document, p: &Palette) -> Option<El> {
    if document.colors.is_empty() {
        return None;
    }
    let shown = shown_groups(document);
    let count: usize = shown.iter().map(|&group| document.colors[group].swatches.len()).sum();
    let body = col().key("editor/colors").max_h(LIST_HEIGHT).scroll(ScrollSpec { stick_bottom: false })
        .px(space::SM).py(space::SM).gap(space::SM)
        .children(shown.into_iter().map(|group| group_view(document, group, p)));
    Some(pane(pane_header("Colors", Some(count), false, p).key("editor/pane/colors"), Some(body), p))
}

/// The groups to list: the selected texture's, or every one when it has
/// none (or nothing is selected).
fn shown_groups(document: &Document) -> Vec<usize> {
    let of_selected: Vec<usize> = document
        .selected
        .map(|selected| (0..document.colors.len()).filter(|&group| document.colors[group].textures.contains(&selected)).collect())
        .unwrap_or_default();
    if of_selected.is_empty() { (0..document.colors.len()).collect() } else { of_selected }
}

fn group_view(document: &Document, index: usize, p: &Palette) -> El {
    let group = &document.colors[index];
    let pick = document.color_pick.filter(|pick| pick.group == index);
    let picked_swatch = pick.and_then(|pick| match pick.change {
        Change::Swatch { index, .. } => Some(index),
        Change::Hue(_) => None,
    });
    let swatches = group.swatches.iter().enumerate().take(SHOWN).map(|(swatch_index, swatch)| {
        el().key(format!("editor/color/{index}/{swatch_index}")).size(SWATCH, SWATCH).radius(radius::CONTROL / 2.0)
            .fill(srgb(swatch.rgba)).border(if picked_swatch == Some(swatch_index) { 2.0 } else { 1.0 }, if picked_swatch == Some(swatch_index) { p.accent_text } else { p.line_strong })
            .cursor(rune::Cursor::Pointer).role("button").label(hex(swatch.rgba)).help(hex(swatch.rgba)).focusable(true)
            .on_press_tx(move |app: &mut App, tx| colors::pick_swatch(app, index, swatch_index, tx))
    });
    let hidden = group.swatches.len().saturating_sub(SHOWN);
    let head = row().gap(space::XS).items_center()
        .child(label(group.title.clone()).font(text::LABEL).color(p.ink).truncate().grow(1.0).min_w(0.0))
        .children(group.shifts_hue().then(|| {
            text_action("Shift hue", false, p).key(format!("editor/hue/{index}")).on_press_tx(move |app: &mut App, tx| colors::pick_hue(app, index, tx))
        }));
    col().gap(space::XXS)
        .child(head)
        .child(row().flex_wrap().gap(space::XXS).items_center()
            .children(swatches)
            .children((hidden > 0).then(|| label(format!("+{hidden}")).font(text::SMALL).color(p.muted))))
        .children(pick.map(|pick| pick_view(group, pick, p)))
}

/// The controls for the color being changed, under its group.
fn pick_view(group: &ColorGroup, pick: ColorPick, p: &Palette) -> El {
    let mut controls = col().key("editor/pick").gap(space::XXS).px(space::XS).py(space::XS).radius(radius::CONTROL).fill(p.raise);
    match pick.change {
        Change::Swatch { index, rgba } => {
            let Some(swatch) = group.swatches.get(index) else { return el() };
            let chip = |rgba: [u8; 4]| el().size(28.0, SWATCH).radius(radius::CONTROL / 2.0).fill(srgb(rgba)).border(1.0, p.line_strong);
            let uses = match &swatch.source {
                Source::Vertices(_) => format!("{} vertices", swatch.uses),
                Source::Diffuse(_) => "material".to_owned(),
            };
            controls = controls.child(row().gap(space::XS).items_center()
                .child(chip(swatch.rgba))
                .child(chip(rgba))
                .child(label(hex(rgba)).font(text::MONO_SMALL).color(p.ink))
                .child(label(uses).font(text::SMALL).color(p.muted)));
            let channels: &[&str] = if swatch.alpha { &["R", "G", "B", "A"] } else { &["R", "G", "B"] };
            for (channel, &name) in channels.iter().enumerate() {
                let value = rgba[channel];
                let track = slider(f32::from(value) / 255.0, p).key(format!("editor/pick/{name}")).label(name)
                    .range_value(f64::from(value), 0.0, 255.0).range_step(1.0)
                    .on_drag(move |app: &mut App, drag: &mut Drag, _: &mut Tx| {
                        colors::set_channel(app, channel, drag.x / drag.w.max(1.0) * 255.0);
                    })
                    .on_step(move |app: &mut App, steps: i32, _: &mut Tx| {
                        colors::set_channel(app, channel, f32::from(value) + steps as f32);
                    });
                controls = controls.child(slider_row(name, track, value.to_string(), p));
            }
        }
        Change::Hue(degrees) => {
            let track = slider((degrees + 180.0) / 360.0, p).key("editor/pick/hue").label("Hue")
                .range_value(f64::from(degrees), -180.0, 180.0).range_step(1.0)
                .on_drag(|app: &mut App, drag: &mut Drag, _: &mut Tx| colors::set_hue(app, drag.x / drag.w.max(1.0) * 360.0 - 180.0))
                .on_step(move |app: &mut App, steps: i32, _: &mut Tx| colors::set_hue(app, degrees + steps as f32));
            controls = controls
                .child(label(format!("Turns all {} colors around the hue circle", group.swatches.len())).font(text::SMALL).color(p.muted).wrap())
                .child(slider_row("Hue", track, format!("{degrees:+.0}°"), p));
        }
    }
    controls.child(row().gap(space::XS).pt(space::XXS)
        .child(el().grow(1.0))
        .child(button("Cancel", ButtonKind::Secondary, p).key("editor/pick/cancel").on_press(colors::cancel))
        .child(button("Apply", ButtonKind::Primary, p).key("editor/pick/apply").on_press(colors::apply)))
}

/// A slider with its name before it and its value after.
fn slider_row(name: &str, track: El, value: String, p: &Palette) -> El {
    row().gap(space::XS).items_center()
        .child(label(name.to_owned()).font(text::MONO_SMALL).color(p.muted).w(28.0))
        .child(col().grow(1.0).min_w(0.0).child(track))
        .child(label(value).font(text::MONO_SMALL).color(p.muted).w(36.0))
}
