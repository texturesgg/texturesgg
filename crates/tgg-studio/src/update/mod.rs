//! In-app updates for `tgg-studio`: launch check, manual check from Settings,
//! and an update dialog that downloads, verifies by SHA-256, installs, and
//! restarts the app (or explains when the binary is managed externally, e.g. Nix).

pub mod actions;
pub mod view;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tgg_app::update::{ApplyOutcome, InstallMethod, Progress, Release};

/// App-update state on [`crate::state::App`].
#[derive(Default)]
pub struct Update {
    /// Newer release found at launch or by a manual check.
    pub available: Option<Release>,
    /// Status of a manual check started from Settings.
    pub check: Check,
    /// Open update dialog, when the player clicked the update banner or Settings button.
    pub dialog: Option<UpdateDialog>,
}

impl Update {
    /// Whether an update download or install is currently in progress.
    pub fn is_working(&self) -> bool {
        self.dialog
            .as_ref()
            .is_some_and(|dialog| matches!(dialog.stage, Stage::Working { .. }))
    }
}

/// Result of a manual "Check now" in Settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Check {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Failed(String),
}

/// The open update dialog.
pub struct UpdateDialog {
    pub release: Release,
    pub method: InstallMethod,
    pub stage: Stage,
}

/// Where the update dialog is in its flow.
pub enum Stage {
    Ready,
    Working {
        progress: Arc<Mutex<Option<Progress>>>,
        cancel: Arc<AtomicBool>,
    },
    OpenedInstaller,
    Failed(String),
}

/// Results coming back from background update jobs.
pub enum Done {
    /// Silent check at launch.
    Launched(Option<Release>),
    /// Manual check triggered from Settings.
    Checked(Result<Option<Release>, String>),
    /// In-app download + verify + install completed.
    Installed(Result<ApplyOutcome, String>),
}
