//! What the player presses or drags: buttons, the sidebar's rows, tabs,
//! text actions, chips, and the slider.

use super::tokens::{Palette, radius, size, space, text as type_};
use rune::{El, col, el, row, text};

/// What a button is for. A quiet kind joins when a screen needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one action a place is for: an ink fill.
    Primary,
    /// Any other action: outlined.
    Secondary,
}

/// A button reading `label`.
pub fn button(label: &str, kind: ButtonKind, p: &Palette) -> El {
    let (fill, ink, border) = match kind {
        ButtonKind::Primary => (p.ink, p.on_ink, p.ink),
        ButtonKind::Secondary => (p.raise, p.ink, p.line_strong),
    };
    row().h(size::CONTROL).px(space::SM).items_center().radius(radius::CONTROL)
        .fill(fill).border(size::LINE, border).hover(p.hover).press(p.hover).cursor(rune::Cursor::Pointer)
        .role("button").label(label).focusable(true)
        .child(text(label).font(type_::STRONG).color(ink))
}

/// A square button holding a glyph, as the title row's controls are.
pub fn icon_button(glyph: El, label: &str, p: &Palette) -> El {
    el().size(size::CONTROL, size::CONTROL).radius(radius::CONTROL).center()
        .hover(p.hover).press(p.hover).cursor(rune::Cursor::Pointer).role("button").label(label).help(label).focusable(true)
        .child(glyph)
}

/// A place row's words, in from its edge.
pub const PLACE_INSET: f32 = 10.0;

/// A row in the sidebar: a place, highlighted while it shows.
pub fn place_row(label: &str, count: Option<usize>, current: bool, p: &Palette) -> El {
    let mut row = row().h(size::ROW).px(PLACE_INSET).items_center().radius(radius::CONTROL)
        .hover(p.hover).cursor(rune::Cursor::Pointer).role("button").label(label).focusable(true)
        .child(text(label).font(type_::STRONG).color(if current { p.ink } else { p.muted }))
        .child(el().grow(1.0));
    if let Some(count) = count {
        row = row.child(text(count.to_string()).font(type_::MONO_SMALL).color(p.faint));
    }
    row
}

/// A text tab: its label and count, underlined in ink while it's chosen.
pub fn tab(label: &str, count: usize, chosen: bool, p: &Palette) -> El {
    let ink = if chosen { p.ink } else { p.muted };
    col().cursor(rune::Cursor::Pointer).role("tab").label(label).focusable(true)
        .child(row().h(34.0).gap(space::SNUG).items_center()
            .child(text(label).font(type_::STRONG).color(ink))
            .child(text(count.to_string()).font(type_::MONO_SMALL).color(p.faint)))
        .child(el().h(size::LINE_STRONG).w_full().fill(if chosen { p.ink } else { rune::Color::TRANSPARENT }))
}

/// A small text action under a card or beside a row ("Install", "Remove"):
/// quiet until hovered. Its padding is `TEXT_ACTION_INSET`, which a row of
/// them pulls back to line their words up with the text above.
pub fn text_action(label: &str, danger: bool, p: &Palette) -> El {
    text(label).font(type_::LABEL).color(if danger { p.danger } else { p.muted })
        .role("button").label(label).focusable(true).hover(p.hover).cursor(rune::Cursor::Pointer)
        .px(TEXT_ACTION_INSET).py(space::TIGHT).radius(radius::CONTROL)
}

/// A text action's padding either side of its words.
pub const TEXT_ACTION_INSET: f32 = space::XXS;

/// One choice of several, as a small outlined label: filled and edged in
/// gold while it's the chosen one. The caller gives it its role
/// (`radio`), key and press.
pub fn chip(label: &str, chosen: bool, p: &Palette) -> El {
    text(label).font(type_::LABEL).color(if chosen { p.ink } else { p.muted })
        .px(space::XS).py(3.0).radius(radius::CONTROL)
        .fill(if chosen { p.selected } else { rune::Color::TRANSPARENT })
        .border(size::LINE, if chosen { p.accent_text } else { p.line })
        .hover(p.hover).cursor(rune::Cursor::Pointer).label(label).checked(chosen).focusable(true)
}

/// A slider's track at `fraction` of the way along: a filled run, a knob,
/// and the rest. The caller keys it, labels it, and gives it its range,
/// drag and steps. The knob's width is kept free at the end, so at the
/// far end it sits inside the track.
pub fn slider(fraction: f32, p: &Palette) -> El {
    const KNOB: f32 = 12.0;
    let fraction = fraction.clamp(0.0, 1.0);
    row().h(18.0).w_full().pr(KNOB).items_center().cursor(rune::Cursor::Pointer).role("slider")
        .child(el().w_frac(fraction).min_w(0.0).h(3.0).radius(1.5).fill(p.accent))
        .child(el().size(KNOB, KNOB).radius(KNOB / 2.0).fill(p.ink).border(size::LINE, p.line_strong))
        .child(el().grow(1.0).min_w(0.0).h(3.0).radius(1.5).fill(p.line_strong))
}

/// An on/off switch: a short track, gold while on, its knob gliding to the
/// side it's on. Square-cornered like every control (no pills). The caller
/// keys it (the knob's motion follows the key), labels it, and makes
/// pressing it flip.
pub fn switch(key: &str, on: bool, p: &Palette) -> El {
    const TRACK: (f32, f32) = (30.0, 16.0);
    const KNOB: f32 = 12.0;
    const INSET: f32 = (TRACK.1 - KNOB) / 2.0;
    let knob = el().key(format!("{key}/knob")).size(KNOB, KNOB).radius(radius::CONTROL / 2.0)
        .fill(if on { p.on_ink } else { p.muted }).motion(super::tokens::motion::SNAPPY)
        .absolute(if on { TRACK.0 - KNOB - INSET } else { INSET }, INSET);
    el().key(key).size(TRACK.0, TRACK.1).radius(radius::CONTROL - 1.0)
        .fill(if on { p.accent } else { p.line_strong }).cursor(rune::Cursor::Pointer)
        .role("switch").checked(on).focusable(true)
        .child(knob)
}

/// A one-line text field showing `edit`, its `placeholder` in faint ink
/// while it's empty: a search, a setting. The caller keys it and routes
/// its edits back with `.on_edit`.
pub fn text_field(edit: &rune::TextEdit, placeholder: &str, p: &Palette) -> El {
    rune::text_input(edit, type_::BODY).single_line()
        .placeholder(placeholder, p.faint).caret(p.ink, p.selected).color(p.ink)
        // Padded to a control's height around one line of body text.
        .px(space::SM).py(space::XXS + 1.0).radius(radius::CONTROL)
        .fill(p.sunk).border(size::LINE, p.line_strong).label(placeholder)
}

/// A search field: a text field whose placeholder says what it searches.
pub fn search_field(edit: &rune::TextEdit, placeholder: &str, p: &Palette) -> El {
    text_field(edit, placeholder, p)
}
