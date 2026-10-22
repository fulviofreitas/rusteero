//! Shared wiremock test harness (`MockEero`) used by every later phase's HTTP integration test.
//!
//! This file is a *module*, not its own integration-test binary: it lives at
//! `tests/common/mod.rs` rather than `tests/common.rs` specifically so cargo never tries to run
//! it as a standalone test target (see the crate's testing conventions' "File Structure" section).
//! Any file directly under `tests/` pulls it in with `mod common;`.
//!
//! [`MockEero`] answers one design question up front, correctly, for every one of the ~150 tests
//! later phases will write against it: [`Transport::builder`]'s `base_url` setter derives *both*
//! the `/2.2` and `/2.3` bases from the same root (see `src/transport.rs`'s
//! `TransportBuilder::base_url` docs), so a single `wiremock::MockServer` stands in for the real
//! Eero cloud's two hosts — a test can assert a device PUT really went to `/2.3/...` while a GET
//! from the same [`Transport`] went to `/2.2/...`, exactly like the real API.

// Not every helper here has a caller in every test binary that pulls in this module with `mod
// common;` — e.g. `transport_with_store` has no caller yet in this phase's own
// `harness_smoke.rs`, but is required surface for phase 4's client+cache suite and phase 2's
// storage-backed auth tests. A plain `cargo clippy` would otherwise flag whichever subset any one
// binary doesn't happen to call as dead code.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, registry};
use wiremock::MockServer;
use wiremock::matchers::{HeaderExactMatcher, header};

use rusteero::auth::AuthApi;
use rusteero::auth::Session;
use rusteero::auth::flow::LoginFlow;
use rusteero::storage::CredentialStore;
use rusteero::transport::Transport;

/// A session token every wiremock test can share, so no individual test hand-writes the literal
/// `"s=test-token"` cookie value.
///
/// Pair with [`session_cookie`] to match a request authenticated with this token, or
/// [`session_cookie_for`] for any other token (e.g. one obtained from a fixture).
pub const TEST_TOKEN: &str = "test-token";

/// A running wiremock server plus the handful of `rusteero` entry points every HTTP integration
/// test needs, all pre-wired to point at it.
///
/// Build one with [`MockEero::start`], register `wiremock::Mock`s against `mock.server`, then
/// build whichever `rusteero` type the test under way needs
/// ([`MockEero::transport_with_token`], [`MockEero::login_flow`],
/// [`MockEero::auth_api_with_token`], ...) — every one of them already points at
/// [`MockEero::uri`] for *both* API versions (see the module docs).
pub struct MockEero {
    /// The running mock server. Public so a test can `Mock::given(..).mount(&mock.server)`
    /// directly and rely on `wiremock`'s own `.expect(n)` call-count verification, which runs
    /// when the returned guard is dropped.
    pub server: MockServer,
}

impl MockEero {
    /// Starts a fresh wiremock server bound to an ephemeral local port.
    pub async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    /// The mock server's base URL (e.g. `http://127.0.0.1:54321`), with no trailing slash.
    #[must_use]
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// Builds a [`Transport`] pointed at this mock server with no session and no credential
    /// store configured — for exercising the unauthenticated paths (`LoginFlow::start`, the
    /// "Not authenticated" precondition, a fresh `refresh_session` with nothing to refresh, ...).
    #[must_use]
    pub fn transport_anonymous(&self) -> Transport {
        Transport::builder()
            .base_url(self.uri())
            .build()
            .expect("a `MockServer`'s own URI is always a valid base URL")
    }

    /// Builds a [`Transport`] pointed at this mock server, pre-seeded with a valid session
    /// carrying `token` (via [`Session::from_token`]) and no credential store.
    #[must_use]
    pub fn transport_with_token(&self, token: &str) -> Transport {
        Transport::builder()
            .base_url(self.uri())
            .session(Some(Session::from_token(token)))
            .build()
            .expect("a `MockServer`'s own URI is always a valid base URL")
    }

    /// Builds a [`Transport`] pointed at this mock server, pre-seeded with a valid session
    /// carrying `token`, backed by `store` — for exercising persistence side effects
    /// (`Transport::set_session`, `Transport::refresh_session`, the three
    /// `AuthApi::clear_*`/`logout` scopes, ...).
    #[must_use]
    pub fn transport_with_store(&self, token: &str, store: Arc<dyn CredentialStore>) -> Transport {
        Transport::builder()
            .base_url(self.uri())
            .session(Some(Session::from_token(token)))
            .store(Some(store))
            .build()
            .expect("a `MockServer`'s own URI is always a valid base URL")
    }

    /// Builds a [`LoginFlow`] pointed at this mock server, with no session and no credential
    /// store — the seam the interactive login handshake tests use.
    #[must_use]
    pub fn login_flow(&self) -> LoginFlow {
        LoginFlow::with_transport(self.transport_anonymous())
    }

    /// Builds an [`AuthApi`] pointed at this mock server, wrapping a [`Transport`] already
    /// authenticated with `token` (see [`MockEero::transport_with_token`]).
    #[must_use]
    pub fn auth_api_with_token(&self, token: &str) -> AuthApi {
        AuthApi::new(self.transport_with_token(token))
    }
}

/// Matches a request's `Cookie` header against exactly `s=`[`TEST_TOKEN`].
///
/// The shared matcher every test built on
/// `mock.`[`transport_with_token`](MockEero::transport_with_token)`(TEST_TOKEN)` should use, so
/// the literal cookie string is written in exactly one place in the whole test suite.
#[must_use]
pub fn session_cookie() -> HeaderExactMatcher {
    session_cookie_for(TEST_TOKEN)
}

/// Matches a request's `Cookie` header against exactly `s=<token>`, for tests that authenticate
/// with a token other than [`TEST_TOKEN`] (e.g. a freshly-verified login token, or a
/// post-refresh token read out of a fixture).
#[must_use]
pub fn session_cookie_for(token: &str) -> HeaderExactMatcher {
    header("cookie", format!("s={token}"))
}

/// Matches a request's `X-User-Token` header against exactly [`TEST_TOKEN`] — the *primary*
/// credential at `v8.0.4` (see `src/transport.rs`'s credential-placement docs). Pair with
/// [`session_cookie`] to assert both the primary header and the legacy cookie are present.
#[must_use]
pub fn user_token_header() -> HeaderExactMatcher {
    user_token_header_for(TEST_TOKEN)
}

/// Matches a request's `X-User-Token` header against exactly `token`. See [`session_cookie_for`]
/// for the sibling legacy-cookie matcher.
#[must_use]
pub fn user_token_header_for(token: &str) -> HeaderExactMatcher {
    header("x-user-token", token)
}

/// Reads a fixture file's contents as a `String`.
///
/// `name` is a bare file name (e.g. `"devices.json"`), resolved against this crate's
/// `tests/fixtures/` directory regardless of the current working directory `cargo test` happens
/// to run from.
///
/// # Panics
///
/// Panics if `name` does not exist under `tests/fixtures/` or cannot be read — a missing fixture
/// is a test-authoring bug, not a runtime condition any test should handle gracefully.
#[must_use]
pub fn fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {}: {err}", path.display()))
}

/// Reads and parses a fixture file as a [`serde_json::Value`]. See [`fixture`] for the panic
/// contract this shares.
///
/// # Panics
///
/// Panics under the same conditions as [`fixture`], and additionally if the file's contents are
/// not valid JSON.
#[must_use]
pub fn fixture_json(name: &str) -> serde_json::Value {
    serde_json::from_str(&fixture(name))
        .unwrap_or_else(|err| panic!("fixture {name} is not valid JSON: {err}"))
}

/// Resolves `name` against this crate's `tests/fixtures/` directory.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// ===================== log capture =====================

/// One captured `tracing` event: its [`Level`], its target (the emitting module path, e.g.
/// `rusteero::links`), and its `message` field rendered as text.
///
/// Structured fields other than `message` are not part of the record: this crate inlines every
/// value it logs into the message text, as `eero-api` does, so the message is what a test pins.
pub type LogRecord = (Level, String, String);

/// Keeps a [`capture_logs`] subscriber installed on the current thread; dropping it restores the
/// subscriber that was in place before. Bind it to a named variable (`let (_guard, logs) = ...`,
/// not `_`), or the capture ends immediately.
pub type LogGuard = DefaultGuard;

/// A handle onto the events [`capture_logs`] recorded on the current thread. Cheap to clone; every
/// clone sees the same records, and the handle stays readable after its [`LogGuard`] is dropped.
#[derive(Clone, Default)]
pub struct LogCapture {
    records: Arc<Mutex<Vec<LogRecord>>>,
}

impl LogCapture {
    /// Every captured event, oldest first.
    #[must_use]
    pub fn records(&self) -> Vec<LogRecord> {
        self.lock().clone()
    }

    /// The messages of every captured event at exactly `level`, oldest first.
    #[must_use]
    pub fn messages(&self, level: Level) -> Vec<String> {
        self.lock()
            .iter()
            .filter(|(record_level, ..)| *record_level == level)
            .map(|(.., message)| message.clone())
            .collect()
    }

    /// The messages of every captured `WARN` event, oldest first.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        self.messages(Level::WARN)
    }

    /// The messages of every captured `ERROR` event, oldest first.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        self.messages(Level::ERROR)
    }

    /// Whether any captured event at exactly `level` has a message containing `needle`.
    #[must_use]
    pub fn contains(&self, level: Level, needle: &str) -> bool {
        self.lock()
            .iter()
            .any(|(record_level, _, message)| *record_level == level && message.contains(needle))
    }

    /// How many captured events at exactly `level` have a message containing `needle`.
    #[must_use]
    pub fn count(&self, level: Level, needle: &str) -> usize {
        self.lock()
            .iter()
            .filter(|(record_level, _, message)| *record_level == level && message.contains(needle))
            .count()
    }

    /// Whether nothing at all has been captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Forgets everything captured so far, e.g. to drop the noise of a test's setup phase before
    /// the call under test.
    pub fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<LogRecord>> {
        // A panic while a test thread held the lock must not hide the records from the
        // assertion that reports it.
        self.records.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Collects the `message` field of one event.
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            value.clone_into(&mut self.message);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
}

/// The `tracing` layer behind [`capture_logs`]: appends every event, at every level, to a shared
/// vector.
struct CaptureLayer {
    records: Arc<Mutex<Vec<LogRecord>>>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let metadata = event.metadata();
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((
                *metadata.level(),
                metadata.target().to_owned(),
                visitor.message,
            ));
    }
}

/// Installs a capturing `tracing` subscriber for the **current thread** and returns the
/// [`LogGuard`] that keeps it installed plus the [`LogCapture`] handle to read what was logged.
///
/// Use it to pin a changed or new `WARN`/`ERROR` line, or to prove a warning was *not* emitted:
///
/// ```ignore
/// let (_guard, logs) = common::capture_logs();
/// // ... drive the call under test ...
/// assert!(logs.contains(tracing::Level::WARN, "Issuing write (set nightlight for eero)"));
/// ```
///
/// Events are captured at every level and from every crate, so filter by level and message text
/// rather than counting all records.
///
/// # Current thread only
///
/// The subscriber is installed with `tracing::subscriber::set_default`, which is thread-local.
/// That is exactly right for `#[tokio::test]`, whose default runtime is current-thread, so every
/// task the test spawns runs on the test's own thread. Events logged on any other thread — a
/// `#[tokio::test(flavor = "multi_thread")]` worker, a `std::thread::spawn`ed thread, a
/// `tokio::task::spawn_blocking` closure, reqwest's connection tasks on a separate runtime — are
/// **not** captured. The capture is also isolated per test: tests running in parallel on their own
/// threads never see each other's events.
#[must_use]
pub fn capture_logs() -> (LogGuard, LogCapture) {
    let capture = LogCapture::default();
    let subscriber = registry().with(CaptureLayer {
        records: Arc::clone(&capture.records),
    });
    (tracing::subscriber::set_default(subscriber), capture)
}
