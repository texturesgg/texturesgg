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

/// A text tab: its label and count, underlined in ink while it's chosen.
pub fn tab(label: &str, count: usize, chosen: bool, p: &Palette) -> El {
    let ink = if chosen { p.ink } else { p.muted };
    col().role("tab").label(label).focusable(true)
        .child(row().h(34.0).gap(6.0).items_center()
            .child(text(label).font(type_::STRONG).color(ink))
            .child(text(count.to_string()).font(type_::MONO_SMALL).color(p.faint)))
        .child(el().h(2.0).w_full().fill(if chosen { p.ink } else { rune::Color::TRANSPARENT }))
}

/// A plate: the frame a model is shown in, on the color hsd-render clears
/// to, so a render sits in it seamlessly. Square-cornered; the caller sizes it.
pub fn plate(p: &Palette) -> El {
    el().fill(p.plate).border(1.0, p.line)
}

/// A hairline across its container: the rule under tabs, between rows.
pub fn rule(p: &Palette) -> El {
    el().h(1.0).w_full().fill(p.line)
}

/// The play glyph: a triangle pointing on, 10 × 11 points.
pub fn play_glyph(ink: rune::Color) -> El {
    rune::canvas(rune::Shape::new().view_box(10.0, 11.0).fill([[1.0, 0.0], [10.0, 5.5], [1.0, 11.0]], ink)).size(10.0, 11.0)
}

/// The pause glyph: two bars, 9 × 11 points.
pub fn pause_glyph(ink: rune::Color) -> El {
    row().gap(3.0).child(el().size(3.0, 11.0).fill(ink)).child(el().size(3.0, 11.0).fill(ink))
}

/// A pane's header: its name, a count, and a chevron that says whether it's
/// folded. The caller makes pressing it fold the pane.
pub fn pane_header(title: &str, count: Option<usize>, folded: bool, p: &Palette) -> El {
    let chevron = rune::canvas(rune::Shape::new().view_box(10.0, 10.0)
        .stroke(if folded { [[3.5, 1.5], [7.0, 5.0], [3.5, 8.5]] } else { [[1.5, 3.5], [5.0, 7.0], [8.5, 3.5]] }, 1.5, p.faint))
        .size(10.0, 10.0);
    row().h(38.0).pl(space::SM).pr(space::SM).gap(space::XS).items_center().hover(p.hover)
        .role("button").label(title).focusable(true)
        .child(text(title).font(type_::TITLE).color(p.ink))
        .children(count.map(|count| text(count.to_string()).font(type_::MONO_SMALL).color(p.faint)))
        .child(el().grow(1.0))
        .child(chevron)
}

/// A pane: a header over a body, which shows unless it's folded. Panes float
/// beside the stage, so they're the one flat surface with a shadow.
pub fn pane(header: El, body: Option<El>, p: &Palette) -> El {
    col().fill(p.surface).border(1.0, p.line_strong).radius(radius::CONTROL).clip_rounded(radius::CONTROL)
        .child(header)
        .children(body.map(|body| col().child(rule(p)).child(body)))
}

/// A row in a pane: what's on its left, what's on its right, highlighted
/// while it's the chosen one. The caller keys it and makes pressing choose it.
pub fn pane_row(left: El, right: Option<El>, chosen: bool, p: &Palette) -> El {
    row().h(size::ROW + 2.0).px(space::SM).gap(space::SM).items_center()
        .fill(if chosen { p.selected } else { rune::Color::TRANSPARENT }).hover(p.hover)
        .child(el().w(2.0).h(size::ROW + 2.0).fill(if chosen { p.accent } else { rune::Color::TRANSPARENT }).absolute(0.0, 0.0))
        .child(col().grow(1.0).min_w(0.0).child(left))
        .children(right)
}
