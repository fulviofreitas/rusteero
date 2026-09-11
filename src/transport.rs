//! Request core: status mapping, response size cap, redirect refusal, refresh retry.
//!
//! Ported from `eero-api`'s `src/eero/api/base.py` (see
//! `.claude/tasks/briefs/base.md` for the full behaviour brief) and, for the refresh handshake
//! only, `src/eero/api/auth.py` (`.claude/tasks/briefs/auth.md`). [`Transport`] is the single
//! place that turns an HTTP status/body into either an `Envelope` or a typed `Error` — every
//! endpoint module built on top of it (phase 3) is expected to be a thin wrapper that supplies a
//! [`crate::routes::Route`] and path/query/body values and nothing else.
//!
//! # Locking
//!
//! The current [`crate::auth::Session`] lives behind a `std::sync::RwLock`, not a
//! `tokio::sync::RwLock`. Every critical section that touches it (`session_snapshot`,
//! `current_token`, `replace_session`, `set_session`) is a handful of clones/comparisons with no
//! `.await` inside — an async lock would only add executor overhead for no benefit here. No lock
//! guard is ever held across an `.await` point anywhere in this module; the async methods
//! (`send`, `send_with_query`, `send_raw`, `refresh_session`) always take a session snapshot (or
//! replace it) in its own statement, before or after any network call, never around one.
//!
//! # The refresh-retry divergence (task brief gotcha G1)
//!
//! `eero-api`'s 401-triggered refresh retry re-sends the retried request with the *original,
//! pre-refresh* token (`api/base.py:296-298`), which immediately clobbers the fresh cookie the
//! refresh itself just set on the same shared cookie jar — so in the real Python library the
//! retry can never actually benefit from a successful refresh. `rusteero` deliberately does not
//! reproduce this: [`Transport::send_with_query`] re-reads the session from the shared state
//! *after* calling [`Transport::refresh_session`] and retries with the refreshed token. See the
//! comment at the retry call site for the exact citation.

use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, SystemTime};

use reqwest::header::{COOKIE, HeaderMap, LOCATION, RETRY_AFTER, SET_COOKIE};
use reqwest::redirect::Policy;
use reqwest::{Client, Method, StatusCode};
use secrecy::SecretString;
use serde_json::{Value, json};
use url::Url;

use crate::auth::Session;
use crate::consts;
use crate::envelope::Envelope;
use crate::error::{self, Error};
use crate::routes::{self, ApiVersion, Route};
use crate::storage::CredentialStore;

/// The request core shared by every authenticated and unauthenticated Eero cloud API call.
///
/// Build one with [`Transport::builder`]. See the module docs for the locking discipline and
/// the deliberate refresh-retry divergence from `eero-api`.
#[derive(Debug)]
pub struct Transport {
    http: Client,
    base_22: Url,
    base_23: Url,
    session: RwLock<Option<Session>>,
    store: Option<Arc<dyn CredentialStore>>,
}

impl Transport {
    /// Starts building a [`Transport`] with [`TransportBuilder`]'s defaults: the real Eero
    /// cloud hosts, reqwest's own `User-Agent`, no session, no credential store, and the
    /// timeouts from `consts`.
    #[must_use]
    pub fn builder() -> TransportBuilder {
        TransportBuilder::default()
    }

    /// Returns a snapshot of the currently configured session, if any.
    ///
    /// This is a clone of the in-memory state at the moment of the call; it does not reach out
    /// to the credential store and does not re-validate expiry beyond what `Session::is_valid`
    /// already encodes.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.session_snapshot()
    }

    /// Replaces the in-memory session and, if a credential store is configured, persists the
    /// change: `Some(session)` calls `CredentialStore::save`, `None` calls
    /// `CredentialStore::clear`. There is no direct Python equivalent — this is the single
    /// primitive `auth::AuthApi` (and the not-yet-built `Client` layer) composes into
    /// `login`/`verify`/`logout`/`set_session_token`-shaped operations.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if a configured credential store failed to persist the change.
    /// The in-memory session is updated regardless of whether persistence succeeds.
    pub fn set_session(&self, session: Option<Session>) -> Result<(), Error> {
        // Security finding F3: the in-memory session must update *before* this method returns,
        // and unconditionally of the store's outcome — `AuthApi::logout`/`clear_auth_data`/
        // `clear_session_token`'s own docs, and this method's own doc comment above, promise
        // exactly that. Persisting first and `?`-returning on failure would leave a live token
        // in memory whenever the configured store errors (routine for a keyring backend on a
        // headless host), even though the caller was told the in-memory session no longer holds
        // it. `store.save`/`store.clear` only need a borrow, so `session` is computed against
        // first and *then* moved into `replace_session` on every path — including the failure
        // path, via `result?` running only after the move.
        let Some(store) = &self.store else {
            self.replace_session(session);
            return Ok(());
        };
        let result = match &session {
            Some(s) => store.save(s),
            None => store.clear(),
        };
        self.replace_session(session);
        result?;
        Ok(())
    }

    /// Whether a session is configured, has a non-empty token, and has not passed its
    /// client-fabricated expiry — purely a local check, matching `Session::is_valid`. Never
    /// makes a network call and never attempts a refresh (contrast with `eero-api`'s
    /// `ensure_authenticated()`, which is a client-layer, not transport-layer, concern here).
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.session_snapshot()
            .is_some_and(|session| session.is_valid())
    }

    /// Sends an authenticated request with no query-string parameters. Equivalent to
    /// `send_with_query(route, path_params, &[], body)`.
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

    /// Sends an authenticated request, attaching `Cookie: s=<token>` from the current session.
    ///
    /// Performs the one-shot server-driven refresh retry described in the module docs: on a
    /// `401` whose body carries `meta.error == "error.session.refresh"`
    /// (`api/base.py:220-256`), calls [`Transport::refresh_session`] and, if it reports success,
    /// retries the identical request exactly once with the refreshed token before giving up.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made (`api/base.py`'s per-method `get_auth_token()` preamble,
    /// reproduced here since every endpoint method funnels through this one call site). Returns
    /// whatever status-mapped error the request produces otherwise (see the crate's `Error`
    /// docs), or propagates a refresh failure unmodified if the refresh hook itself errors.
    pub async fn send_with_query(
        &self,
        route: &Route,
        path_params: &[(&str, &str)],
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Envelope, Error> {
        let token = self.current_token().ok_or_else(not_authenticated)?;
        let url = self.render_url(route, path_params)?;

        let exchange = self
            .execute_raw(
                route.method.clone(),
                url.clone(),
                query,
                body.as_ref(),
                Some(&token),
            )
            .await?;

        if exchange.status.as_u16() == 401 && refresh_signal_detected(&exchange.body) {
            let refreshed = self.refresh_session().await?;
            if refreshed {
                // Divergence from eero-api (api/base.py:296-298; task brief gotcha G1): Python
                // re-passes the *original, pre-refresh* token into the retried request, which
                // then clobbers the fresh cookie the refresh itself just set on the shared
                // cookie jar — the retry never actually benefits from the refresh. rusteero
                // re-reads the session from the shared state *after* the refresh completes and
                // retries with the refreshed token instead of reproducing that bug.
                let retry_token = self.current_token().ok_or_else(not_authenticated)?;
                let retry = self
                    .execute_raw(
                        route.method.clone(),
                        url.clone(),
                        query,
                        body.as_ref(),
                        Some(&retry_token),
                    )
                    .await?;
                return status_to_envelope(
                    retry.status,
                    &retry.body,
                    &retry.url,
                    retry.retry_after,
                );
            }
            // `refreshed == false`: fall through and raise the original 401 below, exactly like
            // Python's "Session refresh returned False; raising auth exception" path.
        }

        status_to_envelope(
            exchange.status,
            &exchange.body,
            &exchange.url,
            exchange.retry_after,
        )
    }

    /// Sends an unauthenticated (or explicit-token) request: no "not authenticated" precondition,
    /// no refresh retry. Used by the login handshake (`LoginFlow`/`PendingLogin`, a later phase)
    /// and internally by [`Transport::refresh_session`].
    ///
    /// The returned value's `set_cookie_session` surfaces any `Set-Cookie: s=...` header the
    /// server sent on this response — `rusteero` has no implicit cookie jar to catch it the way
    /// `eero-api`'s aiohttp session does (task brief gotcha #11), so this is the only place such
    /// a cookie can ever become visible to a caller.
    pub(crate) async fn send_raw(
        &self,
        route: &Route,
        path_params: &[(&str, &str)],
        query: &[(&str, String)],
        body: Option<Value>,
        token: Option<&SecretString>,
    ) -> Result<RawResponse, Error> {
        let url = self.render_url(route, path_params)?;
        let exchange = self
            .execute_raw(route.method.clone(), url, query, body.as_ref(), token)
            .await?;
        let envelope = status_to_envelope(
            exchange.status,
            &exchange.body,
            &exchange.url,
            exchange.retry_after,
        )?;
        Ok(RawResponse {
            envelope,
            set_cookie_session: exchange.set_cookie_session,
        })
    }

    /// Attempts to refresh the current session, ported from `AuthAPI.refresh_session`
    /// (`api/auth.py:279-340`; see `.claude/tasks/briefs/auth.md` for the full behaviour brief
    /// this reproduces, including the exception-hierarchy subtlety noted at the terminal-error
    /// match arm below).
    ///
    /// Tries `routes::LOGIN_REFRESH` then `routes::ACCOUNT_REFRESH`, in that order, with body
    /// `{"refresh_token": <current refresh token>}` and no attached cookie (mirroring
    /// `api/auth.py:301-304`, which omits `auth_token` for this call). On success, reads
    /// `data.session_token` (a *different* wire key from the login/verify handshake's
    /// `user_token`) and `data.refresh_token`, updates the in-memory session, and persists
    /// through the configured credential store if any — a store failure here is logged at WARN
    /// and never masks a successful refresh (decision D-13).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("No refresh token available")` if the current session has
    /// no refresh token (the literal, unwrapped Python message, `api/auth.py:295-296`) — in
    /// practice this is the common case, since a normal login/verify never yields one (task
    /// brief finding (d)). Propagates a network/timeout error, or a `401`/`429`/other
    /// non-`404` status from *either* refresh route unmodified once past that precondition. A
    /// terminal `404`-from-both-routes or a terminal non-`404` `Error::Api` from either route is
    /// **not** propagated as an error: both instead clear the local session (persisting the
    /// clear, best-effort) and return `Ok(false)`, exactly matching `api/auth.py:305-338`.
    ///
    /// **Divergence from eero-api (documentation finding F7, `api/auth.py:308-311`)**: a
    /// network/timeout failure from either refresh route surfaces here as this crate's ordinary
    /// [`Error::Network`]/[`Error::Timeout`], whose `Display` is the same fixed, call-site-
    /// agnostic text used by every other network failure in the crate (`"Network error: {0}"`,
    /// `"Request timed out"`). Python instead raises a call-site-specific
    /// `EeroNetworkException(f"Network error during session refresh: {err}")` here. This port
    /// deliberately does not thread refresh-specific wording through `Error::Network`/
    /// `Error::Timeout` — doing so would mean either a new field those variants carry for no
    /// other caller, or matching on which call site produced the error, both worse than the
    /// small, intentional loss of this one message's specificity.
    pub async fn refresh_session(&self) -> Result<bool, Error> {
        let current = self.session_snapshot();
        let refresh_token = current
            .as_ref()
            .and_then(Session::expose_refresh_token)
            .filter(|token| !token.is_empty())
            .map(str::to_owned);

        let Some(refresh_token) = refresh_token else {
            return Err(Error::Authentication(
                "No refresh token available".to_owned(),
            ));
        };

        let body = json!({ "refresh_token": refresh_token });

        for route in [&routes::LOGIN_REFRESH, &routes::ACCOUNT_REFRESH] {
            match self
                .send_raw(route, &[], &[], Some(body.clone()), None)
                .await
            {
                Ok(raw) => {
                    let data = raw.envelope.data();
                    let Some(new_token) = data
                        .get(consts::SESSION_TOKEN_KEY)
                        .and_then(Value::as_str)
                        .filter(|token| !token.is_empty())
                    else {
                        // `SESSION_TOKEN_KEY` missing or empty: Python returns `False` without
                        // clearing or persisting anything (`api/auth.py:332`).
                        return Ok(false);
                    };
                    let new_refresh_token =
                        data.get(consts::REFRESH_TOKEN_KEY).and_then(Value::as_str);
                    let refreshed = build_refreshed_session(new_token, new_refresh_token)?;
                    self.replace_session(Some(refreshed.clone()));
                    self.persist_warn_only(&refreshed).await;
                    return Ok(true);
                }
                // A 404 from this route: try the next one in the tuple order.
                Err(Error::Api { status: 404, .. }) => {}
                // Any other `Error::Api` (400, 403, 5xx, ...) is the direct analogue of a
                // generic `EeroAPIException` in Python — terminal, clears local state, and
                // returns `Ok(false)` rather than propagating (`api/auth.py:312-316`). A `401`
                // is *not* `Error::Api` in this crate's type (it is `Error::Authentication`,
                // mirroring `EeroAuthenticationException`'s separate class in Python), so it
                // falls to the `Err(other)` arm below and propagates unmodified — reproducing
                // the same sibling-exception-hierarchy gap the task brief documents for
                // `login`/`verify`/`resend_verification_code` (brief gotcha #1), which applies
                // here too since `refresh_session` never catches `Error::Authentication` either.
                Err(Error::Api { status, .. }) => {
                    // `message` is intentionally never logged here (finding 1): it is either the
                    // raw truncated response body or "Invalid JSON response: " + the truncated
                    // body, and a successful refresh response legitimately carries
                    // `data.session_token`/`data.refresh_token`. A malformed-JSON 200 from this
                    // very refresh call would otherwise put live credentials into an ERROR log
                    // line. `status` is a bare status code and carries nothing sensitive.
                    tracing::error!(status, "session refresh failed");
                    self.replace_session(Some(Session::empty()));
                    self.persist_warn_only(&Session::empty()).await;
                    return Ok(false);
                }
                Err(other) => return Err(other),
            }
        }

        tracing::error!("session refresh failed: no refresh endpoint was accepted by the server");
        self.replace_session(Some(Session::empty()));
        self.persist_warn_only(&Session::empty()).await;
        Ok(false)
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
    /// `Session::is_valid`. This is the exact gate `send`/`send_with_query` use for the
    /// "Not authenticated" precondition.
    fn current_token(&self) -> Option<SecretString> {
        self.session_snapshot()
            .filter(Session::is_valid)
            .map(|session| session.token().clone())
    }

    /// Overwrites the in-memory session without touching the credential store. Used internally
    /// by `refresh_session` (which has its own, warn-only persistence policy) and by
    /// `set_session` (which persists synchronously right after calling this).
    fn replace_session(&self, session: Option<Session>) {
        let mut guard = self.session.write().unwrap_or_else(PoisonError::into_inner);
        *guard = session;
    }

    /// Persists `session` to the configured credential store, if any, via
    /// `tokio::task::spawn_blocking` (the store's trait is intentionally synchronous — see
    /// `crate::storage`'s docs). A failure here is logged at WARN and never returned to the
    /// caller: per decision D-13, a store failure must never mask a successful refresh.
    async fn persist_warn_only(&self, session: &Session) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let session = session.clone();
        match tokio::task::spawn_blocking(move || store.save(&session)).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "failed to persist session to credential store");
            }
            Err(join_err) => {
                tracing::warn!(error = %join_err, "credential store task panicked");
            }
        }
    }

    /// Returns the configured base `Url` for `version`: the real Eero host by default, or the
    /// host derived from `TransportBuilder::base_url` when one was supplied.
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
    /// a test double, which is exactly what `TransportBuilder::base_url` needs in order to point
    /// a single wiremock server at both API versions. Percent-encoding and placeholder semantics
    /// are identical to `Route::render` (see that function's docs for the `.`/`..`-segment
    /// caveat); the only difference is which base `Url` the segments are pushed onto.
    fn render_url(&self, route: &Route, params: &[(&str, &str)]) -> Result<Url, Error> {
        let mut url = self.base_for(route.version).clone();
        {
            let mut segments = url.path_segments_mut().map_err(|()| Error::Validation {
                field: "base_url".to_owned(),
                message: "configured base URL cannot be used as a path base".to_owned(),
            })?;
            for part in route.path.split('/') {
                if part.is_empty() {
                    // Leading/trailing/doubled slashes in a template contribute no segment.
                    continue;
                }
                if let Some(name) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    let value = params
                        .iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| *value)
                        .ok_or_else(|| Error::Validation {
                            field: name.to_owned(),
                            message: "missing value for path parameter".to_owned(),
                        })?;
                    segments.push(value);
                } else {
                    segments.push(part);
                }
            }
        }
        Ok(url)
    }

    /// Sends one HTTP request and returns its status, body, and any incidental headers this
    /// crate cares about (`Set-Cookie: s=...`, `Retry-After`) — everything *before*
    /// status-code-specific interpretation, which is `status_to_envelope`'s job.
    ///
    /// Handles, in order: attaching the `Cookie: s=<token>` header when `token` is `Some`
    /// (`api/base.py:148-152`); sending the request; logging method + rendered path + status
    /// only, never headers, cookies, or bodies (see the crate-level security rules); refusing
    /// any `3xx` redirect immediately, before the body is streamed at all
    /// (`api/base.py:158-186`); and streaming the body with the 10 MiB cap.
    async fn execute_raw(
        &self,
        method: Method,
        url: Url,
        query: &[(&str, String)],
        body: Option<&Value>,
        token: Option<&SecretString>,
    ) -> Result<RawExchange, Error> {
        // Query parameters are appended directly onto the `Url` via `url`'s own percent-encoding
        // (`Url::query_pairs_mut`) rather than `reqwest::RequestBuilder::query`, which requires
        // reqwest's `query` cargo feature — not enabled for this crate (no new dependencies /
        // feature flags without an explicit decision). `url::form_urlencoded` (which backs
        // `query_pairs_mut`) is already pulled in transitively by the `url` crate, an existing
        // direct dependency.
        let mut url = url;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }

        let mut request = self.http.request(method.clone(), url.clone());
        if let Some(body) = body {
            request = request.json(body);
        }
        if let Some(token) = token {
            // The crate's controlled exposure point for a token that isn't (necessarily) part of
            // a `Session` — this call site is shared with `auth::flow::PendingLogin`'s
            // pre-`Session` login token, see `Session::expose_secret_token`'s docs; every other
            // consumer of a token must go through `secrecy::ExposeSecret` explicitly.
            request = request.header(
                COOKIE,
                format!(
                    "{}={}",
                    consts::SESSION_COOKIE_NAME,
                    Session::expose_secret_token(token)
                ),
            );
        }

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

        // Security finding F4: a caller-supplied `reqwest::Client` (`TransportBuilder::http`)
        // can keep reqwest's default `Policy::limited(10)` instead of this crate's own
        // `Policy::none()`, in which case a `3xx` response is followed *inside* `request.send()`
        // above and never reaches the `status.is_redirection()` check below at all — `status`
        // and `response` at this point already reflect the final hop. `reqwest`'s
        // `remove_sensitive_headers` compares host+port only, not scheme, so e.g. an
        // `https://api-user.e2ro.com` -> `http://api-user.e2ro.com:443` redirect keeps
        // `Cookie: s=<token>` attached and replays a live session token in cleartext over an
        // unencrypted hop. This defends uniformly, regardless of which client was injected: if
        // the URL the response actually came from differs from the URL we asked for, a redirect
        // was followed, so refuse it here with the same error shape the `is_redirection` arm
        // below produces for a `Policy::none()` client's un-followed `3xx` — before the body
        // (which could be the redirect target's, not the requested resource's) is ever read.
        if response.url() != &url {
            return Err(Error::Api {
                status: status.as_u16(),
                message: format!(
                    "Redirect not followed: {} -> {}",
                    status.as_u16(),
                    response.url()
                ),
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
                url: Some(url.to_string()),
            });
        }

        let set_cookie_session = extract_set_cookie_session(&response);
        let retry_after = parse_retry_after(response.headers());
        let body_text = read_capped_body(response).await?;

        Ok(RawExchange {
            status,
            body: body_text,
            set_cookie_session,
            retry_after,
            url,
        })
    }
}

/// Response of an unauthenticated call, including any fresh session cookie.
pub(crate) struct RawResponse {
    pub envelope: Envelope,
    /// Read by `auth::flow::PendingLogin::verify` to prefer a fresh `Set-Cookie: s=...` session
    /// token over the original login token, when the server sends one (see that method's docs).
    pub set_cookie_session: Option<SecretString>,
}

/// The ingredients of one HTTP exchange, before status-code interpretation.
struct RawExchange {
    status: StatusCode,
    body: String,
    set_cookie_session: Option<SecretString>,
    retry_after: Option<Duration>,
    /// The exact URL requested, including any query string appended in `Transport::execute_raw`
    /// — used for error messages instead of the pre-query `Url` the caller originally rendered.
    url: Url,
}

/// The literal `Error::Authentication("Not authenticated")` guard every authenticated call
/// raises when no valid session is configured — factored out since it is constructed at two
/// call sites (the initial precondition and the post-refresh retry).
fn not_authenticated() -> Error {
    Error::Authentication("Not authenticated".to_owned())
}

/// Turns a response status and (already fully read) body into an `Envelope` or the matching
/// `Error`, reproducing `api/base.py:207-269`'s status-code chain exactly (see
/// `.claude/tasks/briefs/base.md` §9-10 for the line-by-line citation this implements):
///
/// - `204`, or any `2xx` with an empty/whitespace-only body, becomes `Envelope::empty()` — the
///   `204` check short-circuits *before* the whitespace check, so a (spec-violating) `204` with
///   a non-empty body still yields `{}` without ever attempting to parse it.
/// - Any other `2xx` is parsed as JSON; invalid JSON becomes `Error::Api` (not `Error::Json`,
///   which is reserved for `Envelope::data_as`) with message `"Invalid JSON response: ..."`.
/// - `401` becomes `Error::Authentication("Authentication failed: ...")`.
/// - `404` becomes `Error::Api` with `"Resource not found: .... URL: ..."`.
/// - `429` becomes `Error::RateLimit { retry_after }` (`retry_after` is an addition on top of
///   the Python contract, which discards the response body and never reads this header at all).
/// - Every other non-`2xx` becomes `Error::Api` with the truncated body as `message`.
///
/// Every body embedded in an error message is passed through
/// [`error::sanitize_body_for_error`] first: a body that parses as JSON is redacted
/// (`crate::redact::redact_sensitive`) before truncation, matching `_truncate_for_error`'s
/// truncation behaviour exactly for the common case (a typical error body carries none of the
/// redacted key substrings, so it is unchanged in substance) while closing the credential leak a
/// verbatim `_truncate_for_error` port would otherwise reproduce on `login`/`login/verify`/
/// `login/refresh`/`account/refresh` error bodies (security finding F5) — see
/// `sanitize_body_for_error`'s own docs for the unparseable-body fallback and the deliberate byte-
/// layout divergence from Python this introduces.
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
                message: format!(
                    "Invalid JSON response: {}",
                    error::sanitize_body_for_error(body)
                ),
                url: Some(url.to_string()),
            });
    }

    match code {
        401 => Err(Error::Authentication(format!(
            "Authentication failed: {}",
            error::sanitize_body_for_error(body)
        ))),
        404 => Err(Error::Api {
            status: 404,
            message: format!(
                "Resource not found: {}. URL: {url}",
                error::sanitize_body_for_error(body)
            ),
            url: Some(url.to_string()),
        }),
        429 => Err(Error::RateLimit { retry_after }),
        _ => Err(Error::Api {
            status: code,
            message: error::sanitize_body_for_error(body),
            url: Some(url.to_string()),
        }),
    }
}

/// Streams `response`'s body with a running size check, matching `eero-api`'s streamed,
/// cap-checked read (`api/base.py:188-205`): as soon as the accumulated length would *exceed*
/// `consts::MAX_RESPONSE_BYTES`, this aborts immediately without appending the chunk that pushed
/// it over — a body of exactly the cap succeeds, one byte more does not. Unlike Python's fixed
/// `65536`-byte chunker, this reads whatever chunk size `reqwest`'s `Response::chunk` yields
/// from the underlying transport; the cap check runs after every chunk regardless of its size,
/// so the strictly-greater-than semantics are identical either way.
///
/// Returns `Error::Api` (carrying the response's real status, not a synthetic one, matching
/// `api/base.py:198-201`) on an oversized body, and on a body that is not valid UTF-8 — the
/// latter has no Python equivalent (a raw `UnicodeDecodeError` would propagate uncaught there,
/// per the task brief); this crate cannot leave a body undecoded, so it surfaces the same error
/// shape instead of panicking or losing the failure.
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
                url: Some(response.url().to_string()),
            });
        }
        buffer.extend_from_slice(&chunk);
    }

    String::from_utf8(buffer).map_err(|err| Error::Api {
        status: status.as_u16(),
        message: format!("Response body is not valid UTF-8: {err}"),
        url: Some(response.url().to_string()),
    })
}

/// Extracts the value of a `Set-Cookie: s=...` header from `response`, if the server sent one.
///
/// No application code in `eero-api` ever reads this header explicitly (task brief gotcha #11);
/// `rusteero` has no implicit cookie jar to catch it silently, so this is the one place a fresh
/// session cookie can surface to a caller at all — used only by `Transport::send_raw`'s
/// `RawResponse`, read by `auth::flow::PendingLogin::verify`'s login/verify handshake.
fn extract_set_cookie_session(response: &reqwest::Response) -> Option<SecretString> {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|raw| {
            let (name, rest) = raw.split_once('=')?;
            if name.trim() != consts::SESSION_COOKIE_NAME {
                return None;
            }
            let value = rest.split(';').next().unwrap_or("").trim();
            (!value.is_empty()).then(|| SecretString::from(value.to_owned()))
        })
}

/// Parses a `Retry-After` header value into a `Duration`, accepting both the delta-seconds form
/// (a bare integer number of seconds) and the HTTP-date form (RFC 1123, e.g. `"Wed, 21 Oct 2015
/// 07:28:00 GMT"`). Returns `None` when the header is absent or its value matches neither form —
/// this must never itself become an error: `eero-api` never reads this header at all, so there
/// is no upstream behaviour to preserve, only a new addition to keep conservative
/// (`Error::RateLimit::retry_after`'s docs).
///
/// A `Retry-After` date already in the past parses to `Some(Duration::ZERO)` rather than `None`,
/// keeping the "absent or unparseable -> `None`" contract literal: a valid, if stale, date is
/// neither of those two cases.
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
/// (`meta.error == "error.session.refresh"`, ported from `api/base.py:284-293`). Any parse
/// failure — invalid JSON, a `meta` that is not a JSON object, or a missing `error` key —
/// degrades silently to `false` rather than propagating an error; this sniff must never itself
/// raise, matching Python's own bare `try: ... except Exception: body = None`.
fn refresh_signal_detected(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    value
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get("error"))
        .and_then(Value::as_str)
        == Some(consts::REFRESH_ERROR_CODE)
}

/// Maps a transport-level `reqwest::Error` onto this crate's `Error`, matching `eero-api`'s
/// split between `asyncio.TimeoutError` -> `Error::Timeout` and `aiohttp.ClientError` ->
/// `Error::Network` (`api/base.py:270-275`).
fn map_reqwest_error(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        Error::Timeout
    } else {
        Error::Network(err)
    }
}

/// Builds a `Session` carrying `new_token` and `new_refresh_token`, with a fresh `+30`-day
/// expiry fabricated the same way `Session::from_token` does.
///
/// `auth::session`'s on-disk representation type keeps its fields private to that module by
/// design (decision D-5's wire-format guarantee), so a session with *both* a fresh token and a
/// fresh refresh token cannot be assembled by touching private state from here. Instead this
/// round-trips through the same crate-internal JSON wire contract `Session::to_json`
/// (`pub(crate)`, finding F7) / `Session::from_json` use for storage: fabricate a token-only
/// session (which computes the correct expiry), splice the refresh token into its serialized
/// form, then re-parse. This never reaches into a private field, and stays inside `Session`'s
/// contract because `transport` and `session` are both part of this same crate.
fn build_refreshed_session(
    new_token: &str,
    new_refresh_token: Option<&str>,
) -> Result<Session, Error> {
    let base = Session::from_token(new_token);
    let mut value: Value = serde_json::from_str(&base.to_json()?)?;
    value["refresh_token"] = match new_refresh_token {
        Some(rt) => Value::String(rt.to_owned()),
        None => Value::Null,
    };
    Ok(Session::from_json(&value.to_string())?)
}

/// Parses a base URL string into a `Url` validated to be usable as a path base (i.e.
/// `Url::path_segments_mut` will succeed on it), for use by `TransportBuilder::build`.
fn parse_base(raw: &str) -> Result<Url, Error> {
    let mut url = Url::parse(raw).map_err(|err| Error::Validation {
        field: "base_url".to_owned(),
        message: format!("not a valid URL: {err}"),
    })?;
    if url.path_segments_mut().is_err() {
        return Err(Error::Validation {
            field: "base_url".to_owned(),
            message: "must be an absolute URL that can be used as a base".to_owned(),
        });
    }
    Ok(url)
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
    session: Option<Session>,
    store: Option<Arc<dyn CredentialStore>>,
    timeout: Duration,
    read_timeout: Duration,
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self {
            http: None,
            base_root: None,
            user_agent: None,
            session: None,
            store: None,
            timeout: consts::REQUEST_TIMEOUT,
            read_timeout: consts::READ_TIMEOUT,
        }
    }
}

impl TransportBuilder {
    /// Supplies a fully-configured `reqwest::Client` instead of letting `build` construct one.
    ///
    /// When set, `user_agent`, `timeout`, and `read_timeout` are ignored entirely — the supplied
    /// client is used exactly as given, which is the intended escape hatch for tests that need
    /// full control over the HTTP layer.
    ///
    /// # Warning
    ///
    /// A caller-supplied client silently discards two of this crate's safety guarantees: redirect
    /// refusal (`reqwest::redirect::Policy::none()`) and the request/read timeouts in
    /// [`crate::consts`]. With such a client, a same-host-same-port `3xx` response is followed
    /// instead of surfacing `Error::Api` with a `"Redirect not followed: ..."` message, and there
    /// is no 30-second ceiling on a hung request. Build your own client with
    /// `.redirect(reqwest::redirect::Policy::none())` and explicit `.timeout(..)` /
    /// `.read_timeout(..)` calls before passing it here if you need both custom configuration
    /// *and* these guarantees.
    #[must_use]
    pub fn http(mut self, client: Client) -> Self {
        self.http = Some(client);
        self
    }

    /// Overrides the base host for *both* API versions, deriving each from `base_url` by
    /// stripping any trailing `/` and appending `/2.2` or `/2.3` respectively — e.g.
    /// `base_url("http://127.0.0.1:9999")` yields `http://127.0.0.1:9999/2.2` and
    /// `http://127.0.0.1:9999/2.3`. This lets a single wiremock server stand in for both real
    /// Eero cloud hosts in tests. When unset, `build` uses the real hosts
    /// (`consts::API_BASE_22`/`consts::API_BASE_23`) unchanged.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_root = Some(base_url.into());
        self
    }

    /// Sets the `User-Agent` header reqwest sends. `None` (the default) means "send reqwest's
    /// own default `User-Agent`" — this crate deliberately does not send `eero-api`'s unused
    /// mobile-style `User-Agent` constant (decision D-7); pass `Some(..)` to opt into a custom
    /// value instead.
    #[must_use]
    pub fn user_agent(mut self, user_agent: Option<String>) -> Self {
        self.user_agent = user_agent;
        self
    }

    /// Seeds the transport with an initial session. `None` (the default) starts with no
    /// session, matching every `send`/`send_with_query` call failing with
    /// `Error::Authentication("Not authenticated")` until one is set via
    /// `Transport::set_session` or a successful refresh.
    #[must_use]
    pub fn session(mut self, session: Option<Session>) -> Self {
        self.session = session;
        self
    }

    /// Sets the credential store used by `Transport::set_session` and the persistence side of
    /// `Transport::refresh_session`. `None` (the default) means no persistence at all — the
    /// session only ever lives in memory.
    #[must_use]
    pub fn store(mut self, store: Option<Arc<dyn CredentialStore>>) -> Self {
        self.store = store;
        self
    }

    /// Overrides the overall request timeout (`consts::REQUEST_TIMEOUT` by default). Ignored if
    /// `http` was also called.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Overrides the per-read timeout (`consts::READ_TIMEOUT` by default). Ignored if `http` was
    /// also called.
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
    /// absolute URL usable as a path base. Returns `Error::Network` if constructing the
    /// underlying `reqwest::Client` fails (only reachable when `http` was not called; failure
    /// here is exceedingly rare and generally indicates a broken TLS backend).
    pub fn build(self) -> Result<Transport, Error> {
        let (base_22, base_23) = self.build_bases()?;

        let http = if let Some(client) = self.http {
            client
        } else {
            let mut builder = Client::builder()
                .timeout(self.timeout)
                .read_timeout(self.read_timeout)
                .redirect(Policy::none());
            if let Some(user_agent) = self.user_agent {
                builder = builder.user_agent(user_agent);
            }
            builder.build().map_err(Error::Network)?
        };

        Ok(Transport {
            http,
            base_22,
            base_23,
            session: RwLock::new(self.session),
            store: self.store,
        })
    }

    /// Computes the effective 2.2/2.3 base `Url`s: the real Eero hosts by default, or both
    /// derived from `base_root` per `TransportBuilder::base_url`'s doc comment when set.
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

/// The version path segment (e.g. `"2.2"`) baked into `version`'s production base URL
/// ([`ApiVersion::base_url`]) — `TransportBuilder::build_bases`'s single source of truth for
/// deriving a caller-supplied override base, instead of re-stating `"2.2"`/`"2.3"` as a second
/// literal that could silently drift from `consts::API_BASE_22`/`consts::API_BASE_23` (finding
/// 3: routes.rs/consts.rs must be the only place wire paths exist). Slices the `'static` base
/// URL string itself rather than parsing it into a `Url`, so no allocation is needed and the
/// returned segment borrows straight from the `'static` constant.
///
/// # Panics
///
/// Never in practice: [`ApiVersion::base_url`] always returns one of `consts::API_BASE_22` /
/// `consts::API_BASE_23`, both of which are non-empty absolute URLs, so `rsplit('/').next()`
/// always yields at least one item (pinned by
/// `consts::tests::base_urls_are_versioned_variants_of_the_same_host`).
fn version_segment(version: ApiVersion) -> &'static str {
    version
        .base_url()
        .rsplit('/')
        .next()
        .expect("base_url is a non-empty static string; rsplit always yields at least one item")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use reqwest::Method;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use url::Url;

    use super::{Route, Transport, parse_retry_after, refresh_signal_detected};
    use crate::error::Error;
    use crate::routes::{ACCOUNT, ApiVersion};
    use crate::storage::{CredentialStore, StorageError};

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

    // ===================== refresh_session error-arm log field (finding F1) =====================

    /// A minimal `tracing::Subscriber` that records every event's fields (name and
    /// debug-rendered value) into a shared buffer, ignoring spans entirely.
    ///
    /// The same hand-rolled-`Subscriber` technique `tests/transport.rs`'s
    /// `PathCapturingSubscriber` uses for `query_string_is_never_logged_in_the_path` — that file
    /// is a separate compilation unit from this crate's own unit tests (an integration test
    /// cannot be `mod`-included from `src/`, and vice versa), so the technique is reproduced
    /// here rather than imported. No new dependency: this is a hand-written
    /// `tracing_core::Subscriber` impl, not a `tracing-subscriber` `Layer`.
    struct FieldCapturingSubscriber(Arc<Mutex<Vec<String>>>);

    struct FieldVisitor<'a>(&'a mut Vec<String>);

    impl tracing::field::Visit for FieldVisitor<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.push(format!("{}={value:?}", field.name()));
        }
    }

    impl tracing::Subscriber for FieldCapturingSubscriber {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut fields = self
                .0
                .lock()
                .expect("capture mutex is never held across a panic");
            let mut visitor = FieldVisitor(&mut fields);
            event.record(&mut visitor);
        }

        fn enter(&self, _span: &tracing::span::Id) {}

        fn exit(&self, _span: &tracing::span::Id) {}
    }

    /// Fails the Explore verification's "a test that cannot fail" finding: the previous version
    /// of this test never called `refresh_session` and never reached the `tracing::error!` call
    /// site at all — it asserted on a bare `u16` it had constructed itself. This version drives
    /// `refresh_session` for real, over a local mock server, down its `Err(Error::Api)` arm with
    /// a credential-carrying malformed 200 body, and inspects the *actual* captured tracing
    /// output via [`FieldCapturingSubscriber`].
    #[tokio::test]
    async fn refresh_failure_log_field_never_carries_the_response_body() {
        // Deliberately truncated / malformed: reproduces the exact body a cut-off response from
        // `login/refresh` or `account/refresh` would produce, legitimately carrying
        // `data.session_token`/`data.refresh_token` — a live credential — inside an `Error::Api`
        // that `status_to_envelope`'s invalid-JSON-on-2xx arm constructs.
        let credential_carrying_body = concat!(
            r#"{"meta":{"code":200},"data":{"session_token":"eyJsecret-session-value","#,
            r#""refresh_token":"eyJsecret-refresh-value"#,
        );
        assert!(
            serde_json::from_str::<serde_json::Value>(credential_carrying_body).is_err(),
            "sanity: the fixture must actually be malformed JSON, matching the real hazard"
        );

        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/2.2/login/refresh"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_string(credential_carrying_body),
            )
            .expect(1)
            .mount(&server)
            .await;

        let session = crate::auth::Session::from_json(
            r#"{"session_id":"tok","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#,
        )
        .expect("well-formed literal session json");
        let transport = Transport::builder()
            .base_url(server.uri())
            .session(Some(session))
            .build()
            .expect("a MockServer's own URI is always a valid base URL");

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let subscriber = FieldCapturingSubscriber(Arc::clone(&captured));

        let refreshed = {
            let _guard = tracing::subscriber::set_default(subscriber);
            transport
                .refresh_session()
                .await
                .expect("a terminal Error::Api from a refresh call becomes Ok(false), not Err")
        };
        assert!(
            !refreshed,
            "a malformed refresh response must not be reported as a successful refresh"
        );

        let logged = captured
            .lock()
            .expect("capture mutex is never held across a panic")
            .join(" | ");
        assert!(
            !logged.contains("eyJsecret"),
            "logged fields leaked the credential-carrying body: {logged}"
        );
        assert!(!logged.contains("session-value"), "logged: {logged}");
        assert!(!logged.contains("refresh-value"), "logged: {logged}");
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
    fn refresh_signal_not_detected_when_meta_is_not_an_object() {
        for bad_meta in ["\"oops\"", "42", "[1,2]", "null"] {
            let body = format!(r#"{{"meta":{bad_meta}}}"#);
            assert!(!refresh_signal_detected(&body), "meta = {bad_meta}");
        }
    }

    #[test]
    fn refresh_signal_not_detected_on_invalid_json() {
        assert!(!refresh_signal_detected("not json at all"));
    }

    #[test]
    fn refresh_signal_not_detected_when_error_key_is_missing() {
        assert!(!refresh_signal_detected(r#"{"meta":{"code":401}}"#));
    }

    // ===================== render_url / base selection =====================

    fn v2_3_route() -> Route {
        Route {
            method: Method::PUT,
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
    fn overridden_base_and_production_base_share_the_same_version_segments() {
        // Finding 3: `build_bases` used to re-derive "2.2"/"2.3" by string formatting instead of
        // from `ApiVersion`, so the override base and the production base could silently
        // desync from `consts::API_BASE_22`/`consts::API_BASE_23` on a future version bump. This
        // pins both bases to derive the *same* version segment at runtime, so a future bump that
        // updates only one of the two call sites fails this test instead of passing silently.
        let production = Transport::builder().build().expect("builds with defaults");
        let overridden = Transport::builder()
            .base_url("http://127.0.0.1:9999")
            .build()
            .expect("builds with an overridden base");

        let version_segment = |url: &Url| -> String {
            url.path_segments()
                .and_then(|mut segments| segments.next().map(str::to_owned))
                .expect("rendered URL always has a version path segment")
        };

        let prod_22 = production
            .render_url(&ACCOUNT, &[])
            .expect("v2.2 route renders against the production base");
        let over_22 = overridden
            .render_url(&ACCOUNT, &[])
            .expect("v2.2 route renders against the overridden base");
        assert_eq!(
            version_segment(&prod_22),
            version_segment(&over_22),
            "a consts::API_BASE_22 version bump must not desync the override base from production"
        );

        let route_23 = v2_3_route();
        let path_params: &[(&str, &str)] = &[("network_id", "1"), ("device_id", "2")];
        let prod_23 = production
            .render_url(&route_23, path_params)
            .expect("v2.3 route renders against the production base");
        let over_23 = overridden
            .render_url(&route_23, path_params)
            .expect("v2.3 route renders against the overridden base");
        assert_eq!(
            version_segment(&prod_23),
            version_segment(&over_23),
            "a consts::API_BASE_23 version bump must not desync the override base from production"
        );

        assert_ne!(
            version_segment(&prod_22),
            version_segment(&prod_23),
            "2.2 and 2.3 must remain distinct API versions"
        );
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

    /// A [`CredentialStore`] whose `save`/`clear` always fail, for pinning security finding F3:
    /// `set_session` must clear the in-memory session *before* attempting to persist, and
    /// unconditionally of the outcome, so a failing store never leaves a live token behind.
    #[derive(Debug)]
    struct AlwaysFailingStore;

    impl CredentialStore for AlwaysFailingStore {
        fn load(&self) -> Result<crate::auth::Session, StorageError> {
            Ok(crate::auth::Session::empty())
        }

        fn save(&self, _session: &crate::auth::Session) -> Result<(), StorageError> {
            Err(StorageError::Backend {
                backend: "always-failing-test-store".to_owned(),
                message: "deliberate failure".to_owned(),
            })
        }

        fn clear(&self) -> Result<(), StorageError> {
            Err(StorageError::Backend {
                backend: "always-failing-test-store".to_owned(),
                message: "deliberate failure".to_owned(),
            })
        }
    }

    #[test]
    fn set_session_clears_memory_even_when_the_store_fails_to_persist() {
        let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailingStore);
        let transport = Transport::builder()
            .session(Some(crate::auth::Session::from_token("tok-123")))
            .store(Some(store))
            .build()
            .expect("builds with an initial session and a store");
        assert!(transport.is_authenticated());

        let err = transport
            .set_session(None)
            .expect_err("the configured store always fails to clear");
        assert!(matches!(err, Error::Storage(_)));

        // The point of finding F3: the in-memory session must be cleared regardless of the
        // store's outcome, exactly as `set_session`'s own doc comment promises.
        assert!(
            !transport.is_authenticated(),
            "in-memory session must be cleared even though the store failed"
        );
        assert!(transport.session().is_none());
    }

    #[test]
    fn set_session_installs_a_new_session_in_memory_even_when_the_store_fails_to_save() {
        let store: Arc<dyn CredentialStore> = Arc::new(AlwaysFailingStore);
        let transport = Transport::builder()
            .store(Some(store))
            .build()
            .expect("builds with a store but no initial session");
        assert!(!transport.is_authenticated());

        let err = transport
            .set_session(Some(crate::auth::Session::from_token("tok-456")))
            .expect_err("the configured store always fails to save");
        assert!(matches!(err, Error::Storage(_)));

        assert!(
            transport.is_authenticated(),
            "in-memory session must be installed even though the store failed to persist it"
        );
    }

    // ===================== injected client / redirect defence (finding F4) =====================

    #[tokio::test]
    async fn injected_client_that_follows_redirects_is_still_refused() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/2.2/account"))
            .respond_with(
                wiremock::ResponseTemplate::new(302)
                    .insert_header("Location", format!("{}/2.2/elsewhere", server.uri())),
            )
            .expect(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/2.2/elsewhere"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(r#"{"data":{}}"#))
            .expect(1)
            .mount(&server)
            .await;

        // A caller-supplied client that keeps reqwest's default redirect-following policy —
        // exactly the hazard `TransportBuilder::http`'s `# Warning` section documents.
        let following_client = reqwest::Client::builder()
            .build()
            .expect("a default reqwest client always builds");

        let transport = Transport::builder()
            .base_url(server.uri())
            .http(following_client)
            .session(Some(crate::auth::Session::from_token("tok")))
            .build()
            .expect("builds with an injected client");

        let err = transport
            .send(&ACCOUNT, &[], None)
            .await
            .expect_err("a followed redirect must surface as an error, not the hop's body");
        assert!(
            matches!(err, Error::Api { .. }),
            "expected Error::Api, got {err:?}"
        );
    }
}
