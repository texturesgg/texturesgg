//! What things sit on: plates and cards, rules, panes and their rows, a
//! page's head, and dialogs.

use super::tokens::{Palette, elevation, radius, size, space, text as type_};
use rune::{El, ScrollSpec, col, el, row, text};

/// A plate: the frame a model is shown in, on the color hsd-render clears
/// to, so a render sits in it seamlessly. Square-cornered; the caller sizes it.
pub fn plate(p: &Palette) -> El {
    el().fill(p.plate).border(size::LINE, p.line)
}

/// A card's plate: a card wide, 4:5.
pub fn card_plate(p: &Palette) -> El {
    plate(p).w(size::CARD).aspect_ratio(4.0 / 5.0)
}

/// A card: its picture (a `card_plate`), its name, and its status line
/// under that (`status`). The caller keys it, and gives it a press or
/// actions below.
pub fn card(picture: El, name: &str, status: El, p: &Palette) -> El {
    col().w(size::CARD).gap(space::SNUG).child(picture).child(
        col()
            .gap(space::TIGHT)
            .child(text(name).font(type_::STRONG).color(p.ink).truncate())
            .child(status),
    )
}

/// Room that takes what's left of a row, pushing what follows to its end.
pub fn spacer() -> El {
    el().grow(1.0)
}

/// A hairline across its container: the rule under tabs, between rows.
pub fn rule(p: &Palette) -> El {
    el().h(size::LINE).w_full().fill(p.line)
}

/// A top-level page's head: its name, and a line saying what it holds.
pub fn page_head(title: &str, line: &str, p: &Palette) -> El {
    col()
        .gap(space::SNUG)
        .child(text(title).font(type_::H1).color(p.ink))
        .child(text(line).font(type_::BODY).color(p.muted))
}

/// A pane's header: its name, a count, and a chevron that says whether it's
/// folded. The caller makes pressing it fold the pane.
pub fn pane_header(title: &str, count: Option<usize>, folded: bool, p: &Palette) -> El {
    let chevron = rune::canvas(rune::Shape::new().view_box(10.0, 10.0).stroke(
        if folded {
            [[3.5, 1.5], [7.0, 5.0], [3.5, 8.5]]
        } else {
            [[1.5, 3.5], [5.0, 7.0], [8.5, 3.5]]
        },
        1.5,
        p.faint,
    ))
    .size(10.0, 10.0);
    row()
        .h(38.0)
        .pl(space::SM)
        .pr(space::SM)
        .gap(space::XS)
        .items_center()
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(title)
        .focusable(true)
        .child(text(title).font(type_::TITLE).color(p.ink))
        .children(count.map(|count| {
            text(count.to_string())
                .font(type_::MONO_SMALL)
                .color(p.faint)
        }))
        .child(el().grow(1.0))
        .child(chevron)
}

/// A pane: a header over a body, which shows unless it's folded. Panes float
/// beside the stage, so they're the one flat surface with a shadow.
pub fn pane(header: El, body: Option<El>, p: &Palette) -> El {
    col()
        .fill(p.surface)
        .border(size::LINE, p.line_strong)
        .radius(radius::CONTROL)
        .clip_rounded(radius::CONTROL)
        .child(header)
        .children(body.map(|body| col().child(rule(p)).child(body)))
}

/// A row in a pane: what's on its left, what's on its right, highlighted
/// while it's the chosen one. The caller keys it and makes pressing choose it.
pub fn pane_row(left: El, right: Option<El>, chosen: bool, p: &Palette) -> El {
    row()
        .min_h(size::ROW + space::TIGHT)
        .px(space::SM)
        .py(space::XXS)
        .gap(space::SM)
        .items_center()
        .fill(if chosen {
            p.selected
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(p.hover)
        .child(
            el().w(size::LINE_STRONG)
                .h_full()
                .fill(if chosen {
                    p.accent
                } else {
                    rune::Color::TRANSPARENT
                })
                .absolute(0.0, 0.0),
        )
        .child(col().grow(1.0).min_w(0.0).child(left))
        .children(right)
}

/// How wide a dialog is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogWidth {
    /// A question and its answers.
    Narrow,
    /// A list to look over.
    Wide,
}

/// A dialog's parts: its head, what's in it (between rules, when there's
/// anything), and its buttons, in a row: dismissing ones first, then
/// `spacer()`, then the ones that act, the main one last.
pub struct Dialog {
    pub title: String,
    pub line: String,
    pub body: Option<El>,
    pub buttons: Vec<El>,
}

/// A dialog over a scrim, keyed `key` (the scrim `{key}/scrim`), its role
/// `dialog` or `alertdialog`. It's modal: `dismiss` runs on Escape or a
/// click outside.
pub fn dialog<S: 'static>(
    key: &str,
    role: &'static str,
    width: DialogWidth,
    parts: Dialog,
    dismiss: impl Fn(&mut S) + 'static,
    p: &Palette,
) -> El {
    let width = match width {
        DialogWidth::Narrow => size::DIALOG,
        DialogWidth::Wide => size::DIALOG_WIDE,
    };
    let (y, blur) = elevation::DIALOG;
    let mut dialog = col()
        .key(key)
        .w(width)
        .max_h(size::DIALOG_MOST_HEIGHT)
        .shrink()
        .min_h(0.0)
        .fill(p.raise)
        .border(size::LINE, p.line_strong)
        .radius(radius::CONTROL)
        .shadow(y, blur, p.shadow)
        .role(role)
        .label(parts.title.clone())
        .modal(true, dismiss)
        .child(
            col()
                .px(space::LG)
                .pt(space::LG)
                .pb(space::MD)
                .gap(space::SNUG)
                .child(text(parts.title).font(type_::H3).color(p.ink))
                .child(text(parts.line).font(type_::BODY).color(p.muted).wrap()),
        )
        .child(rule(p));
    if let Some(body) = parts.body {
        // What's in it scrolls when it's taller than the dialog may be, so
        // the head and the buttons always show.
        let body = col()
            .key(format!("{key}/body"))
            .shrink()
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(body);
        dialog = dialog.child(body).child(rule(p));
    }
    dialog = dialog.child(
        row()
            .px(space::LG)
            .py(space::MD)
            .gap(space::XS)
            .items_center()
            .children(parts.buttons),
    );
    // A column, so a dialog taller than the window shrinks to fit inside it.
    col()
        .key(format!("{key}/scrim"))
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(p.scrim)
        .center()
        .py(space::LG)
        .child(dialog)
}

/// A panel: a flat surface holding one thing on a page (the game, a set),
/// padded and outlined. The caller sizes it.
/// A row of a dialog's form: what it is and a line saying more on the
/// left, its control on the right. The caller keys it.
pub fn setting_row(name: &str, line: impl Into<String>, control: El, p: &Palette) -> El {
    let line = text(line.into()).font(type_::SMALL).color(p.muted).wrap();
    setting_row_with(name, line, control, p)
}

/// [`setting_row`] with a line of the caller's own: one saying what's wrong
/// with what's entered, in the danger color.
pub fn setting_row_with(name: &str, line: El, control: El, p: &Palette) -> El {
    row()
        .px(space::LG)
        .py(space::SM)
        .gap(space::MD)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(text(name.to_owned()).font(type_::STRONG).color(p.ink))
                .child(line),
        )
        .child(control)
}

pub fn panel(p: &Palette) -> El {
    col()
        .px(space::MD)
        .py(space::MD)
        .gap(space::SM)
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL)
}

/// A notice in a slim row: what needs the player (an update, a version to
/// go back to, a missing disc), and the one action for it at its end. The
/// caller passes the words and the action already built.
pub fn callout(words: El, action: Option<El>, p: &Palette) -> El {
    row()
        .min_h(size::CONTROL + 2.0 * space::XS)
        .px(space::SM)
        .py(space::XS)
        .gap(space::SM)
        .items_center()
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL)
        .child(col().grow(1.0).min_w(0.0).child(words))
        .children(action)
}
