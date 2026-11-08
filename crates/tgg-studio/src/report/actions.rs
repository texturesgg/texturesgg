//! Opening, copying and sending a report.

use super::{Progress, Report};
use crate::jobs::Done;
use crate::state::App;
use rune::Tx;

/// The community Discord.
pub const DISCORD_URL: &str = "https://discord.gg/RzFFFg3J4g";

/// The most a send's answer is read.
const ANSWER_LIMIT: u64 = 4096;

/// Show the report for the log as it stands; `crashed` when the last
/// session did.
pub fn open(app: &mut App, crashed: bool) {
    app.report = Some(Report::now(crashed));
}

pub fn close(app: &mut App) {
    app.report = None;
}

/// Copy the report, to paste it anywhere.
pub fn copy(app: &mut App, tx: &mut Tx) {
    let Some(report) = &mut app.report else { return };
    tx.copy(report.text.clone());
    if !matches!(report.progress, Progress::Sent(_)) {
        report.progress = Progress::Copied;
    }
}

/// Open the community Discord in the browser.
pub fn discord(app: &mut App, tx: &mut Tx) {
    if let Err(error) = tx.open_url(DISCORD_URL) {
        app.tell(format!("Couldn't open Discord: {error}"), true);
    }
}

/// Send the report to textures.gg; once it's sent, the button closes.
pub fn send(app: &mut App) {
    let Some(report) = &mut app.report else { return };
    match report.progress {
        Progress::Sending => return,
        Progress::Sent(_) => return close(app),
        _ => {}
    }
    report.progress = Progress::Sending;
    let text = report.text.clone();
    app.jobs.spawn("report", move || {
        #[derive(serde::Deserialize)]
        struct Sent {
            id: String,
        }
        let url = format!("{}/api/editor/reports", tgg_app::net::api_url());
        let sent = tgg_app::net::post_json(&url, &serde_json::json!({ "report": text }), ANSWER_LIMIT)
            .map_err(|error| error.to_string())
            .and_then(|answer| serde_json::from_str::<Sent>(&answer).map(|sent| sent.id).map_err(|error| error.to_string()));
        Done::ReportSent(sent)
    });
}

/// The send's answer.
pub fn sent(app: &mut App, result: Result<String, String>) -> bool {
    let Some(report) = &mut app.report else { return false };
    report.progress = match result {
        Ok(id) => Progress::Sent(id),
        Err(error) => {
            tgg_app::log(&format!("report not sent: {error}"));
            Progress::Failed(error)
        }
    };
    true
}
