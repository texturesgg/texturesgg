//! Report a problem: the player sees exactly what would be
//! sent (the app's version, their system, and the end of the log) and then
//! copies it, takes it to Discord, or sends it. Nothing leaves without that
//! choice. After a panic the next launch offers the same.

use crate::{log, net};
use gpui::{
    ClipboardItem, Context, EventEmitter, InteractiveElement, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Window, div,
};
use std::path::Path;
use tgg_ui::tokens::{font, radius, space, text};
use tgg_ui::{ButtonVariant, Dialog, Theme, rem};

/// The community Discord.
const DISCORD_URL: &str = "https://discord.gg/RzFFFg3J4g";

/// How much of the log's end a report carries.
const LOG_BYTES: u64 = 48 * 1024;
const LOG_LINES: usize = 300;

/// The report for the log as it stands.
fn current() -> String {
    compose(&log::recent(LOG_BYTES), dirs::home_dir().as_deref())
}

/// What is running and the last lines of `log`, with the player's home
/// folder written as `~` so their account name stays out of it.
fn compose(log: &str, home: Option<&Path>) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let recent = lines[lines.len().saturating_sub(LOG_LINES)..].join("\n");
    let report = format!("{}\n\n{recent}", log::version());
    match home.and_then(Path::to_str) {
        Some(home) if !home.is_empty() => report.replace(home, "~"),
        _ => report,
    }
}

/// Send `report` to textures.gg; the id it is kept under.
fn send(api: &str, report: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Sent {
        id: String,
    }
    let response = net::post_json(
        &format!("{api}/api/editor/reports"),
        &serde_json::json!({ "report": report }),
        4096,
    )
    .map_err(|error| error.to_string())?;
    serde_json::from_str::<Sent>(&response)
        .map(|sent| sent.id)
        .map_err(|error| error.to_string())
}

#[derive(Debug, PartialEq)]
enum Progress {
    Unsent,
    Copied,
    Sending,
    Sent(String),
    Failed(String),
}

pub(crate) enum ReportEvent {
    Close,
}

pub(crate) struct Report {
    /// Exactly what Copy copies and Send sends.
    text: String,
    lines: Vec<SharedString>,
    /// Offered because the last session panicked.
    crashed: bool,
    /// The API a report is sent to.
    api: String,
    progress: Progress,
    scroll: ScrollHandle,
}

impl EventEmitter<ReportEvent> for Report {}

impl Report {
    pub(crate) fn new(crashed: bool) -> Self {
        Self::of(current(), crashed, net::api_url())
    }

    fn of(text: String, crashed: bool, api: String) -> Self {
        let scroll = ScrollHandle::new();
        // The latest lines say what went wrong.
        scroll.scroll_to_bottom();
        Self {
            lines: text.lines().map(|line| line.to_owned().into()).collect(),
            text,
            crashed,
            api,
            progress: Progress::Unsent,
            scroll,
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.text.clone()));
        if !matches!(self.progress, Progress::Sent(_)) {
            self.progress = Progress::Copied;
        }
        cx.notify();
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        match self.progress {
            Progress::Sending => return,
            Progress::Sent(_) => return cx.emit(ReportEvent::Close),
            _ => {}
        }
        self.progress = Progress::Sending;
        cx.notify();
        let text = self.text.clone();
        let api = self.api.clone();
        cx.spawn(async move |report, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move { send(&api, &text) })
                .await;
            report
                .update(cx, |report, cx| {
                    report.progress = match sent {
                        Ok(id) => Progress::Sent(id),
                        Err(error) => {
                            log(&format!("report not sent: {error}"));
                            Progress::Failed(error)
                        }
                    };
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }
}

/// The report's height in the dialog, in web pixels.
const REPORT_HEIGHT: f32 = 280.0;
const REPORT_WIDTH: f32 = 640.0;

impl Render for Report {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let (status, color) = match &self.progress {
            Progress::Unsent => (String::new(), palette.muted),
            Progress::Copied => ("Copied".into(), palette.muted),
            Progress::Sending => ("Sending…".into(), palette.muted),
            Progress::Sent(id) => (
                format!("Sent. Mention report {id} on Discord."),
                palette.success,
            ),
            Progress::Failed(error) => (
                format!("Couldn't send ({error}). Copy it to Discord instead."),
                palette.danger,
            ),
        };
        let report = div()
            .id("report-text")
            .h(rem(REPORT_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(rem(space::XS))
            .rounded(rem(radius::MD))
            .border_1()
            .border_color(palette.line.to_gpui())
            .bg(palette.bg.to_gpui())
            .font_family(font::MONO)
            .text_size(rem(text::XS))
            // An empty line still takes a line's height.
            .children(
                self.lines
                    .iter()
                    .map(|line| div().min_h(rem(text::XS * 1.4)).child(line.clone())),
            );
        let close = cx.entity();
        let discord = cx.entity();
        let copy = cx.entity();
        let send = cx.entity();
        Dialog::new(
            "report",
            if self.crashed {
                "textures.gg quit unexpectedly"
            } else {
                "Report a problem"
            },
            move |_, cx| close.update(cx, |_, cx| cx.emit(ReportEvent::Close)),
        )
        .width(REPORT_WIDTH)
        .description("Sending shares exactly this, and nothing else.")
        .body(
            div()
                .flex()
                .flex_col()
                .gap(rem(space::XS))
                .child(report)
                .child(
                    div()
                        .min_h(rem(text::SM * 1.4))
                        .text_size(rem(text::SM))
                        .text_color(color.to_gpui())
                        .child(status),
                ),
        )
        .action("Discord", ButtonVariant::Ghost, move |_, cx| {
            discord.update(cx, |_, cx| cx.open_url(DISCORD_URL))
        })
        .action("Copy", ButtonVariant::Secondary, move |_, cx| {
            copy.update(cx, |report, cx| report.copy(cx))
        })
        .action(
            match self.progress {
                Progress::Sent(_) => "Done",
                _ => "Send",
            },
            ButtonVariant::Primary,
            move |_, cx| send.update(cx, |report, cx| report.send(cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{LOG_LINES, Progress, Report, compose, send};
    use crate::net::tests::serve_once;
    use gpui::{AppContext, TestAppContext};
    use std::path::Path;

    #[test]
    fn a_report_is_the_version_and_the_end_of_the_log_without_the_home_folder() {
        let log = (0..LOG_LINES + 5)
            .map(|line| format!("line {line} in /home/falco/games"))
            .collect::<Vec<_>>()
            .join("\n");
        let report = compose(&log, Some(Path::new("/home/falco")));
        let lines: Vec<&str> = report.lines().collect();

        assert!(lines[0].starts_with("textures.gg editor "));
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "line 5 in ~/games");
        assert_eq!(lines.len(), LOG_LINES + 2);
        assert!(!report.contains("falco"));
    }

    #[test]
    fn sending_posts_the_report_and_returns_its_id() {
        let (api, served) = serve_once("201 Created", r#"{"id":"abc123"}"#);
        assert_eq!(send(&api, "the \"report\"\n"), Ok("abc123".into()));
        assert_eq!(
            served.join().expect("served"),
            r#"{"report":"the \"report\"\n"}"#
        );

        let (api, served) = serve_once(
            "429 Too Many Requests",
            r#"{"error":"Rate limit exceeded"}"#,
        );
        assert!(send(&api, "again").is_err());
        served.join().expect("served");
    }

    #[gpui::test]
    fn the_dialog_sends_once_and_then_names_the_report(cx: &mut TestAppContext) {
        let (api, served) = serve_once("201 Created", r#"{"id":"abc123"}"#);
        let report = cx.new(|_| Report::of("what happened".into(), false, api));
        report.update(cx, |report, cx| {
            report.send(cx);
            // A second press while sending sends nothing more.
            report.send(cx);
            assert_eq!(report.progress, Progress::Sending);
        });
        cx.run_until_parked();

        assert_eq!(
            served.join().expect("served"),
            r#"{"report":"what happened"}"#
        );
        report.read_with(cx, |report, _| {
            assert_eq!(report.progress, Progress::Sent("abc123".into()));
        });
    }
}
