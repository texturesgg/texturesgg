//! The app's network calls. There are few, and each is one the player
//! asked for or can turn off. They block, so callers run
//! them on the background executor.

use std::time::Duration;

/// textures.gg's API, or the one `TGG_API_URL` names (a local one).
pub(crate) fn api_url() -> String {
    std::env::var("TGG_API_URL").unwrap_or_else(|_| "https://api.textures.gg".into())
}

/// No response is waited on longer than this.
const TIMEOUT: Duration = Duration::from_secs(15);

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(concat!("textures.gg-editor/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// POST `body` as JSON and return the response's text, at most
/// `limit` bytes of it.
pub(crate) fn post_json(
    url: &str,
    body: &serde_json::Value,
    limit: u64,
) -> Result<String, ureq::Error> {
    agent()
        .post(url)
        .content_type("application/json")
        .send(body.to_string())?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
}

/// GET `url` and return the response's text, at most `limit` bytes of it.
pub(crate) fn get(url: &str, limit: u64) -> Result<String, ureq::Error> {
    agent()
        .get(url)
        .call()?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// Serve one request with `status` and `body`; the URL to reach it at,
    /// and the request's body once it has been answered.
    pub(crate) fn serve_once(
        status: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let served = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("header");
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().expect("length");
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).expect("body");
            write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("respond");
            String::from_utf8(request).expect("utf-8")
        });
        (url, served)
    }
}
