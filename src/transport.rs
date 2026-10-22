//! Request core: header/credential construction, status mapping, response size cap, redirect
//! refusal, GET-only bounded retry, and the server-driven refresh-and-replay handshake.
//!
//! Ported from `eero-api`'s `src/eero/api/base.py` at `v8.0.4` (request pipeline) and
//! `src/eero/api/auth.py` (the refresh handshake). [`Transport`] is the single place that turns
//! an HTTP status/body into either an `Envelope` or a typed `Error` — every endpoint module built
//! on top of it is expected to be a thin wrapper that supplies a route and path/query/body values
//! and nothing else.
//!
//! # The v8.0.4 request API
//!
//! [`Transport::request`]/[`Transport::resource`]/[`Transport::nested`] are the request-building
//! entry points; see `.claude/tasks/briefs/v8/transport-api.md` for a worked example of each.
//! Every endpoint module builds on `resource`/`nested` (or, for the login handshake and session
//! refresh, `request`/`request_with_token` directly against a fixed-path [`Resource`]). Each of the
//! three has a `*_with_timeout` twin taking an optional per-call [`RequestTimeout`] that replaces
//! the transport's default timeout for that one request.
//!
//! # Locking
//!
//! The current [`crate::auth::Session`] lives behind a `std::sync::RwLock`, not a
//! `tokio::sync::RwLock`. Every critical section that touches it is a handful of clones/
//! comparisons with no `.await` inside — an async lock would only add executor overhead for no
//! benefit here. No lock guard is ever held across an `.await` point anywhere in this module.
//!
//! # Session persistence
//!
//! A session change is applied to memory first, in one step that also stamps it with a
//! per-`Transport` *generation*; only then is the configured [`CredentialStore`] told. The store
//! calls are blocking, so the async entry points ([`Transport::set_session_async`] and the
//! credential-destroying branch of [`Transport::refresh_session`]) run them on the runtime's
//! blocking pool (`tokio::task::spawn_blocking`) and every task that runs during that hop already
//! sees the new in-memory session. Persists are serialised, and one whose generation has been
//! overtaken by a newer change before it reaches the store is skipped, so after any interleaving
//! of concurrent changes the stored record matches the final in-memory session: the last change
//! applied wins, as in `eero-api` 8.1.10 (`auth_storage.py:199-203`).
//! [`Transport::credentials_persisted`] reports the outcome of the most recent persist that
//! actually reached the store.
//!
//! # The refresh-retry divergence (task brief gotcha G1)
//!
//! `eero-api`'s 401-triggered refresh retry re-sends the retried request with the *original,
//! pre-refresh* token (`api/base.py:625-658`), which matters little at `v8.0.4` since a
//! successful refresh never actually rotates the token (`refresh_session`'s own docs) — but
//! `rusteero` still deliberately re-reads the session from shared state *after* the refresh
//! completes rather than reusing the token captured before it, so the retry observes whatever the
//! refresh handshake left behind (including a credential-clearing terminal failure, which turns
//! the retry into another "Not authenticated" precondition failure rather than silently reusing a
//! since-cleared token).

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, Weak};
use std::time::{Duration, SystemTime};

use reqwest::header::{
    ACCEPT, CONTENT_TYPE, COOKIE, HeaderMap, HeaderName, LOCATION, RETRY_AFTER, USER_AGENT,
};
use reqwest::redirect::Policy;
use reqwest::{Client, Method, StatusCode};
use secrecy::SecretString;
use serde_json::Value;
use tokio::sync::watch;
use url::Url;

use crate::auth::Session;
use crate::consts;
use crate::envelope::Envelope;
use crate::error::{Error, StorageError};
use crate::errors;
use crate::redact;
use crate::routes::{self, ApiVersion, Nested, Resource};
use crate::storage::{self, CredentialStore};

/// The body a request carries, mirroring `eero-api`'s `RequestEncoding`
/// (`api/base.py:54-75,324-361`).
///
/// Exactly one carrier is active per request: `Json` sets `Content-Type: application/json` and
/// serializes `value` as the body (`reqwest::RequestBuilder::json`); `Form` sets
/// `Content-Type: application/x-www-form-urlencoded` and url-encodes the pairs in order
/// (`reqwest::RequestBuilder::form`); `EmptyJsonString` sends the literal two-byte body `""` with
/// `Content-Type: application/json` set explicitly — the exact shape `login/refresh` requires
/// (`api/base.py:483-484,501-502`); `None` sends no body and no `Content-Type` at all.
#[derive(Debug, Clone)]
pub enum RequestBody {
    /// No body, no `Content-Type`.
    None,
    /// A JSON body.
    Json(Value),
    /// A `application/x-www-form-urlencoded` body, encoded from `field=value` pairs in order.
    Form(Vec<(String, String)>),
    /// The literal two-byte body `""`, with `Content-Type: application/json`.
    EmptyJsonString,
}

/// A per-call timeout for one request, mirroring the `aiohttp.ClientTimeout(total=...,
/// sock_read=...)` that `eero-api`'s transport methods accept as `timeout=`
/// (`api/base.py:712-716`).
///
/// Passed as `Some(timeout)` to [`Transport::request_with_timeout`],
/// [`Transport::resource_with_timeout`] or [`Transport::nested_with_timeout`], it **replaces the
/// transport's default wholesale** for that one request, exactly as `aiohttp` uses a supplied
/// `ClientTimeout` as-is without merging it with the session default: a field left as `None`
/// means "no limit" for that request and never falls back to the transport default. So
/// `RequestTimeout::total(Duration::from_secs(120))` is a 120 s total with *no* read bound at
/// all, and [`RequestTimeout::default`] is unbounded on both axes (like `aiohttp.ClientTimeout()`).
/// To keep the transport default, pass `None` instead of a `RequestTimeout`. A field set to
/// [`Duration::ZERO`] is also unbounded, as in `aiohttp`.
///
/// - `total` bounds one attempt from the moment the request is sent until its (size-capped)
///   body has been read in full.
/// - `read` bounds each wait for activity from the server: the wait for the response head
///   (which, in `reqwest`, includes connecting and writing the request) and then every wait for
///   the next chunk of the body.
///
/// Elapsed bounds surface as [`Error::Timeout`]. Every attempt of a `GET` retry gets the full
/// budget (the bounds restart on each attempt), and the one-shot replay after a
/// `error.session.refresh` `401` keeps the per-call value; the refresh `POST` itself always uses
/// the transport default.
///
/// The crate enforces both bounds itself, so the `reqwest::Client` it builds carries no timeout
/// of its own; [`TransportBuilder::timeout`] and [`TransportBuilder::read_timeout`] only set the
/// transport default. With [`TransportBuilder::http_builder`] that default is unbounded and the
/// caller's builder governs: a supplied `total` replaces a total timeout set on that builder,
/// but a `read_timeout` set on it applies below this crate and cannot be disabled or replaced
/// per call.
///
/// # Cancellation
///
/// A request that times out (or whose future is dropped) closes its connection rather than
/// returning it to the pool with an unread response, so its response can never be handed to a
/// later request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestTimeout {
    /// Upper bound on one whole attempt. `None` or [`Duration::ZERO`] means unbounded.
    pub total: Option<Duration>,
    /// Upper bound on each wait for the server (response head, then every body chunk). `None` or
    /// [`Duration::ZERO`] means unbounded.
    pub read: Option<Duration>,
}

impl RequestTimeout {
    /// A timeout with explicit `total` and `read` bounds; `None` means unbounded.
    #[must_use]
    pub const fn new(total: Option<Duration>, read: Option<Duration>) -> Self {
        Self { total, read }
    }

    /// A timeout with only a `total` bound and **no** read bound, like
    /// `aiohttp.ClientTimeout(total=...)`.
    #[must_use]
    pub const fn total(total: Duration) -> Self {
        Self {
            total: Some(total),
            read: None,
        }
    }
}

/// The bounds one attempt runs under: a [`RequestTimeout`] resolved against the transport
/// default, with zero already turned into "unbounded".
#[derive(Debug, Clone, Copy)]
struct Bounds {
    total: Option<Duration>,
    read: Option<Duration>,
    /// Whether a per-call [`RequestTimeout`] was supplied (and so replaces any client-level
    /// total timeout configured on a caller-supplied `reqwest::ClientBuilder`).
    per_call: bool,
}

/// The request core shared by every authenticated and unauthenticated Eero cloud API call.
///
/// Build one with [`Transport::builder`]. See the module docs for the locking discipline and the
/// deliberate refresh-retry divergence from `eero-api`.
#[derive(Debug)]
pub struct Transport {
    http: Client,
    /// Scheme + authority (host, and port when non-default) of the configured API host, with no
    /// path — what [`crate::links`]/[`crate::params`] and every [`Resource`]/[`Nested`] route
    /// take as `host`. Derived from the 2.2 base URL's origin at build time, so it already
    /// reflects a `TransportBuilder::base_url` override (e.g. a wiremock server in tests).
    api_host: Url,
    session: RwLock<Option<Session>>,
    store: Option<Arc<dyn CredentialStore>>,
    /// Generation stamp, store-I/O gate and last-outcome flag for session persistence; shared
    /// (`Arc`) so a persist can move onto a blocking-pool thread. See the module docs.
    persistence: Arc<Persistence>,
    storage_failures: StorageFailures,
    user_agent: String,
    accept_language: String,
    send_legacy_cookie: bool,
    get_retries: u32,
    /// The bounds applied to a request that supplies no per-call [`RequestTimeout`]: the
    /// builder's `timeout`/`read_timeout`, or unbounded under `TransportBuilder::http_builder`.
    timeout: RequestTimeout,
    /// Single-flight coalescing slot for [`Transport::refresh_session`]: a [`Weak`] reference to
    /// the current leader's broadcast channel, if a refresh is in flight. See
    /// [`Transport::refresh_session`]'s own docs for the self-healing rationale (a dropped/
    /// cancelled leader's `Arc` drops too, so `Weak::upgrade` starts failing again immediately —
    /// no explicit slot-clearing step is needed, and a cancelled leader can never permanently wedge
    /// every future refresh attempt).
    refresh_slot: tokio::sync::Mutex<Weak<watch::Sender<Option<bool>>>>,
}

/// How a [`Transport`] treats a failed write to its configured credential store (decision D-13).
///
/// Set via [`TransportBuilder::storage_failures`]; honoured at every credential-store call site
/// in this module ([`Transport::set_session`], [`Transport::set_session_async`], and the
/// credential-clearing branch of [`Transport::refresh_session`]). This is unrelated to whether
/// the *in-memory* session is updated: that happens unconditionally and before persistence is
/// even attempted, regardless of this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageFailures {
    /// Log the failure at `WARN` and proceed as though the operation had succeeded. The default.
    #[default]
    Warn,
    /// Return [`Error::Storage`] immediately instead of swallowing the failure.
    Fatal,
}

/// Session-persistence bookkeeping shared between a [`Transport`] and the blocking tasks that
/// write its credential store (see the module docs, "Session persistence").
#[derive(Debug)]
struct Persistence {
    /// Stamp of the most recent in-memory session change. Bumped by `Transport::replace_session`
    /// under the session write lock.
    generation: AtomicU64,
    /// Serialises "is my stamp still the latest?" plus the store call, so persists reach the
    /// store one at a time and a skipped one can never overwrite a newer record.
    io: Mutex<()>,
    /// The value behind [`Transport::credentials_persisted`].
    last_ok: AtomicBool,
}

impl Persistence {
    fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            io: Mutex::new(()),
            last_ok: AtomicBool::new(true),
        }
    }

    /// Persists one session change to `store` — `Some` saves, `None` clears — unless a newer
    /// change has already been applied to memory, in which case the newer change's own persist
    /// is the one that must reach the store and this returns `Ok(())` without touching it.
    ///
    /// Blocking: runs the store call on the calling thread, under the `io` gate.
    fn persist(
        &self,
        store: &dyn CredentialStore,
        ticket: u64,
        session: Option<&Session>,
    ) -> Result<(), StorageError> {
        // A store that panicked in an earlier persist poisons the gate; the gate guards no data,
        // so later persists carry on regardless.
        let _gate = self.io.lock().unwrap_or_else(PoisonError::into_inner);
        if self.generation.load(Ordering::SeqCst) != ticket {
            tracing::debug!("session persist skipped: a newer session change supersedes it");
            return Ok(());
        }
        let Some(session) = session else {
            // Reset before the store is called, so a failing clear still reads as persisted.
            self.last_ok.store(true, Ordering::SeqCst);
            return store.clear();
        };
        // A panicking store counts as a failed save before the panic carries on.
        match std::panic::catch_unwind(AssertUnwindSafe(|| store.save(session))) {
            Ok(result) => {
                self.last_ok.store(result.is_ok(), Ordering::SeqCst);
                result
            }
            Err(payload) => {
                self.last_ok.store(false, Ordering::SeqCst);
                std::panic::resume_unwind(payload)
            }
        }
    }
}

/// A [`CredentialStore`] view pinned to one session change, so the shared
/// [`storage::save_async`]/[`storage::clear_async`] blocking-pool adapters (and their
/// `JoinError` handling, which never interpolates a panic payload) can run a generation-checked
/// persist: `save`/`clear` go through [`Persistence::persist`] with this change's stamp.
#[derive(Debug)]
struct TicketedStore {
    store: Arc<dyn CredentialStore>,
    persistence: Arc<Persistence>,
    ticket: u64,
}

impl CredentialStore for TicketedStore {
    fn load(&self) -> Result<Session, StorageError> {
        self.store.load()
    }

    fn save(&self, session: &Session) -> Result<(), StorageError> {
        self.persistence
            .persist(self.store.as_ref(), self.ticket, Some(session))
    }

    fn clear(&self) -> Result<(), StorageError> {
        self.persistence
            .persist(self.store.as_ref(), self.ticket, None)
    }
}

impl Transport {
    /// Starts building a [`Transport`] with [`TransportBuilder`]'s defaults: the real Eero cloud
    /// hosts, [`consts::DEFAULT_USER_AGENT`], [`consts::DEFAULT_ACCEPT_LANGUAGE`], the legacy
    /// cookie sent by default, no `GET` retries, no session, no credential store, and the
    /// timeouts from `consts`.
    #[must_use]
    pub fn builder() -> TransportBuilder {
        TransportBuilder::default()
    }

    /// Scheme + authority of the configured API host, with no path — the `host` argument every
    /// [`crate::links`]/[`crate::params`] helper, and [`Resource::resolve`]/[`Nested::resolve`],
    /// take explicitly.
    #[must_use]
    pub fn api_host(&self) -> &Url {
        &self.api_host
    }

    /// The `User-Agent` this transport sends on every request: [`consts::DEFAULT_USER_AGENT`]
    /// unless [`TransportBuilder::user_agent`] overrode it.
    ///
    /// Ported from `BaseAPI.user_agent` (`base.py:516-519`). The value is fixed when the
    /// transport is built (and validated then), so reading it never fails and never changes.
    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Returns a snapshot of the currently configured session, if any.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.session_snapshot()
    }

    /// Replaces the in-memory session and, if a credential store is configured, persists the
    /// change on the calling thread: `Some(session)` calls `CredentialStore::save`, `None` calls
    /// `CredentialStore::clear`.
    ///
    /// # Blocking
    ///
    /// The store call runs inline, so a slow or hung backend (a native keyring/Secret Service
    /// call, a slow `fsync`) blocks the calling thread for as long as it takes. Call this from
    /// plain threads and tests only; from async code use [`Transport::set_session_async`], which
    /// moves the store I/O onto the runtime's blocking pool.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `session` is `Some` and its token is empty or contains a
    /// byte outside the printable-ASCII range (or a CR/LF) — the same shape check
    /// `AuthApi::set_session_token` applies, reproduced here since this is the one place every
    /// session change ultimately funnels through. Nothing changes in that case, including
    /// [`Transport::credentials_persisted`].
    ///
    /// Otherwise honours this transport's [`StorageFailures`] policy: under the default
    /// [`StorageFailures::Warn`], a configured credential store failing to persist the change is
    /// logged at `WARN` and this returns `Ok(())` regardless; under [`StorageFailures::Fatal`],
    /// it returns `Error::Storage` instead. Either way, the in-memory session is updated
    /// regardless of whether persistence succeeds, and *before* the (potentially slow or hung)
    /// blocking store call even starts — a hung Secret Service/D-Bus call or a slow `fsync` must
    /// not leave a stale token observable via `is_authenticated()`/`session()`, or attached to a
    /// concurrent in-flight request, for the duration of that call.
    pub fn set_session(&self, session: Option<Session>) -> Result<(), Error> {
        if let Some(ref s) = session {
            crate::auth::session::validate_token_shape("token", s.expose_token())?;
        }
        let Some(store) = &self.store else {
            self.replace_session(session);
            return Ok(());
        };
        let for_store = session.clone();
        let ticket = self.replace_session(session);
        let outcome = self
            .persistence
            .persist(store.as_ref(), ticket, for_store.as_ref());
        self.apply_storage_policy(outcome)
    }

    /// Replaces the in-memory session and persists the change without blocking the async
    /// runtime: the same contract as [`Transport::set_session`], with the credential-store call
    /// moved onto the blocking pool (`tokio::task::spawn_blocking`).
    ///
    /// The validation, the in-memory change and the hand-off to the blocking pool all happen
    /// synchronously, in the first poll of the returned future and before it first yields, so
    /// every task that runs during the hop already sees the new session, and dropping the future
    /// after that point does not cancel the persist (the blocking task runs to completion). When
    /// several changes are in flight at once the last one applied wins: an older persist that
    /// reaches the store after a newer in-memory change is skipped, so the stored record always
    /// ends up matching the final in-memory session (see the module docs). Needs a Tokio
    /// runtime; works on both the current-thread and the multi-thread flavours.
    ///
    /// # Errors
    ///
    /// As [`Transport::set_session`]. Under [`StorageFailures::Fatal`] a store task that panics
    /// (or is cancelled at runtime shutdown) is reported as [`Error::Storage`] with a fixed
    /// message that never carries the panic payload; under [`StorageFailures::Warn`] it is logged
    /// at `WARN` and this returns `Ok(())`.
    pub async fn set_session_async(&self, session: Option<Session>) -> Result<(), Error> {
        if let Some(ref s) = session {
            crate::auth::session::validate_token_shape("token", s.expose_token())?;
        }
        let Some(store) = &self.store else {
            self.replace_session(session);
            return Ok(());
        };
        let for_store = session.clone();
        let ticket = self.replace_session(session);
        let ticketed: Arc<dyn CredentialStore> = Arc::new(TicketedStore {
            store: Arc::clone(store),
            persistence: Arc::clone(&self.persistence),
            ticket,
        });
        let outcome = match for_store {
            Some(s) => storage::save_async(ticketed, s).await,
            None => storage::clear_async(ticketed).await,
        };
        self.apply_storage_policy(outcome)
    }

    /// Whether the most recent attempt to persist a session change to the configured credential
    /// store succeeded.
    ///
    /// `true` initially, and always `true` when no store is configured. Each persist that reaches
    /// the store sets it from its outcome: a save sets it to whether the store accepted the
    /// session, so an `Err` from the store makes it `false` even under [`StorageFailures::Warn`],
    /// where the failure is otherwise only a log line. A clear resets it to `true` before the
    /// store is called, so it is `true` after a clear even if that clear failed (the credential
    /// being destroyed is no longer one that needs to survive a restart). A persist skipped
    /// because a newer session change overtook it leaves the value untouched, as does a rejected
    /// (invalid) session.
    ///
    /// The flag only says that the process-exit survival of the *current* session is at risk; it
    /// never affects whether the session works in memory.
    #[must_use]
    pub fn credentials_persisted(&self) -> bool {
        self.persistence.last_ok.load(Ordering::SeqCst)
    }

    /// Applies this transport's [`StorageFailures`] policy to the outcome of one persist.
    fn apply_storage_policy(&self, outcome: Result<(), StorageError>) -> Result<(), Error> {
        match outcome {
            Ok(()) => Ok(()),
            Err(err) => match self.storage_failures {
                StorageFailures::Warn => {
                    tracing::warn!(error = %err, "failed to persist session to credential store");
                    Ok(())
                }
                StorageFailures::Fatal => Err(err.into()),
            },
        }
    }

    /// Whether a session is configured and has a non-empty token.
    ///
    /// Purely a local check (`Session::is_valid`, itself a token-presence check — `v8.0.4` has no
    /// client-side expiry, see [`Session`]'s own docs). Never makes a network call and never
    /// attempts a refresh.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.session_snapshot()
            .is_some_and(|session| session.is_valid())
    }

    // ============================= v8.0.4 request API =============================

    /// Sends an authenticated request built from an already-resolved `url`.
    ///
    /// Performs the one-shot server-driven refresh retry described in the module docs: on a
    /// `401` whose body carries `meta.error == "error.session.refresh"`, calls
    /// [`Transport::refresh_session`] and, if it reports success, retries the identical request
    /// exactly once with a freshly re-read token before giving up. Runs through the same bounded
    /// `GET`-only retry every request in this transport shares (`TransportBuilder::get_retries`).
    ///
    /// Runs under the transport's default timeout; use [`Transport::request_with_timeout`] to
    /// replace it for one call.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request produces
    /// otherwise, or propagates a refresh failure unmodified if the refresh hook itself errors.
    pub async fn request(
        &self,
        method: Method,
        url: Url,
        query: &[(&str, String)],
        body: RequestBody,
    ) -> Result<Envelope, Error> {
        self.request_with_timeout(method, url, query, body, None)
            .await
    }

    /// [`Transport::request`] with an optional per-call [`RequestTimeout`].
    ///
    /// `Some(timeout)` replaces the transport's default timeout wholesale for this call (see
    /// [`RequestTimeout`]); `None` keeps the default. The value applies to every attempt of the
    /// bounded `GET` retry and to the replay after a server-driven refresh, each of which gets
    /// the full budget; the refresh `POST` itself always runs under the transport default.
    ///
    /// # Errors
    ///
    /// As [`Transport::request`]; an elapsed bound is [`Error::Timeout`].
    pub async fn request_with_timeout(
        &self,
        method: Method,
        url: Url,
        query: &[(&str, String)],
        body: RequestBody,
        timeout: Option<RequestTimeout>,
    ) -> Result<Envelope, Error> {
        let token = self.current_token().ok_or_else(not_authenticated)?;
        let exchange = self
            .execute_with_get_retry(method.clone(), &url, query, &body, Some(&token), timeout)
            .await?;

        if exchange.status.as_u16() == 401 && refresh_signal_detected(&exchange.body) {
            let refreshed = self.refresh_session().await?;
            if refreshed {
                let retry_token = self.current_token().ok_or_else(not_authenticated)?;
                let retry = self
                    .execute_with_get_retry(method, &url, query, &body, Some(&retry_token), timeout)
                    .await?;
                return status_to_envelope(
                    retry.status,
                    &retry.body,
                    &retry.url,
                    retry.retry_after,
                );
            }
        }

        status_to_envelope(
            exchange.status,
            &exchange.body,
            &exchange.url,
            exchange.retry_after,
        )
    }

    /// Sends an authenticated request to a [`Resource`]: `route.resolve(self.api_host(),
    /// id_or_url, parent)` followed by [`Transport::request`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `route.resolve` fails, otherwise as [`Transport::request`].
    pub async fn resource(
        &self,
        route: &Resource,
        id_or_url: &str,
        parent: Option<&Value>,
        query: &[(&str, String)],
        body: RequestBody,
    ) -> Result<Envelope, Error> {
        self.resource_with_timeout(route, id_or_url, parent, query, body, None)
            .await
    }

    /// [`Transport::resource`] with an optional per-call [`RequestTimeout`], applied as in
    /// [`Transport::request_with_timeout`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `route.resolve` fails, otherwise as
    /// [`Transport::request_with_timeout`].
    pub async fn resource_with_timeout(
        &self,
        route: &Resource,
        id_or_url: &str,
        parent: Option<&Value>,
        query: &[(&str, String)],
        body: RequestBody,
        timeout: Option<RequestTimeout>,
    ) -> Result<Envelope, Error> {
        let url = route.resolve(self.api_host(), id_or_url, parent)?;
        self.request_with_timeout(route.method.clone(), url, query, body, timeout)
            .await
    }

    /// Sends an authenticated request to a [`Nested`] two-level resource: `route.resolve(
    /// self.api_host(), network, child, parent)` followed by [`Transport::request`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `route.resolve` fails, otherwise as [`Transport::request`].
    pub async fn nested(
        &self,
        route: &Nested,
        network: &str,
        child: &str,
        parent: Option<&Value>,
        query: &[(&str, String)],
        body: RequestBody,
    ) -> Result<Envelope, Error> {
        self.nested_with_timeout(route, network, child, parent, query, body, None)
            .await
    }

    /// [`Transport::nested`] with an optional per-call [`RequestTimeout`], applied as in
    /// [`Transport::request_with_timeout`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `route.resolve` fails, otherwise as
    /// [`Transport::request_with_timeout`].
    #[allow(clippy::too_many_arguments)]
    pub async fn nested_with_timeout(
        &self,
        route: &Nested,
        network: &str,
        child: &str,
        parent: Option<&Value>,
        query: &[(&str, String)],
        body: RequestBody,
        timeout: Option<RequestTimeout>,
    ) -> Result<Envelope, Error> {
        let url = route.resolve(self.api_host(), network, child, parent)?;
        self.request_with_timeout(route.method.clone(), url, query, body, timeout)
            .await
    }

    /// Sends a request authenticated with an explicit (or absent) token: no "not authenticated"
    /// precondition, no refresh-and-replay. Used by the login handshake
    /// (`LoginFlow`/`PendingLogin`) and internally by [`Transport::refresh_session`] — none of
    /// `login`/`login/verify`/`login/resend`/`logout`/`login/refresh` should trigger (or be
    /// eligible for) the ordinary refresh replay, matching `AuthAPI` never wiring its own refresh
    /// hook onto itself (`api/base.py:873-878`, preventing recursion on the refresh endpoint's
    /// own 401).
    ///
    /// # Errors
    ///
    /// Returns whatever status-mapped [`Error`] the request produces.
    pub(crate) async fn request_with_token(
        &self,
        method: Method,
        url: Url,
        query: &[(&str, String)],
        body: RequestBody,
        token: Option<&SecretString>,
    ) -> Result<Envelope, Error> {
        let exchange = self
            .execute_with_get_retry(method, &url, query, &body, token, None)
            .await?;
        status_to_envelope(
            exchange.status,
            &exchange.body,
            &exchange.url,
            exchange.retry_after,
        )
    }

    /// Attempts to refresh the current session, ported from `AuthAPI._do_refresh`
    /// (`api/auth.py:415-477`) with single-flight coalescing ported from
    /// `AuthAPI.refresh_session` (`api/auth.py:331-413`).
    ///
    /// Sends `POST` [`routes::LOGIN_REFRESH`] (the only refresh route at `v8.0.4` — `v6.2.0`'s
    /// `account/refresh` fallback is gone) with body [`RequestBody::EmptyJsonString`],
    /// authenticated by the *current* session token — there is no separate refresh token at
    /// `v8.0.4`, the session token authenticates its own refresh. On success, the response body is
    /// **not** parsed for a new token at all: the server-issued token is discarded per SDK policy
    /// and the current token stays current.
    ///
    /// # Single-flight coalescing
    ///
    /// Concurrent callers on this `Transport` share one in-flight refresh: the first caller
    /// becomes the *leader* and performs the actual network call; every other concurrent caller
    /// becomes a *waiter*, capped at [`consts::SESSION_REFRESH_GUARD_TIMEOUT`] (30s) — a waiter
    /// whose wait exceeds that cap gives up and returns `Ok(false)` rather than raising, so its
    /// own original error (e.g. the `401` that triggered the refresh in the first place) surfaces
    /// instead of a fabricated one. A leader whose future is dropped or cancelled before it
    /// finishes resolves every waiter to `Ok(false)` too (see `Transport::refresh_slot`'s own
    /// docs for the self-healing mechanism this relies on — no explicit cleanup step is needed,
    /// and a single cancellation can never permanently wedge every future refresh attempt).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("No session token available. Login first.")` if no valid
    /// session is configured. On a terminal `Error::Authentication` from the refresh call itself:
    /// if its `error_code` classifies into [`errors::ErrorGroup::Verification`] or
    /// [`errors::ErrorGroup::SessionRefresh`] (the credential-retention groups), credentials are
    /// **retained** and this returns `Ok(false)`; otherwise credentials are **destroyed**
    /// (in-memory, plus `CredentialStore::clear` if configured — honouring this transport's
    /// [`StorageFailures`] policy, so a `Fatal`-policy clear failure surfaces as `Error::Storage`
    /// here) and this returns `Ok(false)`. `Error::Network`/`Error::Timeout` propagate unmodified,
    /// credentials untouched. Every other `Error` is treated as non-terminal: credentials are left
    /// untouched and this returns `Ok(false)`.
    pub async fn refresh_session(&self) -> Result<bool, Error> {
        enum Role {
            Leader(Arc<watch::Sender<Option<bool>>>),
            Waiter(watch::Receiver<Option<bool>>),
        }

        let role = {
            let mut guard = self.refresh_slot.lock().await;
            if let Some(sender) = guard.upgrade() {
                let rx = sender.subscribe();
                Role::Waiter(rx)
            } else {
                let (tx, _rx) = watch::channel(None);
                let tx = Arc::new(tx);
                *guard = Arc::downgrade(&tx);
                Role::Leader(tx)
            }
        };

        match role {
            Role::Leader(tx) => {
                let result = self.do_refresh().await;
                let broadcast = match &result {
                    Ok(refreshed) => Some(*refreshed),
                    // Waiters cannot cheaply observe the leader's own `Error` (not `Clone`); a
                    // network/timeout/terminal failure here is reported to waiters as "did not
                    // succeed" rather than propagated — the leader's own caller still sees the
                    // real error via `result` below.
                    Err(_) => Some(false),
                };
                let _ = tx.send(broadcast);
                result
            }
            Role::Waiter(mut rx) => {
                let wait = async {
                    loop {
                        if let Some(value) = *rx.borrow_and_update() {
                            return value;
                        }
                        if rx.changed().await.is_err() {
                            // The leader's `Arc<Sender>` was dropped without ever sending —
                            // either it finished (and this race lost to the slot already being
                            // re-claimed) or its future was cancelled. Either way: `false`.
                            return false;
                        }
                    }
                };
                match tokio::time::timeout(consts::SESSION_REFRESH_GUARD_TIMEOUT, wait).await {
                    Ok(value) => Ok(value),
                    Err(_elapsed) => Ok(false),
                }
            }
        }
    }

    /// The actual refresh network call, run only by [`Transport::refresh_session`]'s leader.
    async fn do_refresh(&self) -> Result<bool, Error> {
        let session = self.session_snapshot();
        let Some(token) = session.filter(Session::is_valid).map(|s| s.token().clone()) else {
            return Err(Error::authentication(
                "No session token available. Login first.",
            ));
        };

        let url = routes::LOGIN_REFRESH.resolve(self.api_host(), "", None)?;
        match self
            .request_with_token(
                Method::POST,
                url,
                &[],
                RequestBody::EmptyJsonString,
                Some(&token),
            )
            .await
        {
            Ok(_envelope) => {
                tracing::debug!("Session refreshed; server-issued token ignored per SDK policy");
                Ok(true)
            }
            Err(Error::Authentication { error_code, .. }) => {
                let group = errors::classify_error_code(error_code.as_deref());
                let retain = matches!(
                    group,
                    Some(errors::ErrorGroup::Verification | errors::ErrorGroup::SessionRefresh)
                );
                if !retain {
                    self.set_session_async(None).await?;
                }
                Ok(false)
            }
            Err(err @ (Error::Network(_) | Error::Timeout)) => Err(err),
            Err(_other) => Ok(false),
        }
    }

    /// Clones the in-memory session out from behind the lock. The guard never survives past this
    /// one statement, so it is never held across an `.await` by any caller.
    fn session_snapshot(&self) -> Option<Session> {
        self.session
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Returns the current session's token, but only if the session is present and
    /// `Session::is_valid`. This is the exact gate `request`/`send`-family methods use for the
    /// "Not authenticated" precondition.
    fn current_token(&self) -> Option<SecretString> {
        self.session_snapshot()
            .filter(Session::is_valid)
            .map(|session| session.token().clone())
    }

    /// Overwrites the in-memory session without touching the credential store, and returns the
    /// generation stamp of this change.
    ///
    /// The stamp is taken while the write lock is still held, so stamps are issued in exactly
    /// the order the in-memory changes were applied.
    fn replace_session(&self, session: Option<Session>) -> u64 {
        let mut guard = self.session.write().unwrap_or_else(PoisonError::into_inner);
        *guard = session;
        self.persistence.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether `url`'s scheme, host, and port (default-normalised) all match this transport's
    /// configured [`Transport::api_host`].
    ///
    /// Ported from `_validate_absolute_url`'s host/scheme comparison (`base.py:394-406`), plus a
    /// port comparison this crate adds: `eero-api`'s production host never carries a port, so
    /// Python has no need to compare one, but `TransportBuilder::base_url` (used by every test in
    /// this crate) legitimately does — two different local mock servers share the loopback
    /// hostname but differ only by port, and a credential-placement gate that ignored port would
    /// not actually withhold the credential from a "foreign" mock server in that common case.
    fn matches_configured_host(&self, url: &Url) -> bool {
        let scheme_matches = url.scheme().eq_ignore_ascii_case(self.api_host.scheme());
        let host_matches = matches!(
            (url.host_str(), self.api_host.host_str()),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b)
        );
        let port_matches = url.port_or_known_default() == self.api_host.port_or_known_default();
        scheme_matches && host_matches && port_matches
    }

    /// Attaches the session credential to `request`, when `token` is `Some` — ported from
    /// `_build_credentials` (`base.py:363-417`).
    ///
    /// Attached only when `url`'s scheme/host/port match this transport's configured API host
    /// (see [`Transport::matches_configured_host`]): `X-User-Token: <token>` always, plus —
    /// only if [`TransportBuilder::send_legacy_cookie`] is on (the default) — a
    /// `Cookie: s=<token>` header. On a mismatch, no credential is attached and a `WARNING` is
    /// logged with Python's wording, reproduced verbatim (`base.py:407-412`).
    fn apply_credentials(
        &self,
        mut request: reqwest::RequestBuilder,
        url: &Url,
        token: Option<&SecretString>,
    ) -> reqwest::RequestBuilder {
        let Some(token) = token else {
            return request;
        };
        if !self.matches_configured_host(url) {
            let scheme_matches = url.scheme().eq_ignore_ascii_case(self.api_host.scheme());
            let host_matches = matches!(
                (url.host_str(), self.api_host.host_str()),
                (Some(a), Some(b)) if a.eq_ignore_ascii_case(b)
            );
            let reason = if host_matches && !scheme_matches {
                "non-matching scheme"
            } else {
                "foreign host"
            };
            tracing::warn!(
                reason,
                scheme = url.scheme(),
                host = url.host_str().unwrap_or(""),
                "Session header withheld from a request to a {reason}: {}://{}",
                url.scheme(),
                url.host_str().unwrap_or("")
            );
            return request;
        }

        let token_str = Session::expose_secret_token(token);
        request = request.header(user_token_header(), token_str);
        if self.send_legacy_cookie {
            request = request.header(
                COOKIE,
                format!("{}={}", consts::SESSION_COOKIE_NAME, token_str),
            );
        }
        request
    }

    /// Applies `body` to `request`, per [`RequestBody`]'s doc comment.
    ///
    /// `RequestBody::Form` is encoded by hand via `url::form_urlencoded` (already a transitive
    /// dependency of this crate's direct `url` dependency, used elsewhere for query-string
    /// encoding) rather than `reqwest::RequestBuilder::form`, which needs reqwest's `form` cargo
    /// feature — not enabled for this crate (no new dependencies/feature flags without an
    /// explicit decision).
    fn apply_body(request: reqwest::RequestBuilder, body: &RequestBody) -> reqwest::RequestBuilder {
        match body {
            RequestBody::None => request,
            RequestBody::Json(value) => request.json(value),
            RequestBody::Form(pairs) => {
                let mut serializer = url::form_urlencoded::Serializer::new(String::new());
                for (key, value) in pairs {
                    serializer.append_pair(key, value);
                }
                request
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(serializer.finish())
            }
            RequestBody::EmptyJsonString => request
                .header(CONTENT_TYPE, "application/json")
                .body("\"\""),
        }
    }

    /// Runs [`Transport::execute_once`] with the bounded `GET`-only retry `_request_with_get_retry`
    /// implements (`base.py:722-767`).
    ///
    /// Writes (`method != GET`) always attempt exactly once, regardless of
    /// [`TransportBuilder::get_retries`]. A `GET` is retried, up to `get_retries` additional
    /// times, on a transport error ([`Error::Network`]/[`Error::Timeout`]) or a `5xx` status —
    /// never on a `4xx` (including `429`, which carries its own `Retry-After` semantics a caller
    /// should honour explicitly rather than have this loop silently retry). A fixed delay
    /// ([`consts::GET_RETRY_DELAY`]) separates attempts; each retry is logged at `WARN`.
    ///
    /// `timeout` is applied afresh to every attempt, so each one gets the full budget rather than
    /// sharing one deadline (`_request_with_get_retry` forwards the same `timeout` kwarg to each
    /// `_request` call).
    async fn execute_with_get_retry(
        &self,
        method: Method,
        url: &Url,
        query: &[(&str, String)],
        body: &RequestBody,
        token: Option<&SecretString>,
        timeout: Option<RequestTimeout>,
    ) -> Result<RawExchange, Error> {
        let mut attempt: u32 = 0;
        loop {
            let outcome = self
                .execute_once(&method, url, query, body, token, timeout)
                .await;
            let is_get = method == Method::GET;
            let retryable = is_get
                && attempt < self.get_retries
                && match &outcome {
                    Ok(exchange) => exchange.status.as_u16() >= 500,
                    Err(Error::Network(_) | Error::Timeout) => true,
                    Err(_) => false,
                };
            if !retryable {
                return outcome;
            }
            attempt += 1;
            // `attempt` counts the retries started so far, so the first retry reads `2/N`, as in
            // `_request_with_get_retry` (`base.py:1033-1038`). Widened because `get_retries` is
            // an unbounded `u32`. The WARN line carries no URL: the path holds network, device
            // and eero identifiers, which only the DEBUG line may show.
            let shown = u64::from(attempt) + 1;
            let max_attempts = u64::from(self.get_retries) + 1;
            tracing::warn!(
                attempt = shown,
                max_attempts,
                "Retrying GET request after transient failure (attempt {shown}/{max_attempts})"
            );
            tracing::debug!(path = url.path(), "Retrying GET {}", url.path());
            tokio::time::sleep(consts::GET_RETRY_DELAY).await;
        }
    }

    /// Sends one HTTP request under the bounds `timeout` (or the transport default) resolves to,
    /// and returns its status, body, and any incidental headers this crate cares about
    /// (`Retry-After`) — everything *before* status-code-specific interpretation, which is
    /// `status_to_envelope`'s job.
    ///
    /// The `total` bound wraps the whole attempt ([`Transport::send_and_read`]: send plus capped
    /// body read); the `read` bound is applied inside it. Either elapsing is [`Error::Timeout`].
    ///
    /// A transport failure (timeout or network error) is logged here, once per attempt, with
    /// [`log_transport_failure`].
    async fn execute_once(
        &self,
        method: &Method,
        url: &Url,
        query: &[(&str, String)],
        body: &RequestBody,
        token: Option<&SecretString>,
        timeout: Option<RequestTimeout>,
    ) -> Result<RawExchange, Error> {
        let bounds = self.bounds_for(timeout);
        let attempt = self.send_and_read(method, url, query, body, token, bounds);
        let outcome = match bounds.total {
            Some(total) => tokio::time::timeout(total, attempt)
                .await
                .unwrap_or_else(|_elapsed| Err(Error::Timeout)),
            None => attempt.await,
        };
        if let Err(err) = &outcome {
            log_transport_failure(method, url, err);
        }
        outcome
    }

    /// Resolves the bounds one attempt runs under: `per_call` replaces the transport default
    /// wholesale when supplied, and a zero duration means unbounded either way.
    fn bounds_for(&self, per_call: Option<RequestTimeout>) -> Bounds {
        let effective = per_call.unwrap_or(self.timeout);
        Bounds {
            total: effective.total.filter(|d| !d.is_zero()),
            read: effective.read.filter(|d| !d.is_zero()),
            per_call: per_call.is_some(),
        }
    }

    /// Builds and sends one request and reads its capped body, with `bounds.read` applied around
    /// `send()` and around every body-chunk read (the `total` bound is the caller's job, see
    /// [`Transport::execute_once`]).
    ///
    /// Builds `Accept: application/json`, `User-Agent`, and `X-Accept-Language` on every request
    /// (`build_request_headers`, `base.py:124-164`); attaches the session credential via
    /// [`Transport::apply_credentials`]; applies `body` via [`Transport::apply_body`]; refuses any
    /// `3xx` redirect immediately, before the body is streamed at all; and streams the body with
    /// the response-size cap.
    async fn send_and_read(
        &self,
        method: &Method,
        url: &Url,
        query: &[(&str, String)],
        body: &RequestBody,
        token: Option<&SecretString>,
        bounds: Bounds,
    ) -> Result<RawExchange, Error> {
        // Query parameters are appended directly onto the `Url` via `url`'s own percent-encoding
        // rather than `reqwest::RequestBuilder::query`, which requires reqwest's `query` cargo
        // feature — not enabled for this crate.
        let mut url = url.clone();
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }

        let mut request = self.http.request(method.clone(), url.clone());
        request = request.header(ACCEPT, "application/json");
        request = request.header(USER_AGENT, &self.user_agent);
        request = request.header(x_accept_language_header(), &self.accept_language);
        request = self.apply_credentials(request, &url, token);
        request = Self::apply_body(request, body);
        if bounds.per_call {
            // A per-call value replaces any total timeout configured on a caller-supplied
            // `reqwest::ClientBuilder` too (`RequestBuilder::timeout` overrides the client-level
            // one). The real total is enforced by `execute_once`, so this only switches the
            // client-level bound off for the request; `Duration::MAX` is clamped by tokio to a
            // deadline that never fires.
            request = request.timeout(Duration::MAX);
        }

        let response = match bounds.read {
            Some(read) => tokio::time::timeout(read, request.send())
                .await
                .map_err(|_elapsed| Error::Timeout)?,
            None => request.send().await,
        }
        .map_err(map_reqwest_error)?;
        let status = response.status();

        // Method, rendered path, and status only — never headers, cookies, or bodies, so a
        // session token can never reach a log line via this instrumentation.
        tracing::debug!(
            method = %method,
            path = url.path(),
            status = status.as_u16(),
            "eero transport request"
        );

        // Security finding F4 (superseded): `TransportBuilder::build` now forces
        // `Policy::none()` onto every `reqwest::Client` it constructs — including one built from
        // a caller-supplied `reqwest::ClientBuilder` via `TransportBuilder::http_builder` — so a
        // `3xx` response can no longer be followed *inside* `request.send()` above in the first
        // place. This check is kept anyway as a defence-in-depth belt-and-suspenders assertion:
        // if it ever fired, that would mean the redirect policy was somehow not applied.
        if response.url() != &url {
            return Err(Error::Api {
                status: status.as_u16(),
                message: format!("Redirect followed unexpectedly: {status} at {}", url.path()),
                envelope: None,
                error_code: None,
                url: Some(url.to_string()),
            });
        }

        if status.is_redirection() {
            // Security finding F5: the message must never carry the full request URL (which can
            // embed a session token in its query string) or the full `Location` value (which can
            // carry an attacker-chosen query string of its own) — only the status, the request
            // *path*, and, when present, the `Location` header's *host* are safe to render here.
            // The complete URL is still available, unredacted, via the non-`Display` `url` field.
            let location_url = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .and_then(|value| Url::parse(value).ok().or_else(|| url.join(value).ok()));
            // The WARN line names the status only; the server-supplied `Location` (whose path
            // can carry identifiers) is for DEBUG, without its query, as `_request` logs it
            // (`base.py:777-778`).
            tracing::warn!(
                status = status.as_u16(),
                "Redirect response {} blocked",
                status.as_u16()
            );
            tracing::debug!(
                "Blocked redirect Location: {}",
                location_url.as_ref().map_or_else(
                    || "<none>".to_owned(),
                    |loc| format!("{}{}", loc.host_str().unwrap_or(""), loc.path())
                )
            );
            let message = location_url.map_or_else(
                || {
                    format!(
                        "Redirect not followed: {} at {} (no Location header)",
                        status.as_u16(),
                        url.path()
                    )
                },
                |loc| {
                    format!(
                        "Redirect not followed: {} at {} -> {}",
                        status.as_u16(),
                        url.path(),
                        loc.host_str().unwrap_or("unknown host")
                    )
                },
            );
            return Err(Error::Api {
                status: status.as_u16(),
                message,
                envelope: None,
                error_code: None,
                url: Some(url.to_string()),
            });
        }

        let retry_after = parse_retry_after(response.headers());
        let body_text = read_capped_body(response, bounds.read).await?;

        Ok(RawExchange {
            status,
            body: body_text,
            retry_after,
            url,
        })
    }
}

/// Header name for the primary session credential (`X-User-Token`), built lazily so a malformed
/// `'static` literal would be caught by this crate's own test suite rather than trusted blindly.
fn user_token_header() -> HeaderName {
    HeaderName::from_static("x-user-token")
}

/// Header name for `X-Accept-Language`. See [`user_token_header`]'s doc comment.
fn x_accept_language_header() -> HeaderName {
    HeaderName::from_static("x-accept-language")
}

/// The ingredients of one HTTP exchange, before status-code interpretation.
struct RawExchange {
    status: StatusCode,
    body: String,
    retry_after: Option<Duration>,
    /// The exact URL requested, including any query string appended in `Transport::execute_once`
    /// — used for error messages instead of the pre-query `Url` the caller originally rendered.
    url: Url,
}

/// The literal `Error::Authentication("Not authenticated")` guard every authenticated call
/// raises when no valid session is configured.
fn not_authenticated() -> Error {
    Error::authentication("Not authenticated")
}

/// Turns a response status and (already fully read) body into an `Envelope` or the matching
/// `Error`, reproducing `api/base.py`'s status-code chain at `v8.0.4`:
///
/// - `204`, or any `2xx` with an empty/whitespace-only body, becomes `Envelope::empty()`.
/// - Any other `2xx` is parsed as JSON; invalid JSON becomes `Error::Api` with message
///   `"Invalid JSON response ({n} bytes)"` — a byte count only, never body content.
/// - Every other non-`2xx`, non-`3xx` status is classified by [`errors::error_for_response`].
///
/// Every debug log line in this function logs `status` and, for a JSON body, the *redacted*
/// envelope ([`redact::redact_sensitive`]) — mirroring `_log_error_body` (`base.py:78-105`) —
/// never the raw body text.
fn status_to_envelope(
    status: StatusCode,
    body: &str,
    url: &Url,
    retry_after: Option<Duration>,
) -> Result<Envelope, Error> {
    let code = status.as_u16();

    if (200..300).contains(&code) {
        if status == StatusCode::NO_CONTENT || body.trim().is_empty() {
            return Ok(Envelope::empty());
        }
        return serde_json::from_str::<Value>(body)
            .map(Envelope::from_value)
            .map_err(|_| Error::Api {
                status: code,
                message: format!("Invalid JSON response ({} bytes)", body.len()),
                envelope: None,
                error_code: None,
                url: Some(url.to_string()),
            });
    }

    let envelope = errors::parse_envelope(body);
    let error_code = envelope.as_ref().and_then(errors::error_code_from_envelope);

    // The redaction sits inside the macro's field list, which `tracing` evaluates only when the
    // callsite is enabled, so a disabled DEBUG level costs no redaction (`logging.py:321-322`).
    tracing::debug!(
        status = code,
        envelope = ?envelope.as_ref().map(redacted_for_log),
        "eero transport error response"
    );

    let mut err = errors::error_for_response(code, envelope, error_code, retry_after);
    if let Error::Api { url: err_url, .. } = &mut err {
        *err_url = Some(url.to_string());
    }
    Err(err)
}

/// Streams `response`'s body with a running size check, allowing `read` (when bounded) for each
/// chunk to arrive.
async fn read_capped_body(
    mut response: reqwest::Response,
    read: Option<Duration>,
) -> Result<String, Error> {
    let status = response.status();
    let mut buffer: Vec<u8> = Vec::new();

    while let Some(chunk) = next_chunk(&mut response, read).await? {
        if buffer.len() + chunk.len() > consts::MAX_RESPONSE_BYTES {
            return Err(Error::Api {
                status: status.as_u16(),
                message: format!(
                    "Response body exceeded max size of {} bytes",
                    consts::MAX_RESPONSE_BYTES
                ),
                envelope: None,
                error_code: None,
                url: Some(response.url().to_string()),
            });
        }
        buffer.extend_from_slice(&chunk);
    }

    String::from_utf8(buffer).map_err(|err| Error::Api {
        status: status.as_u16(),
        message: format!("Response body is not valid UTF-8: {err}"),
        envelope: None,
        error_code: None,
        url: Some(response.url().to_string()),
    })
}

/// Reads the next body chunk, failing with [`Error::Timeout`] if `read` is bounded and elapses
/// first.
async fn next_chunk(
    response: &mut reqwest::Response,
    read: Option<Duration>,
) -> Result<Option<bytes::Bytes>, Error> {
    let chunk = match read {
        Some(read) => tokio::time::timeout(read, response.chunk())
            .await
            .map_err(|_elapsed| Error::Timeout)?,
        None => response.chunk().await,
    };
    chunk.map_err(map_reqwest_error)
}

/// Parses a `Retry-After` header value into a `Duration`, accepting both the delta-seconds form
/// and the HTTP-date form (RFC 1123). Returns `None` when the header is absent or its value
/// matches neither form.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim().to_owned();

    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let naive = jiff::civil::DateTime::strptime("%a, %d %b %Y %H:%M:%S GMT", &raw).ok()?;
    let zoned = naive.to_zoned(jiff::tz::TimeZone::UTC).ok()?;
    let target = SystemTime::from(zoned);
    Some(
        target
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

/// Defensively checks whether a `401` response body carries the server-driven refresh signal
/// (`classify_error_code(error_code) is ErrorGroup.SESSION_REFRESH`, ported from
/// `api/base.py:602-680`). Any parse failure degrades silently to `false`.
fn refresh_signal_detected(body: &str) -> bool {
    let Some(envelope) = errors::parse_envelope(body) else {
        return false;
    };
    let error_code = errors::error_code_from_envelope(&envelope);
    errors::classify_error_code(error_code.as_deref()) == Some(errors::ErrorGroup::SessionRefresh)
}

thread_local! {
    /// How many envelopes [`redacted_for_log`] has redacted on this thread; lets a unit test
    /// prove that a disabled log level does no redaction work.
    #[cfg(test)]
    static REDACTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Renders `envelope` with every sensitive field redacted, for a log line.
fn redacted_for_log(envelope: &Value) -> String {
    #[cfg(test)]
    REDACTIONS.with(|count| count.set(count.get() + 1));
    redact::redact_sensitive(envelope).to_string()
}

/// Maps a transport-level `reqwest::Error` onto this crate's `Error`.
fn map_reqwest_error(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        Error::Timeout
    } else {
        Error::Network(err)
    }
}

/// The fixed token that names the kind of a network failure on the `ERROR` line.
///
/// Stands in for the exception class name `eero-api` logs (`type(err).__name__`,
/// `base.py:990`), which `reqwest` has no equivalent of. It is a closed mapping from `reqwest`'s
/// own predicates and never derived from the error's text, which embeds the request URL.
fn network_error_kind(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_redirect() {
        "redirect"
    } else if err.is_body() {
        "body"
    } else if err.is_decode() {
        "decode"
    } else if err.is_request() {
        "request"
    } else {
        "other"
    }
}

/// The underlying causes of a network failure, joined, for the `DEBUG` line.
///
/// `reqwest::Error`'s own `Display` is `error sending request for url (<full URL>)`, query string
/// included, so it is never rendered. The causes below it (hyper's, the OS's) do not carry the
/// URL. With no cause, the kind stands in.
fn network_error_cause(err: &reqwest::Error) -> String {
    let mut causes = Vec::new();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        causes.push(cause.to_string());
        source = cause.source();
    }
    if causes.is_empty() {
        format!("{} error", network_error_kind(err))
    } else {
        causes.join(": ")
    }
}

/// Logs one failed attempt the way `_request` does (`base.py:981-992`): an `ERROR` line with the
/// HTTP method (and, for a network error, the kind) and nothing else, then a `DEBUG` line with
/// the request path. Neither line carries the host or the query string, and the `ERROR` line
/// carries no part of the path either, because the path holds network, device and eero ids.
/// Errors other than a timeout or a network failure are not this function's business.
fn log_transport_failure(method: &Method, url: &Url, error: &Error) {
    match error {
        Error::Timeout => {
            tracing::error!(method = %method, "Request timed out on {method} request");
            tracing::debug!(
                method = %method,
                path = url.path(),
                "Request to {} timed out",
                url.path()
            );
        }
        Error::Network(err) => {
            let kind = network_error_kind(err);
            tracing::error!(method = %method, kind, "Network error ({kind}) on {method} request");
            tracing::debug!(
                method = %method,
                path = url.path(),
                "Network error: {} for URL: {}",
                network_error_cause(err),
                url.path()
            );
        }
        _ => {}
    }
}

/// Parses a base URL string into a `Url` validated to be usable as a path base, for use by
/// `TransportBuilder::build`.
fn parse_base(raw: &str) -> Result<Url, Error> {
    let mut url = Url::parse(raw)
        .map_err(|err| Error::validation("base_url", format!("not a valid URL: {err}")))?;
    if url.path_segments_mut().is_err() {
        return Err(Error::validation(
            "base_url",
            "must be an absolute URL that can be used as a base",
        ));
    }
    Ok(url)
}

/// Validates that `value` is safe to use as an HTTP header value: printable ASCII with no CR/LF.
///
/// Ported from `_validate_header_value` (`base.py:108-121`,
/// `_HEADER_VALUE_RE = re.compile(r"^[\x20-\x7E]*$")`) — note the `*`, not `+`: an *empty* header
/// value is valid by this rule (unlike a session token, which [`crate::auth::session::validate_token_shape`]
/// additionally requires to be non-empty).
///
/// # Errors
///
/// Returns [`Error::Validation`] with the fixed message `"header value must be printable ASCII
/// with no CR/LF"` if `value` contains any byte outside `0x20..=0x7E`.
fn validate_header_value(field: &str, value: &str) -> Result<(), Error> {
    if value.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        Ok(())
    } else {
        Err(Error::validation(
            field,
            "header value must be printable ASCII with no CR/LF",
        ))
    }
}

/// Builds a [`Transport`].
///
/// Every setter takes `self` by value and returns `Self`, so calls chain naturally; `build` is
/// the only fallible step. See each setter's docs for its default when unset.
#[derive(Debug)]
pub struct TransportBuilder {
    http_builder: Option<reqwest::ClientBuilder>,
    base_root: Option<String>,
    user_agent: Option<String>,
    accept_language: Option<String>,
    send_legacy_cookie: bool,
    get_retries: u32,
    session: Option<Session>,
    store: Option<Arc<dyn CredentialStore>>,
    storage_failures: StorageFailures,
    timeout: Duration,
    read_timeout: Duration,
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self {
            http_builder: None,
            base_root: None,
            user_agent: None,
            accept_language: None,
            send_legacy_cookie: true,
            get_retries: 0,
            session: None,
            store: None,
            storage_failures: StorageFailures::default(),
            timeout: consts::REQUEST_TIMEOUT,
            read_timeout: consts::READ_TIMEOUT,
        }
    }
}

impl TransportBuilder {
    /// Supplies a `reqwest::ClientBuilder` to customize the underlying HTTP client, instead of
    /// letting `build` construct one from scratch with this crate's defaults.
    ///
    /// # Security (finding F4)
    ///
    /// Regardless of what redirect policy `builder` carries — or omits — `build` calls
    /// `.redirect(reqwest::redirect::Policy::none())` on it **last**, immediately before
    /// `.build()`, so the resulting client can never follow a redirect no matter what this
    /// escape hatch is used to configure. This is not optional: reqwest's redirect machinery
    /// strips only the `Authorization`/`Cookie` headers on a cross-host hop, never this crate's
    /// primary `X-User-Token` credential, and re-sends a `307`/`308` request body verbatim — a
    /// followed redirect could leak the session token to, or replay a write against, a host this
    /// crate never intended to talk to. There is deliberately no way to opt back into redirect
    /// following through this crate's public API.
    ///
    /// When set, this crate's own [`TransportBuilder::timeout`]/[`TransportBuilder::read_timeout`]
    /// are **ignored** — `builder` is used exactly as given, aside from the forced redirect
    /// policy above, and the transport default is unbounded. Configure timeouts on `builder`
    /// itself, or leave both unset entirely (e.g. `reqwest::ClientBuilder::new()` with nothing
    /// else called) for a test that pairs `tokio::time::pause()` with a real HTTP round trip —
    /// the crate's own timeouts and a paused clock do not mix.
    ///
    /// A per-call [`RequestTimeout`] still applies on top of `builder`: its `total` replaces a
    /// total timeout set on `builder`, but a `read_timeout` set on `builder` applies below this
    /// crate and keeps capping every request whatever the per-call value says.
    #[must_use]
    pub fn http_builder(mut self, builder: reqwest::ClientBuilder) -> Self {
        self.http_builder = Some(builder);
        self
    }

    /// Overrides the base host for *both* API versions, deriving each from `base_url` by
    /// stripping any trailing `/` and appending `/2.2` or `/2.3` respectively. [`Transport::api_host`]
    /// is derived from this override too. When unset, `build` uses the real hosts.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_root = Some(base_url.into());
        self
    }

    /// Sets the `User-Agent` header sent on every request. `None` (the default) sends
    /// [`consts::DEFAULT_USER_AGENT`]; [`consts::LEGACY_USER_AGENT`] reproduces the previous
    /// client version's capability set.
    ///
    /// Validated at [`TransportBuilder::build`] time (printable ASCII, no CR/LF); a bad value is
    /// an [`Error::Validation`] on field `"User-Agent"`. Read the effective value back with
    /// [`Transport::user_agent`].
    #[must_use]
    pub fn user_agent(mut self, user_agent: Option<String>) -> Self {
        self.user_agent = user_agent;
        self
    }

    /// Sets the `X-Accept-Language` header sent on every request. `None` (the default) sends
    /// [`consts::DEFAULT_ACCEPT_LANGUAGE`].
    ///
    /// Validated at [`TransportBuilder::build`] time (printable ASCII, no CR/LF); a bad value is
    /// an [`Error::Validation`] on field `"X-Accept-Language"`.
    #[must_use]
    pub fn accept_language(mut self, accept_language: impl Into<String>) -> Self {
        self.accept_language = Some(accept_language.into());
        self
    }

    /// Whether to also attach a legacy `Cookie: s=<token>` header alongside the primary
    /// `X-User-Token` credential. Defaults to `true` (`send_legacy_cookie`'s Python default,
    /// `base.py:241,254-257`).
    #[must_use]
    pub fn send_legacy_cookie(mut self, send_legacy_cookie: bool) -> Self {
        self.send_legacy_cookie = send_legacy_cookie;
        self
    }

    /// Sets how many additional attempts a `GET` request gets on a transport error or a `5xx`
    /// status, beyond the first. Defaults to `0` (disabled) — see
    /// `Transport::execute_with_get_retry`.
    #[must_use]
    pub fn get_retries(mut self, get_retries: u32) -> Self {
        self.get_retries = get_retries;
        self
    }

    /// Seeds the transport with an initial session. `None` (the default) starts with no session.
    #[must_use]
    pub fn session(mut self, session: Option<Session>) -> Self {
        self.session = session;
        self
    }

    /// Sets the credential store used by `Transport::set_session`, `Transport::set_session_async`
    /// and the credential-clearing branch of `Transport::refresh_session`. `None` (the default)
    /// means no persistence at all.
    #[must_use]
    pub fn store(mut self, store: Option<Arc<dyn CredentialStore>>) -> Self {
        self.store = store;
        self
    }

    /// Sets this transport's policy for a failed credential-store operation (decision D-13).
    #[must_use]
    pub fn storage_failures(mut self, storage_failures: StorageFailures) -> Self {
        self.storage_failures = storage_failures;
        self
    }

    /// Overrides the transport's default overall (total) request timeout — the default for any
    /// request that supplies no per-call [`RequestTimeout`]. [`Duration::ZERO`] means unbounded.
    /// Ignored if [`TransportBuilder::http_builder`] was also called.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides the transport's default per-read timeout — the default for any request that
    /// supplies no per-call [`RequestTimeout`]. [`Duration::ZERO`] means unbounded. Ignored if
    /// [`TransportBuilder::http_builder`] was also called.
    #[must_use]
    pub fn read_timeout(mut self, read_timeout: Duration) -> Self {
        self.read_timeout = read_timeout;
        self
    }

    /// Builds the `Transport`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if `base_url` was set to a string that does not parse as an
    /// absolute URL usable as a path base, or if `accept_language`/`user_agent` contains a byte
    /// outside the printable-ASCII range (or a CR/LF). The field is `"base_url"`,
    /// `"X-Accept-Language"` or `"User-Agent"`, checked in that order, so a call with several bad
    /// values reports the first. Returns `Error::Network` if constructing the underlying
    /// `reqwest::Client` fails.
    ///
    /// # Security (finding F4)
    ///
    /// `.redirect(reqwest::redirect::Policy::none())` is applied last, immediately before
    /// `.build()`, regardless of whether the client comes from this crate's own defaults or from
    /// a caller-supplied [`TransportBuilder::http_builder`] — see that method's own docs.
    pub fn build(self) -> Result<Transport, Error> {
        // Both bases are validated here, even though only the 2.2 base's origin is kept
        // (`api_host`, below): a bad `base_url` override must fail at build time regardless of
        // which API version a caller's first request happens to target.
        let (base_22, _base_23) = self.build_bases()?;
        let api_host = {
            let origin = base_22.origin().ascii_serialization();
            Url::parse(&origin)
                .map_err(|err| Error::validation("base_url", format!("not a valid URL: {err}")))?
        };

        // The header name is the field name and `X-Accept-Language` is checked first, as in
        // `BaseAPI.__init__` (`base.py:474-478`), so a caller who got both wrong sees the same
        // error `eero-api` raises.
        let accept_language = self
            .accept_language
            .unwrap_or_else(|| consts::DEFAULT_ACCEPT_LANGUAGE.to_owned());
        validate_header_value("X-Accept-Language", &accept_language)?;
        let user_agent = self
            .user_agent
            .unwrap_or_else(|| consts::DEFAULT_USER_AGENT.to_owned());
        validate_header_value("User-Agent", &user_agent)?;

        // Neither branch puts a timeout on the `reqwest::Client` this crate builds: both bounds
        // are enforced per attempt in `Transport::execute_once` so a per-call `RequestTimeout`
        // can replace them (reqwest has no per-request read timeout). A caller-supplied builder
        // keeps whatever timeouts it carries itself.
        let (http, timeout) = if let Some(builder) = self.http_builder {
            // Timeouts are deliberately not forced onto a caller-supplied builder — see
            // `TransportBuilder::http_builder`'s own docs. The redirect policy is not
            // negotiable: it always wins, even over a policy `builder` explicitly set.
            let http = builder
                .redirect(Policy::none())
                .build()
                .map_err(Error::Network)?;
            (http, RequestTimeout::default())
        } else {
            let http = Client::builder()
                .redirect(Policy::none())
                .build()
                .map_err(Error::Network)?;
            (
                http,
                RequestTimeout::new(Some(self.timeout), Some(self.read_timeout)),
            )
        };

        Ok(Transport {
            http,
            api_host,
            session: RwLock::new(self.session),
            store: self.store,
            persistence: Arc::new(Persistence::new()),
            storage_failures: self.storage_failures,
            user_agent,
            accept_language,
            send_legacy_cookie: self.send_legacy_cookie,
            get_retries: self.get_retries,
            timeout,
            refresh_slot: tokio::sync::Mutex::new(Weak::new()),
        })
    }

    /// Computes the effective 2.2/2.3 base `Url`s.
    fn build_bases(&self) -> Result<(Url, Url), Error> {
        let (raw_22, raw_23) = match &self.base_root {
            Some(root) => {
                let trimmed = root.trim_end_matches('/');
                (
                    format!("{trimmed}/{}", ApiVersion::V2_2.segment()),
                    format!("{trimmed}/{}", ApiVersion::V2_3.segment()),
                )
            }
            None => (
                consts::API_BASE_22.to_owned(),
                consts::API_BASE_23.to_owned(),
            ),
        };
        Ok((parse_base(&raw_22)?, parse_base(&raw_23)?))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use url::Url;

    use super::{
        REDACTIONS, RequestTimeout, Transport, parse_retry_after, refresh_signal_detected,
        status_to_envelope, validate_header_value,
    };
    use crate::error::Error;
    use crate::routes::{ACCOUNT, ApiVersion, Resource};

    // ===================== parse_retry_after =====================

    #[test]
    fn retry_after_parses_delta_seconds_form() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("120"));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(120)));
    }

    #[test]
    fn retry_after_parses_delta_seconds_form_with_surrounding_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("  30  "));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(30)));
    }

    #[test]
    fn retry_after_parses_http_date_form_in_the_future() {
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Thu, 01 Jan 2099 00:00:00 GMT"),
        );
        let parsed = parse_retry_after(&headers).expect("valid HTTP-date form parses");
        assert!(parsed > Duration::from_secs(0));
    }

    #[test]
    fn retry_after_http_date_in_the_past_is_zero_not_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 01 Jan 2003 00:00:00 GMT"),
        );
        assert_eq!(parse_retry_after(&headers), Some(Duration::ZERO));
    }

    #[test]
    fn retry_after_absent_header_is_none() {
        let headers = HeaderMap::new();
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn retry_after_garbage_value_is_none_not_an_error() {
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("not a retry-after value"),
        );
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn retry_after_negative_number_is_none() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("-5"));
        assert_eq!(parse_retry_after(&headers), None);
    }

    // ===================== refresh_signal_detected =====================

    #[test]
    fn refresh_signal_detected_on_exact_match() {
        assert!(refresh_signal_detected(
            r#"{"meta":{"code":401,"error":"error.session.refresh"}}"#
        ));
    }

    #[test]
    fn refresh_signal_not_detected_on_a_different_error_value() {
        assert!(!refresh_signal_detected(
            r#"{"meta":{"code":401,"error":"invalid_credentials"}}"#
        ));
    }

    #[test]
    fn refresh_signal_not_detected_when_meta_is_absent() {
        assert!(!refresh_signal_detected(r#"{"data":{}}"#));
    }

    #[test]
    fn refresh_signal_not_detected_on_invalid_json() {
        assert!(!refresh_signal_detected("not json at all"));
    }

    // ===================== lazy redaction of error bodies =====================

    /// Runs `status_to_envelope` on a 400 with a JSON error body under a subscriber that lets
    /// events through up to `level`, and returns how many envelopes were redacted meanwhile.
    fn redactions_at(level: tracing::Level) -> usize {
        const BODY: &str =
            r#"{"meta":{"code":400,"error":"error.request.invalid"},"data":{"token":"x"}}"#;
        let url = Url::parse("http://mock.test:1234/2.2/account").unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(level)
            .with_writer(std::io::sink)
            .finish();
        REDACTIONS.with(|count| count.set(0));
        tracing::subscriber::with_default(subscriber, || {
            let _ = status_to_envelope(reqwest::StatusCode::BAD_REQUEST, BODY, &url, None);
        });
        REDACTIONS.with(std::cell::Cell::get)
    }

    #[test]
    fn a_disabled_debug_level_never_invokes_the_redactor() {
        assert_eq!(redactions_at(tracing::Level::WARN), 0);
        // Non-vacuity: with DEBUG enabled the same call does redact, exactly once.
        assert_eq!(redactions_at(tracing::Level::DEBUG), 1);
    }

    // ===================== validate_header_value =====================

    #[test]
    fn validate_header_value_accepts_empty() {
        validate_header_value("x", "").expect("empty header value matches the `*` rule");
    }

    #[test]
    fn validate_header_value_rejects_crlf() {
        let err = validate_header_value("x", "a\r\nb").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "x"));
    }

    #[test]
    fn validate_header_value_accepts_printable_ascii() {
        validate_header_value("x", "eero/3.0 (iPhone; iOS 17.0)").expect("printable ascii ok");
    }

    // ===================== RequestTimeout / bounds_for =====================

    #[test]
    fn request_timeout_default_is_unbounded_on_both_axes() {
        let timeout = RequestTimeout::default();
        assert_eq!(timeout.total, None);
        assert_eq!(timeout.read, None);
    }

    #[test]
    fn request_timeout_constructors_set_the_documented_fields() {
        let both = RequestTimeout::new(Some(Duration::from_secs(5)), Some(Duration::from_secs(2)));
        assert_eq!(both.total, Some(Duration::from_secs(5)));
        assert_eq!(both.read, Some(Duration::from_secs(2)));

        let total_only = RequestTimeout::total(Duration::from_secs(120));
        assert_eq!(total_only.total, Some(Duration::from_secs(120)));
        assert_eq!(total_only.read, None, "total() leaves the read bound unset");
    }

    #[test]
    fn bounds_default_to_the_builder_timeouts_when_no_per_call_value_is_supplied() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let bounds = transport.bounds_for(None);
        assert_eq!(bounds.total, Some(crate::consts::REQUEST_TIMEOUT));
        assert_eq!(bounds.read, Some(crate::consts::READ_TIMEOUT));
        assert!(!bounds.per_call);
    }

    #[test]
    fn a_per_call_value_replaces_the_default_wholesale() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let bounds = transport.bounds_for(Some(RequestTimeout::total(Duration::from_secs(120))));
        assert_eq!(bounds.total, Some(Duration::from_secs(120)));
        assert_eq!(
            bounds.read, None,
            "the default read bound must not leak back in"
        );
        assert!(bounds.per_call);

        let bounds = transport.bounds_for(Some(RequestTimeout::default()));
        assert_eq!((bounds.total, bounds.read), (None, None));
    }

    #[test]
    fn zero_durations_mean_unbounded_for_defaults_and_per_call_values() {
        let transport = Transport::builder()
            .timeout(Duration::ZERO)
            .read_timeout(Duration::ZERO)
            .build()
            .expect("builds with zero defaults");
        let bounds = transport.bounds_for(None);
        assert_eq!((bounds.total, bounds.read), (None, None));

        let bounds = transport.bounds_for(Some(RequestTimeout::new(
            Some(Duration::ZERO),
            Some(Duration::from_secs(1)),
        )));
        assert_eq!(bounds.total, None);
        assert_eq!(bounds.read, Some(Duration::from_secs(1)));
    }

    #[test]
    fn a_caller_supplied_http_builder_leaves_the_default_unbounded() {
        let transport = Transport::builder()
            .http_builder(reqwest::Client::builder())
            .timeout(Duration::from_secs(1))
            .read_timeout(Duration::from_secs(1))
            .build()
            .expect("builds with a caller-supplied builder");
        let bounds = transport.bounds_for(None);
        assert_eq!((bounds.total, bounds.read), (None, None));
    }

    // ===================== Resource::resolve / base selection =====================

    fn v2_3_resource() -> Resource {
        Resource {
            method: reqwest::Method::PUT,
            version: ApiVersion::V2_3,
            template: "networks/{id}/devices",
            link: None,
        }
    }

    #[test]
    fn default_bases_resolve_against_the_real_eero_host() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let url = ACCOUNT
            .resolve(transport.api_host(), "", None)
            .expect("a fixed path always resolves");
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/account");
    }

    #[test]
    fn overridden_base_url_is_used_for_both_api_versions() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");

        let v22 = ACCOUNT
            .resolve(transport.api_host(), "", None)
            .expect("v2.2 resource resolves");
        assert_eq!(v22.as_str(), "http://127.0.0.1:9999/2.2/account");

        let resource = v2_3_resource();
        let v23 = resource
            .resolve(transport.api_host(), "123", None)
            .expect("v2.3 resource resolves");
        assert_eq!(
            v23.as_str(),
            "http://127.0.0.1:9999/2.3/networks/123/devices"
        );
    }

    #[test]
    fn api_host_has_no_path_and_reflects_the_override() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");
        assert_eq!(transport.api_host().as_str(), "http://127.0.0.1:9999/");
        assert_eq!(transport.api_host().path(), "/");
    }

    #[test]
    fn api_host_defaults_to_the_real_eero_host() {
        let transport = Transport::builder().build().expect("builds with defaults");
        assert_eq!(transport.api_host().host_str(), Some("api-user.e2ro.com"));
        assert_eq!(transport.api_host().scheme(), "https");
    }

    #[test]
    fn overridden_base_url_trailing_slash_is_not_doubled() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999/")
            .build()
            .expect("builds with a trailing-slash base");
        let url = ACCOUNT
            .resolve(transport.api_host(), "", None)
            .expect("resource resolves");
        assert_eq!(url.as_str(), "http://127.0.0.1:9999/2.2/account");
    }

    #[test]
    fn invalid_base_url_is_rejected_at_build_time() {
        let err = Transport::builder()
            .base_url("not a url")
            .build()
            .expect_err("malformed base URL must fail fast");
        assert!(matches!(err, Error::Validation { field, .. } if field == "base_url"));
    }

    // ===================== session snapshot / is_authenticated =====================

    #[test]
    fn no_session_means_not_authenticated_and_no_snapshot() {
        let transport = Transport::builder().build().expect("builds with defaults");
        assert!(!transport.is_authenticated());
        assert!(transport.session().is_none());
    }

    #[test]
    fn set_session_updates_the_snapshot_and_authentication_state() {
        let transport = Transport::builder().build().expect("builds with defaults");
        transport
            .set_session(Some(crate::auth::Session::from_token("tok-123")))
            .expect("no store configured, cannot fail");
        assert!(transport.is_authenticated());
        assert!(transport.session().is_some());

        transport
            .set_session(None)
            .expect("clearing with no store cannot fail");
        assert!(!transport.is_authenticated());
        assert!(transport.session().is_none());
    }

    #[test]
    fn set_session_rejects_an_empty_token() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let err = transport
            .set_session(Some(crate::auth::Session::from_token("")))
            .expect_err("an empty token must be rejected");
        assert!(matches!(err, Error::Validation { field, .. } if field == "token"));
        assert!(!transport.is_authenticated());
    }

    #[test]
    fn set_session_rejects_a_token_with_a_control_character() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let err = transport
            .set_session(Some(crate::auth::Session::from_token("tok\r\nX-Evil: 1")))
            .expect_err("a control character in the token must be rejected");
        assert!(matches!(err, Error::Validation { field, .. } if field == "token"));
    }

    // ===================== matches_configured_host =====================

    #[test]
    fn matches_configured_host_true_for_the_configured_scheme_host_and_port() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");
        let url = Url::parse("http://127.0.0.1:9999/2.2/account").unwrap();
        assert!(transport.matches_configured_host(&url));
    }

    #[test]
    fn matches_configured_host_false_for_a_different_host() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");
        let url = Url::parse("http://evil.example:9999/2.2/account").unwrap();
        assert!(!transport.matches_configured_host(&url));
    }

    #[test]
    fn matches_configured_host_false_for_a_different_port() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");
        let url = Url::parse("http://127.0.0.1:8888/2.2/account").unwrap();
        assert!(!transport.matches_configured_host(&url));
    }

    #[test]
    fn matches_configured_host_false_for_a_different_scheme() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");
        let url = Url::parse("https://127.0.0.1:9999/2.2/account").unwrap();
        assert!(!transport.matches_configured_host(&url));
    }
}
