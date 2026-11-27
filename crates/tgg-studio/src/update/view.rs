//! The in-app update dialog.

use super::{Stage, UpdateDialog, actions};
use crate::mods::size_text;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, dialog, progress, spacer,
    text_action,
};
use rune::{El, col, row, text as label};
use tgg_app::update::Stage as DownloadStage;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    let dialog_state = app.update.dialog.as_ref()?;
    let release = &dialog_state.release;
    let current = env!("CARGO_PKG_VERSION");
    let asset = release.asset_for(&dialog_state.method);
    let can_self_update = dialog_state.method.can_install() && asset.is_some();

    let mut meta_parts = Vec::new();
    if let Some(date) = release.released.as_deref().filter(|s| !s.is_empty()) {
        meta_parts.push(format!("Released {date}"));
    }
    if let Some(asset) = asset
        && asset.size > 0
    {
        meta_parts.push(size_text(asset.size));
    }

    let body = match &dialog_state.stage {
        Stage::Ready => ready_body(dialog_state, can_self_update, &meta_parts, p),
        Stage::Working { progress: slot, .. } => working_body(&release.version, slot, p),
        Stage::OpenedInstaller => col().py(space::SM).child(
            label("Opened the disk image. Drag textures.gg to Applications to finish updating.")
                .key("update/opened")
                .font(text::BODY)
                .color(p.ink)
                .wrap(),
        ),
        Stage::Failed(error) => col()
            .py(space::SM)
            .gap(space::XS)
            .child(
                label(error.clone())
                    .key("update/error")
                    .font(text::BODY)
                    .color(p.danger)
                    .wrap(),
            )
            .child(
                label("You can try again or download the release from textures.gg.")
                    .font(text::SMALL)
                    .color(p.muted)
                    .wrap(),
            ),
    };

    let buttons = dialog_buttons(dialog_state, can_self_update, p);
    let parts = Dialog {
        title: format!("Update to textures.gg {}", release.version),
        line: format!("You're on {current}."),
        body: Some(body),
        buttons,
    };
    Some(dialog(
        "update",
        "dialog",
        DialogWidth::Narrow,
        parts,
        actions::close,
        p,
    ))
}

fn ready_body(
    state: &UpdateDialog,
    can_self_update: bool,
    meta_parts: &[String],
    p: &Palette,
) -> El {
    let explanation = if let Some(reason) = state.method.managed_reason() {
        reason.to_owned()
    } else if !can_self_update {
        "No automatic installer is listed for this platform yet. Download the new version from textures.gg."
            .to_owned()
    } else if state.method.restarts() {
        format!(
            "Download, verify, and install textures.gg {}. The app will restart when it's ready.",
            state.release.version
        )
    } else {
        format!(
            "Download and verify textures.gg {}, then open the disk image to install it.",
            state.release.version
        )
    };

    let mut body = col().py(space::SM).gap(space::XS).child(
        label(explanation)
            .key("update/summary")
            .font(text::BODY)
            .color(p.ink)
            .wrap(),
    );
    if !meta_parts.is_empty() {
        body = body.child(
            label(meta_parts.join(" · "))
                .key("update/meta")
                .font(text::SMALL)
                .color(p.muted),
        );
    }
    if state.release.notes_url.is_some() {
        body = body.child(
            row().ml(-TEXT_ACTION_INSET).child(
                text_action("Release notes", false, p)
                    .key("update/notes")
                    .on_press_tx(actions::open_notes),
            ),
        );
    }
    body
}

fn working_body(
    version: &str,
    slot: &std::sync::Arc<std::sync::Mutex<Option<tgg_app::update::Progress>>>,
    p: &Palette,
) -> El {
    let current = slot.lock().ok().and_then(|guard| guard.clone());
    let (status, fraction) = match current {
        None => (format!("Connecting for textures.gg {version}…"), 0.0),
        Some(progress) => match progress.stage {
            DownloadStage::Downloading => {
                let text = match progress.total {
                    Some(total) if total > 0 => format!(
                        "Downloading… {} / {}",
                        size_text(progress.bytes),
                        size_text(total)
                    ),
                    _ => format!("Downloading… {}", size_text(progress.bytes)),
                };
                let frac = progress
                    .total
                    .filter(|&total| total > 0)
                    .map_or(0.0, |total| {
                        (progress.bytes as f32 / total as f32).clamp(0.0, 1.0)
                    });
                (text, frac)
            }
            DownloadStage::Verifying => ("Verifying download checksum…".to_owned(), 0.95),
            DownloadStage::Installing => ("Installing update…".to_owned(), 1.0),
        },
    };
    col()
        .key("update/work")
        .py(space::SM)
        .gap(space::XS)
        .child(
            label(status)
                .key("update/progress-label")
                .font(text::SMALL)
                .color(p.muted),
        )
        .child(progress(fraction, p))
}

fn dialog_buttons(state: &UpdateDialog, can_self_update: bool, p: &Palette) -> Vec<El> {
    match &state.stage {
        Stage::Ready if can_self_update => {
            let primary = if state.method.restarts() {
                "Update and restart"
            } else {
                "Download"
            };
            vec![
                button("Later", ButtonKind::Secondary, p)
                    .key("update/close")
                    .on_press(actions::close),
                spacer(),
                text_action("Download page", false, p)
                    .key("update/download")
                    .on_press_tx(actions::open_download),
                button(primary, ButtonKind::Primary, p)
                    .key("update/install")
                    .on_press(actions::start_install),
            ]
        }
        Stage::Ready => vec![
            button("Close", ButtonKind::Secondary, p)
                .key("update/close")
                .on_press(actions::close),
            spacer(),
            button("Download page", ButtonKind::Primary, p)
                .key("update/download")
                .on_press_tx(actions::open_download),
        ],
        Stage::Working { .. } => vec![
            spacer(),
            button("Cancel", ButtonKind::Secondary, p)
                .key("update/cancel")
                .on_press(actions::cancel),
        ],
        Stage::OpenedInstaller => vec![
            spacer(),
            button("Done", ButtonKind::Primary, p)
                .key("update/close")
                .on_press(actions::close),
        ],
        Stage::Failed(_) => vec![
            button("Close", ButtonKind::Secondary, p)
                .key("update/close")
                .on_press(actions::close),
            spacer(),
            button("Download page", ButtonKind::Secondary, p)
                .key("update/download")
                .on_press_tx(actions::open_download),
            button("Try again", ButtonKind::Primary, p)
                .key("update/install")
                .on_press(actions::start_install),
        ],
    }
}
