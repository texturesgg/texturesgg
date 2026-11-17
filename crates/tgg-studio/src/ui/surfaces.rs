//! What things sit on: cards and plates, list rows, section labels, panes,
//! a page's title, and sheets (dialogs).

use super::tokens::{Palette, elevation, motion, radius, size, space, text as type_};
use rune::{El, ScrollSpec, col, el, row, text};

/// A plate: the frame a model is shown in, on the color hsd-render clears
/// to, so a render sits in it seamlessly. The caller sizes it.
pub fn plate(p: &Palette) -> El {
    el().fill(p.plate)
}

/// A card's plate: a card wide, 4:5, its corners rounded.
pub fn card_plate(p: &Palette) -> El {
    plate(p)
        .w(size::CARD)
        .aspect_ratio(4.0 / 5.0)
        .radius(radius::CARD)
        .clip_rounded(radius::CARD)
}

/// A picture card: its picture (a `card_plate`), its name, and its status
/// line under that (`status`). The caller keys it, and gives it a press or
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

/// A hairline across its container, where one has to show.
pub fn rule(p: &Palette) -> El {
    el().h(size::LINE).w_full().fill(p.line)
}

/// A page's one big title, and a line under it when there's one.
pub fn page_head(title: &str, line: &str, p: &Palette) -> El {
    col()
        .gap(space::XXS)
        .child(text(title).font(type_::H1).color(p.ink))
        .children((!line.is_empty()).then(|| text(line).font(type_::BODY).color(p.muted).wrap()))
}

/// A section's label, over its group: muted, small.
pub fn section(label: &str, p: &Palette) -> El {
    text(label)
        .font(type_::LABEL)
        .color(p.faint)
        .pb(space::XS + space::TIGHT)
}

/// A row in a list: padded, rounded, washed while hovered. The caller
/// fills it, keys it, and makes pressing it do something.
pub fn list_row(p: &Palette) -> El {
    row()
        .px(space::SM)
        .py(space::ROW_Y)
        .gap(space::SM)
        .items_center()
        .radius(radius::CARD - space::TIGHT)
        .hover(p.hover)
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
        size::ICON_STROKE,
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
        .child(text(title).font(type_::STRONG).color(p.ink))
        .children(count.map(|count| {
            text(count.to_string())
                .font(type_::MONO_SMALL)
                .color(p.faint)
        }))
        .child(el().grow(1.0))
        .child(chevron)
}

/// A pane: a header over a body, which shows unless it's folded. Panes
/// float beside the stage, so they're opaque.
pub fn pane(header: El, body: Option<El>, p: &Palette) -> El {
    col()
        .fill(p.raise)
        .border(size::LINE, p.line)
        .radius(radius::CARD)
        .clip_rounded(radius::CARD)
        .child(header)
        .children(body.map(|body| col().child(rule(p)).child(body)))
}

/// A row in a pane: what's on its left, what's on its right, filled while
/// it's the chosen one. The caller keys it and makes pressing choose it.
pub fn pane_row(left: El, right: Option<El>, chosen: bool, p: &Palette) -> El {
    row()
        .min_h(size::ROW + space::TIGHT)
        .px(space::SM)
        .py(space::XXS)
        .gap(space::SM)
        .items_center()
        .fill(if chosen {
            p.current
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(p.hover)
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
    /// A skin's sheet: its picture beside what it does.
    Sheet,
}

/// A dialog's parts: its head, what's in it, and its buttons, in a row:
/// dismissing ones first, then `spacer()`, then the ones that act, the main
/// one last.
pub struct Dialog {
    pub title: String,
    pub line: String,
    pub body: Option<El>,
    pub buttons: Vec<El>,
}

/// A sheet over a scrim, keyed `key` (the scrim `{key}/scrim`), its role
/// `dialog` or `alertdialog`. It fades and rises in. It's modal: `dismiss`
/// runs on Escape or a click outside.
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
        DialogWidth::Sheet => size::SHEET,
    };
    let (y, blur) = elevation::DIALOG;
    let mut dialog = col()
        .key(key)
        .w(width)
        .max_h(size::DIALOG_MOST_HEIGHT)
        .shrink()
        .min_h(0.0)
        .fill(p.raise)
        .border(size::LINE, p.line)
        .radius(radius::CARD + space::XXS)
        .clip_rounded(radius::CARD + space::XXS)
        .shadow(y, blur, p.shadow)
        .transition(motion::enter())
        .role(role)
        .label(parts.title.clone())
        .modal(true, dismiss)
        .child(
            col()
                .px(space::LG)
                .pt(space::LG)
                .pb(space::SM)
                .gap(space::XXS)
                .child(text(parts.title).font(type_::H3).color(p.ink))
                .children(
                    (!parts.line.is_empty())
                        .then(|| text(parts.line).font(type_::BODY).color(p.muted).wrap()),
                ),
        );
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
        dialog = dialog.child(body);
    }
    dialog = dialog.child(
        row()
            .px(space::LG)
            .pt(space::SM)
            .pb(space::LG)
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

/// A card: a 3.5% white wash with a 5% hairline inside it, padded. Holds
/// one thing on a page (the game, a set). The caller sizes it.
pub fn panel(p: &Palette) -> El {
    col()
        .padding(space::MD)
        .gap(space::SM)
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CARD)
}

/// A notice in a slim card: what needs the player (an update, a mod left
/// out, a missing disc), and the one action for it at its end. The caller
/// passes the words and the action already built.
pub fn callout(words: El, action: Option<El>, p: &Palette) -> El {
    row()
        .min_h(size::CONTROL + 2.0 * space::XS)
        .px(space::MD)
        .py(space::XS)
        .gap(space::SM)
        .items_center()
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CARD)
        .child(col().grow(1.0).min_w(0.0).child(words))
        .children(action)
}

/// A row standing in for the title bar: on macOS, where the window draws
/// none under its traffic lights, pressing its background moves the
/// window. Linux and Windows keep the system's title bar, so the row is
/// only a row there.
pub fn title_bar(element: El) -> El {
    if cfg!(target_os = "macos") {
        element.window_drag()
    } else {
        element
    }
}
