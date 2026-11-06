//! The Catalog's components, built from Rune's elements and the design
//! tokens: buttons, the sidebar's rows, the title row's window controls,
//! and a page's head. Each is a function of the palette returning an
//! element; the caller adds its key and what pressing it does.

pub mod fonts;
pub mod tokens;

use rune::{El, col, el, row, text};
use tokens::{Palette, radius, size, space, text as type_};

/// What a button is for. Outlined and quiet kinds join as screens need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one action a place is for: an ink fill.
    Primary,
}

/// A button reading `label`.
pub fn button(label: &str, kind: ButtonKind, p: &Palette) -> El {
    let (fill, ink, border) = match kind {
        ButtonKind::Primary => (p.ink, p.on_ink, p.ink),
    };
    row().h(size::CONTROL).px(space::SM).items_center().radius(radius::CONTROL)
        .fill(fill).border(1.0, border).hover(p.hover).press(p.hover)
        .role("button").label(label).focusable(true)
        .child(text(label).font(type_::STRONG).color(ink))
}

/// A row in the sidebar: a place, highlighted while it shows.
pub fn place_row(label: &str, count: Option<usize>, current: bool, p: &Palette) -> El {
    let mut row = row().h(size::ROW).px(10.0).items_center().radius(radius::CONTROL)
        .hover(p.hover).role("button").label(label).focusable(true)
        .child(text(label).font(type_::STRONG).color(if current { p.ink } else { p.muted }))
        .child(el().grow(1.0));
    if let Some(count) = count {
        row = row.child(text(count.to_string()).font(type_::MONO_SMALL).color(p.faint));
    }
    row
}

/// The sidebar toggle's glyph: a window with its sidebar drawn in.
pub fn sidebar_glyph(p: &Palette) -> El {
    el().size(16.0, 13.0).radius(2.5).border(1.5, p.muted)
        .child(el().size(1.5, 13.0).fill(p.muted).absolute(5.0, 0.0))
}

/// The close glyph: two strokes crossed.
pub fn close_glyph(p: &Palette) -> El {
    let stroke = |turn: f32| el().size(13.0, 1.5).radius(0.75).fill(p.muted).rotate(turn).absolute(0.5, 5.75);
    el().size(14.0, 13.0).child(stroke(45.0)).child(stroke(-45.0))
}

/// A square button holding a glyph, as the title row's controls are.
pub fn icon_button(glyph: El, label: &str, p: &Palette) -> El {
    el().size(size::CONTROL, size::CONTROL).radius(radius::CONTROL).center()
        .hover(p.hover).press(p.hover).role("button").label(label).help(label).focusable(true)
        .child(glyph)
}

/// A top-level page's head: its name, and a line saying what it holds.
pub fn page_head(title: &str, line: &str, p: &Palette) -> El {
    col().gap(6.0)
        .child(text(title).font(type_::H1).color(p.ink))
        .child(text(line).font(type_::BODY).color(p.muted))
}
