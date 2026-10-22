//! v8.0.4 auth suite: the interactive login handshake (`LoginFlow`/`PendingLogin`) and the
//! network-facing half of `AuthApi` (`login`/`resend`/`verify`, `logout`, the local
//! `set_session_token`/`clear_*` scopes), all against a local wiremock server per the crate's
//! testing conventions.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use rstest::rstest;
use secrecy::ExposeSecret;
use serde_json::json;
use tracing::Level;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{
    MockEero, TEST_TOKEN, capture_logs, fixture, fixture_json, session_cookie, user_token_header,
    user_token_header_for,
};
use rusteero::api::EeroApi;
use rusteero::auth::flow::{LoginFlow, PendingLogin};
use rusteero::auth::{AuthApi, Session};
use rusteero::error::{Error, StorageError};
use rusteero::storage::{ChainedStore, CredentialStore, FileStore, MemoryStore};
use rusteero::transport::{RequestBody, StorageFailures, Transport};

/// The `data.user_token` value baked into `fixtures/login.json`.
fn login_fixture_token() -> String {
    fixture_json("login.json")["data"]["user_token"]
        .as_str()
        .expect("fixtures/login.json always carries a string data.user_token")
        .to_owned()
}

/// Mounts the shared `/2.2/login` mock (responding with `fixtures/login.json`) and completes
/// `LoginFlow::start`, handing back the resulting `PendingLogin` together with the login token
/// `fixtures/login.json` carries.
async fn start_pending_login(mock: &MockEero) -> anyhow::Result<(PendingLogin, String)> {
    start_pending_login_on(mock, mock.login_flow()).await
}

/// As [`start_pending_login`], on a caller-built flow: one whose transport shares a credential
/// store with something the test inspects, so the flow touching that store can fail the test.
async fn start_pending_login_on(
    mock: &MockEero,
    flow: LoginFlow,
) -> anyhow::Result<(PendingLogin, String)> {
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let pending = flow.start("you@example.com").await?;
    Ok((pending, login_fixture_token()))
}

// ===================== LoginFlow::start: form encoding =====================

#[tokio::test]
async fn login_flow_start_posts_a_form_encoded_body_unauthenticated() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("login=you%40example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let pending = mock.login_flow().start("you@example.com").await?;

    // `PendingLogin` keeps its login token private; the only way to prove it holds
    // `fixtures/login.json`'s `data.user_token` from an external test crate is to observe the
    // *next* request it issues carrying it as `X-User-Token`.
    Mock::given(method("POST"))
        .and(path("/2.2/login/resend"))
        .and(user_token_header_for(&login_fixture_token()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;
    pending.resend().await?;
    Ok(())
}

/// A `LoginFlow` built on a transport with an overridden User-Agent sends it on the login
/// handshake too (`eero-api` 8.0.5 threads `user_agent` through `AuthAPI`).
#[tokio::test]
async fn login_flow_start_sends_the_transports_user_agent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(header("user-agent", rusteero::consts::LEGACY_USER_AGENT))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("login=user%40example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let transport = Transport::builder()
        .base_url(mock.uri())
        .user_agent(Some(rusteero::consts::LEGACY_USER_AGENT.to_owned()))
        .build()?;
    LoginFlow::with_transport(transport)
        .start("user@example.com")
        .await?;
    Ok(())
}

#[tokio::test]
async fn login_flow_start_with_a_phone_number_sends_an_identical_request_shape()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(body_string("login=%2B15555550100"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    mock.login_flow().start("+15555550100").await?;
    Ok(())
}

#[tokio::test]
async fn login_flow_start_missing_user_token_is_authentication_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({ "meta": { "code": 200 }, "data": {} }).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a response with no data.user_token at all must fail");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Login failed: No user token received")
    );
    Ok(())
}

#[tokio::test]
async fn login_flow_start_empty_user_token_is_authentication_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            json!({ "meta": { "code": 200 }, "data": { "user_token": "" } }).to_string(),
        ))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("an empty-string data.user_token must fail");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Login failed: No user token received")
    );
    Ok(())
}

#[tokio::test]
async fn login_flow_start_a_400_validation_error_is_wrapped_as_authentication() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"meta":{"code":400,"error":"error.form.email.malformed"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("not-an-email")
        .await
        .expect_err("a server-rejected identifier must fail");
    match err {
        Error::Authentication { message, .. } => {
            assert!(message.starts_with("Login failed: "));
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn login_flow_start_a_401_propagates_unwrapped() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a 401 during login must still surface as Error::Authentication");
    match err {
        Error::Authentication { message, .. } => {
            assert!(
                !message.starts_with("Login failed: "),
                "a real 401 must not be re-wrapped with the Login-failed prefix: {message}"
            );
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

// ===================== PendingLogin::resend: JSON body (the one asymmetric case) =====================

#[tokio::test]
async fn pending_login_resend_posts_json_empty_body_with_the_login_token() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/resend"))
        .and(user_token_header_for(&login_token))
        .and(header("content-type", "application/json"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let envelope = pending.resend().await?;
    assert_eq!(envelope.into_value(), json!({}));
    Ok(())
}

// ===================== PendingLogin::verify: form encoding =====================

#[tokio::test]
async fn pending_login_verify_posts_a_form_encoded_body_with_the_login_token() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .and(user_token_header_for(&login_token))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("code=123456"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert!(session.is_valid());
    Ok(())
}

#[tokio::test]
async fn pending_login_verify_response_is_discarded_the_login_token_becomes_the_session()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let session = pending.verify("123456").await?;
    assert_eq!(session.token().expose_secret(), login_token);
    Ok(())
}

#[tokio::test]
async fn pending_login_verify_a_401_propagates_unwrapped() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, _login_token) = start_pending_login(&mock).await?;

    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.verification.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = pending
        .verify("000000")
        .await
        .expect_err("a wrong code must surface as Error::Authentication");
    match err {
        Error::Authentication { message, .. } => {
            assert!(
                !message.starts_with("Verification failed: "),
                "a real 401 must not be re-wrapped: {message}"
            );
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

// ===================== AuthApi::set_session_token =====================

#[tokio::test]
async fn auth_api_set_session_token_empty_is_validation_error_with_no_requests() {
    let mock = MockEero::start().await;
    let auth = AuthApi::new(mock.transport_anonymous());

    let err = auth
        .set_session_token("")
        .await
        .expect_err("an empty token must be rejected before any request");
    assert!(matches!(
        err,
        Error::Validation { ref field, ref message, .. }
            if field == "token" && message == "must be a non-empty string"
    ));
}

#[tokio::test]
async fn auth_api_set_session_token_installs_a_session_and_persists_it_to_the_store()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder()
        .base_url(mock.uri())
        .store(Some(Arc::clone(&store)))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);
    assert!(!auth.is_authenticated());

    auth.set_session_token("t").await?;

    assert!(auth.is_authenticated());
    let persisted = store.load()?;
    assert_eq!(persisted.token().expose_secret(), "t");
    Ok(())
}

// ===================== AuthApi::clear_session_token / clear_auth_data: now identical =====================

async fn assert_clear_scope_clears_everything(
    clear: impl AsyncFn(&AuthApi) -> Result<(), Error>,
) -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token("tok")))
        .store(Some(Arc::clone(&store)))
        .build()
        .expect("a `MockServer`'s own URI is always a valid base URL");
    let auth = AuthApi::new(transport);

    clear(&auth).await?;

    assert!(!auth.is_authenticated());
    assert!(auth.session().is_none());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_clear_session_token_clears_everything() -> anyhow::Result<()> {
    assert_clear_scope_clears_everything(AuthApi::clear_session_token).await
}

#[tokio::test]
async fn auth_api_clear_auth_data_clears_everything() -> anyhow::Result<()> {
    assert_clear_scope_clears_everything(AuthApi::clear_auth_data).await
}

// ===================== AuthApi::ensure_authenticated =====================

#[tokio::test]
async fn auth_api_ensure_authenticated_with_no_session_is_authentication_error_with_no_requests() {
    let mock = MockEero::start().await;
    let auth = AuthApi::new(mock.transport_anonymous());

    let err = auth
        .ensure_authenticated()
        .await
        .expect_err("no session configured means this must fail locally");
    assert!(
        matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Not authenticated")
    );
}

// ===================== AuthApi::logout =====================

#[tokio::test]
async fn auth_api_logout_when_not_authenticated_returns_false_with_no_network_call() {
    let mock = MockEero::start().await;
    // No `Mock` registered at all: if `logout` ever attempted a network call, wiremock would
    // panic with "no mock matched" rather than let this reach the assertion below.
    let auth = AuthApi::new(mock.transport_anonymous());

    let result = auth
        .logout()
        .await
        .expect("no session configured: the local guard fires, no network call attempted");
    assert!(!result);
}

#[tokio::test]
async fn auth_api_logout_posts_a_form_body_with_the_cookie_shaped_field_and_clears_session()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(format!("Cookie=s%3D{TEST_TOKEN}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let result = auth.logout().await?;
    assert!(result);

    assert!(!auth.is_authenticated());
    let persisted = store.load()?;
    assert!(persisted.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_swallows_a_401_and_still_clears_session_and_store() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.expired"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let result = auth
        .logout()
        .await
        .expect("v8.0.4 logout never propagates a network/API error");
    assert!(result);
    assert!(!auth.is_authenticated());
    assert!(store.load()?.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_swallows_a_429_and_still_clears_session_and_store() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock.server)
        .await;

    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let result = auth.logout().await?;
    assert!(result);
    assert!(!auth.is_authenticated());
    assert!(store.load()?.token().expose_secret().is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_swallows_a_network_error_and_still_clears_session_and_store()
-> anyhow::Result<()> {
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder()
        // No listener at all on this port.
        .base_url("http://127.0.0.1:1")
        .session(Some(Session::from_token(TEST_TOKEN)))
        .store(Some(Arc::clone(&store)))
        .build()?;
    let auth = AuthApi::new(transport);

    let result = auth
        .logout()
        .await
        .expect("a network failure during logout must never propagate");
    assert!(result);
    assert!(!auth.is_authenticated());
    assert!(store.load()?.token().expose_secret().is_empty());
    Ok(())
}

// ===================== Session persistence: async setters, last-awaited-wins, credentials_persisted =====================

/// Which store call a [`ProbeStore`] parks (on a blocking-pool thread) until [`ProbeStore::open`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParkOn {
    Never,
    AnySave,
    SaveOf(&'static str),
    AnyClear,
}

/// The longest a parked store call waits for [`ProbeStore::open`]. A test whose assertions fail
/// between `wait_parked()` and `open()` would otherwise leave the blocking-pool thread parked
/// forever, and the runtime's drop would wait for it without end.
const PARK_LIMIT: Duration = Duration::from_secs(10);

/// A [`CredentialStore`] test double over a [`MemoryStore`]: records every call and the thread it
/// ran on, can be told to fail, and can park a chosen call until it is opened.
///
/// A parked call never outlives [`PARK_LIMIT`] (it then fails with a [`StorageError`]), and one
/// that arrives on the thread that built the store, which is the thread driving a
/// `#[tokio::test]` runtime, panics instead of parking: a store call that ran inline on the
/// runtime would otherwise block the very runtime whose timeouts are meant to report it. A test
/// that parks creates a [`ProbeStore::release_on_drop`] guard so a failing assertion releases the
/// parked thread.
///
/// Integration-test binaries cannot share code except through `tests/common`, so
/// `tests/client_core.rs` carries its own trimmed copy.
#[derive(Debug)]
struct ProbeStore {
    inner: MemoryStore,
    park_on: ParkOn,
    test_thread: ThreadId,
    gate_open: Mutex<bool>,
    gate_changed: Condvar,
    parked: AtomicBool,
    failing: AtomicBool,
    saves: Mutex<Vec<String>>,
    clears: AtomicUsize,
    threads: Mutex<Vec<ThreadId>>,
}

impl ProbeStore {
    fn new(park_on: ParkOn) -> Arc<Self> {
        Arc::new(Self {
            inner: MemoryStore::new(),
            park_on,
            test_thread: std::thread::current().id(),
            gate_open: Mutex::new(false),
            gate_changed: Condvar::new(),
            parked: AtomicBool::new(false),
            failing: AtomicBool::new(false),
            saves: Mutex::new(Vec::new()),
            clears: AtomicUsize::new(0),
            threads: Mutex::new(Vec::new()),
        })
    }

    /// Makes every later `save`/`clear` return `Err` (after any parking) when `failing`.
    fn set_failing(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }

    /// Seeds the record behind the double without recording a call.
    fn seed(&self, token: &str) {
        self.inner
            .save(&Session::from_token(token))
            .expect("seeding the in-memory record cannot fail");
    }

    /// Releases every parked call, and lets later ones through.
    fn open(&self) {
        *self.gate_open.lock().expect("gate lock") = true;
        self.gate_changed.notify_all();
    }

    /// Opens the gate when dropped, so a test that fails with a store call still parked releases
    /// the blocking-pool thread instead of hanging the runtime's shutdown.
    fn release_on_drop(self: &Arc<Self>) -> OpenOnDrop {
        OpenOnDrop(Arc::clone(self))
    }

    /// Waits (up to 5 s) until some call is parked inside the store.
    async fn wait_parked(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.parked.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("a store call should have reached the parking gate");
    }

    fn saved_tokens(&self) -> Vec<String> {
        self.saves.lock().expect("saves lock").clone()
    }

    fn clear_count(&self) -> usize {
        self.clears.load(Ordering::SeqCst)
    }

    fn call_threads(&self) -> Vec<ThreadId> {
        self.threads.lock().expect("threads lock").clone()
    }

    /// The token currently held by the underlying record (empty string when cleared).
    fn stored_token(&self) -> String {
        self.inner
            .load()
            .expect("in-memory load cannot fail")
            .token()
            .expose_secret()
            .to_owned()
    }

    /// Parks the calling blocking-pool thread until [`ProbeStore::open`], or fails after
    /// [`PARK_LIMIT`].
    fn park(&self) -> Result<(), StorageError> {
        assert_ne!(
            std::thread::current().id(),
            self.test_thread,
            "a store call ran on the runtime thread instead of the blocking pool"
        );
        self.parked.store(true, Ordering::SeqCst);
        let open = self.gate_open.lock().expect("gate lock");
        let (_open, waited) = self
            .gate_changed
            .wait_timeout_while(open, PARK_LIMIT, |open| !*open)
            .expect("gate wait");
        if waited.timed_out() {
            return Err(StorageError::Backend {
                backend: "probe".to_owned(),
                message: "the gate was never opened".to_owned(),
            });
        }
        Ok(())
    }

    fn failure(&self) -> Result<(), StorageError> {
        if self.failing.load(Ordering::SeqCst) {
            Err(StorageError::Backend {
                backend: "probe".to_owned(),
                message: "deliberate failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

/// See [`ProbeStore::release_on_drop`].
struct OpenOnDrop(Arc<ProbeStore>);

impl Drop for OpenOnDrop {
    fn drop(&mut self) {
        self.0.open();
    }
}

impl CredentialStore for ProbeStore {
    fn load(&self) -> Result<Session, StorageError> {
        self.inner.load()
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        let token = session.token().expose_secret().to_owned();
        self.saves.lock().expect("saves lock").push(token.clone());
        self.threads
            .lock()
            .expect("threads lock")
            .push(std::thread::current().id());
        let parks = match self.park_on {
            ParkOn::AnySave => true,
            ParkOn::SaveOf(only) => only == token,
            ParkOn::Never | ParkOn::AnyClear => false,
        };
        if parks {
            self.park()?;
        }
        self.failure()?;
        self.inner.save(session)
    }

    fn clear(&self) -> Result<(), StorageError> {
        self.clears.fetch_add(1, Ordering::SeqCst);
        self.threads
            .lock()
            .expect("threads lock")
            .push(std::thread::current().id());
        if self.park_on == ParkOn::AnyClear {
            self.park()?;
        }
        self.failure()?;
        self.inner.clear()
    }
}

/// Ten 5 ms timer ticks under a 5 s cap. They can only complete if the runtime keeps polling
/// other tasks, which a store call running inline on the runtime thread would prevent.
async fn heartbeat() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the runtime keeps making progress while a store call is parked");
}

/// Polls (up to 5 s) until the in-memory session carries `token` (`None`: no session).
async fn wait_for_session_token(auth: &AuthApi, token: Option<&str>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let current = auth
                .session()
                .map(|session| session.token().expose_secret().to_owned());
            if current.as_deref() == token {
                return;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the in-memory session should reach the expected state");
}

fn auth_with_store(store: &Arc<ProbeStore>, policy: StorageFailures) -> Arc<AuthApi> {
    let store: Arc<dyn CredentialStore> = Arc::clone(store) as Arc<dyn CredentialStore>;
    let transport = Transport::builder()
        .store(Some(store))
        .storage_failures(policy)
        .build()
        .expect("a transport with defaults and a store builds");
    Arc::new(AuthApi::new(transport))
}

fn refresh_rejected(code: &str) -> ResponseTemplate {
    ResponseTemplate::new(401)
        .set_body_string(format!(r#"{{"meta":{{"code":401,"error":"{code}"}}}}"#))
}

// ----- the setters persist through the store, off the test thread -----

#[tokio::test]
async fn async_setters_persist_through_the_store_on_a_blocking_thread() -> anyhow::Result<()> {
    let probe = ProbeStore::new(ParkOn::Never);
    let auth = auth_with_store(&probe, StorageFailures::Warn);

    auth.set_session_token("first-token").await?;
    assert_eq!(probe.saved_tokens(), ["first-token"]);
    assert_eq!(probe.stored_token(), "first-token");
    assert_eq!(probe.clear_count(), 0);

    auth.clear_session_token().await?;
    assert_eq!(probe.clear_count(), 1);
    assert!(probe.stored_token().is_empty());

    auth.set_session_token("second-token").await?;
    auth.clear_auth_data().await?;
    assert_eq!(probe.clear_count(), 2);
    assert!(probe.stored_token().is_empty());
    assert!(!auth.is_authenticated());

    let test_thread = std::thread::current().id();
    let threads = probe.call_threads();
    assert_eq!(threads.len(), 4, "two saves and two clears");
    assert!(
        threads.iter().all(|thread| *thread != test_thread),
        "every store call runs on a blocking-pool thread, never on the runtime thread"
    );
    Ok(())
}

#[tokio::test]
async fn set_session_token_with_an_invalid_token_never_reaches_the_store() {
    let probe = ProbeStore::new(ParkOn::Never);
    let auth = auth_with_store(&probe, StorageFailures::Fatal);

    for bad in ["", "tok\r\nX-Evil: 1"] {
        let err = auth
            .set_session_token(bad)
            .await
            .expect_err("an invalid token is rejected before anything changes");
        assert!(matches!(err, Error::Validation { ref field, .. } if field == "token"));
    }
    assert!(probe.saved_tokens().is_empty());
    assert_eq!(probe.clear_count(), 0);
    assert!(!auth.is_authenticated());
}

#[tokio::test]
async fn auth_api_set_session_token_store_write_does_not_block_the_runtime() -> anyhow::Result<()> {
    let probe = ProbeStore::new(ParkOn::AnySave);
    let _release = probe.release_on_drop();
    let auth = auth_with_store(&probe, StorageFailures::Warn);

    let task = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.set_session_token("injected-token").await }
    });
    probe.wait_parked().await;

    assert!(
        auth.is_authenticated(),
        "the in-memory session is installed before the store write finishes"
    );
    heartbeat().await;
    assert!(
        !task.is_finished(),
        "the setter waits for the parked store write"
    );
    assert!(
        probe.stored_token().is_empty(),
        "nothing is persisted until released"
    );

    probe.open();
    task.await??;
    assert_eq!(probe.stored_token(), "injected-token");
    assert_ne!(probe.call_threads()[0], std::thread::current().id());
    Ok(())
}

#[tokio::test]
async fn auth_api_clear_session_token_store_clear_does_not_block_the_runtime() -> anyhow::Result<()>
{
    let probe = ProbeStore::new(ParkOn::AnyClear);
    let _release = probe.release_on_drop();
    probe.seed("stored-token");
    let transport = Transport::builder()
        .session(Some(Session::from_token("stored-token")))
        .store(Some(Arc::clone(&probe) as Arc<dyn CredentialStore>))
        .build()?;
    let auth = Arc::new(AuthApi::new(transport));

    let task = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.clear_session_token().await }
    });
    probe.wait_parked().await;

    assert!(
        !auth.is_authenticated(),
        "the in-memory session is cleared before the store clear finishes"
    );
    heartbeat().await;
    assert!(
        !task.is_finished(),
        "the call waits for the parked store clear"
    );
    assert_eq!(
        probe.stored_token(),
        "stored-token",
        "still persisted until released"
    );

    probe.open();
    task.await??;
    assert!(probe.stored_token().is_empty());
    assert_ne!(probe.call_threads()[0], std::thread::current().id());
    Ok(())
}

#[tokio::test]
async fn auth_api_logout_store_clear_does_not_block_the_runtime() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(user_token_header())
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(format!("Cookie=s%3D{TEST_TOKEN}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let probe = ProbeStore::new(ParkOn::AnyClear);
    let _release = probe.release_on_drop();
    probe.seed(TEST_TOKEN);
    let store: Arc<dyn CredentialStore> = Arc::clone(&probe) as Arc<dyn CredentialStore>;
    let auth = Arc::new(AuthApi::new(mock.transport_with_store(TEST_TOKEN, store)));

    let task = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.logout().await }
    });
    probe.wait_parked().await;

    assert!(
        !auth.is_authenticated(),
        "logout clears the in-memory session before the store clear finishes"
    );
    heartbeat().await;
    assert!(
        !task.is_finished(),
        "logout waits for the parked store clear"
    );
    assert_eq!(probe.stored_token(), TEST_TOKEN);

    probe.open();
    assert!(task.await??, "logout reports success");
    assert!(probe.stored_token().is_empty());
    assert_eq!(probe.clear_count(), 1);
    assert_ne!(probe.call_threads()[0], std::thread::current().id());
    Ok(())
}

// ----- terminal refresh: credential destruction goes through the same async path -----

#[tokio::test]
async fn refresh_session_terminal_clear_runs_off_the_runtime_thread() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(user_token_header())
        .and(session_cookie())
        .and(body_string("\"\""))
        .respond_with(refresh_rejected("error.session.expired"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let probe = ProbeStore::new(ParkOn::AnyClear);
    let _release = probe.release_on_drop();
    probe.seed(TEST_TOKEN);
    let store: Arc<dyn CredentialStore> = Arc::clone(&probe) as Arc<dyn CredentialStore>;
    let auth = Arc::new(AuthApi::new(mock.transport_with_store(TEST_TOKEN, store)));

    let task = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.refresh_session().await }
    });
    probe.wait_parked().await;

    assert!(
        !auth.is_authenticated(),
        "the terminal clear empties memory before the store clear finishes"
    );
    heartbeat().await;
    assert!(
        !task.is_finished(),
        "the refresh waits for the parked store clear"
    );

    probe.open();
    assert!(!task.await??, "a terminal refresh failure is Ok(false)");
    assert!(probe.stored_token().is_empty());
    assert_eq!(probe.clear_count(), 1);
    assert_ne!(probe.call_threads()[0], std::thread::current().id());
    Ok(())
}

#[tokio::test]
async fn refresh_session_terminal_clear_under_fatal_policy_returns_a_storage_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(user_token_header())
        .and(session_cookie())
        .and(body_string("\"\""))
        .respond_with(refresh_rejected("error.session.revoked"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let probe = ProbeStore::new(ParkOn::Never);
    probe.seed(TEST_TOKEN);
    probe.set_failing(true);
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .store(Some(Arc::clone(&probe) as Arc<dyn CredentialStore>))
        .storage_failures(StorageFailures::Fatal)
        .build()?;
    let auth = AuthApi::new(transport);

    let err = auth
        .refresh_session()
        .await
        .expect_err("Fatal policy surfaces the failed credential destruction");
    assert!(matches!(err, Error::Storage(_)), "got {err:?}");
    assert!(
        !auth.is_authenticated(),
        "the in-memory session is destroyed even though the store clear failed"
    );
    assert_eq!(probe.clear_count(), 1);
    Ok(())
}

#[tokio::test]
async fn request_terminal_refresh_failure_clears_the_store_and_surfaces_the_original_401()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(refresh_rejected("error.session.refresh"))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/refresh"))
        .and(user_token_header())
        .and(session_cookie())
        .and(body_string("\"\""))
        .respond_with(refresh_rejected("error.session.expired"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let probe = ProbeStore::new(ParkOn::Never);
    probe.seed(TEST_TOKEN);
    let store: Arc<dyn CredentialStore> = Arc::clone(&probe) as Arc<dyn CredentialStore>;
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, store));

    let url = url::Url::parse(&format!("{}/2.2/account", mock.uri()))?;
    let err = auth
        .transport()
        .request(reqwest::Method::GET, url, &[], RequestBody::None)
        .await
        .expect_err("the original 401 surfaces once the refresh fails terminally");
    match err {
        Error::Authentication { error_code, .. } => {
            assert_eq!(error_code.as_deref(), Some("error.session.refresh"));
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    assert!(!auth.is_authenticated());
    assert!(probe.stored_token().is_empty());
    assert_eq!(probe.clear_count(), 1);
    assert_ne!(probe.call_threads()[0], std::thread::current().id());
    Ok(())
}

// ----- last awaited wins (concurrent session changes) -----

#[tokio::test]
async fn concurrent_setters_leave_the_last_one_in_memory_and_in_the_store() -> anyhow::Result<()> {
    // "zero" parks inside the store, holding the persistence gate, while "first" and then
    // "second" are applied to memory; their persists queue behind it.
    let probe = ProbeStore::new(ParkOn::SaveOf("zero"));
    let _release = probe.release_on_drop();
    let auth = auth_with_store(&probe, StorageFailures::Warn);

    let zero = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.set_session_token("zero").await }
    });
    probe.wait_parked().await;

    let first = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.set_session_token("first").await }
    });
    wait_for_session_token(&auth, Some("first")).await;
    let second = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.set_session_token("second").await }
    });
    wait_for_session_token(&auth, Some("second")).await;
    heartbeat().await;

    probe.open();
    zero.await??;
    first.await??;
    second.await??;

    let session = auth
        .session()
        .expect("the last setter's session is installed");
    assert_eq!(session.token().expose_secret(), "second");
    assert_eq!(probe.stored_token(), "second");
    let mut saved = probe.saved_tokens();
    saved.sort();
    assert_eq!(
        saved,
        ["second", "zero"],
        "the superseded \"first\" persist is skipped, not replayed"
    );
    Ok(())
}

#[tokio::test]
async fn a_clear_issued_after_a_save_is_never_overtaken_by_it() -> anyhow::Result<()> {
    let probe = ProbeStore::new(ParkOn::SaveOf("doomed"));
    let _release = probe.release_on_drop();
    let auth = auth_with_store(&probe, StorageFailures::Warn);

    let save = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.set_session_token("doomed").await }
    });
    probe.wait_parked().await;

    let clear = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.clear_session_token().await }
    });
    wait_for_session_token(&auth, None).await;

    probe.open();
    save.await??;
    clear.await??;

    assert!(!auth.is_authenticated());
    assert!(
        probe.stored_token().is_empty(),
        "the save that started first must not resurrect the cleared session"
    );
    assert_eq!(probe.clear_count(), 1);
    Ok(())
}

#[tokio::test]
async fn a_save_issued_after_a_clear_is_never_overtaken_by_it() -> anyhow::Result<()> {
    let probe = ProbeStore::new(ParkOn::AnyClear);
    let _release = probe.release_on_drop();
    probe.seed("old-token");
    let transport = Transport::builder()
        .session(Some(Session::from_token("old-token")))
        .store(Some(Arc::clone(&probe) as Arc<dyn CredentialStore>))
        .build()?;
    let auth = Arc::new(AuthApi::new(transport));

    let clear = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.clear_auth_data().await }
    });
    probe.wait_parked().await;

    let save = tokio::spawn({
        let auth = Arc::clone(&auth);
        async move { auth.set_session_token("new-token").await }
    });
    wait_for_session_token(&auth, Some("new-token")).await;

    probe.open();
    clear.await??;
    save.await??;

    assert_eq!(
        auth.session()
            .expect("the later save is installed")
            .token()
            .expose_secret(),
        "new-token"
    );
    assert_eq!(probe.stored_token(), "new-token");
    Ok(())
}

// ----- credentials_persisted -----

#[tokio::test]
async fn credentials_persisted_defaults_to_true_with_and_without_a_store() {
    let mock = MockEero::start().await;
    let anonymous = AuthApi::new(mock.transport_anonymous());
    assert!(anonymous.credentials_persisted());
    assert!(anonymous.transport().credentials_persisted());

    let probe = ProbeStore::new(ParkOn::Never);
    let stored = auth_with_store(&probe, StorageFailures::Warn);
    assert!(stored.credentials_persisted());

    let api = EeroApi::new(mock.transport_anonymous());
    assert!(api.credentials_persisted());
}

#[tokio::test]
async fn without_a_store_every_session_change_reads_as_persisted() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let auth = AuthApi::new(mock.transport_anonymous());

    auth.set_session_token("memory-only").await?;
    assert!(auth.is_authenticated());
    assert!(auth.credentials_persisted());
    auth.clear_session_token().await?;
    assert!(auth.credentials_persisted());
    Ok(())
}

#[tokio::test]
async fn a_memory_store_reads_as_persisted() -> anyhow::Result<()> {
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    let transport = Transport::builder().store(Some(store)).build()?;
    let auth = AuthApi::new(transport);

    auth.set_session_token("kept").await?;
    assert!(auth.credentials_persisted());
    Ok(())
}

#[tokio::test]
async fn set_session_token_under_warn_reports_a_failed_save() -> anyhow::Result<()> {
    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let auth = auth_with_store(&probe, StorageFailures::Warn);

    auth.set_session_token("injected_token").await?;
    assert!(auth.is_authenticated(), "the session works in memory");
    assert!(
        !auth.credentials_persisted(),
        "but it will not survive a restart"
    );

    // A later success flips it back.
    probe.set_failing(false);
    auth.set_session_token("injected_token_2").await?;
    assert!(auth.credentials_persisted());
    assert_eq!(probe.stored_token(), "injected_token_2");

    // Fail again, then destroy the credential: the clear resets the flag even when it fails.
    probe.set_failing(true);
    auth.set_session_token("injected_token_3").await?;
    assert!(!auth.credentials_persisted());
    auth.clear_session_token().await?;
    assert!(auth.credentials_persisted());
    assert!(!auth.is_authenticated());
    Ok(())
}

#[tokio::test]
async fn set_session_token_under_fatal_returns_the_error_after_installing_the_session() {
    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let auth = auth_with_store(&probe, StorageFailures::Fatal);

    let err = auth
        .set_session_token("injected_token")
        .await
        .expect_err("Fatal policy surfaces the failed save");
    assert!(matches!(err, Error::Storage(_)), "got {err:?}");
    assert_eq!(
        auth.session()
            .expect("installed before persisting")
            .token()
            .expose_secret(),
        "injected_token"
    );
    assert!(!auth.credentials_persisted());
}

#[tokio::test]
async fn a_rejected_token_leaves_credentials_persisted_unchanged() {
    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let auth = auth_with_store(&probe, StorageFailures::Warn);
    auth.set_session_token("injected_token")
        .await
        .expect("Warn swallows the store failure");
    assert!(!auth.credentials_persisted());

    auth.set_session_token("")
        .await
        .expect_err("an empty token is rejected");
    assert!(
        !auth.credentials_persisted(),
        "the rejection changes nothing"
    );
    assert_eq!(
        probe.saved_tokens(),
        ["injected_token"],
        "and never reaches the store"
    );
    assert_eq!(
        auth.session()
            .expect("the earlier session is untouched")
            .token()
            .expose_secret(),
        "injected_token"
    );
}

#[tokio::test]
async fn logout_resets_credentials_persisted_even_when_the_store_clear_fails() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;

    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let store: Arc<dyn CredentialStore> = Arc::clone(&probe) as Arc<dyn CredentialStore>;
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, store));
    auth.set_session_token(TEST_TOKEN).await?;
    assert!(!auth.credentials_persisted());

    assert!(auth.logout().await?);
    assert!(auth.credentials_persisted());
    Ok(())
}

// ===================== Login handshake: the stored session is only replaced by a verified one =====================

/// A temp dir whose `cookies.json` holds a saved session, plus its exact bytes.
struct StoredSession {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    store: Arc<FileStore>,
    before: Vec<u8>,
}

impl StoredSession {
    const TOKEN: &'static str = "stored-token";

    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cookies.json");
        let store = Arc::new(FileStore::new(&path));
        store
            .save(&Session::from_token(Self::TOKEN))
            .expect("seeding the file store succeeds");
        let before = std::fs::read(&path).expect("the seeded file exists");
        Self {
            _dir: dir,
            path,
            store,
            before,
        }
    }

    /// A fresh `LoginFlow` whose transport carries this stored session and store.
    fn login_flow(&self, mock: &MockEero) -> LoginFlow {
        LoginFlow::with_transport(
            mock.transport_with_store(Self::TOKEN, Arc::clone(&self.store) as _),
        )
    }

    /// Asserts the file is byte-identical and a second store instance still loads the token.
    fn assert_untouched(&self) {
        assert_eq!(
            std::fs::read(&self.path).expect("the file still exists"),
            self.before,
            "the stored credential file is byte-identical"
        );
        let reloaded = FileStore::new(&self.path).load().expect("load succeeds");
        assert_eq!(reloaded.token().expose_secret(), Self::TOKEN);
    }
}

#[tokio::test]
async fn login_flow_start_never_touches_a_configured_store() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("login=you%40example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    let stored = StoredSession::new();

    let _pending = stored.login_flow(&mock).start("you@example.com").await?;

    stored.assert_untouched();
    let requests = mock
        .server
        .received_requests()
        .await
        .expect("recording is on");
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].headers.get("x-user-token").is_none(),
        "the login request is unauthenticated even though the transport holds a session"
    );
    assert!(
        requests[0].headers.get("cookie").is_none(),
        "and it carries no legacy session cookie either, which this transport sends by default"
    );
    Ok(())
}

#[tokio::test]
async fn a_rejected_login_leaves_the_store_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string(
                r#"{"meta":{"code":400,"error":"error.form.invalid.identifier"}}"#,
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    let stored = StoredSession::new();

    let err = stored
        .login_flow(&mock)
        .start("nobody")
        .await
        .expect_err("the identifier is rejected");

    assert!(
        matches!(&err, Error::Authentication { message, .. } if message.starts_with("Login failed: ")),
        "got {err:?}"
    );
    stored.assert_untouched();
    Ok(())
}

#[tokio::test]
async fn a_login_over_a_dead_connection_leaves_the_store_byte_identical() -> anyhow::Result<()> {
    let stored = StoredSession::new();
    let transport = Transport::builder()
        // No listener at all on this port.
        .base_url("http://127.0.0.1:1")
        .session(Some(Session::from_token(StoredSession::TOKEN)))
        .store(Some(Arc::clone(&stored.store) as Arc<dyn CredentialStore>))
        .build()?;

    let err = LoginFlow::with_transport(transport)
        .start("you@example.com")
        .await
        .expect_err("nothing is listening");

    assert!(matches!(err, Error::Network(_)), "got {err:?}");
    stored.assert_untouched();
    Ok(())
}

#[tokio::test]
async fn a_tokenless_login_leaves_the_store_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(json!({ "meta": { "code": 200 }, "data": {} }).to_string()),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    let stored = StoredSession::new();

    let err = stored
        .login_flow(&mock)
        .start("you@example.com")
        .await
        .expect_err("no user token came back");

    assert!(
        matches!(&err, Error::Authentication { message, .. } if message == "Login failed: No user token received"),
        "got {err:?}"
    );
    stored.assert_untouched();
    Ok(())
}

#[tokio::test]
async fn a_cancelled_login_leaves_the_store_byte_identical() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture("login.json"))
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&mock.server)
        .await;
    let stored = StoredSession::new();

    let flow = stored.login_flow(&mock);
    let outcome =
        tokio::time::timeout(Duration::from_millis(50), flow.start("you@example.com")).await;

    assert!(outcome.is_err(), "the login was cancelled while in flight");
    stored.assert_untouched();
    Ok(())
}

#[tokio::test]
async fn only_a_successful_verify_can_replace_the_stored_session() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("login.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string("code=000000"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.verification.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .and(body_string("code=123456"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("verify.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    let stored = StoredSession::new();

    // A wrong code: the verification fails and the store is exactly as it was.
    let first = stored.login_flow(&mock).start("you@example.com").await?;
    let err = first
        .verify("000000")
        .await
        .expect_err("a wrong code is rejected");
    assert!(matches!(err, Error::Authentication { .. }), "got {err:?}");
    stored.assert_untouched();

    // A right code yields the verified session; installing it is what replaces the stored one.
    let second = stored.login_flow(&mock).start("you@example.com").await?;
    let session = second.verify("123456").await?;
    stored.assert_untouched();
    let auth = AuthApi::new(
        mock.transport_with_store(StoredSession::TOKEN, Arc::clone(&stored.store) as _),
    );
    auth.transport().set_session_async(Some(session)).await?;
    assert_eq!(
        FileStore::new(&stored.path).load()?.token().expose_secret(),
        login_fixture_token()
    );
    Ok(())
}

/// The in-memory half of "a pending login never becomes the active session" is a matter of
/// ownership, not behaviour: `LoginFlow::with_transport` takes its transport by value and
/// `Transport` is not `Clone`, so no flow can be pointed at an `AuthApi`'s transport, and no test
/// could make it so. What can fail is the shared record: the flow here runs on its own transport
/// over the same credential store as the `AuthApi`, so a handshake that saved the unverified login
/// token would show up in the store.
#[tokio::test]
async fn a_pending_login_never_becomes_the_active_session() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
    store.save(&Session::from_token(TEST_TOKEN))?;
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));
    let flow = LoginFlow::with_transport(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

    let (_pending, login_token) = start_pending_login_on(&mock, flow).await?;

    assert_ne!(login_token, TEST_TOKEN);
    assert!(auth.is_authenticated());
    assert_eq!(
        auth.session()
            .expect("the installed session")
            .token()
            .expose_secret(),
        TEST_TOKEN,
        "the unverified login token is never the in-memory session"
    );
    assert_eq!(
        store.load()?.token().expose_secret(),
        TEST_TOKEN,
        "nor the stored one"
    );
    Ok(())
}

/// `credentials_persisted` is per transport, so a flow's own transport cannot change an
/// `AuthApi`'s by construction (see the test above). The flow here shares the `AuthApi`'s store,
/// which does make it observable that the handshake saves and clears nothing.
#[tokio::test]
async fn a_login_on_another_transport_leaves_credentials_persisted_alone() -> anyhow::Result<()> {
    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let auth = auth_with_store(&probe, StorageFailures::Warn);
    auth.set_session_token("injected_token").await?;
    assert!(!auth.credentials_persisted());

    let mock = MockEero::start().await;
    let flow =
        LoginFlow::with_transport(mock.transport_with_store(TEST_TOKEN, Arc::clone(&probe) as _));
    let _ = start_pending_login_on(&mock, flow).await?;

    assert!(
        !auth.credentials_persisted(),
        "the flag keeps describing the last session that was actually saved"
    );
    assert_eq!(
        probe.saved_tokens(),
        ["injected_token"],
        "the handshake saved nothing"
    );
    assert_eq!(probe.clear_count(), 0, "and cleared nothing");
    Ok(())
}

// ===================== Errors the login flow re-wraps carry no status; a real 401 keeps its own =====================

#[tokio::test]
async fn a_rewrapped_login_error_carries_no_status() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"meta":{"code":400,"error":"error.form.email.malformed"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("not-an-email")
        .await
        .expect_err("the identifier is rejected");

    match err {
        Error::Authentication {
            message,
            envelope,
            error_code,
            status,
        } => {
            assert!(message.starts_with("Login failed: "), "{message}");
            assert_eq!(
                status, None,
                "the re-wrapped exception is a new one, with no status"
            );
            assert_eq!(error_code.as_deref(), Some("error.form.email.malformed"));
            assert!(envelope.is_some(), "the envelope is carried over");
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn a_rewrapped_login_api_error_carries_no_status() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a 500 fails the login");

    match err {
        Error::Authentication {
            message, status, ..
        } => {
            assert!(
                message.starts_with("Login failed: API error 500"),
                "{message}"
            );
            assert_eq!(status, None);
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn a_rewrapped_verification_error_carries_no_status() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (pending, _) = start_pending_login(&mock).await?;
    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = pending
        .verify("123456")
        .await
        .expect_err("a 500 fails the verification");

    match err {
        Error::Authentication {
            message, status, ..
        } => {
            assert!(
                message.starts_with("Verification failed: API error 500"),
                "{message}"
            );
            assert_eq!(status, None);
        }
        other => panic!("expected Error::Authentication, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn a_real_401_through_login_keeps_its_status() -> anyhow::Result<()> {
    // Python's `login` only catches `EeroAPIException` and `EeroValidationException`, so a 401
    // propagates as the exception the transport raised, status included.
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("a 401 fails the login");

    assert!(
        matches!(
            &err,
            Error::Authentication {
                status: Some(401),
                ..
            }
        ),
        "got {err:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_real_401_through_verify_keeps_its_status() -> anyhow::Result<()> {
    // `verify` re-raises `EeroAuthenticationException` unchanged (`auth.py:296-302`).
    let mock = MockEero::start().await;
    let (pending, _) = start_pending_login(&mock).await?;
    Mock::given(method("POST"))
        .and(path("/2.2/login/verify"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.verification.invalid"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = pending
        .verify("000000")
        .await
        .expect_err("a wrong code is rejected");

    assert!(
        matches!(
            &err,
            Error::Authentication {
                status: Some(401),
                ..
            }
        ),
        "got {err:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_rate_limit_through_login_is_not_rewrapped() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/login"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = mock
        .login_flow()
        .start("you@example.com")
        .await
        .expect_err("rate limited");

    assert!(
        matches!(
            &err,
            Error::RateLimit {
                status: Some(429),
                ..
            }
        ),
        "got {err:?}"
    );
    Ok(())
}

// ===================== A 2xx body that is not a JSON object =====================
//
// The transport rejects such a body as `Error::Api { status, "Unexpected JSON response:
// expected an object" }`, and the flow's re-wrapping turns that into Python's outcomes and
// messages (`auth.py:186-336`).

mod non_object_bodies {
    use super::*;

    const UNEXPECTED: &str = "API error 200: Unexpected JSON response: expected an object";

    #[rstest]
    #[case::array("[]")]
    #[case::string("\"ok\"")]
    #[case::null("null")]
    #[tokio::test]
    async fn login_with_a_non_object_body_fails_as_an_authentication_error(#[case] body: &str) {
        let mock = MockEero::start().await;
        Mock::given(method("POST"))
            .and(path("/2.2/login"))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string("login=user%40example.test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&mock.server)
            .await;

        let err = mock
            .login_flow()
            .start("user@example.test")
            .await
            .expect_err("a non-object body is not a login");

        match err {
            Error::Authentication {
                message,
                envelope,
                error_code,
                status,
            } => {
                assert_eq!(message, format!("Login failed: {UNEXPECTED}"));
                assert!(envelope.is_none());
                assert!(error_code.is_none());
                assert_eq!(status, None);
            }
            other => panic!("expected Error::Authentication, got {other:?}"),
        }
    }

    #[rstest]
    #[case::array("[]")]
    #[case::string("\"ok\"")]
    #[case::null("null")]
    #[tokio::test]
    async fn verify_with_a_non_object_body_fails_as_an_authentication_error(#[case] body: &str) {
        let mock = MockEero::start().await;
        let (pending, login_token) = start_pending_login(&mock).await.expect("login works");
        Mock::given(method("POST"))
            .and(path("/2.2/login/verify"))
            .and(user_token_header_for(&login_token))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string("code=123456"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&mock.server)
            .await;

        let err = pending
            .verify("123456")
            .await
            .expect_err("a non-object body is not a verification");

        match err {
            Error::Authentication {
                message,
                envelope,
                status,
                ..
            } => {
                assert_eq!(message, format!("Verification failed: {UNEXPECTED}"));
                assert!(envelope.is_none());
                assert_eq!(status, None);
            }
            other => panic!("expected Error::Authentication, got {other:?}"),
        }
    }

    #[rstest]
    #[case::array("[]")]
    #[case::string("\"ok\"")]
    #[case::null("null")]
    #[tokio::test]
    async fn resend_with_a_non_object_body_is_an_api_error(#[case] body: &str) {
        let mock = MockEero::start().await;
        let (pending, login_token) = start_pending_login(&mock).await.expect("login works");
        Mock::given(method("POST"))
            .and(path("/2.2/login/resend"))
            .and(user_token_header_for(&login_token))
            .and(body_json(json!({})))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&mock.server)
            .await;

        let err = pending
            .resend()
            .await
            .expect_err("a non-object body is an API-level failure");

        match err {
            Error::Api {
                status,
                message,
                envelope,
                ..
            } => {
                assert_eq!(status, 200);
                assert_eq!(message, "Unexpected JSON response: expected an object");
                assert!(envelope.is_none());
            }
            other => panic!("expected Error::Api, got {other:?}"),
        }
    }

    #[rstest]
    #[case::array("[]")]
    #[case::string("\"ok\"")]
    #[case::null("null")]
    #[tokio::test]
    async fn logout_with_a_non_object_body_warns_once_and_still_clears_everything(
        #[case] body: &str,
    ) {
        let (_guard, logs) = capture_logs();
        let mock = MockEero::start().await;
        Mock::given(method("POST"))
            .and(path("/2.2/logout"))
            .and(user_token_header())
            .and(session_cookie())
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string(format!("Cookie=s%3D{TEST_TOKEN}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&mock.server)
            .await;
        let store: Arc<dyn CredentialStore> = Arc::new(MemoryStore::new());
        store
            .save(&Session::from_token(TEST_TOKEN))
            .expect("seed the store");
        let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&store)));

        let outcome = auth.logout().await.expect("logout never raises for this");

        assert!(outcome, "logout reports success");
        assert!(auth.session().is_none());
        assert!(!store.load().expect("load succeeds").is_valid());
        assert_eq!(
            logs.warnings(),
            vec![format!("Logout API call failed: {UNEXPECTED}")]
        );
    }
}

// ===================== What logout logs =====================

#[tokio::test]
async fn logout_logs_a_401_at_debug_and_nothing_at_warn() -> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(r#"{"meta":{"code":401,"error":"error.session.expired"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    let auth = AuthApi::new(mock.transport_with_token(TEST_TOKEN));

    assert!(auth.logout().await?);

    assert!(logs.warnings().is_empty(), "{:?}", logs.warnings());
    assert!(logs.contains(
        Level::DEBUG,
        "Session already invalid on server, clearing local credentials"
    ));
    Ok(())
}

#[tokio::test]
async fn logout_logs_any_other_failed_call_at_warn_with_the_error() -> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock.server)
        .await;
    let auth = AuthApi::new(mock.transport_with_token(TEST_TOKEN));

    assert!(auth.logout().await?);

    let warnings = logs.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("Logout API call failed: API error 500"),
        "{warnings:?}"
    );
    assert!(!auth.is_authenticated());
    Ok(())
}

// ===================== Store failures under the Warn policy: exactly one warning =====================

/// A store that logs its own `WARN` from inside every `save` and `clear`, on whatever
/// blocking-pool thread runs it.
#[derive(Debug, Default)]
struct WarningStore {
    inner: MemoryStore,
}

impl CredentialStore for WarningStore {
    fn load(&self) -> Result<Session, StorageError> {
        self.inner.load()
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        tracing::warn!("store says: saving");
        self.inner.save(session)
    }

    fn clear(&self) -> Result<(), StorageError> {
        tracing::warn!("store says: clearing");
        self.inner.clear()
    }
}

#[tokio::test]
async fn a_store_warning_logged_on_the_blocking_pool_reaches_the_callers_subscriber()
-> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;
    let store: Arc<dyn CredentialStore> = Arc::new(WarningStore::default());
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, store));

    auth.set_session_token("injected_token").await?;
    assert_eq!(logs.count(Level::WARN, "store says: saving"), 1);

    assert!(auth.logout().await?);
    assert_eq!(logs.count(Level::WARN, "store says: clearing"), 1);
    Ok(())
}

#[tokio::test]
async fn a_failed_save_under_warn_logs_exactly_one_warning() -> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let auth = auth_with_store(&probe, StorageFailures::Warn);

    auth.set_session_token("injected_token").await?;

    let warnings = logs.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("failed to persist session to credential store"));
    assert!(!auth.credentials_persisted());
    Ok(())
}

#[tokio::test]
async fn a_failed_clear_during_logout_under_warn_logs_exactly_one_warning() -> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;
    let probe = ProbeStore::new(ParkOn::Never);
    probe.set_failing(true);
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&probe) as _));

    assert!(auth.logout().await?);

    // The probe logs nothing and does not claim to, so the transport reports the failure, and
    // calls it a failed clear.
    let warnings = logs.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("failed to clear session from credential store"));
    assert!(!warnings[0].contains("persist"));
    Ok(())
}

#[tokio::test]
async fn a_successful_persist_logs_no_warning() -> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;
    let probe = ProbeStore::new(ParkOn::Never);
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::clone(&probe) as _));

    auth.set_session_token("injected_token").await?;
    assert!(auth.logout().await?);

    assert!(logs.warnings().is_empty(), "{:?}", logs.warnings());
    assert!(logs.errors().is_empty());
    Ok(())
}

/// A store that behaves like the shipped ones when a write is accepted and not retained: it logs
/// the one fixed `WARN` itself and returns [`StorageError::NotRetained`].
#[derive(Debug, Default)]
struct UnretainingStore;

impl CredentialStore for UnretainingStore {
    fn load(&self) -> Result<Session, StorageError> {
        Ok(Session::empty())
    }

    fn save(&self, _session: &Session) -> Result<(), StorageError> {
        tracing::warn!("Keyring did not retain the session");
        Err(StorageError::NotRetained {
            backend: "keyring".to_owned(),
        })
    }

    fn clear(&self) -> Result<(), StorageError> {
        Ok(())
    }

    fn reports_failures(&self) -> bool {
        true
    }
}

/// The transport logs a failure at `DEBUG` rather than `WARN` when the store says it reports its
/// own failures: the store has already logged the one fixed `WARN`, and a second one would be a
/// duplicate.
mod not_retained_logging {
    use super::*;

    #[tokio::test]
    async fn an_unretained_save_under_warn_logs_only_the_stores_own_warning() -> anyhow::Result<()>
    {
        let (_guard, logs) = capture_logs();
        let transport = Transport::builder()
            .store(Some(Arc::new(UnretainingStore) as Arc<dyn CredentialStore>))
            .build()?;
        let auth = AuthApi::new(transport);

        auth.set_session_token("injected_token").await?;

        assert_eq!(
            logs.warnings(),
            vec!["Keyring did not retain the session".to_owned()]
        );
        assert!(auth.is_authenticated(), "the session works in memory");
        assert!(!auth.credentials_persisted());
        Ok(())
    }
}

// ===================== Destroying credentials survives a failing backend =====================

/// A backend whose `clear` always fails, over a [`MemoryStore`] it otherwise delegates to.
#[derive(Debug, Default)]
struct ClearFailsStore {
    inner: MemoryStore,
}

impl CredentialStore for ClearFailsStore {
    fn load(&self) -> Result<Session, StorageError> {
        self.inner.load()
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        self.inner.save(session)
    }

    fn clear(&self) -> Result<(), StorageError> {
        Err(StorageError::Backend {
            backend: "clear-fails".to_owned(),
            message: "deliberate failure".to_owned(),
        })
    }
}

fn chain_with_failing_primary_clear() -> (Arc<ChainedStore>, Arc<MemoryStore>) {
    let fallback = Arc::new(MemoryStore::new());
    fallback
        .save(&Session::from_token(TEST_TOKEN))
        .expect("seed the fallback");
    let chained = Arc::new(ChainedStore::new(
        Arc::new(ClearFailsStore::default()),
        Arc::clone(&fallback) as _,
    ));
    (chained, fallback)
}

#[tokio::test]
async fn logout_survives_a_failing_primary_clear() -> anyhow::Result<()> {
    let (_guard, logs) = capture_logs();
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(user_token_header())
        .and(session_cookie())
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(format!("Cookie=s%3D{TEST_TOKEN}")))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock.server)
        .await;
    let (chained, fallback) = chain_with_failing_primary_clear();
    let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, chained));

    assert!(auth.logout().await?);

    assert!(!auth.is_authenticated());
    assert!(
        !fallback.load()?.is_valid(),
        "the fallback was still cleared"
    );
    // The chain reports its own failed clear, so the transport adds no second line (and none
    // that calls a clear a persist).
    assert_eq!(logs.warnings(), vec![CHAIN_CLEAR_WARNING.to_owned()]);
    assert!(logs.errors().is_empty());
    Ok(())
}

const CHAIN_CLEAR_WARNING: &str = "Could not clear every credential storage backend; a stored \
     session may remain in the one that failed";

#[tokio::test]
async fn clear_session_token_and_clear_auth_data_survive_a_failing_primary_clear()
-> anyhow::Result<()> {
    for destroy in [0, 1] {
        let mock = MockEero::start().await;
        let (chained, fallback) = chain_with_failing_primary_clear();
        let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, chained));

        if destroy == 0 {
            auth.clear_session_token().await?;
        } else {
            auth.clear_auth_data().await?;
        }

        assert!(!auth.is_authenticated());
        assert!(!fallback.load()?.is_valid());
    }
    Ok(())
}

#[tokio::test]
async fn a_single_failing_backend_under_fatal_names_the_backend_that_kept_a_copy()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (chained, fallback) = chain_with_failing_primary_clear();
    let transport = Transport::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .store(Some(chained as Arc<dyn CredentialStore>))
        .storage_failures(StorageFailures::Fatal)
        .build()?;
    let auth = AuthApi::new(transport);

    let err = auth
        .clear_session_token()
        .await
        .expect_err("Fatal surfaces the surviving copy");

    assert!(
        matches!(
            &err,
            Error::Storage(StorageError::Backend { backend, .. }) if backend == "chained-primary"
        ),
        "got {err:?}"
    );
    assert!(!auth.is_authenticated(), "memory is cleared regardless");
    assert!(!fallback.load()?.is_valid());
    Ok(())
}

// ===================== The shipped stores: one line per failure =====================

/// The shipped stores on a Linux host with no Secret Service (a container, CI, a headless box):
/// the keyring cannot be opened at all, so every keyring save and clear fails. Each failure is
/// logged exactly once, by the store that failed, and the transport adds no second line.
///
/// On a machine that does have a reachable keyring (a desktop session) there is nothing to
/// observe and these tests must not write to it, so they return early there.
#[cfg(all(target_os = "linux", feature = "keyring"))]
mod without_a_keyring {
    use rusteero::storage::KeyringStore;

    use super::*;

    const UNPERSISTED: &str = "Keyring did not retain the session; credentials are not persisted \
         and will be lost when this process exits";
    const UNCLEARED: &str = "Keyring could not be cleared; a stored session may remain in it";

    /// A keyring store on an entry of its own, or `None` when a keyring is reachable.
    fn unreachable_keyring(test: &str) -> Option<KeyringStore> {
        let store = KeyringStore::with_entry(
            "rusteero-test-service-no-backend",
            format!("rusteero-test-account-no-backend-{test}"),
        );
        store.load().is_err().then_some(store)
    }

    fn auth_over(store: Arc<dyn CredentialStore>) -> AuthApi {
        AuthApi::new(
            Transport::builder()
                .session(Some(Session::from_token(TEST_TOKEN)))
                .store(Some(store))
                .build()
                .expect("a transport with a store builds"),
        )
    }

    #[tokio::test]
    async fn a_bare_keyring_that_rejects_the_write_warns_exactly_once() -> anyhow::Result<()> {
        let Some(keyring) = unreachable_keyring("bare_save") else {
            return Ok(());
        };
        let (_guard, logs) = capture_logs();
        let auth = auth_over(Arc::new(keyring));

        auth.set_session_token("injected_token").await?;

        assert_eq!(logs.warnings(), vec![UNPERSISTED.to_owned()]);
        assert!(logs.errors().is_empty());
        assert!(!auth.credentials_persisted());
        assert!(auth.is_authenticated(), "the session works in memory");
        assert!(!logs.mentions("injected_token"));
        Ok(())
    }

    #[tokio::test]
    async fn a_bare_keyring_that_cannot_clear_warns_exactly_once() -> anyhow::Result<()> {
        let Some(keyring) = unreachable_keyring("bare_clear") else {
            return Ok(());
        };
        let (_guard, logs) = capture_logs();
        let auth = auth_over(Arc::new(keyring));

        auth.clear_session_token().await?;

        assert_eq!(logs.warnings(), vec![UNCLEARED.to_owned()]);
        assert!(logs.errors().is_empty());
        assert!(!auth.is_authenticated());
        Ok(())
    }

    /// The default `create_storage(use_keyring: true, Some(file))` shape: a silenced keyring in
    /// front of a file. The file takes the write, so nothing is worth a line above `DEBUG`.
    #[tokio::test]
    async fn a_keyring_chained_before_a_file_saves_to_the_file_without_a_warning()
    -> anyhow::Result<()> {
        let Some(keyring) = unreachable_keyring("chain_save") else {
            return Ok(());
        };
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("cookies.json");
        let (_guard, logs) = capture_logs();
        let chain = ChainedStore::new(
            Arc::new(keyring.warn_on_unpersisted(false)),
            Arc::new(FileStore::new(&file)),
        );
        let auth = auth_over(Arc::new(chain));

        auth.set_session_token("injected_token").await?;

        assert!(logs.warnings().is_empty(), "{:?}", logs.warnings());
        assert!(logs.errors().is_empty(), "{:?}", logs.errors());
        assert!(auth.credentials_persisted());
        assert!(std::fs::read_to_string(&file)?.contains("injected_token"));
        Ok(())
    }

    /// The same chain on a clear: the keyring cannot be cleared and the chain says so once, with
    /// Python's fixed text. The keyring is silenced, so it adds nothing, and the transport adds
    /// no line that calls a clear a persist.
    #[tokio::test]
    async fn a_keyring_chained_before_a_file_warns_exactly_once_on_every_kind_of_clear()
    -> anyhow::Result<()> {
        for destroy in 0..3 {
            let Some(keyring) = unreachable_keyring(&format!("chain_clear_{destroy}")) else {
                return Ok(());
            };
            let dir = tempfile::tempdir()?;
            let file = dir.path().join("cookies.json");
            let mock = MockEero::start().await;
            Mock::given(method("POST"))
                .and(path("/2.2/logout"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&mock.server)
                .await;
            let chain = ChainedStore::new(
                Arc::new(keyring.warn_on_unpersisted(false)),
                Arc::new(FileStore::new(&file)),
            );
            let auth = AuthApi::new(mock.transport_with_store(TEST_TOKEN, Arc::new(chain)));
            let (_guard, logs) = capture_logs();

            match destroy {
                0 => auth.clear_session_token().await?,
                1 => auth.clear_auth_data().await?,
                _ => assert!(auth.logout().await?),
            }

            assert_eq!(
                logs.warnings(),
                vec![CHAIN_CLEAR_WARNING.to_owned()],
                "destroy {destroy}"
            );
            assert!(logs.errors().is_empty(), "destroy {destroy}");
        }
        Ok(())
    }
}
