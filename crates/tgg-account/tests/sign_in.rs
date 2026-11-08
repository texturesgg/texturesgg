//! Sign-in against a fake textures.gg API on a local port.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tgg_account::{Account, Credential, Credentials, Error, Poll, Site};
use tgg_mod::Net;

/// A request the fake API received.
#[derive(Debug, Clone)]
struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Value,
}

type Handler = dyn Fn(&Request) -> (u16, Value) + Send + Sync;

/// An HTTP server answering each request with `handler`, one connection per
/// request, and keeping what it was asked.
struct FakeApi {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl FakeApi {
    fn start(handler: impl Fn(&Request) -> (u16, Value) + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let handler: Arc<Handler> = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                let (status, body) = handler(&request);
                seen.lock().unwrap().push(request);
                let body = body.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { url, requests }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut length = 0;
    let mut authorization = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':')?;
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.trim().parse().ok()?,
            "authorization" => authorization = Some(value.trim().to_owned()),
            _ => {}
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        authorization,
        body: serde_json::from_slice(&body).unwrap_or_default(),
    })
}

fn device_code(expires_in: u64) -> Value {
    json!({
        "device_code": "device-1",
        "user_code": "ABCDEFGH",
        "verification_uri": "https://textures.gg/device",
        "verification_uri_complete": "https://textures.gg/device?user_code=ABCDEFGH",
        "expires_in": expires_in,
        "interval": 1,
    })
}

/// `/api/users/me` for the token `granted`.
fn me(request: &Request) -> (u16, Value) {
    if request.authorization.as_deref() == Some("Bearer granted") {
        (
            200,
            json!({ "id": "user-1", "name": "someone", "email": "a@b.c" }),
        )
    } else {
        (401, json!({ "error": "Unauthorized" }))
    }
}

fn site(api: &FakeApi, folder: &tempfile::TempDir) -> Site {
    let credentials = Credentials::at(folder.path().join("textures.gg").join("credentials.json"));
    Site::with_credentials(&api.url, "tgg-account-test", credentials)
}

fn someone() -> Account {
    Account {
        handle: "someone".into(),
        display_name: None,
        id: Some("user-1".into()),
    }
}

#[test]
fn device_login_polls_until_confirmed_then_saves_the_token() {
    let polls = Mutex::new(0);
    let api = FakeApi::start(move |request| match request.path.as_str() {
        "/api/auth/device/code" => (200, device_code(900)),
        "/api/auth/device/token" => {
            let mut polls = polls.lock().unwrap();
            *polls += 1;
            match *polls {
                1 => (400, json!({ "error": "authorization_pending" })),
                2 => (400, json!({ "error": "slow_down" })),
                _ => (
                    200,
                    json!({ "access_token": "granted", "token_type": "Bearer" }),
                ),
            }
        }
        "/api/users/me" => me(request),
        _ => (404, json!({ "error": "not found" })),
    });
    let folder = tempfile::tempdir().unwrap();
    let site = site(&api, &folder);
    assert_eq!(site.whoami().unwrap(), None);

    let login = site.begin_device_login().unwrap();
    assert_eq!(login.display_code(), "ABCD-EFGH");
    assert_eq!(
        login.link(),
        "https://textures.gg/device?user_code=ABCDEFGH"
    );
    assert_eq!(site.poll_device_login(&login).unwrap(), Poll::Pending);
    assert_eq!(site.poll_device_login(&login).unwrap(), Poll::SlowDown);
    assert_eq!(
        site.poll_device_login(&login).unwrap(),
        Poll::Done(someone())
    );

    let code = &api.requests()[0];
    assert_eq!(code.method, "POST");
    assert_eq!(code.body["client_id"], "tgg-cli");
    let poll = &api.requests()[1];
    assert_eq!(poll.body["device_code"], "device-1");

    // Saved, and used for requests as the user.
    let saved = site.credentials().get(&api.url).unwrap().unwrap();
    assert_eq!(saved.token, "granted");
    assert_eq!(saved.account(), someone());
    let signed_in = site.whoami().unwrap().unwrap();
    assert!(signed_in.verified);
    assert_eq!(signed_in.account, someone());
    let mine: Value = site.get("/api/users/me").unwrap();
    assert_eq!(mine["name"], "someone");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(site.credentials().path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn blocking_login_shows_the_code_and_returns_the_account() {
    let api = FakeApi::start(|request| match request.path.as_str() {
        "/api/auth/device/code" => (200, device_code(900)),
        "/api/auth/device/token" => (200, json!({ "access_token": "granted" })),
        "/api/users/me" => me(request),
        _ => (404, json!({})),
    });
    let folder = tempfile::tempdir().unwrap();
    let site = site(&api, &folder);
    let mut shown = None;
    let account = site
        .login(false, |login| shown = Some(login.display_code()))
        .unwrap();
    assert_eq!(account, someone());
    assert_eq!(shown.as_deref(), Some("ABCD-EFGH"));
}

#[test]
fn a_login_ends_when_the_code_expires_or_is_cancelled() {
    let answer = Arc::new(Mutex::new("expired_token"));
    let error = answer.clone();
    let api = FakeApi::start(move |request| match request.path.as_str() {
        "/api/auth/device/code" => (200, device_code(900)),
        "/api/auth/device/token" => (400, json!({ "error": *error.lock().unwrap() })),
        _ => (404, json!({})),
    });
    let folder = tempfile::tempdir().unwrap();
    let site = site(&api, &folder);

    let login = site.begin_device_login().unwrap();
    assert_eq!(site.poll_device_login(&login).unwrap(), Poll::Expired);
    *answer.lock().unwrap() = "access_denied";
    assert_eq!(site.poll_device_login(&login).unwrap(), Poll::Denied);
    assert_eq!(site.login(false, |_| {}), Err(Error::Denied));

    // A code past its expiry isn't polled at all.
    let mut late = site.begin_device_login().unwrap();
    late.expires_at = std::time::Instant::now();
    let before = api.requests().len();
    assert_eq!(site.poll_device_login(&late).unwrap(), Poll::Expired);
    assert_eq!(api.requests().len(), before);
    assert_eq!(site.whoami().unwrap(), None);
}

#[test]
fn credentials_read_the_existing_file_and_keep_other_entries() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("credentials.json");
    // As `tgg login` has always written it.
    std::fs::write(
        &path,
        r#"{
  "https://api.textures.gg": { "token": "old", "name": "someone" },
  "http://localhost:8787": { "token": "local", "name": "dev", "future": 1 }
}"#,
    )
    .unwrap();
    let credentials = Credentials::at(&path);

    let saved = credentials
        .get("https://api.textures.gg/")
        .unwrap()
        .unwrap();
    assert_eq!(saved.token, "old");
    assert_eq!(
        saved.account(),
        Account {
            handle: "someone".into(),
            display_name: None,
            id: None
        }
    );

    let new = Credential::new("new", &someone());
    credentials.set("https://api.textures.gg", &new).unwrap();
    assert_eq!(
        credentials.get("https://api.textures.gg").unwrap(),
        Some(new)
    );
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["http://localhost:8787"]["future"], 1);
    assert_eq!(file["https://api.textures.gg"]["name"], "someone");

    assert!(
        credentials
            .remove("https://api.textures.gg")
            .unwrap()
            .is_some()
    );
    assert_eq!(credentials.get("https://api.textures.gg").unwrap(), None);
    assert_eq!(
        credentials
            .get("http://localhost:8787")
            .unwrap()
            .unwrap()
            .token,
        "local"
    );

    std::fs::write(&path, "not json").unwrap();
    assert!(matches!(
        credentials.get("https://api.textures.gg"),
        Err(Error::Credentials { .. })
    ));
}

#[test]
fn whoami_offline_answers_the_saved_account_and_expired_tokens_are_errors() {
    let folder = tempfile::tempdir().unwrap();
    // A port nothing listens on.
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let credentials = Credentials::at(folder.path().join("credentials.json"));
    credentials
        .set(&closed, &Credential::new("granted", &someone()))
        .unwrap();
    let offline = Site::with_credentials(&closed, "test", credentials.clone());
    let signed_in = offline.whoami().unwrap().unwrap();
    assert!(!signed_in.verified);
    assert_eq!(signed_in.account, someone());
    assert!(matches!(
        offline.get::<Value>("/api/users/me"),
        Err(Error::Offline { .. })
    ));

    let api = FakeApi::start(|_| (401, json!({ "error": "Unauthorized" })));
    credentials
        .set(&api.url, &Credential::new("revoked", &someone()))
        .unwrap();
    let site = Site::with_credentials(&api.url, "test", credentials);
    assert_eq!(site.whoami(), Err(Error::SessionExpired));
    assert!(matches!(site.signed_in(), Err(Error::SessionExpired)));
}

#[test]
fn logout_ends_the_session_and_forgets_the_token() {
    let api = FakeApi::start(|_| (200, json!({ "success": true })));
    let folder = tempfile::tempdir().unwrap();
    let site = site(&api, &folder);
    assert_eq!(site.logout().unwrap(), None);
    assert!(matches!(
        site.get::<Value>("/api/users/me"),
        Err(Error::NotSignedIn)
    ));

    site.credentials()
        .set(&api.url, &Credential::new("granted", &someone()))
        .unwrap();
    assert_eq!(site.logout().unwrap(), Some(someone()));
    let sign_out = &api.requests()[0];
    assert_eq!(sign_out.path, "/api/auth/sign-out");
    assert_eq!(sign_out.authorization.as_deref(), Some("Bearer granted"));
    assert_eq!(site.token().unwrap(), None);
}

#[test]
fn the_registry_net_sends_the_token_to_the_api_only() {
    let api = FakeApi::start(|_| (200, json!({})));
    let folder = tempfile::tempdir().unwrap();
    let base = format!("{}/api-base", api.url);
    let credentials = Credentials::at(folder.path().join("credentials.json"));
    credentials
        .set(&base, &Credential::new("granted", &someone()))
        .unwrap();
    let site = Site::with_credentials(&base, "test", credentials);
    let http = tgg_mod::HttpNet::with_urls("test", &base, &format!("{}/releases", api.url));
    let net = site.net(http).unwrap();

    net.get(&format!("{base}/catalog"), 1024).unwrap();
    net.get(&format!("{}/releases/latest.json", api.url), 1024)
        .unwrap();
    net.get(&format!("{base}-elsewhere/x"), 1024).unwrap();
    let sent: Vec<_> = api
        .requests()
        .into_iter()
        .map(|request| (request.path, request.authorization))
        .collect();
    assert_eq!(
        sent,
        [
            ("/api-base/catalog".into(), Some("Bearer granted".into())),
            ("/releases/latest.json".into(), None),
            ("/api-base-elsewhere/x".into(), None),
        ]
    );
}
