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
//! [`Transport::request`]/[`Transport::resource`]/[`Transport::nested`] are the current,
//! `RequestBody`-aware entry points; see `.claude/tasks/briefs/v8/transport-api.md` for a worked
//! example of each. [`Transport::send`]/[`Transport::send_with_query`] are kept as thin,
//! JSON-only wrappers over [`Transport::request`] purely so the endpoint modules not yet migrated
//! onto the `Resource`/`Nested` route model keep compiling and their tests keep passing; new code
//! should prefer `request`/`resource`/`nested`.
//!
//! # Locking
//!
//! The current [`crate::auth::Session`] lives behind a `std::sync::RwLock`, not a
//! `tokio::sync::RwLock`. Every critical section that touches it is a handful of clones/
//! comparisons with no `.await` inside — an async lock would only add executor overhead for no
//! benefit here. No lock guard is ever held across an `.await` point anywhere in this module.
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

use std::sync::{Arc, PoisonError, RwLock, Weak};
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
use crate::error::Error;
use crate::errors;
use crate::redact;
use crate::routes::{self, ApiVersion, Nested, Resource, Route};
use crate::storage::CredentialStore;

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

/// The request core shared by every authenticated and unauthenticated Eero cloud API call.
///
/// Build one with [`Transport::builder`]. See the module docs for the locking discipline and the
/// deliberate refresh-retry divergence from `eero-api`.
#[derive(Debug)]
pub struct Transport {
    http: Client,
    base_22: Url,
    base_23: Url,
    /// Scheme + authority (host, and port when non-default) of the configured API host, with no
    /// path — what [`crate::links`]/[`crate::params`] and every [`Resource`]/[`Nested`] route
    /// take as `host`. Derived from `base_22`'s origin at build time, so it already reflects a
    /// `TransportBuilder::base_url` override (e.g. a wiremock server in tests).
    api_host: Url,
    session: RwLock<Option<Session>>,
    store: Option<Arc<dyn CredentialStore>>,
    storage_failures: StorageFailures,
    user_agent: String,
    accept_language: String,
    send_legacy_cookie: bool,
    get_retries: u32,
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
/// in this module ([`Transport::set_session`], and the credential-clearing branch of
/// [`Transport::refresh_session`]). This is unrelated to whether the *in-memory* session is
/// updated: that happens unconditionally and before persistence is even attempted, regardless of
/// this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageFailures {
    /// Log the failure at `WARN` and proceed as though the operation had succeeded. The default.
    #[default]
    Warn,
    /// Return [`Error::Storage`] immediately instead of swallowing the failure.
    Fatal,
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

    /// Returns a snapshot of the currently configured session, if any.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.session_snapshot()
    }

    /// Replaces the in-memory session and, if a credential store is configured, persists the
    /// change: `Some(session)` calls `CredentialStore::save`, `None` calls
    /// `CredentialStore::clear`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `session` is `Some` and its token is empty or contains a
    /// byte outside the printable-ASCII range (or a CR/LF) — the same shape check
    /// `AuthApi::set_session_token` applies, reproduced here since this is the one place every
    /// session change ultimately funnels through.
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
        self.replace_session(session);
        let result = match &for_store {
            Some(s) => store.save(s),
            None => store.clear(),
        };
        match result {
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

    /// This transport's configured [`StorageFailures`] policy.
    #[must_use]
    pub(crate) fn storage_failures(&self) -> StorageFailures {
        self.storage_failures
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
        let token = self.current_token().ok_or_else(not_authenticated)?;
        let exchange = self
            .execute_with_get_retry(method.clone(), &url, query, &body, Some(&token))
            .await?;

        if exchange.status.as_u16() == 401 && refresh_signal_detected(&exchange.body) {
            let refreshed = self.refresh_session().await?;
            if refreshed {
                let retry_token = self.current_token().ok_or_else(not_authenticated)?;
                let retry = self
                    .execute_with_get_retry(method, &url, query, &body, Some(&retry_token))
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
        let url = route.resolve(self.api_host(), id_or_url, parent)?;
        self.request(route.method.clone(), url, query, body).await
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
        let url = route.resolve(self.api_host(), network, child, parent)?;
        self.request(route.method.clone(), url, query, body).await
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
            .execute_with_get_retry(method, &url, query, &body, token)
            .await?;
        status_to_envelope(
            exchange.status,
            &exchange.body,
            &exchange.url,
            exchange.retry_after,
        )
    }

    /// Sends an authenticated request with no query-string parameters and a JSON-or-absent body,
    /// through the legacy [`Route`] model. Equivalent to
    /// `send_with_query(route, path_params, &[], body)`.
    ///
    /// **Legacy.** Kept only so endpoint modules not yet migrated onto [`Resource`]/[`Nested`]
    /// keep compiling; see `.claude/tasks/briefs/v8/transport-api.md` for the migration this
    /// wrapper stands in for.
    ///
    /// # Errors
    ///
    /// See [`Transport::send_with_query`].
    pub async fn send(
        &self,
        route: &Route,
        path_params: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Envelope, Error> {
        self.send_with_query(route, path_params, &[], body).await
    }

    /// Sends an authenticated request through the legacy [`Route`] model, JSON-only.
    ///
    /// **Legacy.** A thin wrapper over [`Transport::request`]: renders `route`'s path template
    /// against `path_params` using this transport's configured base for `route`'s API version
    /// (via `Transport::render_url`, which — unlike [`Resource`]/[`Nested`] — always targets
    /// the *configured* base, so a `TransportBuilder::base_url` override still applies), then maps
    /// `body` to [`RequestBody::Json`] (`Some`) or [`RequestBody::None`] (`None`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `route`'s template cannot be rendered against
    /// `path_params`, otherwise as [`Transport::request`].
    pub async fn send_with_query(
        &self,
        route: &Route,
        path_params: &[(&str, &str)],
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Envelope, Error> {
        let url = self.render_url(route, path_params)?;
        let body = body.map_or(RequestBody::None, RequestBody::Json);
        self.request(route.method.clone(), url, query, body).await
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

        let url = self.render_url(&routes::LOGIN_REFRESH, &[])?;
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
                    self.set_session(None)?;
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

    /// Overwrites the in-memory session without touching the credential store.
    fn replace_session(&self, session: Option<Session>) {
        let mut guard = self.session.write().unwrap_or_else(PoisonError::into_inner);
        *guard = session;
    }

    /// Returns the configured base `Url` for `version`.
    fn base_for(&self, version: ApiVersion) -> &Url {
        match version {
            ApiVersion::V2_2 => &self.base_22,
            ApiVersion::V2_3 => &self.base_23,
        }
    }

    /// Renders `route`'s path template against `params` into a full request `Url`, using this
    /// transport's configured base for the route's API version.
    ///
    /// This intentionally duplicates the (small) segment-substitution loop from
    /// `routes::Route::render` rather than calling it directly: `Route::render` always builds
    /// against the *real* Eero host baked into `ApiVersion::base_url`, with no way to substitute
    /// a test double, which is exactly what `TransportBuilder::base_url` needs. Every substituted
    /// value is checked by `routes::validate_segment` — the same function `Route::render` calls —
    /// before it is pushed onto the URL.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if the configured base URL cannot be extended
    /// (`field: "base_url"`), if `route`'s template references a placeholder absent from
    /// `params` (`field` is the placeholder name), or if a substituted value fails
    /// `routes::validate_segment`.
    pub(crate) fn render_url(&self, route: &Route, params: &[(&str, &str)]) -> Result<Url, Error> {
        let mut url = self.base_for(route.version).clone();
        {
            let mut segments = url.path_segments_mut().map_err(|()| {
                Error::validation(
                    "base_url",
                    "configured base URL cannot be used as a path base",
                )
            })?;
            for part in route.path.split('/') {
                if part.is_empty() {
                    continue;
                }
                if let Some(name) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    let value = params
                        .iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| *value)
                        .ok_or_else(|| {
                            Error::validation(name, "missing value for path parameter")
                        })?;
                    routes::validate_segment(value)
                        .map_err(|reason| Error::validation(name, reason.to_string()))?;
                    segments.push(value);
                } else {
                    segments.push(part);
                }
            }
        }
        Ok(url)
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
    async fn execute_with_get_retry(
        &self,
        method: Method,
        url: &Url,
        query: &[(&str, String)],
        body: &RequestBody,
        token: Option<&SecretString>,
    ) -> Result<RawExchange, Error> {
        let mut attempt: u32 = 0;
        loop {
            let outcome = self
                .execute_once(method.clone(), url.clone(), query, body, token)
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
            tracing::warn!(
                attempt,
                method = %method,
                path = url.path(),
                "retrying GET request"
            );
            tokio::time::sleep(consts::GET_RETRY_DELAY).await;
        }
    }

    /// Sends one HTTP request and returns its status, body, and any incidental headers this
    /// crate cares about (`Retry-After`) — everything *before* status-code-specific
    /// interpretation, which is `status_to_envelope`'s job.
    ///
    /// Builds `Accept: application/json`, `User-Agent`, and `X-Accept-Language` on every request
    /// (`build_request_headers`, `base.py:124-164`); attaches the session credential via
    /// [`Transport::apply_credentials`]; applies `body` via [`Transport::apply_body`]; refuses any
    /// `3xx` redirect immediately, before the body is streamed at all; and streams the body with
    /// the response-size cap.
    async fn execute_once(
        &self,
        method: Method,
        url: Url,
        query: &[(&str, String)],
        body: &RequestBody,
        token: Option<&SecretString>,
    ) -> Result<RawExchange, Error> {
        // Query parameters are appended directly onto the `Url` via `url`'s own percent-encoding
        // rather than `reqwest::RequestBuilder::query`, which requires reqwest's `query` cargo
        // feature — not enabled for this crate.
        let mut url = url;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }

        let mut request = self.http.request(method.clone(), url.clone());
        request = request.header(ACCEPT, "application/json");
        request = request.header(USER_AGENT, &self.user_agent);
        request = request.header(x_accept_language_header(), &self.accept_language);
        request = self.apply_credentials(request, &url, token);
        request = Self::apply_body(request, body);

        let response = request.send().await.map_err(map_reqwest_error)?;
        let status = response.status();

        // Method, rendered path, and status only — never headers, cookies, or bodies, so a
        // session token can never reach a log line via this instrumentation.
        tracing::debug!(
            method = %method,
            path = url.path(),
            status = status.as_u16(),
            "eero transport request"
        );

        // Security finding F4: a caller-supplied `reqwest::Client` (`TransportBuilder::http`) can
        // keep reqwest's default `Policy::limited(10)` instead of this crate's own
        // `Policy::none()`, in which case a `3xx` response is followed *inside* `request.send()`
        // above and never reaches the `status.is_redirection()` check below at all.
        if response.url() != &url {
            return Err(Error::Api {
                status: status.as_u16(),
                message: format!(
                    "Redirect followed by a caller-supplied client: {url} -> {}",
                    response.url()
                ),
                envelope: None,
                error_code: None,
                url: Some(url.to_string()),
            });
        }

        if status.is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty());
            let message = location.map_or_else(
                || {
                    format!(
                        "Redirect not followed: {} (no Location header)",
                        status.as_u16()
                    )
                },
                |loc| format!("Redirect not followed: {} -> {loc}", status.as_u16()),
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
        let body_text = read_capped_body(response).await?;

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

    let redacted_envelope = envelope
        .as_ref()
        .map(|value| redact::redact_sensitive(value).to_string());
    tracing::debug!(
        status = code,
        envelope = ?redacted_envelope,
        "eero transport error response"
    );

    let mut err = errors::error_for_response(code, envelope, error_code, retry_after);
    if let Error::Api { url: err_url, .. } = &mut err {
        *err_url = Some(url.to_string());
    }
    Err(err)
}

/// Streams `response`'s body with a running size check.
async fn read_capped_body(mut response: reqwest::Response) -> Result<String, Error> {
    let status = response.status();
    let mut buffer: Vec<u8> = Vec::new();

    while let Some(chunk) = response.chunk().await.map_err(map_reqwest_error)? {
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

/// Maps a transport-level `reqwest::Error` onto this crate's `Error`.
fn map_reqwest_error(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        Error::Timeout
    } else {
        Error::Network(err)
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
    http: Option<Client>,
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
            http: None,
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
    /// Supplies a fully-configured `reqwest::Client` instead of letting `build` construct one.
    ///
    /// When set, `timeout`/`read_timeout` are ignored — the supplied client is used exactly as
    /// given (headers are still built explicitly per-request by `Transport::execute_once`
    /// regardless of which client is used).
    ///
    /// # Warning
    ///
    /// A caller-supplied client silently discards two of this crate's safety guarantees: redirect
    /// refusal (`reqwest::redirect::Policy::none()`) and the request/read timeouts in
    /// [`crate::consts`]. `Transport::execute_once`'s own response-URL check (security finding
    /// F4) still refuses a *followed* redirect before its body is ever read, regardless of which
    /// client is injected here, but there is no 30-second ceiling on a hung request either way.
    #[must_use]
    pub fn http(mut self, client: Client) -> Self {
        self.http = Some(client);
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
    /// [`consts::DEFAULT_USER_AGENT`] — decision D-7 is superseded at `v8.0.4`, where this header
    /// is genuinely sent.
    #[must_use]
    pub fn user_agent(mut self, user_agent: Option<String>) -> Self {
        self.user_agent = user_agent;
        self
    }

    /// Sets the `X-Accept-Language` header sent on every request. `None` (the default) sends
    /// [`consts::DEFAULT_ACCEPT_LANGUAGE`].
    ///
    /// Validated at [`TransportBuilder::build`] time (printable ASCII, no CR/LF); see
    /// `validate_header_value`.
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

    /// Sets the credential store used by `Transport::set_session` and the credential-clearing
    /// branch of `Transport::refresh_session`. `None` (the default) means no persistence at all.
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

    /// Overrides the overall request timeout. Ignored if `http` was also called.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides the per-read timeout. Ignored if `http` was also called.
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
    /// outside the printable-ASCII range (or a CR/LF). Returns `Error::Network` if constructing
    /// the underlying `reqwest::Client` fails.
    pub fn build(self) -> Result<Transport, Error> {
        let (base_22, base_23) = self.build_bases()?;
        let api_host = {
            let origin = base_22.origin().ascii_serialization();
            Url::parse(&origin)
                .map_err(|err| Error::validation("base_url", format!("not a valid URL: {err}")))?
        };

        let user_agent = self
            .user_agent
            .unwrap_or_else(|| consts::DEFAULT_USER_AGENT.to_owned());
        validate_header_value("user_agent", &user_agent)?;
        let accept_language = self
            .accept_language
            .unwrap_or_else(|| consts::DEFAULT_ACCEPT_LANGUAGE.to_owned());
        validate_header_value("X-Accept-Language", &accept_language)?;

        let http = if let Some(client) = self.http {
            client
        } else {
            Client::builder()
                .timeout(self.timeout)
                .read_timeout(self.read_timeout)
                .redirect(Policy::none())
                .build()
                .map_err(Error::Network)?
        };

        Ok(Transport {
            http,
            base_22,
            base_23,
            api_host,
            session: RwLock::new(self.session),
            store: self.store,
            storage_failures: self.storage_failures,
            user_agent,
            accept_language,
            send_legacy_cookie: self.send_legacy_cookie,
            get_retries: self.get_retries,
            refresh_slot: tokio::sync::Mutex::new(Weak::new()),
        })
    }

    /// Computes the effective 2.2/2.3 base `Url`s.
    fn build_bases(&self) -> Result<(Url, Url), Error> {
        let (raw_22, raw_23) = match &self.base_root {
            Some(root) => {
                let trimmed = root.trim_end_matches('/');
                (
                    format!("{trimmed}/{}", version_segment(ApiVersion::V2_2)),
                    format!("{trimmed}/{}", version_segment(ApiVersion::V2_3)),
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

/// The version path segment (e.g. `"2.2"`) baked into `version`'s production base URL.
///
/// # Panics
///
/// Never in practice: [`ApiVersion::base_url`] always returns one of `consts::API_BASE_22` /
/// `consts::API_BASE_23`, both non-empty absolute URLs, so `rsplit('/').next()` always yields at
/// least one item.
fn version_segment(version: ApiVersion) -> &'static str {
    version
        .base_url()
        .rsplit('/')
        .next()
        .expect("base_url is a non-empty static string; rsplit always yields at least one item")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use url::Url;

    use super::{
        Route, Transport, parse_retry_after, refresh_signal_detected, validate_header_value,
    };
    use crate::error::Error;
    use crate::routes::{ACCOUNT, ApiVersion};

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

    // ===================== render_url / base selection =====================

    fn v2_3_route() -> Route {
        Route {
            method: reqwest::Method::PUT,
            version: ApiVersion::V2_3,
            path: "networks/{network_id}/devices/{device_id}",
        }
    }

    #[test]
    fn default_bases_render_the_real_eero_hosts() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let url = transport
            .render_url(&ACCOUNT, &[])
            .expect("no placeholders needed");
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/account");
    }

    #[test]
    fn overridden_base_url_is_used_for_both_api_versions() {
        let transport = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");

        let v22 = transport
            .render_url(&ACCOUNT, &[])
            .expect("v2.2 route renders");
        assert_eq!(v22.as_str(), "http://127.0.0.1:9999/2.2/account");

        let route = v2_3_route();
        let v23 = transport
            .render_url(&route, &[("network_id", "123"), ("device_id", "aa:bb")])
            .expect("v2.3 route renders");
        assert_eq!(
            v23.as_str(),
            "http://127.0.0.1:9999/2.3/networks/123/devices/aa:bb"
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
        let url = transport.render_url(&ACCOUNT, &[]).expect("route renders");
        assert_eq!(url.as_str(), "http://127.0.0.1:9999/2.2/account");
    }

    #[test]
    fn render_url_missing_placeholder_is_a_validation_error() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let route = v2_3_route();
        let err = transport
            .render_url(&route, &[("network_id", "123")])
            .expect_err("device_id is missing");
        assert!(matches!(err, Error::Validation { field, .. } if field == "device_id"));
    }

    #[test]
    fn invalid_base_url_is_rejected_at_build_time() {
        let err = Transport::builder()
            .base_url("not a url")
            .build()
            .expect_err("malformed base URL must fail fast");
        assert!(matches!(err, Error::Validation { field, .. } if field == "base_url"));
    }

    #[test]
    fn render_url_rejects_a_value_that_becomes_a_dot_segment_after_stripping() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let route = v2_3_route();
        let err = transport
            .render_url(&route, &[("network_id", "100"), ("device_id", "..\n")])
            .expect_err("\"..\\n\" must not be allowed to collapse the rendered path");
        assert!(matches!(err, Error::Validation { field, .. } if field == "device_id"));
    }

    #[test]
    fn render_url_rejects_an_empty_placeholder_value() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let route = v2_3_route();
        let err = transport
            .render_url(&route, &[("network_id", "100"), ("device_id", "")])
            .expect_err("an empty value must not collapse the route onto its collection");
        assert!(matches!(err, Error::Validation { field, .. } if field == "device_id"));
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
