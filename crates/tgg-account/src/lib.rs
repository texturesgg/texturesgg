//! Signing in to textures.gg, shared by tgg-studio and the `tgg` command
//! line: one login, one credentials file.
//!
//! Login is OAuth's device flow (RFC 8628): the API hands out a code, the
//! user confirms it on the site's /device page, and the token the API then
//! grants is saved in [`Credentials`] and sent as `Authorization: Bearer` on
//! later requests.
//!
//! A GUI drives the flow itself: [`Site::begin_device_login`] once, then
//! [`Site::poll_device_login`] every [`DeviceLogin::interval`] until it
//! answers [`Poll::Done`]. A command line calls [`Site::login`], which does
//! both and blocks.

mod credentials;
mod device;

pub use credentials::{Credential, Credentials};
pub use device::{DeviceLogin, Poll};

use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

/// The client id the site's API accepts for device logins.
pub const CLIENT_ID: &str = "tgg-cli";

/// A textures.gg account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The name the site shows and links to.
    pub handle: String,
    pub display_name: Option<String>,
    /// Unknown only for a sign-in saved by an older `tgg` and not yet
    /// checked against the site.
    pub id: Option<String>,
}

/// Who's signed in, from [`Site::whoami`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub account: Account,
    /// The site confirmed the sign-in just now. False when it couldn't be
    /// reached: `account` is what was saved, which may since have been
    /// signed out elsewhere.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("you're not signed in to textures.gg")]
    NotSignedIn,
    /// The site refused the saved token: it expired or was signed out.
    #[error("your textures.gg sign-in has expired or was removed; sign in again")]
    SessionExpired,
    /// The device login's code expired before it was confirmed.
    #[error("the sign-in code expired; start again")]
    Expired,
    #[error("the sign-in was cancelled on textures.gg")]
    Denied,
    /// Nothing answered: no connection, the host isn't found, or it timed
    /// out.
    #[error("can't reach {api}: {reason}")]
    Offline { api: String, reason: String },
    #[error("{api} refused to start a sign-in ({status})")]
    Refused { api: String, status: u16 },
    #[error("the sign-in failed ({status})")]
    LoginFailed { status: u16 },
    /// The API's own `{ error }` message, or which path answered what.
    #[error("{message}")]
    Api { status: u16, message: String },
    #[error("reading {what}: {reason}")]
    Unreadable { what: String, reason: String },
    #[error("{}: {reason}", path.display())]
    Credentials { path: PathBuf, reason: String },
    #[error("this system has no config folder")]
    NoConfigFolder,
}

/// The textures.gg API, and the sign-in saved for it.
pub struct Site {
    api: String,
    agent: ureq::Agent,
    credentials: Credentials,
}

impl Site {
    /// `api` with the credentials file in its usual place. `user_agent`
    /// names the program, such as `tgg/0.4.0`.
    pub fn new(api: &str, user_agent: &str) -> Result<Self, Error> {
        Ok(Self::with_credentials(
            api,
            user_agent,
            Credentials::open_default()?,
        ))
    }

    pub fn with_credentials(api: &str, user_agent: &str, credentials: Credentials) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(user_agent)
            // The API answers errors with a JSON body worth reading.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            api: credentials::key(api).to_owned(),
            agent,
            credentials,
        }
    }

    /// The API's base URL, without a trailing `/`.
    pub fn api(&self) -> &str {
        &self.api
    }

    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// The saved token, if signed in.
    pub fn token(&self) -> Result<Option<String>, Error> {
        Ok(self.credentials.get(&self.api)?.map(|saved| saved.token))
    }

    /// Who's signed in: `None` if no one, the site's answer if it can be
    /// reached, and otherwise what was saved (`verified: false`). A sign-in
    /// the site refuses is [`Error::SessionExpired`]; it stays saved until
    /// [`Site::logout`] or the next login.
    pub fn whoami(&self) -> Result<Option<SignedIn>, Error> {
        let Some(saved) = self.credentials.get(&self.api)? else {
            return Ok(None);
        };
        match self.me(&saved.token) {
            Ok(account) => {
                // Keep the saved name current for offline answers.
                if saved.account() != account {
                    self.credentials
                        .set(&self.api, &Credential::new(saved.token, &account))?;
                }
                Ok(Some(SignedIn {
                    account,
                    verified: true,
                }))
            }
            Err(Error::Offline { .. }) => Ok(Some(SignedIn {
                account: saved.account(),
                verified: false,
            })),
            Err(error) => Err(error),
        }
    }

    /// Check the saved sign-in still works, so a caller that acts as the
    /// user can say so before failing on whatever it asks for first.
    pub fn signed_in(&self) -> Result<&Self, Error> {
        self.get::<Value>("/api/users/me")?;
        Ok(self)
    }

    /// Sign out: end the session on the site (best effort; it may be
    /// unreachable) and forget the token. Returns who was signed in.
    pub fn logout(&self) -> Result<Option<Account>, Error> {
        let Some(saved) = self.credentials.get(&self.api)? else {
            return Ok(None);
        };
        let _ = self
            .agent
            .post(self.url("/api/auth/sign-out"))
            .header("authorization", format!("Bearer {}", saved.token))
            .send_json(serde_json::json!({}));
        self.credentials.remove(&self.api)?;
        Ok(Some(saved.account()))
    }

    /// `net` sending the saved token to the API, if signed in; `net`
    /// unchanged otherwise.
    pub fn net(&self, net: tgg_mod::HttpNet) -> Result<tgg_mod::HttpNet, Error> {
        Ok(match self.token()? {
            Some(token) => net.with_token(token),
            None => net,
        })
    }

    /// GET a path of the API as the signed-in user.
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let token = self.require_token()?;
        let request = self
            .agent
            .get(self.url(path))
            .header("authorization", bearer(&token));
        self.read(request.call(), path)
    }

    /// Whether a path of the API answers the signed-in user with something,
    /// rather than 404.
    pub fn exists(&self, path: &str) -> Result<bool, Error> {
        let token = self.require_token()?;
        let response = self
            .agent
            .get(self.url(path))
            .header("authorization", bearer(&token))
            .call();
        let response = self.reached(response)?;
        if response.status() == 404 {
            return Ok(false);
        }
        self.read::<Value>(Ok(response), path)?;
        Ok(true)
    }

    /// POST JSON to a path of the API as the signed-in user.
    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, Error> {
        let token = self.require_token()?;
        let request = self
            .agent
            .post(self.url(path))
            .header("authorization", bearer(&token));
        self.read(request.send_json(body), path)
    }

    /// The account a token belongs to.
    fn me(&self, token: &str) -> Result<Account, Error> {
        let request = self
            .agent
            .get(self.url("/api/users/me"))
            .header("authorization", bearer(token));
        let me: Value = self.read(request.call(), "/api/users/me")?;
        let text = |field: &str| me[field].as_str().filter(|text| !text.is_empty());
        Ok(Account {
            handle: text("name").unwrap_or_default().to_owned(),
            display_name: text("displayName").map(str::to_owned),
            id: text("id").map(str::to_owned),
        })
    }

    fn require_token(&self) -> Result<String, Error> {
        self.token()?.ok_or(Error::NotSignedIn)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.api)
    }

    /// The response, or [`Error::Offline`] when nothing answered.
    fn reached(
        &self,
        response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<ureq::http::Response<ureq::Body>, Error> {
        response.map_err(|error| Error::Offline {
            api: self.api.clone(),
            reason: error.to_string(),
        })
    }

    /// The body of a 2xx response, or the API's `{ error }` as the error.
    fn read<T: DeserializeOwned>(
        &self,
        response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
        path: &str,
    ) -> Result<T, Error> {
        let mut response = self.reached(response)?;
        let status = response.status().as_u16();
        if status == 401 {
            return Err(Error::SessionExpired);
        }
        if !response.status().is_success() {
            let message = response
                .body_mut()
                .read_json::<Value>()
                .ok()
                .and_then(|body| body["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{path} answered {}", response.status()));
            return Err(Error::Api { status, message });
        }
        response
            .body_mut()
            .read_json()
            .map_err(|error| Error::Unreadable {
                what: path.to_owned(),
                reason: error.to_string(),
            })
    }
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}
