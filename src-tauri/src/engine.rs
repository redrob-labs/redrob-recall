//! The shared Redrob Code daemon as this app reaches it.
//!
//! One `redrob serve --register` runs per user and every Redrob app attaches to it (redrob-code
//! `docs/LOCAL-ENGINE-API.md`, section 3). The daemon holds the Redrob console key and calls the
//! console with its own credential; this app sends the daemon's Basic secret, its app name and a
//! session id on every chat request and never sees a console key on that path. There is
//! deliberately no fallback to the console: two paths would hide whether masking applied.
//!
//! `redrob-engine-client` finds the record (`~/.redrob/run/server.json`, `REDROB_RUN_DIR`) and
//! starts the daemon when none answers (`REDROB_CODE_BIN`, `~/.redrob/bin/redrob`, `redrob` on
//! PATH). Its calls block, so they run off the async runtime here.

use crate::shared_account::SharedAccount;
use parking_lot::Mutex;
use redrob_engine_client::{app_headers, openai_base, Record, Unavailable};
use reqwest::{header::HeaderValue, StatusCode};
use serde_json::Value;
use std::{fmt, sync::Arc, time::Duration};

/// How this app names itself to the daemon (`x-redrob-app`), the id the console knows it by.
pub const APP_NAME: &str = "recall";

/// Where a person gets Redrob Code when it is not installed: the launcher installs it.
pub const INSTALL_URL: &str = "https://github.com/redrob-labs/redrob-launcher/releases/latest";

/// The sentence shown when no `redrob` binary exists. The link is [`INSTALL_URL`].
pub const NOT_INSTALLED_MESSAGE: &str =
    "Redrob Code is not installed. Install it with Redrob Launcher.";

/// The provider id the daemon lists the Redrob console under (`GET /provider`, `PUT /auth/redrob`).
pub const REDROB_PROVIDER_ID: &str = "redrob";

/// The daemon lists a credential-less Redrob provider with this placeholder; it is not a key.
const PUBLIC_SENTINEL: &str = "public";

/// The daemon's own answer when it holds no console credential (LOCAL-ENGINE-API.md, Errors).
pub const ENGINE_NOT_AUTHENTICATED: &str = "engine_not_authenticated";

/// How long a daemon started by this app stays up with nobody attached.
pub const DEFAULT_IDLE_EXIT_MINUTES: u32 = 10;

/// What the app renders for a missing engine: the install sentence with its link, or the start
/// failure's own reason. No fallback is named because there is none.
pub fn unavailable_message(unavailable: &Unavailable) -> String {
    match unavailable {
        Unavailable::NotInstalled => format!("{NOT_INSTALLED_MESSAGE} {INSTALL_URL}"),
        Unavailable::StartFailed(reason) => {
            format!("The Redrob Code engine could not be started: {reason}")
        }
    }
}

/// Why a daemon call did not produce an answer. HTTP statuses are not errors here: the caller
/// reads them off the response, because Ask classifies them for the person and sign-in only
/// needs the boolean.
#[derive(Debug)]
pub enum EngineError {
    /// No daemon to talk to, told apart as the two sentences the UI shows.
    Unavailable(Unavailable),
    /// The daemon could not be reached or answered no readable response.
    Transport(reqwest::Error),
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Unavailable(unavailable) => {
                formatter.write_str(&unavailable_message(unavailable))
            }
            EngineError::Transport(error) if error.is_timeout() => {
                formatter.write_str("Redrob Code took too long to respond. Try again.")
            }
            EngineError::Transport(_) => formatter
                .write_str("Redrob Code could not be reached. Open Redrob Launcher and try again."),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<reqwest::Error> for EngineError {
    fn from(error: reqwest::Error) -> Self {
        EngineError::Transport(error)
    }
}

/// Whether the daemon is signed in to the Redrob console, from its provider list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountStatus {
    pub connected: bool,
    /// Where the daemon's key came from when it has one: `api` (handed over by an app), `env`,
    /// `config`, or `launcher` (Redrob Launcher's shared sign-in, which the engine reads itself).
    pub source: Option<String>,
}

impl AccountStatus {
    pub const fn signed_out() -> Self {
        Self {
            connected: false,
            source: None,
        }
    }

    /// `true` when the key in use is Redrob Launcher's, so the app says so and offers no sign-out.
    pub fn is_from_launcher(&self) -> bool {
        self.connected && self.source.as_deref() == Some("launcher")
    }

    /// Reads `GET /provider`: a `redrob` entry with a credential means signed in. The shape is
    /// `{"all": [{"id", "source", "key", "options": {"apiKey"}}]}`.
    pub fn from_provider_list(list: &Value) -> Self {
        let Some(entry) = list.get("all").and_then(Value::as_array).and_then(|all| {
            all.iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(REDROB_PROVIDER_ID))
        }) else {
            return Self::signed_out();
        };
        if entry
            .pointer("/options/apiKey")
            .and_then(Value::as_str)
            .is_some_and(|key| key == PUBLIC_SENTINEL)
        {
            return Self::signed_out();
        }
        let source = entry.get("source").and_then(Value::as_str);
        let has_key = entry
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|key| !key.trim().is_empty());
        if !(matches!(source, Some("api" | "env" | "launcher")) || has_key) {
            return Self::signed_out();
        }
        Self {
            connected: true,
            source: Some(
                source
                    .filter(|known| matches!(*known, "api" | "env" | "config" | "launcher"))
                    .unwrap_or("api")
                    .to_owned(),
            ),
        }
    }
}

/// Branches on the daemon's error `code`, never on its message, which is localized.
pub fn names_engine_not_authenticated(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/code")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|code| code == ENGINE_NOT_AUTHENTICATED)
}

type Resolver = Arc<dyn Fn() -> Result<Record, Unavailable> + Send + Sync>;

/// The daemon as this app holds it: found (or started) on the first call and kept. The record's
/// secret rotates on every daemon start, so a 401 or a refused connection re-reads the record once
/// and retries once; a second failure is reported.
#[derive(Clone)]
pub struct Engine {
    http: reqwest::Client,
    /// This app run's id, sent as `x-redrob-session` so the console groups its requests.
    session: String,
    record: Arc<Mutex<Option<Record>>>,
    resolve: Resolver,
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("session", &self.session)
            .field(
                "daemon",
                &self.record.lock().as_ref().map(|record| record.url.clone()),
            )
            .finish()
    }
}

impl Engine {
    /// Takes no network action: the daemon is looked up when the first call needs it.
    pub fn new() -> Self {
        Self::with_resolver(Arc::new(|| {
            redrob_engine_client::require_as(APP_NAME, DEFAULT_IDLE_EXIT_MINUTES)
        }))
    }

    /// `new` with the daemon lookup replaced, so tests point at a fake daemon without the real
    /// environment or a `redrob` binary.
    pub fn with_resolver(resolve: Resolver) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .user_agent(concat!("Redrob-Recall/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the HTTP client configuration is static");
        Self {
            http,
            session: uuid::Uuid::new_v4().to_string(),
            record: Arc::new(Mutex::new(None)),
            resolve,
        }
    }

    /// The daemon in hand, or found (or started) now, so the UI can say "not installed" before
    /// the first question.
    pub async fn connect(&self) -> Result<Record, EngineError> {
        if let Some(record) = self.record.lock().clone() {
            return Ok(record);
        }
        self.reconnect().await
    }

    async fn reconnect(&self) -> Result<Record, EngineError> {
        let resolve = self.resolve.clone();
        let record = tokio::task::spawn_blocking(move || resolve())
            .await
            .map_err(|_| {
                EngineError::Unavailable(Unavailable::StartFailed(
                    "the engine lookup was cancelled".into(),
                ))
            })?
            .map_err(EngineError::Unavailable)?;
        *self.record.lock() = Some(record.clone());
        Ok(record)
    }

    /// Sends one request to the daemon, retrying once with a fresh record when the daemon has
    /// evidently restarted (connection refused, or 401 against the secret in hand).
    async fn send<F>(&self, build: F) -> Result<reqwest::Response, EngineError>
    where
        F: Fn(&Record) -> reqwest::RequestBuilder,
    {
        let record = self.connect().await?;
        let first = self.with_headers(&record, build(&record)).send().await;
        let retry = match &first {
            Ok(response) => response.status() == StatusCode::UNAUTHORIZED,
            Err(error) => error.is_connect() || error.is_request(),
        };
        if !retry {
            return Ok(first?);
        }
        *self.record.lock() = None;
        let record = self.reconnect().await?;
        Ok(self.with_headers(&record, build(&record)).send().await?)
    }

    fn with_headers(
        &self,
        record: &Record,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        let mut builder = builder;
        for (name, value) in app_headers(record, APP_NAME, Some(&self.session)) {
            let Ok(mut header) = HeaderValue::from_str(&value) else {
                continue;
            };
            if name == "Authorization" {
                header.set_sensitive(true);
            }
            builder = builder.header(name, header);
        }
        builder
    }

    /// `<daemon>/v1/<path>`: the OpenAI-compatible route.
    fn openai_endpoint(record: &Record, path: &str) -> String {
        format!("{}/{path}", openai_base(record).trim_end_matches('/'))
    }

    /// `<daemon>/<path>`: the daemon's own routes (`/provider`, `/auth/<provider>`).
    fn daemon_endpoint(record: &Record, path: &str) -> String {
        format!("{}/{path}", record.url.trim_end_matches('/'))
    }

    /// `POST <daemon>/v1/chat/completions`, the same OpenAI-compatible shape the console takes.
    /// The response comes back whole: Ask classifies its status and reads its body with limits.
    pub async fn chat_completions<T: serde::Serialize>(
        &self,
        body: &T,
    ) -> Result<reqwest::Response, EngineError> {
        self.send(|record| {
            self.http
                .post(Self::openai_endpoint(record, "chat/completions"))
                .json(body)
        })
        .await
    }

    /// `POST <apiBaseUrl>/chat/completions` with Redrob Launcher's shared key as the bearer: the
    /// shared-account contract's own path, used when the launcher has signed this computer in. The
    /// same OpenAI-compatible shape; the app and session labels travel as on the daemon path.
    pub async fn console_chat_completions<T: serde::Serialize>(
        &self,
        account: &SharedAccount,
        body: &T,
    ) -> Result<reqwest::Response, EngineError> {
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", account.api_key))
            .map_err(|_| {
                EngineError::Unavailable(Unavailable::StartFailed(
                    "the shared Redrob key contains characters that cannot be sent".into(),
                ))
            })?;
        authorization.set_sensitive(true);
        Ok(self
            .http
            .post(account.endpoint("chat/completions"))
            .header("Authorization", authorization)
            .header("x-redrob-app", APP_NAME)
            .header("x-redrob-session", &self.session)
            .json(body)
            .send()
            .await?)
    }

    /// Whether the daemon is signed in to the Redrob console (`GET /provider`), and with whose key.
    pub async fn account_status(&self) -> Result<AccountStatus, EngineError> {
        let response = self
            .send(|record| self.http.get(Self::daemon_endpoint(record, "provider")))
            .await?;
        if !response.status().is_success() {
            return Ok(AccountStatus::signed_out());
        }
        let list: Value = response.json().await?;
        Ok(AccountStatus::from_provider_list(&list))
    }

    /// Hands a console key to the daemon (`PUT /auth/redrob`), which keeps it from here on. A
    /// second call rotates it. The key is consumed: nothing in this app holds it afterwards.
    pub async fn sign_in(&self, key: &str) -> Result<(), EngineError> {
        let body = serde_json::json!({"type": "api", "key": key});
        let response = self
            .send(|record| {
                self.http
                    .put(Self::daemon_endpoint(
                        record,
                        &format!("auth/{REDROB_PROVIDER_ID}"),
                    ))
                    .json(&body)
            })
            .await?;
        // The daemon's failure body could echo the payload; only the status is kept.
        if !response.status().is_success() {
            return Err(EngineError::Transport(
                response
                    .error_for_status()
                    .expect_err("a non-success status is an error"),
            ));
        }
        Ok(())
    }

    /// Removes the console key from the daemon (`DELETE /auth/redrob`). Already gone is fine.
    pub async fn sign_out(&self) -> Result<(), EngineError> {
        let response = self
            .send(|record| {
                self.http.delete(Self::daemon_endpoint(
                    record,
                    &format!("auth/{REDROB_PROVIDER_ID}"),
                ))
            })
            .await?;
        if response.status() == StatusCode::NOT_FOUND || response.status().is_success() {
            return Ok(());
        }
        Err(EngineError::Transport(
            response
                .error_for_status()
                .expect_err("a non-success status is an error"),
        ))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{
        names_engine_not_authenticated, unavailable_message, AccountStatus, Engine, EngineError,
        INSTALL_URL,
    };
    use redrob_engine_client::{Record, Unavailable};
    use serde_json::json;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
    };

    /// Environment-touching tests share the process environment, so they take turns.
    static ENV: Mutex<()> = Mutex::new(());

    /// One request as the fake daemon saw it.
    #[derive(Debug, Clone)]
    pub(crate) struct Seen {
        pub method: String,
        pub path: String,
        pub authorization: Option<String>,
        pub app: Option<String>,
        pub session: Option<String>,
        pub body: String,
    }

    /// A tiny daemon on 127.0.0.1: answers `/api/health`, and everything else with the status and
    /// JSON the test chose, recording what it saw. One thread, one request at a time.
    pub(crate) struct FakeDaemon {
        pub url: String,
        pub seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl FakeDaemon {
        /// `answer(path, authorized) -> (status, json body)`, for every route but `/api/health`.
        pub(crate) fn start(
            secret: &'static str,
            answer: impl Fn(&Seen, bool) -> (u16, String) + Send + 'static,
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let seen = Arc::new(Mutex::new(Vec::new()));
            let log = seen.clone();
            let expected = redrob_engine_client::authorization(secret);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
                        continue;
                    }
                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or_default().to_string();
                    let path = parts.next().unwrap_or_default().to_string();
                    let mut request = Seen {
                        method,
                        path,
                        authorization: None,
                        app: None,
                        session: None,
                        body: String::new(),
                    };
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                            break;
                        }
                        let Some((name, value)) = line.split_once(':') else {
                            continue;
                        };
                        let value = value.trim().to_string();
                        match name.to_ascii_lowercase().as_str() {
                            "authorization" => request.authorization = Some(value),
                            "x-redrob-app" => request.app = Some(value),
                            "x-redrob-session" => request.session = Some(value),
                            "content-length" => length = value.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                    let mut body = vec![0; length];
                    if length > 0 {
                        reader.read_exact(&mut body).unwrap();
                    }
                    request.body = String::from_utf8_lossy(&body).to_string();
                    let authorized = request.authorization.as_deref() == Some(expected.as_str());
                    let (status, body) = if request.path == "/api/health" {
                        if authorized {
                            (200, r#"{"healthy":true}"#.to_string())
                        } else {
                            (401, r#"{"error":{"code":"unauthorized"}}"#.to_string())
                        }
                    } else {
                        answer(&request, authorized)
                    };
                    log.lock().unwrap().push(request);
                    let reason = match status {
                        200 => "OK",
                        401 => "Unauthorized",
                        404 => "Not Found",
                        _ => "Status",
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.flush();
                }
            });
            Self { url, seen }
        }

        pub(crate) fn record(&self, secret: &str) -> Record {
            Record {
                id: "daemon-1".into(),
                version: "0.6.3".into(),
                url: self.url.clone(),
                port: 0,
                pid: 1,
                secret: secret.into(),
            }
        }

        pub(crate) fn engine(&self, secret: &'static str) -> Engine {
            let record = self.record(secret);
            Engine::with_resolver(Arc::new(move || Ok(record.clone())))
        }
    }

    #[test]
    fn a_missing_engine_renders_the_install_sentence_and_the_launcher_link() {
        let message = unavailable_message(&Unavailable::NotInstalled);
        assert_eq!(
            message,
            format!("Redrob Code is not installed. Install it with Redrob Launcher. {INSTALL_URL}")
        );
        assert_eq!(
            unavailable_message(&Unavailable::StartFailed("port busy".into())),
            "The Redrob Code engine could not be started: port busy"
        );
    }

    #[test]
    fn the_provider_list_tells_signed_in_from_public_and_absent() {
        assert_eq!(
            AccountStatus::from_provider_list(&json!({"all": []})),
            AccountStatus::signed_out()
        );
        assert_eq!(
            AccountStatus::from_provider_list(
                &json!({"all": [{"id": "redrob", "options": {"apiKey": "public"}}]})
            ),
            AccountStatus::signed_out()
        );
        assert_eq!(
            AccountStatus::from_provider_list(
                &json!({"all": [{"id": "redrob", "source": "config"}]})
            ),
            AccountStatus::signed_out(),
            "a config entry without a key is not a sign-in"
        );
        let launcher = AccountStatus::from_provider_list(
            &json!({"all": [{"id": "redrob", "source": "launcher"}]}),
        );
        assert!(launcher.connected && launcher.is_from_launcher());
        let api = AccountStatus::from_provider_list(
            &json!({"all": [{"id": "openai"}, {"id": "redrob", "source": "api", "key": "rrk_x"}]}),
        );
        assert_eq!(api.source.as_deref(), Some("api"));
        assert!(!api.is_from_launcher());
    }

    #[test]
    fn the_daemon_sign_in_code_is_read_from_the_error_code_not_the_message() {
        assert!(names_engine_not_authenticated(
            r#"{"error":{"message":"로그인이 필요합니다","code":"engine_not_authenticated"}}"#
                .as_bytes()
        ));
        assert!(!names_engine_not_authenticated(
            br#"{"error":{"message":"engine_not_authenticated"}}"#
        ));
        assert!(!names_engine_not_authenticated(b"not json"));
    }

    #[test]
    fn without_a_redrob_binary_the_real_lookup_says_not_installed() {
        let _guard = ENV
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!(
            "redrob-recall-engine-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("REDROB_CODE_BIN", dir.join("no-such-redrob"));
        std::env::set_var("REDROB_RUN_DIR", &dir);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {

        let engine = Engine::new();
        let error = engine.connect().await.unwrap_err();
        assert!(matches!(
            error,
            EngineError::Unavailable(Unavailable::NotInstalled)
        ));
        assert_eq!(
            error.to_string(),
            "Redrob Code is not installed. Install it with Redrob Launcher. https://github.com/redrob-labs/redrob-launcher/releases/latest"
        );
        // Sign-in and status have nowhere to go either, and say the same thing.
        assert_eq!(
            engine.account_status().await.unwrap_err().to_string(),
            error.to_string()
        );
        assert_eq!(
            engine
                .sign_in("rk_0123456789abcdef")
                .await
                .unwrap_err()
                .to_string(),
            error.to_string()
        );

        });
        std::env::remove_var("REDROB_CODE_BIN");
        std::env::remove_var("REDROB_RUN_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn chat_goes_to_the_daemon_with_its_authorization_and_the_app_name() {
        let daemon = FakeDaemon::start("s3cret", |seen, authorized| {
            assert!(authorized);
            assert_eq!(seen.path, "/v1/chat/completions");
            (
                200,
                r#"{"choices":[{"message":{"role":"assistant","content":"42 [1]"}}],"model":"auto"}"#
                    .into(),
            )
        });
        let engine = daemon.engine("s3cret");
        let response = engine
            .chat_completions(&json!({"model": "auto", "messages": []}))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let seen = daemon.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "POST");
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some(redrob_engine_client::authorization("s3cret").as_str())
        );
        assert_eq!(seen[0].app.as_deref(), Some("recall"));
        assert!(seen[0].session.as_ref().is_some_and(|s| !s.is_empty()));
        assert!(seen[0].body.contains(r#""model":"auto""#));
    }

    #[tokio::test]
    async fn a_stale_secret_re_reads_the_record_once_and_retries() {
        let daemon = FakeDaemon::start("fresh", |_, authorized| {
            if authorized {
                (
                    200,
                    r#"{"all":[{"id":"redrob","source":"launcher"}]}"#.into(),
                )
            } else {
                (401, r#"{"error":{"code":"unauthorized"}}"#.into())
            }
        });
        // The engine starts with the secret of a daemon that has since restarted.
        let stale = daemon.record("stale");
        let fresh = daemon.record("fresh");
        let lookups = Arc::new(Mutex::new(0));
        let count = lookups.clone();
        let engine = Engine::with_resolver(Arc::new(move || {
            let mut count = count.lock().unwrap();
            *count += 1;
            Ok(if *count == 1 {
                stale.clone()
            } else {
                fresh.clone()
            })
        }));
        let status = engine.account_status().await.unwrap();
        assert!(status.connected && status.is_from_launcher());
        assert_eq!(*lookups.lock().unwrap(), 2, "one re-read, no more");
        let seen = daemon.seen.lock().unwrap();
        let provider_calls: Vec<_> = seen.iter().filter(|s| s.path == "/provider").collect();
        assert_eq!(provider_calls.len(), 2);
        assert_ne!(
            provider_calls[0].authorization,
            provider_calls[1].authorization
        );
    }

    #[tokio::test]
    async fn sign_in_hands_the_key_to_the_daemon_and_sign_out_takes_it_back() {
        let daemon = FakeDaemon::start("s", |seen, _| {
            match (seen.method.as_str(), seen.path.as_str()) {
                ("PUT", "/auth/redrob") => (200, "true".into()),
                ("DELETE", "/auth/redrob") => (404, r#"{"error":"none"}"#.into()),
                _ => (404, "{}".into()),
            }
        });
        let engine = daemon.engine("s");
        engine.sign_in("rk_0123456789abcdef").await.unwrap();
        engine.sign_out().await.unwrap();
        let seen = daemon.seen.lock().unwrap();
        let put = seen.iter().find(|s| s.method == "PUT").unwrap();
        assert_eq!(put.path, "/auth/redrob");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&put.body).unwrap(),
            json!({"type": "api", "key": "rk_0123456789abcdef"})
        );
        assert!(seen
            .iter()
            .any(|s| s.method == "DELETE" && s.path == "/auth/redrob"));
    }

    #[test]
    fn the_engine_debug_output_names_no_secret() {
        let daemon = FakeDaemon::start("top-secret", |_, _| (200, "{}".into()));
        let engine = daemon.engine("top-secret");
        *engine.record.lock() = Some(daemon.record("top-secret"));
        let shown = format!("{engine:?}");
        assert!(shown.contains(&daemon.url));
        assert!(!shown.contains("top-secret"));
    }
}
