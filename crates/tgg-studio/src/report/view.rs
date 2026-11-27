//! The report dialog: what would be sent, in a scrolling block that starts
//! at its end (the latest lines say what went wrong), what's become of it,
//! and its actions.

use super::{Progress, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{ButtonKind, Dialog, DialogWidth, button, dialog, spacer};
use rune::{El, ScrollSpec, col, row, text as label};

/// The report's block, at its tallest.
const REPORT_HEIGHT: f32 = 300.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    let report = app.report.as_ref()?;
    let lines = col()
        .key("report/text")
        .max_h(REPORT_HEIGHT)
        .scroll(ScrollSpec { stick_bottom: true })
        .px(space::SM)
        .py(space::XS)
        .fill(p.sunk)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL)
        .child(
            label(report.text.clone())
                .font(text::MONO_SMALL)
                .color(p.muted)
                .wrap(),
        );
    let code_box = match &report.progress {
        Progress::Sent { id, copied } => Some(
            row()
                .key("report/code-box")
                .gap(space::SM)
                .items_center()
                .px(space::SM)
                .py(space::XS)
                .fill(p.surface)
                .border(size::LINE, p.line)
                .radius(radius::CONTROL + 1.0)
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(space::TIGHT)
                        .child(label("Report code").font(text::SMALL).color(p.muted))
                        .child(
                            label(id.clone())
                                .key("report/code")
                                .font(text::MONO_CODE)
                                .color(p.ink)
                                .truncate(),
                        ),
                )
                .child(
                    button(
                        if *copied { "Copied" } else { "Copy code" },
                        ButtonKind::Secondary,
                        p,
                    )
                    .key("report/copy-code")
                    .on_press_tx(actions::copy_code),
                ),
        ),
        _ => None,
    };
    let (status, color) = match &report.progress {
        Progress::Unsent => (String::new(), p.muted),
        Progress::Copied => ("Copied.".into(), p.muted),
        Progress::Sending => ("Sending…".into(), p.muted),
        Progress::Sent { copied: true, .. } => {
            ("Report code copied. Paste it on Discord.".into(), p.success)
        }
        Progress::Sent { .. } => (
            "Sent. Copy the report code above and mention it on Discord.".into(),
            p.success,
        ),
        Progress::Failed(error) => (
            format!("Couldn't send ({error}). Copy it to Discord instead."),
            p.danger,
        ),
    };
    let body = col()
        .py(space::MD)
        .gap(space::XS)
        .child(lines)
        .children(code_box)
        .child(
            label(status)
                .key("report/status")
                .font(text::SMALL)
                .color(color),
        );
    let sent = matches!(report.progress, Progress::Sent { .. });
    let send_words = if sent { "Done" } else { "Send" };
    let copy_words = if sent { "Copy log" } else { "Copy" };
    let parts = Dialog {
        title: if report.crashed {
            "tgg-studio quit unexpectedly".into()
        } else {
            "Report a problem".into()
        },
        line: "This is the log that gets sent.".into(),
        body: Some(body),
        buttons: vec![
            button("Close", ButtonKind::Secondary, p)
                .key("report/close")
                .on_press(actions::close),
            spacer(),
            button("Discord", ButtonKind::Secondary, p)
                .key("report/discord")
                .on_press_tx(actions::discord),
            button(copy_words, ButtonKind::Secondary, p)
                .key("report/copy")
                .on_press_tx(actions::copy),
            button(send_words, ButtonKind::Primary, p)
                .key("report/send")
                .on_press(actions::send),
        ],
    };
    Some(dialog(
        "report",
        "dialog",
        DialogWidth::Wide,
        parts,
        actions::close,
        p,
    ))
}
