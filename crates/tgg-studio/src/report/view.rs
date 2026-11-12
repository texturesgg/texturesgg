//! The report dialog: what would be sent, in a scrolling block that starts
//! at its end (the latest lines say what went wrong), what's become of it,
//! and its actions.

use super::{Progress, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{ButtonKind, Dialog, DialogWidth, button, dialog, spacer};
use rune::{El, ScrollSpec, col, text as label};

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
    let (status, color) = match &report.progress {
        Progress::Unsent => (String::new(), p.muted),
        Progress::Copied => ("Copied.".into(), p.muted),
        Progress::Sending => ("Sending…".into(), p.muted),
        Progress::Sent(id) => (format!("Sent. Mention report {id} on Discord."), p.success),
        Progress::Failed(error) => (
            format!("Couldn't send ({error}). Copy it to Discord instead."),
            p.danger,
        ),
    };
    let body = col()
        .px(space::LG)
        .py(space::MD)
        .gap(space::XS)
        .child(lines)
        .child(
            label(status)
                .key("report/status")
                .font(text::SMALL)
                .color(color),
        );
    let send_words = if matches!(report.progress, Progress::Sent(_)) {
        "Done"
    } else {
        "Send"
    };
    let parts = Dialog {
        title: if report.crashed {
            "tgg-studio quit unexpectedly".into()
        } else {
            "Report a problem".into()
        },
        line: "Sending shares exactly this, and nothing else.".into(),
        body: Some(body),
        buttons: vec![
            button("Close", ButtonKind::Secondary, p)
                .key("report/close")
                .on_press(actions::close),
            spacer(),
            button("Discord", ButtonKind::Secondary, p)
                .key("report/discord")
                .on_press_tx(actions::discord),
            button("Copy", ButtonKind::Secondary, p)
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
