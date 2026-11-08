//! Signing in to textures.gg, through `tgg-account`: one sign-in shared
//! with the `tgg` CLI (the same saved credentials, re-read on every call,
//! so a sign-in in either shows in the other). Signing in is the device
//! flow: the app asks for a code, the player confirms it on the site, and
//! the app polls until they have, a request at a time, never blocking the
//! window. `actions` changes it; `view` draws the sidebar's account row,
//! the sign-in dialog and the account dialog.

pub mod actions;
pub mod view;

use std::sync::Arc;
use std::time::{Duration, Instant};
use tgg_account::{DeviceLogin, Error, Poll, SignedIn, Site};

/// The name tgg-studio gives textures.gg when it asks.
const USER_AGENT: &str = concat!("tgg-studio/", env!("CARGO_PKG_VERSION"));

pub struct SignIn {
    /// textures.gg, and where the sign-in is kept.
    pub site: Arc<Site>,
    /// Who's signed in, as last checked.
    pub who: Option<SignedIn>,
    /// A sign-in under way, its dialog showing.
    pub login: Option<Login>,
    /// The account dialog shows.
    pub account_open: bool,
}

impl SignIn {
    /// textures.gg, the sign-in kept where the CLI keeps it, or (in tests,
    /// and when the platform has no config folder) in `fallback`.
    pub fn new(persist: bool, fallback: std::path::PathBuf) -> Self {
        let api = tgg_app::net::api_url();
        let site = persist
            .then(|| Site::new(&api, USER_AGENT).ok())
            .flatten()
            .unwrap_or_else(|| {
                Site::with_credentials(&api, USER_AGENT, tgg_account::Credentials::at(fallback))
            });
        Self {
            site: Arc::new(site),
            who: None,
            login: None,
            account_open: false,
        }
    }
}

/// A sign-in under way: the code to confirm and where, and the polling.
pub struct Login {
    /// The code as the site writes it ("ABCD-EFGH"), once it's arrived.
    pub code: Option<String>,
    /// The page to confirm it on.
    pub link: Option<String>,
    pub expires: Option<Instant>,
    pub interval: Duration,
    /// When to ask next whether it's confirmed.
    pub next: Option<Instant>,
    /// A poll is on its way.
    pub polling: bool,
    /// What went wrong: the code expired, the site refused, or couldn't be
    /// reached.
    pub problem: Option<String>,
    /// What the polls send; absent in tests, which answer for the site.
    pub device: Option<DeviceLogin>,
}

impl Login {
    /// Asking for a code.
    pub fn asking() -> Self {
        Self {
            code: None,
            link: None,
            expires: None,
            interval: Duration::from_secs(5),
            next: None,
            polling: false,
            problem: None,
            device: None,
        }
    }
}

/// What a sign-in job found.
pub enum Done {
    /// Who's signed in, checked with the site.
    Checked(Result<Option<SignedIn>, Error>),
    /// The code to confirm.
    Begun(Result<DeviceLogin, Error>),
    /// Whether it's confirmed yet.
    Polled(Result<Poll, Error>),
    /// Signed out.
    SignedOut(Result<(), Error>),
}

/// The name an account goes by here: its display name, else its handle.
pub fn shown_name(account: &tgg_account::Account) -> String {
    account
        .display_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| account.handle.clone())
}
