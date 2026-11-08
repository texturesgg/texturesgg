//! Signing in to textures.gg: `tgg login` and `tgg logout`, the saved token,
//! and requests to the site's API as the signed-in user.
//!
//! Login is OAuth's device flow (RFC 8628): the API hands out a code, the
//! user confirms it on the site's /device page, and the token the API then
//! returns is sent as `Authorization: Bearer` on later requests.

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// The client id the site's API accepts for the command line.
const CLIENT_ID: &str = "tgg-cli";
const USER_AGENT: &str = concat!("tgg/", env!("CARGO_PKG_VERSION"));

/// The textures.gg API, and the signed-in user's token for it, if any.
pub struct Site {
    api: String,
    agent: ureq::Agent,
    token: Option<String>,
}

/// What `credentials.json` keeps for one API.
#[derive(Serialize, Deserialize)]
struct Credential {
    token: String,
    name: String,
}

impl Site {
    pub fn new(api: &str) -> Result<Self> {
        let api = api.trim_end_matches('/').to_owned();
        let agent = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            // The API answers errors with a JSON body worth reading.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let token = read_credentials()?.remove(&api).map(|saved| saved.token);
        Ok(Self { api, agent, token })
    }

    /// Check the saved sign-in still works, so a command that acts as the
    /// user can say so instead of failing on whatever it asks for first.
    pub fn signed_in(&self) -> Result<&Self> {
        self.get::<Value>("/api/users/me")?;
        Ok(self)
    }

    /// GET a path of the API as the signed-in user.
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let request = self.agent.get(format!("{}{path}", self.api));
        self.read(self.authorize(request)?.call(), path)
    }

    /// Whether a path of the API answers the signed-in user with something,
    /// rather than 404.
    pub fn exists(&self, path: &str) -> Result<bool> {
        let request = self.agent.get(format!("{}{path}", self.api));
        let response = self
            .authorize(request)?
            .call()
            .with_context(|| format!("reaching {}", self.api))?;
        if response.status() == 404 {
            return Ok(false);
        }
        self.read::<Value>(Ok(response), path)?;
        Ok(true)
    }

    /// POST JSON to a path of the API as the signed-in user.
    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let request = self.agent.post(format!("{}{path}", self.api));
        self.read(self.authorize(request)?.send_json(body), path)
    }

    fn authorize<B>(&self, request: ureq::RequestBuilder<B>) -> Result<ureq::RequestBuilder<B>> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| anyhow!("you're not signed in; run tgg login"))?;
        Ok(request.header("authorization", format!("Bearer {token}")))
    }

    /// The body of a 2xx response, or the API's `{ error }` as the error.
    fn read<T: DeserializeOwned>(
        &self,
        response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
        path: &str,
    ) -> Result<T> {
        let mut response = response.with_context(|| format!("reaching {}", self.api))?;
        let status = response.status();
        if status == 401 {
            bail!("your sign-in has expired or was removed; run tgg login");
        }
        if !status.is_success() {
            let message = response
                .body_mut()
                .read_json::<Value>()
                .ok()
                .and_then(|body| body["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{path} answered {status}"));
            bail!(message);
        }
        response
            .body_mut()
            .read_json()
            .with_context(|| format!("reading {path}"))
    }
}

/// The device code the API hands out, and where the user confirms it.
#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}

/// `tgg login`: confirm a code on the site and save the token it grants.
pub fn login(api: &str, browser: bool) -> Result<()> {
    let mut site = Site::new(api)?;
    let mut response = site
        .agent
        .post(format!("{}/api/auth/device/code", site.api))
        .send_json(json!({ "client_id": CLIENT_ID }))
        .with_context(|| format!("reaching {}", site.api))?;
    if !response.status().is_success() {
        bail!(
            "{} refused to start a sign-in ({})",
            site.api,
            response.status()
        );
    }
    let code: DeviceCode = response.body_mut().read_json()?;

    // Shown the way the site shows it: ABCD-EFGH.
    let user_code = match code.user_code.split_at_checked(4) {
        Some((head, tail)) if code.user_code.len() == 8 => format!("{head}-{tail}"),
        _ => code.user_code.clone(),
    };
    println!("Your code is {user_code}. Confirm it on textures.gg:");
    println!("  {}", code.verification_uri_complete);
    if browser && open::that_detached(&code.verification_uri_complete).is_err() {
        println!("(Open that link in a browser.)");
    }

    let token = poll(&site, &code)?;
    site.token = Some(token.clone());
    let me: Value = site.get("/api/users/me")?;
    let name = me["name"].as_str().unwrap_or_default().to_owned();

    let mut saved = read_credentials()?;
    saved.insert(
        site.api.clone(),
        Credential {
            token,
            name: name.clone(),
        },
    );
    write_credentials(&saved)?;
    println!("Signed in as {name}.");
    Ok(())
}

/// Ask for the token every `interval` seconds until the user confirms or
/// cancels the code, or it expires.
fn poll(site: &Site, code: &DeviceCode) -> Result<String> {
    let mut interval = Duration::from_secs(code.interval.max(1));
    let deadline = std::time::Instant::now() + Duration::from_secs(code.expires_in);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(interval);
        let mut response = site
            .agent
            .post(format!("{}/api/auth/device/token", site.api))
            .send_json(json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": code.device_code,
                "client_id": CLIENT_ID,
            }))
            .with_context(|| format!("reaching {}", site.api))?;
        let body: Value = response.body_mut().read_json().unwrap_or_default();
        if let Some(token) = body["access_token"].as_str() {
            return Ok(token.to_owned());
        }
        match body["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += Duration::from_secs(5),
            Some("access_denied") => bail!("the sign-in was cancelled on textures.gg"),
            Some("expired_token") => break,
            _ => bail!("the sign-in failed ({})", response.status()),
        }
    }
    bail!("the code expired; run tgg login again")
}

/// `tgg logout`: end the session on the site and forget its token.
pub fn logout(api: &str) -> Result<()> {
    let site = Site::new(api)?;
    let mut saved = read_credentials()?;
    let Some(credential) = saved.remove(&site.api) else {
        println!("You're not signed in.");
        return Ok(());
    };
    // Ending the session is best effort; the token is forgotten either way.
    let _ = site
        .agent
        .post(format!("{}/api/auth/sign-out", site.api))
        .header("authorization", format!("Bearer {}", credential.token))
        .send_json(json!({}));
    write_credentials(&saved)?;
    println!("Signed out {}.", credential.name);
    Ok(())
}

/// `credentials.json` in the user's config folder, under textures.gg.
fn credentials_path() -> Result<PathBuf> {
    let config = dirs::config_dir().ok_or_else(|| anyhow!("this system has no config folder"))?;
    Ok(config.join("textures.gg").join("credentials.json"))
}

fn read_credentials() -> Result<BTreeMap<String, Credential>> {
    let path = credentials_path()?;
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| path.display().to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(error).with_context(|| path.display().to_string()),
    }
}

/// Write the credentials readable only by this user.
fn write_credentials(saved: &BTreeMap<String, Credential>) -> Result<()> {
    let path = credentials_path()?;
    let folder = path.parent().expect("the path has a folder");
    std::fs::create_dir_all(folder).with_context(|| folder.display().to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .with_context(|| path.display().to_string())?;
    serde_json::to_writer_pretty(file, saved).with_context(|| path.display().to_string())
}
