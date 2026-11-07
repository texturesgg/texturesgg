//! Fetching from textures.gg and the release mirror, behind a trait so tests
//! and the app's flow tests can serve canned answers ([`crate::FakeNet`]).

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// The textures.gg API, unless `TGG_API` names another.
pub const DEFAULT_API: &str = "https://api.textures.gg";
/// Where tgg-melee releases are published, unless `TGG_RELEASES` names
/// another mirror (a URL, or a folder laid out the same way).
pub const DEFAULT_RELEASES: &str = "https://dl.textures.gg/tgg-melee";

/// Where the registry and port releases come from.
pub trait Net: Send + Sync {
    /// The body at `url`, refusing one longer than `limit` bytes.
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError>;

    /// [`Net::get`] for large files: reports the bytes read so far and the
    /// length the server gave, and stops with [`NetError::Cancelled`] once
    /// `cancel` is set.
    fn download(
        &self,
        url: &str,
        limit: u64,
        on_bytes: &mut dyn FnMut(u64, Option<u64>),
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, NetError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(NetError::Cancelled);
        }
        let body = self.get(url, limit)?;
        on_bytes(body.len() as u64, Some(body.len() as u64));
        Ok(body)
    }

    /// The textures.gg API's base URL, without a trailing `/`.
    fn api_url(&self) -> &str;

    /// The release mirror's base URL (or folder), without a trailing `/`.
    fn releases_url(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    /// Nothing answered: no connection, the host isn't found, or it timed
    /// out.
    #[error("{reason}")]
    Unreachable { url: String, reason: String },
    #[error("{url} doesn't exist")]
    NotFound { url: String },
    #[error("{url} answered {status}")]
    Status { url: String, status: u16 },
    #[error("{url} is larger than {limit} bytes")]
    TooLarge { url: String, limit: u64 },
    #[error("reading {url}: {reason}")]
    Failed { url: String, reason: String },
    #[error("cancelled")]
    Cancelled,
}

/// [`Net`] over HTTPS, or over a folder for a mirror that is one.
pub struct HttpNet {
    api: String,
    releases: String,
    agent: ureq::Agent,
}

impl HttpNet {
    /// textures.gg and its mirror, or the ones `TGG_API` and `TGG_RELEASES`
    /// name. `user_agent` names the program, such as `tgg/0.4.0`.
    pub fn new(user_agent: &str) -> Self {
        let var = |name: &str, default: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_owned())
        };
        Self::with_urls(
            user_agent,
            &var("TGG_API", DEFAULT_API),
            &var("TGG_RELEASES", DEFAULT_RELEASES),
        )
    }

    pub fn with_urls(user_agent: &str, api: &str, releases: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .timeout_connect(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            api: api.trim_end_matches('/').to_owned(),
            releases: releases.trim_end_matches('/').to_owned(),
            agent,
        }
    }

    /// A reader of `url`'s body and its length, if known.
    fn open(&self, url: &str) -> Result<(Box<dyn Read>, Option<u64>), NetError> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            let path = url.strip_prefix("file://").unwrap_or(url);
            return match std::fs::File::open(path) {
                Ok(file) => {
                    let size = file.metadata().ok().map(|meta| meta.len());
                    Ok((Box::new(file), size))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Err(NetError::NotFound { url: url.into() })
                }
                Err(error) => Err(NetError::Failed {
                    url: url.into(),
                    reason: error.to_string(),
                }),
            };
        }
        match self.agent.get(url).call() {
            Ok(response) => {
                let size = response
                    .headers()
                    .get("content-length")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok());
                Ok((Box::new(response.into_body().into_reader()), size))
            }
            // The mirror answers 403 for a file it doesn't have.
            Err(ureq::Error::StatusCode(404 | 403)) => Err(NetError::NotFound { url: url.into() }),
            Err(ureq::Error::StatusCode(status)) => Err(NetError::Status {
                url: url.into(),
                status,
            }),
            Err(
                error @ (ureq::Error::Io(_)
                | ureq::Error::Timeout(_)
                | ureq::Error::HostNotFound
                | ureq::Error::ConnectionFailed),
            ) => Err(NetError::Unreachable {
                url: url.into(),
                reason: error.to_string(),
            }),
            Err(error) => Err(NetError::Failed {
                url: url.into(),
                reason: error.to_string(),
            }),
        }
    }
}

impl Net for HttpNet {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
        self.download(url, limit, &mut |_, _| {}, &AtomicBool::new(false))
    }

    fn download(
        &self,
        url: &str,
        limit: u64,
        on_bytes: &mut dyn FnMut(u64, Option<u64>),
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, NetError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(NetError::Cancelled);
        }
        let (reader, total) = self.open(url)?;
        if total.is_some_and(|total| total > limit) {
            return Err(NetError::TooLarge {
                url: url.into(),
                limit,
            });
        }
        let mut reader = reader.take(limit + 1);
        let mut body = Vec::with_capacity(total.unwrap_or(0) as usize);
        let mut buffer = vec![0; 1 << 16];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(NetError::Cancelled);
            }
            let read = reader.read(&mut buffer).map_err(|error| NetError::Failed {
                url: url.into(),
                reason: error.to_string(),
            })?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read]);
            if body.len() as u64 > limit {
                return Err(NetError::TooLarge {
                    url: url.into(),
                    limit,
                });
            }
            on_bytes(body.len() as u64, total);
        }
        Ok(body)
    }

    fn api_url(&self) -> &str {
        &self.api
    }

    fn releases_url(&self) -> &str {
        &self.releases
    }
}
