//! Interactive one-time-code login flow, separable from any `Client` or
//! [`crate::storage::CredentialStore`].
//!
//! Ported from the session-establishing half of `eero-api`'s `AuthAPI`
//! (`src/eero/api/auth.py:87-237`; see the auth behaviour notes for the full behaviour
//! brief this module implements). `AuthAPI` is a single stateful object: `login()` stashes an
//! unverified token straight into `self._credentials.session_id` (`auth.py:131`), and nothing
//! stops a caller from attempting an authenticated request with it before `verify()` ever runs
//! — the real server would just reject that with a `401`. `rusteero` splits the same handshake
//! into a type-state pair instead: [`LoginFlow::start`] can only ever hand back a
//! [`PendingLogin`], never a [`crate::auth::Session`] directly, and the only way to obtain a
//! `Session` is [`PendingLogin::verify`], which consumes the `PendingLogin` (port plan §3.5). An
//! unverified token can therefore never reach an authenticated endpoint by construction.
//!
//! Both types are usable standalone: [`LoginFlow::new`] and [`LoginFlow::with_transport`] need
//! no credential store and no client facade, only a [`Transport`].

use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

use crate::auth::Session;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The exact message Python logs at ERROR (without raising) when `login()` receives no user
/// token back (`api/auth.py:127`); promoted to an [`Error::Authentication`] message here since a
/// missing token leaves nothing meaningful to hand back as a [`PendingLogin`].
const LOGIN_FAILED_NO_USER_TOKEN: &str = "Login failed: No user token received";

/// Entry point for the interactive login handshake.
///
/// Build one with [`LoginFlow::new`] (real Eero cloud hosts) or [`LoginFlow::with_transport`]
/// (any pre-built [`Transport`], e.g. one pointed at a wiremock server in tests). Call
/// [`LoginFlow::start`] to begin.
#[derive(Debug)]
pub struct LoginFlow {
    transport: Arc<Transport>,
}

impl LoginFlow {
    /// Builds a `LoginFlow` against the real Eero cloud API.
    ///
    /// Pass `Some(client)` to supply your own pre-configured `reqwest::Client` (custom proxy,
    /// TLS, etc.); `None` lets [`crate::transport::TransportBuilder::build`] construct one with
    /// this crate's defaults instead (redirects refused, the timeouts in [`crate::consts`],
    /// reqwest's own `User-Agent` per decision D-7).
    ///
    /// # Warning
    ///
    /// A caller-supplied client silently discards two of this crate's safety guarantees: redirect
    /// refusal (`reqwest::redirect::Policy::none()`) and the request/read timeouts in
    /// [`crate::consts`] — see [`crate::transport::TransportBuilder::http`]'s own `# Warning` for
    /// the full hazard this creates (a same-host-same-port `3xx` is followed instead of surfacing
    /// an error, and there is no ceiling on a hung request). `Transport::send_raw`'s own
    /// response-URL check (security finding F4) still refuses a *followed* redirect before its
    /// body is ever read, regardless of which client is injected here, but a `Some(client)` this
    /// constructor receives is otherwise used exactly as given: build your own client with
    /// `.redirect(reqwest::redirect::Policy::none())` and explicit `.timeout(..)` /
    /// `.read_timeout(..)` calls before passing it here if you need both custom configuration
    /// *and* this crate's default guarantees.
    ///
    /// # Errors
    ///
    /// Propagates [`crate::transport::TransportBuilder::build`]'s error unmodified — in
    /// practice only reachable if constructing the underlying HTTP client itself fails.
    pub fn new(http: Option<reqwest::Client>) -> Result<Self, Error> {
        let mut builder = Transport::builder();
        if let Some(http) = http {
            builder = builder.http(http);
        }
        Ok(Self {
            transport: Arc::new(builder.build()?),
        })
    }

    /// Wraps an already-configured [`Transport`].
    ///
    /// This is the seam tests use to point a `LoginFlow` at a local server via
    /// [`crate::transport::TransportBuilder::base_url`] instead of the real Eero cloud hosts.
    #[must_use]
    pub fn with_transport(transport: Transport) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    /// Starts the login handshake for `user_identifier` — an email address or phone number; the
    /// server (and this client) never distinguishes between the two, matching Python's untyped
    /// pass-through (`auth.py:87`).
    ///
    /// Ported from `login()` (`api/auth.py:87-143`; brief lines 42-76): sends `POST`
    /// [`crate::routes::LOGIN`] with body `{"login": user_identifier}` (`auth.py:115`),
    /// **unauthenticated** — no session cookie is attached, matching Python's freshly-cleared
    /// cookie jar at this point (`auth.py:109`; there is no implicit jar here to clear in the
    /// first place). Reads `data.user_token` out of the response; nothing else in the response
    /// is consulted (`auth.py:120`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] with the message `"Login failed: No user token
    /// received"` if the (otherwise successful) response has no non-empty `data.user_token` —
    /// Python logs the identical message at ERROR and returns `false` rather than raising
    /// (`auth.py:126-128`); this port promotes that to a typed error since there is nothing
    /// meaningful to hand back as a [`PendingLogin`] otherwise. Propagates any transport-level
    /// error (network, timeout, non-2xx status) unmodified — a `401`/`429`/`5xx` here becomes
    /// the matching [`Error`] variant exactly as for any other call through
    /// `Transport::send_raw`.
    pub async fn start(&self, user_identifier: &str) -> Result<PendingLogin, Error> {
        let body = login_request_body(user_identifier);
        let raw = self
            .transport
            .send_raw(&routes::LOGIN, &[], &[], Some(body), None)
            .await?;
        let login_token = extract_user_token(&raw.envelope)?;
        Ok(PendingLogin {
            login_token,
            transport: Arc::clone(&self.transport),
        })
    }
}

/// An in-progress login: a login token has been obtained from [`LoginFlow::start`] but not yet
/// verified with the one-time code Eero emailed or texted.
///
/// Holds the login token as a [`SecretString`] (never a bare `String`) plus a handle to the same
/// [`Transport`] the originating [`LoginFlow`] used, so [`PendingLogin::resend`] and
/// [`PendingLogin::verify`] reuse its configuration (base URL, HTTP client, timeouts) exactly.
///
/// [`PendingLogin::verify`] takes `self` by value: once called, this value is consumed and
/// cannot be reused, which is the type-level guarantee described in the module docs — an
/// unverified login token can never reach an authenticated endpoint through this type. `Debug`
/// is implemented by hand to redact the token unconditionally (see the impl below).
pub struct PendingLogin {
    login_token: SecretString,
    transport: Arc<Transport>,
}

impl PendingLogin {
    /// Asks the server to resend the verification code.
    ///
    /// Ported from `resend_verification_code()` (`api/auth.py:203-237`; brief lines 147-169):
    /// sends `POST` [`crate::routes::LOGIN_RESEND`] with an empty JSON body (`{}`,
    /// `auth.py:226`), attaching `Cookie: s=<login token>` (`auth.py:220,225`). The response
    /// envelope is returned as-is rather than discarded, unlike Python's
    /// `resend_verification_code` (which discards the body and returns a bare `bool`,
    /// `auth.py:223-230`) — every endpoint method in this crate returns
    /// `Result<Envelope, Error>` over the untouched wire payload, including this one.
    ///
    /// # Errors
    ///
    /// Propagates whatever `Transport::send_raw` produces unmodified. This is a deliberate
    /// asymmetry with Python (brief lines 161-167): Python catches only the generic
    /// `EeroAPIException` case and returns `false` instead of raising, while a `401`/`429`
    /// still propagates uncaught as `EeroAuthenticationException`/`EeroRateLimitException` —
    /// this port does not attempt to reproduce that partial catch; every failure mode surfaces
    /// as its matching [`Error`] variant here.
    pub async fn resend(&self) -> Result<Envelope, Error> {
        let raw = self
            .transport
            .send_raw(
                &routes::LOGIN_RESEND,
                &[],
                &[],
                Some(json!({})),
                Some(&self.login_token),
            )
            .await?;
        Ok(raw.envelope)
    }

    /// Completes the login handshake with the one-time `code`, consuming this `PendingLogin`.
    ///
    /// Ported from `verify()` (`api/auth.py:145-201`; brief lines 78-145): sends `POST`
    /// [`crate::routes::LOGIN_VERIFY`] with body `{"code": code}` (`auth.py:173`), attaching
    /// `Cookie: s=<login token>` (`auth.py:167,172`). Python discards the response body
    /// entirely and treats the login token itself as the permanent session token, fabricating
    /// an expiry of `now + `[`crate::consts::SESSION_LIFETIME_DAYS`]` days` (`auth.py:176-182`);
    /// this port does the same via [`Session::from_token`], which fabricates the identical
    /// expiry.
    ///
    /// **PROVISIONAL**, pending live confirmation (the port plan §7.2,
    /// decision D-16): it is not yet confirmed whether the live server issues a fresh `s` cookie
    /// on this response. `eero-api` cannot observe this either way — it never reads
    /// `Set-Cookie` explicitly (brief gotcha #11) — but `rusteero` has no implicit cookie jar to
    /// silently pick one up, so `Transport::send_raw`'s `set_cookie_session` is the only place
    /// such a cookie could ever surface. This method uses a fresh cookie when the server sent
    /// one and otherwise falls back to the login token, which matches Python's only observable
    /// behaviour (the "no fresh cookie" case) exactly.
    ///
    /// # Errors
    ///
    /// Propagates whatever `Transport::send_raw` produces unmodified — in particular, a wrong
    /// code surfaces as [`Error::Authentication`] with message `"Authentication failed: ..."`
    /// from the transport's own `401` mapping, not Python's dead-code
    /// `"Verification code incorrect"` / `"Verification failed: ..."` text (brief lines
    /// 128-143: a real `401` raises the sibling `EeroAuthenticationException` in Python, which
    /// `verify()`'s `except EeroAPIException` clause can never actually catch).
    pub async fn verify(self, code: &str) -> Result<Session, Error> {
        let body = verify_request_body(code);
        let raw = self
            .transport
            .send_raw(
                &routes::LOGIN_VERIFY,
                &[],
                &[],
                Some(body),
                Some(&self.login_token),
            )
            .await?;

        // TODO(live-capture): confirm against a real account whether `login/verify` sets a
        // fresh `s` cookie (rust-port-plan.md §7.2, decision D-16). Until then, the
        // fresh-cookie branch below is exercised only by tests, never by an observed live
        // response — see `resolve_session_token`'s doc comment for both branches.
        let token = resolve_session_token(raw.set_cookie_session, self.login_token);
        Ok(Session::from_token(token.expose_secret()))
    }

    /// The login token this `PendingLogin` is holding, for other in-crate callers that need it
    /// without completing verification (e.g. a future `Client`-level convenience wrapper).
    // No such caller exists yet in this phase, which a plain `cargo clippy` flags as dead code;
    // kept `pub(crate)` (rather than deleted) since the next phase's `Client`/HTTP-test wave is
    // the intended consumer, per the task brief for this module.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn user_token(&self) -> &SecretString {
        &self.login_token
    }
}

impl std::fmt::Debug for PendingLogin {
    /// Redacts the login token unconditionally and omits the `transport` handle entirely,
    /// consistent with [`Session`]'s own hand-written `Debug` impl.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingLogin")
            .field("login_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Builds the request body for [`LoginFlow::start`]: `{"login": user_identifier}`
/// (`auth.py:115`). Factored out so the exact shape is unit-testable without a network round
/// trip.
fn login_request_body(user_identifier: &str) -> Value {
    json!({ "login": user_identifier })
}

/// Builds the request body for [`PendingLogin::verify`]: `{"code": code}` (`auth.py:173`).
/// Factored out so the exact shape is unit-testable without a network round trip.
fn verify_request_body(code: &str) -> Value {
    json!({ "code": code })
}

/// Extracts and validates `data.user_token` from a `login` response (`auth.py:120,126-128`):
/// present and non-empty becomes the login token, anything else becomes
/// [`Error::Authentication`] with [`LOGIN_FAILED_NO_USER_TOKEN`].
fn extract_user_token(envelope: &Envelope) -> Result<SecretString, Error> {
    envelope
        .data()
        .get("user_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(|token| SecretString::from(token.to_owned()))
        .ok_or_else(|| Error::Authentication(LOGIN_FAILED_NO_USER_TOKEN.to_owned()))
}

/// Chooses the session token to use after a successful [`PendingLogin::verify`] call: the fresh
/// `Set-Cookie` session token if the server sent one, otherwise the original login token. See
/// [`PendingLogin::verify`]'s doc comment for the full PROVISIONAL rationale
/// (the port plan §7.2, decision D-16) — this function is factored out so
/// each branch is directly unit-testable ahead of the wiremock tests that exercise it end to
/// end.
fn resolve_session_token(
    fresh_cookie: Option<SecretString>,
    login_token: SecretString,
) -> SecretString {
    fresh_cookie.unwrap_or(login_token)
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};
    use serde_json::json;

    use super::{
        LoginFlow, PendingLogin, extract_user_token, login_request_body, resolve_session_token,
        verify_request_body,
    };
    use crate::envelope::Envelope;
    use crate::error::Error;
    use crate::transport::Transport;

    // ===================== request body construction =====================

    #[test]
    fn login_request_body_matches_python_shape() {
        assert_eq!(
            login_request_body("user@example.com"),
            json!({ "login": "user@example.com" })
        );
    }

    #[test]
    fn verify_request_body_matches_python_shape() {
        assert_eq!(verify_request_body("123456"), json!({ "code": "123456" }));
    }

    // ===================== extract_user_token =====================

    #[test]
    fn extract_user_token_reads_data_user_token() {
        let envelope = Envelope::from_value(json!({ "data": { "user_token": "ut-123" } }));
        let token = extract_user_token(&envelope).expect("token present");
        assert_eq!(token.expose_secret(), "ut-123");
    }

    #[test]
    fn extract_user_token_missing_is_authentication_error_with_pythons_message() {
        let envelope = Envelope::from_value(json!({ "data": {} }));
        let err = extract_user_token(&envelope).unwrap_err();
        assert!(
            matches!(err, Error::Authentication(ref msg) if msg == "Login failed: No user token received")
        );
    }

    #[test]
    fn extract_user_token_empty_string_is_authentication_error() {
        let envelope = Envelope::from_value(json!({ "data": { "user_token": "" } }));
        assert!(extract_user_token(&envelope).is_err());
    }

    #[test]
    fn extract_user_token_no_data_key_at_all_is_authentication_error() {
        let envelope = Envelope::from_value(json!({}));
        assert!(extract_user_token(&envelope).is_err());
    }

    // ===================== resolve_session_token (the one open question, D-16) =====================

    #[test]
    fn resolve_session_token_prefers_a_fresh_cookie_when_present() {
        let fresh = SecretString::from("fresh-cookie".to_owned());
        let login = SecretString::from("login-token".to_owned());
        let resolved = resolve_session_token(Some(fresh), login);
        assert_eq!(resolved.expose_secret(), "fresh-cookie");
    }

    #[test]
    fn resolve_session_token_falls_back_to_the_login_token_when_absent() {
        let login = SecretString::from("login-token".to_owned());
        let resolved = resolve_session_token(None, login);
        assert_eq!(resolved.expose_secret(), "login-token");
    }

    // ===================== Debug redaction =====================

    #[test]
    fn pending_login_debug_never_prints_the_login_token() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let pending = PendingLogin {
            login_token: SecretString::from("super-secret-login-token".to_owned()),
            transport: std::sync::Arc::new(transport),
        };
        let debug = format!("{pending:?}");
        assert!(!debug.contains("super-secret-login-token"));
        assert!(debug.contains("REDACTED"));
    }

    // ===================== usable with no CredentialStore and no Client =====================

    #[test]
    fn login_flow_new_builds_without_any_store_or_client() {
        let flow = LoginFlow::new(None).expect("builds with defaults, no store required");
        assert!(flow.transport.session().is_none());
    }

    #[test]
    fn login_flow_with_transport_needs_no_store() {
        let transport = Transport::builder().build().expect("builds with defaults");
        let flow = LoginFlow::with_transport(transport);
        assert!(!flow.transport.is_authenticated());
    }
}
