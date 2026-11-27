//! Actions for checking, opening, downloading, and installing app updates.

use super::{Check, Done, Stage, UpdateDialog};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::Tx;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tgg_app::update::{self, ApplyOutcome, UpdateError};

/// Open the in-app update dialog for the available release.
pub fn open(app: &mut App) {
    let Some(release) = app.update.available.clone() else {
        return;
    };
    app.preferences.open = false;
    app.preferences.menu = None;
    if app.update.dialog.is_none() {
        app.update.dialog = Some(UpdateDialog {
            release,
            method: update::detect_install_method(),
            stage: Stage::Ready,
        });
    }
}

/// Close the update dialog unless an install is actively writing files.
pub fn close(app: &mut App) {
    if let Some(dialog) = &app.update.dialog
        && let Stage::Working { cancel, .. } = &dialog.stage
    {
        cancel.store(true, Ordering::Relaxed);
    }
    app.update.dialog = None;
}

/// Cancel an in-progress download.
pub fn cancel(app: &mut App) {
    if let Some(dialog) = &mut app.update.dialog {
        if let Stage::Working { cancel, .. } = &dialog.stage {
            cancel.store(true, Ordering::Relaxed);
        }
        dialog.stage = Stage::Ready;
    }
}

/// Manually check textures.gg for an app update (from Settings).
pub fn check_now(app: &mut App) {
    if app.update.check == Check::Checking {
        return;
    }
    app.update.check = Check::Checking;
    app.jobs.spawn("app update check", || {
        let result = update::check_result(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string());
        Job::Update(Done::Checked(result))
    });
}

/// Start downloading, verifying, and installing the available update.
pub fn start_install(app: &mut App) {
    let Some(dialog) = &mut app.update.dialog else {
        return;
    };
    if matches!(dialog.stage, Stage::Working { .. }) {
        return;
    }
    let release = dialog.release.clone();
    let method = dialog.method.clone();
    let progress = Arc::new(Mutex::new(None));
    let cancel = Arc::new(AtomicBool::new(false));
    dialog.stage = Stage::Working {
        progress: progress.clone(),
        cancel: cancel.clone(),
    };

    let progress_slot = progress;
    let cancel_flag = cancel;
    app.jobs.spawn("app update install", move || {
        let on_progress = |p: update::Progress| {
            if let Ok(mut slot) = progress_slot.lock() {
                *slot = Some(p);
            }
        };
        let result = match update::install_release(&release, &method, &on_progress, &cancel_flag) {
            Ok(outcome) => Ok(outcome),
            Err(UpdateError::Cancelled) => Err(String::new()),
            Err(error) => Err(error.to_string()),
        };
        Job::Update(Done::Installed(result))
    });
}

/// Open the download page in the browser.
pub fn open_download(app: &mut App, tx: &mut Tx) {
    if let Err(error) = tx.open_url(update::DOWNLOAD_URL) {
        app.tell(format!("Couldn't open the download page: {error}"), true);
    }
}

/// Open the release notes for the available release.
pub fn open_notes(app: &mut App, tx: &mut Tx) {
    let url = app
        .update
        .dialog
        .as_ref()
        .and_then(|d| d.release.notes_url.as_deref())
        .unwrap_or(update::DOWNLOAD_URL);
    if let Err(error) = tx.open_url(url) {
        app.tell(format!("Couldn't open release notes: {error}"), true);
    }
}

/// Handle a background update job finishing.
pub fn done(app: &mut App, done: Done, tx: &mut Tx) -> bool {
    match done {
        Done::Launched(release) => {
            app.update.available = release;
            app.update.available.is_some()
        }
        Done::Checked(result) => {
            match result {
                Ok(Some(release)) => {
                    app.update.available = Some(release);
                    app.update.check = Check::Idle;
                }
                Ok(None) => {
                    app.update.available = None;
                    app.update.check = Check::UpToDate;
                }
                Err(error) => {
                    tgg_app::log(&format!("manual update check failed: {error}"));
                    app.update.check = Check::Failed(error);
                }
            }
            true
        }
        Done::Installed(result) => {
            match result {
                Ok(ApplyOutcome::Relaunching) => {
                    app.update.dialog = None;
                    tx.quit();
                }
                Ok(ApplyOutcome::OpenedInstaller) => {
                    if let Some(dialog) = &mut app.update.dialog {
                        dialog.stage = Stage::OpenedInstaller;
                    }
                }
                Err(message) if message.is_empty() => {
                    // Cancelled by the player.
                    if let Some(dialog) = &mut app.update.dialog {
                        dialog.stage = Stage::Ready;
                    }
                }
                Err(error) => {
                    tgg_app::log(&format!("app update failed: {error}"));
                    if let Some(dialog) = &mut app.update.dialog {
                        dialog.stage = Stage::Failed(error);
                    }
                }
            }
            true
        }
    }
}
