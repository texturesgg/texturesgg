//! The device flow: a code to confirm on the site, then a token.

use crate::{Account, CLIENT_ID, Credential, Error, Site};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// A sign-in waiting for the user to confirm its code on the site.
#[derive(Debug, Clone)]
pub struct DeviceLogin {
    /// The code to show, as the API gave it ([`DeviceLogin::display_code`]
    /// writes it the way the site does).
    pub user_code: String,
    /// The page where the code is confirmed.
    pub verification_uri: String,
    /// That page with the code filled in, if the API gave one.
    pub verification_uri_complete: Option<String>,
    /// When the code stops working.
    pub expires_at: Instant,
    /// How long to wait between polls.
    pub interval: Duration,
    device_code: String,
}

/// One answer to [`Site::poll_device_login`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    /// Not confirmed yet; poll again after the interval.
    Pending,
    /// Polled too often; wait 5 seconds longer between polls from now on.
    SlowDown,
    /// Confirmed. The token is saved.
    Done(Account),
    /// The code expired before it was confirmed.
    Expired,
    /// The user cancelled the sign-in on the site.
    Denied,
}

impl DeviceLogin {
    /// The code as the site shows it: ABCD-EFGH.
    pub fn display_code(&self) -> String {
        match self.user_code.split_at_checked(4) {
            Some((head, tail)) if self.user_code.len() == 8 => format!("{head}-{tail}"),
            _ => self.user_code.clone(),
        }
    }

    /// The page to open: with the code filled in if possible.
    pub fn link(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .unwrap_or(&self.verification_uri)
    }
}

/// The API's answer to `/api/auth/device/code`.
#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

impl Site {
    /// Start a sign-in: ask the API for a code for the user to confirm at
    /// [`DeviceLogin::link`].
    pub fn begin_device_login(&self) -> Result<DeviceLogin, Error> {
        let response = self
            .agent
            .post(self.url("/api/auth/device/code"))
            .send_json(json!({ "client_id": CLIENT_ID }));
        let mut response = self.reached(response)?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                api: self.api.clone(),
                status: response.status().as_u16(),
            });
        }
        let code: DeviceCode =
            response
                .body_mut()
                .read_json()
                .map_err(|error| Error::Unreadable {
                    what: "/api/auth/device/code".to_owned(),
                    reason: error.to_string(),
                })?;
        Ok(DeviceLogin {
            user_code: code.user_code,
            verification_uri: code.verification_uri,
            verification_uri_complete: code.verification_uri_complete,
            expires_at: Instant::now() + Duration::from_secs(code.expires_in),
            // RFC 8628's default; at least a second whatever the API says.
            interval: Duration::from_secs(code.interval.unwrap_or(5).max(1)),
            device_code: code.device_code,
        })
    }

    /// Ask once whether the user has confirmed the code. On
    /// [`Poll::Done`] the token is saved, replacing any earlier sign-in.
    pub fn poll_device_login(&self, login: &DeviceLogin) -> Result<Poll, Error> {
        if Instant::now() >= login.expires_at {
            return Ok(Poll::Expired);
        }
        let response = self
            .agent
            .post(self.url("/api/auth/device/token"))
            .send_json(json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": login.device_code,
                "client_id": CLIENT_ID,
            }));
        let mut response = self.reached(response)?;
        let status = response.status().as_u16();
        let body: Value = response.body_mut().read_json().unwrap_or_default();
        if let Some(token) = body["access_token"].as_str() {
            let account = self.me(token)?;
            self.credentials
                .set(&self.api, &Credential::new(token, &account))?;
            return Ok(Poll::Done(account));
        }
        match body["error"].as_str() {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => Ok(Poll::SlowDown),
            Some("access_denied") => Ok(Poll::Denied),
            Some("expired_token") => Ok(Poll::Expired),
            _ => Err(Error::LoginFailed { status }),
        }
    }

    /// Sign in, blocking until the user confirms the code or the sign-in
    /// ends. `on_code` is called once with the code to show, before the page
    /// is opened in a browser (if `open_browser`; a browser that won't open
    /// is no error, the link is in the code).
    pub fn login(
        &self,
        open_browser: bool,
        on_code: impl FnOnce(&DeviceLogin),
    ) -> Result<Account, Error> {
        let login = self.begin_device_login()?;
        on_code(&login);
        if open_browser {
            let _ = open::that_detached(login.link());
        }
        let mut interval = login.interval;
        loop {
            std::thread::sleep(interval);
            match self.poll_device_login(&login)? {
                Poll::Pending => {}
                Poll::SlowDown => interval += Duration::from_secs(5),
                Poll::Done(account) => return Ok(account),
                Poll::Expired => return Err(Error::Expired),
                Poll::Denied => return Err(Error::Denied),
            }
        }
    }
}
