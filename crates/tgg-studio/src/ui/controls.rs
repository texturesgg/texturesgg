//! What the player presses or drags: buttons, the sidebar's rows, tabs,
//! text actions, chips, fields, and the slider.

use super::icons::{Icon, icon};
use super::tokens::{Palette, radius, size, space, text as type_};
use rune::{El, col, el, row, text};

/// What a button is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one action a place or sheet is for: off-white, dark words.
    Primary,
    /// Any other action: a white wash.
    Secondary,
}

/// A pill `height` tall reading `label`.
fn pill(label: &str, kind: ButtonKind, height: f32, p: &Palette) -> El {
    let (fill, ink) = match kind {
        ButtonKind::Primary => (p.ink, p.on_ink),
        ButtonKind::Secondary => (p.fill, p.ink),
    };
    let (inset, font) = if height >= size::CONTROL_MAIN {
        (space::LG - space::XXS, type_::STRONG)
    } else if height <= size::CONTROL_SMALL {
        (space::SM, type_::LABEL)
    } else {
        (space::MD - space::TIGHT, type_::STRONG)
    };
    row()
        .h(height)
        .px(inset)
        .gap(space::XS)
        .items_center()
        .justify_center()
        .radius(radius::PILL)
        .fill(fill)
        .hover(p.hover)
        .press(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .focusable(true)
        .child(text(label).font(font).color(ink))
}

/// A button reading `label`, 30 points tall.
pub fn button(label: &str, kind: ButtonKind, p: &Palette) -> El {
    pill(label, kind, size::CONTROL, p)
}

/// A small button, 26 points tall: one in a row of a list.
#[expect(dead_code, reason = "Mods' rows use it, in the Mods pull request")]
pub fn button_small(label: &str, kind: ButtonKind, p: &Palette) -> El {
    pill(label, kind, size::CONTROL_SMALL, p)
}

/// A page's main action, 40 points tall.
#[expect(
    dead_code,
    reason = "Play's Find a match uses it, in the Play pull request"
)]
pub fn button_main(label: &str, kind: ButtonKind, p: &Palette) -> El {
    pill(label, kind, size::CONTROL_MAIN, p)
}

/// A button that can't be pressed now (while other work runs): its shape
/// and place kept, so nothing moves when it can be again, its words faint.
pub fn button_off(label: &str, p: &Palette) -> El {
    row()
        .h(size::CONTROL)
        .px(space::MD - space::TIGHT)
        .items_center()
        .radius(radius::PILL)
        .fill(p.hover)
        .role("button")
        .label(label)
        .child(text(label).font(type_::STRONG).color(p.faint))
}

/// A square button holding a glyph, as the top bar's are.
pub fn icon_button(glyph: El, label: &str, p: &Palette) -> El {
    el().size(size::CONTROL - space::TIGHT, size::CONTROL - space::TIGHT)
        .radius(radius::CONTROL)
        .center()
        .hover(p.current)
        .press(p.current)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .help(label)
        .focusable(true)
        .child(glyph)
}

/// A place row's words, in from its edge.
pub const PLACE_INSET: f32 = 10.0;

/// A row in the sidebar: a place's glyph and name, filled while it shows.
pub fn place_row(glyph: Option<El>, label: &str, current: bool, p: &Palette) -> El {
    row()
        .h(size::ROW)
        .px(PLACE_INSET)
        .gap(PLACE_INSET)
        .items_center()
        .radius(radius::CONTROL)
        .fill(if current {
            p.current
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(p.current)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .focusable(true)
        .children(glyph)
        .child(
            text(label)
                .font(type_::BODY)
                .color(if current { p.ink } else { p.muted }),
        )
}

/// A tab: its words and count, bright while it's chosen.
pub fn tab(label: &str, count: usize, chosen: bool, p: &Palette) -> El {
    let ink = if chosen { p.ink } else { p.faint };
    row()
        .h(size::TAB)
        .gap(space::SNUG)
        .items_center()
        .cursor(rune::Cursor::Pointer)
        .role("tab")
        .label(label)
        .checked(chosen)
        .focusable(true)
        .child(text(label).font(type_::TITLE).color(ink))
        .children((count > 0).then(|| {
            text(count.to_string())
                .font(type_::MONO_SMALL)
                .color(p.faint)
        }))
}

/// A small text action beside a row ("Remove", "Choose another"): quiet
/// until hovered. Its padding is `TEXT_ACTION_INSET`, which a row of them
/// pulls back to line their words up with the text above.
pub fn text_action(label: &str, danger: bool, p: &Palette) -> El {
    text(label)
        .font(type_::LABEL)
        .color(if danger { p.danger } else { p.muted })
        .role("button")
        .label(label)
        .focusable(true)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .px(TEXT_ACTION_INSET)
        .py(space::TIGHT)
        .radius(radius::CONTROL)
}

/// A text action's padding either side of its words.
pub const TEXT_ACTION_INSET: f32 = space::XXS;

/// A chip's height.
const CHIP: f32 = 22.0;

/// One choice of several, a small pill: off-white while it's the chosen
/// one. The caller gives it its role (`radio`), key and press.
pub fn chip(label: &str, chosen: bool, p: &Palette) -> El {
    row()
        .h(CHIP)
        .px(space::XS + 1.0)
        .items_center()
        .radius(radius::PILL)
        .fill(if chosen { p.ink } else { p.fill })
        .hover(if chosen { p.ink } else { p.current })
        .cursor(rune::Cursor::Pointer)
        .label(label)
        .checked(chosen)
        .focusable(true)
        .child(
            text(label)
                .font(type_::SMALL)
                .color(if chosen { p.on_ink } else { p.muted }),
        )
}

/// A slider's track at `fraction` of the way along: a filled run, a knob,
/// and the rest. The caller keys it, labels it, and gives it its range,
/// drag and steps. The knob's width is kept free at the end, so at the
/// far end it sits inside the track.
pub fn slider(fraction: f32, p: &Palette) -> El {
    const KNOB: f32 = 12.0;
    let fraction = fraction.clamp(0.0, 1.0);
    row()
        .h(18.0)
        .w_full()
        .pr(KNOB)
        .items_center()
        .cursor(rune::Cursor::Pointer)
        .role("slider")
        .child(
            el().w_frac(fraction)
                .min_w(0.0)
                .h(3.0)
                .radius(1.5)
                .fill(p.accent),
        )
        .child(el().size(KNOB, KNOB).radius(KNOB / 2.0).fill(p.ink))
        .child(
            el().grow(1.0)
                .min_w(0.0)
                .h(3.0)
                .radius(1.5)
                .fill(p.line_strong),
        )
}

/// An on/off switch: a pill track, yellow while on, its round knob on the
/// side it's on. The caller keys it, labels it, and makes pressing it flip.
pub fn switch(key: &str, on: bool, p: &Palette) -> El {
    const TRACK: (f32, f32) = (34.0, 20.0);
    const KNOB: f32 = 14.0;
    const INSET: f32 = (TRACK.1 - KNOB) / 2.0;
    let knob = el()
        .key(format!("{key}/knob"))
        .size(KNOB, KNOB)
        .radius(KNOB / 2.0)
        .fill(if on { p.on_ink } else { p.muted })
        .absolute(if on { TRACK.0 - KNOB - INSET } else { INSET }, INSET);
    el().key(key)
        .size(TRACK.0, TRACK.1)
        .radius(TRACK.1 / 2.0)
        .fill(if on { p.accent } else { p.fill })
        .cursor(rune::Cursor::Pointer)
        .role("switch")
        .checked(on)
        .focusable(true)
        .child(knob)
}

/// A one-line text field showing `edit`, its `placeholder` in faint ink
/// while it's empty. The caller keys it and routes its edits back with
/// `.on_edit`.
pub fn text_field(edit: &rune::TextEdit, placeholder: &str, p: &Palette) -> El {
    rune::text_input(edit, type_::BODY)
        .single_line()
        .placeholder(placeholder, p.faint)
        .caret(p.ink, p.selected)
        .color(p.ink)
        // Padded to a control's height around one line of body text.
        .px(space::SM)
        .py(space::SNUG + 1.0)
        .radius(radius::CONTROL + 1.0)
        .fill(p.surface)
        .border(size::LINE, p.line_strong)
        .label(placeholder)
}

/// A search, as the top bar holds it: a pill with the search glyph, its
/// placeholder saying what it searches. The caller keys it (the key goes on
/// the field) and routes its edits back with `.on_edit`.
pub fn search_field(edit: &rune::TextEdit, placeholder: &str, p: &Palette) -> El {
    rune::text_input(edit, type_::BODY)
        .single_line()
        .placeholder(placeholder, p.faint)
        .caret(p.ink, p.selected)
        .color(p.ink)
        .pl(space::XXS)
        .pr(space::SM)
        .py(space::SNUG)
        .grow(1.0)
        .min_w(0.0)
        .label(placeholder)
}

/// The pill a [`search_field`] sits in, its glyph first.
pub fn search_box(field: El, p: &Palette) -> El {
    row()
        .w(size::SEARCH)
        .h(size::CONTROL)
        .pl(space::SM)
        .items_center()
        .radius(radius::PILL)
        .fill(p.surface)
        .child(icon(Icon::Search, p.faint))
        .child(field)
}

/// A select: the value chosen and a chevron. The caller keys it (its menu
/// anchors to that key), labels it, and makes pressing it open
/// `select_menu`.
pub fn select(value: &str, open: bool, p: &Palette) -> El {
    row()
        .h(size::CONTROL)
        .min_w(SELECT_WIDTH)
        .pl(space::SM)
        .pr(space::XS)
        .gap(space::XS)
        .items_center()
        .radius(radius::CONTROL + 1.0)
        .fill(if open { p.fill } else { p.surface })
        .border(size::LINE, p.line_strong)
        .hover(p.fill)
        .cursor(rune::Cursor::Pointer)
        .role("combobox")
        .checked(open)
        .focusable(true)
        .child(
            text(value)
                .font(type_::BODY)
                .color(p.ink)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(icon(Icon::Chevron, p.muted))
}

/// A select's narrowest.
pub const SELECT_WIDTH: f32 = 200.0;

/// A select's menu: `items` (each a `select_item`, or `select_rule`) in a
/// raised list under the element keyed `anchor`, flipping above it when
/// there's no room, and scrolling when it's taller than the window allows.
/// Attach it at the window's root, after the select, so it draws over what
/// follows. It's modal: Escape or a click outside runs `dismiss`.
pub fn select_menu<S: 'static>(
    anchor: &str,
    items: Vec<El>,
    dismiss: impl Fn(&mut S) + 'static,
    p: &Palette,
) -> El {
    let (y, blur) = super::tokens::elevation::DIALOG;
    col()
        .key(format!("{anchor}/menu"))
        .min_w(SELECT_WIDTH)
        .padding(space::SNUG)
        .fill(p.raise)
        .border(size::LINE, p.line_strong)
        .radius(radius::CARD - space::XXS)
        .shadow(y / 2.0, blur / 2.0, p.shadow)
        .role("listbox")
        .modal(true, dismiss)
        .anchored_fit(anchor, rune::Side::Bottom, rune::Align::Start, space::XXS)
        .child(
            col()
                .shrink()
                .min_h(0.0)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                })
                .children(items),
        )
}

/// A choice in a select's menu: its words, with a check while it's the one
/// chosen. The caller keys it and makes pressing it choose.
pub fn select_item(label: &str, chosen: bool, p: &Palette) -> El {
    row()
        .h(size::ROW + space::XXS)
        .px(space::PAGE - space::LG + space::SNUG)
        .gap(space::XS)
        .items_center()
        .radius(radius::CONTROL)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("option")
        .checked(chosen)
        .label(label)
        .focusable(true)
        .child(
            text(label)
                .font(if chosen { type_::STRONG } else { type_::BODY })
                .color(if chosen { p.ink } else { p.muted })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
        .children(chosen.then(|| icon(Icon::Check, p.accent)))
}

/// A rule between groups of a select's choices.
pub fn select_rule(p: &Palette) -> El {
    el().h(size::LINE).w_full().my(space::XXS).fill(p.line)
}
