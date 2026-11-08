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
    row()
        .h(size::CONTROL)
        .px(space::SM)
        .items_center()
        .radius(radius::CONTROL)
        .fill(fill)
        .border(size::LINE, border)
        .hover(p.hover)
        .press(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .focusable(true)
        .child(text(label).font(type_::STRONG).color(ink))
}

/// A button that can't be pressed now (while other work runs): its shape
/// and place kept, so nothing moves when it can be again, its words faint.
pub fn button_off(label: &str, p: &Palette) -> El {
    row()
        .h(size::CONTROL)
        .px(space::SM)
        .items_center()
        .radius(radius::CONTROL)
        .border(size::LINE, p.line)
        .role("button")
        .label(label)
        .child(text(label).font(type_::STRONG).color(p.faint))
}

/// A square button holding a glyph, as the title row's controls are.
pub fn icon_button(glyph: El, label: &str, p: &Palette) -> El {
    el().size(size::CONTROL, size::CONTROL)
        .radius(radius::CONTROL)
        .center()
        .hover(p.hover)
        .press(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .help(label)
        .focusable(true)
        .child(glyph)
}

/// A place row's words, in from its edge.
pub const PLACE_INSET: f32 = 10.0;

/// A row in the sidebar: a place, highlighted while it shows.
pub fn place_row(label: &str, count: Option<usize>, current: bool, p: &Palette) -> El {
    let mut row = row()
        .h(size::ROW)
        .px(PLACE_INSET)
        .items_center()
        .radius(radius::CONTROL)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(label)
        .focusable(true)
        .child(
            text(label)
                .font(type_::STRONG)
                .color(if current { p.ink } else { p.muted }),
        )
        .child(el().grow(1.0));
    if let Some(count) = count {
        row = row.child(
            text(count.to_string())
                .font(type_::MONO_SMALL)
                .color(p.faint),
        );
    }
    row
}

/// A text tab: its label and count, underlined in ink while it's chosen.
pub fn tab(label: &str, count: usize, chosen: bool, p: &Palette) -> El {
    let ink = if chosen { p.ink } else { p.muted };
    col()
        .cursor(rune::Cursor::Pointer)
        .role("tab")
        .label(label)
        .focusable(true)
        .child(
            row()
                .h(size::TAB)
                .gap(space::SNUG)
                .items_center()
                .child(text(label).font(type_::STRONG).color(ink))
                .child(
                    text(count.to_string())
                        .font(type_::MONO_SMALL)
                        .color(p.faint),
                ),
        )
        .child(el().h(size::LINE_STRONG).w_full().fill(if chosen {
            p.ink
        } else {
            rune::Color::TRANSPARENT
        }))
}

/// A small text action under a card or beside a row ("Install", "Remove"):
/// quiet until hovered. Its padding is `TEXT_ACTION_INSET`, which a row of
/// them pulls back to line their words up with the text above.
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

/// Starting a game, beside the name of the game it starts: the play glyph
/// and "Play", in ink, padded like a text action.
pub fn play_action(label: &str, p: &Palette) -> El {
    row()
        .gap(space::XXS)
        .items_center()
        .role("button")
        .label(label)
        .focusable(true)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .px(TEXT_ACTION_INSET)
        .py(space::TIGHT)
        .radius(radius::CONTROL)
        .child(super::play_glyph(p.ink))
        .child(text("Play").font(type_::LABEL).color(p.ink))
}

/// A text action's padding either side of its words.
pub const TEXT_ACTION_INSET: f32 = space::XXS;

/// One choice of several, as a small outlined label: filled and edged in
/// gold while it's the chosen one. The caller gives it its role
/// (`radio`), key and press.
pub fn chip(label: &str, chosen: bool, p: &Palette) -> El {
    text(label)
        .font(type_::LABEL)
        .color(if chosen { p.ink } else { p.muted })
        .px(space::XS)
        .py(3.0)
        .radius(radius::CONTROL)
        .fill(if chosen {
            p.selected
        } else {
            rune::Color::TRANSPARENT
        })
        .border(size::LINE, if chosen { p.accent_text } else { p.line })
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .label(label)
        .checked(chosen)
        .focusable(true)
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
        .child(
            el().size(KNOB, KNOB)
                .radius(KNOB / 2.0)
                .fill(p.ink)
                .border(size::LINE, p.line_strong),
        )
        .child(
            el().grow(1.0)
                .min_w(0.0)
                .h(3.0)
                .radius(1.5)
                .fill(p.line_strong),
        )
}

/// An on/off switch: a short track, gold while on, its knob gliding to the
/// side it's on. Square-cornered like every control (no pills). The caller
/// keys it (the knob's motion follows the key), labels it, and makes
/// pressing it flip.
pub fn switch(key: &str, on: bool, p: &Palette) -> El {
    const TRACK: (f32, f32) = (30.0, 16.0);
    const KNOB: f32 = 12.0;
    const INSET: f32 = (TRACK.1 - KNOB) / 2.0;
    let knob = el()
        .key(format!("{key}/knob"))
        .size(KNOB, KNOB)
        .radius(radius::CONTROL / 2.0)
        .fill(if on { p.on_ink } else { p.muted })
        .motion(super::tokens::motion::SNAPPY)
        .absolute(if on { TRACK.0 - KNOB - INSET } else { INSET }, INSET);
    el().key(key)
        .size(TRACK.0, TRACK.1)
        .radius(radius::CONTROL - 1.0)
        .fill(if on { p.accent } else { p.line_strong })
        .cursor(rune::Cursor::Pointer)
        .role("switch")
        .checked(on)
        .focusable(true)
        .child(knob)
}

/// A one-line text field showing `edit`, its `placeholder` in faint ink
/// while it's empty: a search, a setting. The caller keys it and routes
/// its edits back with `.on_edit`.
pub fn text_field(edit: &rune::TextEdit, placeholder: &str, p: &Palette) -> El {
    rune::text_input(edit, type_::BODY)
        .single_line()
        .placeholder(placeholder, p.faint)
        .caret(p.ink, p.selected)
        .color(p.ink)
        // Padded to a control's height around one line of body text.
        .px(space::SM)
        .py(space::XXS + 1.0)
        .radius(radius::CONTROL)
        .fill(p.sunk)
        .border(size::LINE, p.line_strong)
        .label(placeholder)
}

/// A search field: a text field whose placeholder says what it searches.
pub fn search_field(edit: &rune::TextEdit, placeholder: &str, p: &Palette) -> El {
    text_field(edit, placeholder, p)
}

/// A select: the value chosen and a chevron, outlined as a control. The
/// caller keys it (its menu anchors to that key), labels it, and makes
/// pressing it open `select_menu`.
pub fn select(value: &str, open: bool, p: &Palette) -> El {
    let chevron = rune::canvas(rune::Shape::new().view_box(10.0, 10.0).stroke(
        [[1.5, 3.5], [5.0, 7.0], [8.5, 3.5]],
        1.5,
        p.muted,
    ))
    .size(10.0, 10.0);
    row()
        .h(size::CONTROL)
        .min_w(SELECT_WIDTH)
        .px(space::SM)
        .gap(space::XS)
        .items_center()
        .radius(radius::CONTROL)
        .fill(if open { p.hover } else { p.sunk })
        .border(size::LINE, if open { p.accent_text } else { p.line_strong })
        .hover(p.hover)
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
        .child(chevron)
}

/// A select's narrowest.
pub const SELECT_WIDTH: f32 = 200.0;

/// A select's menu: `items` (each a `select_item`, or `select_rule`) in a
/// raised list under the select keyed `anchor`, flipping above it when
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
        .py(space::XXS)
        .fill(p.raise)
        .border(size::LINE, p.line_strong)
        .radius(radius::CONTROL)
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

/// A choice in a select's menu: its words, with a gold mark while it's the
/// one chosen. The caller keys it and makes pressing it choose.
pub fn select_item(label: &str, chosen: bool, p: &Palette) -> El {
    let mark = el()
        .size(size::DOT, size::DOT)
        .radius(size::DOT / 2.0)
        .fill(if chosen {
            p.accent
        } else {
            rune::Color::TRANSPARENT
        });
    row()
        .h(size::ROW)
        .px(space::SM)
        .gap(space::XS)
        .items_center()
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("option")
        .checked(chosen)
        .label(label)
        .focusable(true)
        .child(mark)
        .child(
            text(label)
                .font(type_::BODY)
                .color(if chosen { p.ink } else { p.muted })
                .truncate(),
        )
}

/// A rule between groups of a select's choices.
pub fn select_rule(p: &Palette) -> El {
    el().h(size::LINE).w_full().my(space::XXS).fill(p.line)
}
