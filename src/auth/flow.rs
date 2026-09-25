//! Interactive one-time-code login flow, separable from any `Client` or
//! [`crate::storage::CredentialStore`].
//!
//! Ported from the session-establishing half of `eero-api`'s `AuthAPI` (`src/eero/api/auth.py`
//! at `v8.0.4`). `AuthAPI` is a single stateful object: `login()` stashes an unverified token
//! straight into `self._credentials.session_id`, and nothing stops a caller from attempting an
//! authenticated request with it before `verify()` ever runs — the real server would just reject
//! that with a `401`. `rusteero` splits the same handshake into a type-state pair instead:
//! [`LoginFlow::start`] can only ever hand back a [`PendingLogin`], never a
//! [`crate::auth::Session`] directly, and the only way to obtain a `Session` is
//! [`PendingLogin::verify`], which consumes the `PendingLogin`. An unverified token can therefore
//! never reach an authenticated endpoint by construction.
//!
//! Both types are usable standalone: [`LoginFlow::new`] and [`LoginFlow::with_transport`] need no
//! credential store and no client facade, only a [`Transport`].

use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

use crate::auth::Session;
use crate::consts;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// The exact message this crate raises when `login()` receives no user token back
/// (`api/auth.py:193-196` logs this at DEBUG and returns `False`); promoted to an
/// [`Error::Authentication`] message here since a missing token leaves nothing meaningful to hand
/// back as a [`PendingLogin`] — a documented divergence from Python, whose type-state has no such
/// constraint.
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
    /// Pass `Some(client)` to supply your own pre-configured `reqwest::Client`; `None` lets
    /// [`crate::transport::TransportBuilder::build`] construct one with this crate's defaults
    /// instead (redirects refused, the timeouts in [`crate::consts`], [`consts::DEFAULT_USER_AGENT`]).
    ///
    /// # Warning
    ///
    /// A caller-supplied client silently discards two of this crate's safety guarantees: redirect
    /// refusal and the request/read timeouts — see
    /// [`crate::transport::TransportBuilder::http`]'s own `# Warning`.
    ///
    /// # Errors
    ///
    /// Propagates [`crate::transport::TransportBuilder::build`]'s error unmodified.
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
    /// pass-through.
    ///
    /// Ported from `login()` (`api/auth.py:155-218`): sends `POST` [`crate::routes::LOGIN`] with
    /// a **form** body `login=<user_identifier>` (`auth.py:186-189`), **unauthenticated**. Reads
    /// `data.user_token` out of the response; nothing else in the response is consulted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] with the message `"Login failed: No user token
    /// received"` if the (otherwise successful) response has no non-empty `data.user_token` — see
    /// this module's own doc comment for why this port promotes Python's swallowed `False` return
    /// to a typed error here. Every other `Error::Api`/`Error::Validation`/other
    /// API-classified error is re-wrapped as `Error::Authentication { message: "Login failed:
    /// {err}", envelope, error_code }` (`auth.py:203-218`), so every login failure surfaces
    /// uniformly; `Error::Authentication` (an actual `401`) and `Error::Network`/`Error::Timeout`
    /// propagate unmodified.
    pub async fn start(&self, user_identifier: &str) -> Result<PendingLogin, Error> {
        let url = self.transport.render_url(&routes::LOGIN, &[])?;
        let body = RequestBody::Form(login_request_body(user_identifier));
        let envelope = self
            .transport
            .request_with_token(reqwest::Method::POST, url, &[], body, None)
            .await
            .map_err(|err| wrap_as_authentication("Login", true, err))?;
        let login_token = extract_user_token(&envelope)?;
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
/// [`PendingLogin::verify`] reuse its configuration exactly.
///
/// [`PendingLogin::verify`] takes `self` by value: once called, this value is consumed and
/// cannot be reused — an unverified login token can never reach an authenticated endpoint through
/// this type. `Debug` is implemented by hand to redact the token unconditionally.
pub struct PendingLogin {
    login_token: SecretString,
    transport: Arc<Transport>,
}

impl PendingLogin {
    /// Asks the server to resend the verification code.
    ///
    /// Ported from `resend_verification_code()` (`api/auth.py:220-237`): sends `POST`
    /// [`crate::routes::LOGIN_RESEND`] with a **JSON** body `{}` (the one asymmetric encoding
    /// among the login handshake's requests), authenticated by the login token. The response
    /// envelope is returned as-is rather than discarded, unlike Python's
    /// `resend_verification_code` (which discards the body and returns a bare `bool`) — every
    /// endpoint method in this crate returns `Result<Envelope, Error>` over the untouched wire
    /// payload, including this one.
    ///
    /// # Errors
    ///
    /// Propagates whatever the transport produces unmodified. This is a deliberate asymmetry with
    /// Python: Python catches only the generic `EeroAPIException` case and returns `false`
    /// instead of raising, while a `401`/`429` still propagates uncaught — this port does not
    /// attempt to reproduce that partial catch; every failure mode surfaces as its matching
    /// [`Error`] variant here.
    pub async fn resend(&self) -> Result<Envelope, Error> {
        let url = self.transport.render_url(&routes::LOGIN_RESEND, &[])?;
        self.transport
            .request_with_token(
                reqwest::Method::POST,
                url,
                &[],
                RequestBody::Json(json!({})),
                Some(&self.login_token),
            )
            .await
    }

    /// Completes the login handshake with the one-time `code`, consuming this `PendingLogin`.
    ///
    /// Ported from `verify()` (`api/auth.py:219-263`): sends `POST` [`crate::routes::LOGIN_VERIFY`]
    /// with a **form** body `code=<code>`, authenticated by the login token. The response is
    /// discarded entirely and the login token itself becomes the permanent session token
    /// (`auth.py:247-250`: *"The response carries the user object, not a new token -- the
    /// `session_id` set during `login()` is what's now verified"*) — no fresh `Set-Cookie` is ever
    /// consulted (`v8.0.4` has no `Set-Cookie` reader anywhere in `base.py`/`auth.py`; the earlier
    /// D-16 hedge this method used to apply is removed as of this phase, since it hedged against
    /// a question the Python source itself never answers either way, and never will observably
    /// differ from "use the login token" from outside this crate).
    ///
    /// # Errors
    ///
    /// A `401` (a wrong code) propagates unchanged as [`Error::Authentication`], matching
    /// `auth.py:258-262`'s `except EeroAuthenticationException: raise`. Any other API-classified
    /// error is re-wrapped as `Error::Authentication { message: "Verification failed: {err}",
    /// .. }`, the same shape [`LoginFlow::start`] applies. `Error::Network`/`Error::Timeout`
    /// propagate unmodified.
    pub async fn verify(self, code: &str) -> Result<Session, Error> {
        let url = self.transport.render_url(&routes::LOGIN_VERIFY, &[])?;
        let body = RequestBody::Form(verify_request_body(code));
        self.transport
            .request_with_token(
                reqwest::Method::POST,
                url,
                &[],
                body,
                Some(&self.login_token),
            )
            .await
            .map_err(|err| wrap_as_authentication("Verification", false, err))?;

        Ok(Session::from_token(self.login_token.expose_secret()))
    }

    /// The login token this `PendingLogin` is holding, for other in-crate callers that need it
    /// without completing verification.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn user_token(&self) -> &SecretString {
        &self.login_token
    }
}

impl std::fmt::Debug for PendingLogin {
    /// Redacts the login token unconditionally and omits the `transport` handle entirely.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingLogin")
            .field("login_token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Re-wraps the errors Python's `except` clauses catch at these two call sites into
/// `Error::Authentication { message: "{prefix} failed: {err}", .. }`, carrying the inner error's
/// `envelope`/`error_code` forward. Shared by [`LoginFlow::start`] (`prefix = "Login"`,
/// `wrap_validation = true`) and [`PendingLogin::verify`] (`prefix = "Verification"`,
/// `wrap_validation = false`).
///
/// Exactly the `EeroAPIException` family is wrapped — [`Error::Api`], [`Error::AccessDenied`],
/// [`Error::ClientBlocked`], [`Error::NotFound`], [`Error::PremiumRequired`],
/// [`Error::FeatureUnavailable`] (`exceptions.py:59-92,113-181,192-241,258-303`) — plus
/// [`Error::Validation`] for `login` only (`auth.py:206-217`; `verify` has no such clause,
/// `auth.py:253-263`). Everything else passes through unchanged: `Error::Authentication` (a real
/// `401` already carries its own message), `Error::RateLimit` (`EeroRateLimitException` is a
/// sibling of `EeroAPIException`, not a subclass, `exceptions.py:44-47`), `Network`, `Timeout`,
/// and the Rust-only variants.
fn wrap_as_authentication(prefix: &str, wrap_validation: bool, err: Error) -> Error {
    match err {
        Error::Api { .. }
        | Error::AccessDenied { .. }
        | Error::ClientBlocked { .. }
        | Error::NotFound { .. }
        | Error::PremiumRequired { .. }
        | Error::FeatureUnavailable { .. } => wrap(prefix, err),
        Error::Validation { .. } if wrap_validation => wrap(prefix, err),
        other => other,
    }
}

/// The wrapping step of [`wrap_as_authentication`], split out so the match above stays a pure
/// classification.
fn wrap(prefix: &str, other: Error) -> Error {
    let envelope = other.envelope().cloned();
    let error_code = other.error_code().map(str::to_owned);
    let message = format!("{prefix} failed: {other}");
    Error::Authentication {
        message,
        envelope,
        error_code,
    }
}

/// Builds the request body for [`LoginFlow::start`]: `login=<user_identifier>` (form-encoded).
/// Factored out so the exact shape is unit-testable without a network round trip.
fn login_request_body(user_identifier: &str) -> Vec<(String, String)> {
    vec![("login".to_owned(), user_identifier.to_owned())]
}

/// Builds the request body for [`PendingLogin::verify`]: `code=<code>` (form-encoded). Factored
/// out so the exact shape is unit-testable without a network round trip.
fn verify_request_body(code: &str) -> Vec<(String, String)> {
    vec![("code".to_owned(), code.to_owned())]
}

/// Extracts and validates `data.user_token` from a `login` response: present and non-empty
/// becomes the login token, anything else becomes [`Error::Authentication`] with
/// [`LOGIN_FAILED_NO_USER_TOKEN`].
fn extract_user_token(envelope: &Envelope) -> Result<SecretString, Error> {
    envelope
        .data()
        .get(consts::USER_TOKEN_KEY)
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(|token| SecretString::from(token.to_owned()))
        .ok_or_else(|| Error::authentication(LOGIN_FAILED_NO_USER_TOKEN))
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};
    use serde_json::json;

    use super::{
        LoginFlow, PendingLogin, extract_user_token, login_request_body, verify_request_body,
        wrap_as_authentication,
    };
    use crate::envelope::Envelope;
    use crate::error::Error;
    use crate::transport::Transport;

    // ===================== request body construction =====================

    #[test]
    fn login_request_body_matches_the_form_shape() {
        assert_eq!(
            login_request_body("user@example.com"),
            vec![("login".to_owned(), "user@example.com".to_owned())]
        );
    }

    #[test]
    fn verify_request_body_matches_the_form_shape() {
        assert_eq!(
            verify_request_body("123456"),
            vec![("code".to_owned(), "123456".to_owned())]
        );
    }

    // ===================== extract_user_token =====================

    #[test]
    fn extract_user_token_reads_data_user_token() {
        let envelope = Envelope::from_value(json!({ "data": { "user_token": "ut-123" } }));
        let token = extract_user_token(&envelope).expect("token present");
        assert_eq!(token.expose_secret(), "ut-123");
    }

    #[test]
    fn extract_user_token_missing_is_authentication_error_with_the_expected_message() {
        let envelope = Envelope::from_value(json!({ "data": {} }));
        let err = extract_user_token(&envelope).unwrap_err();
        assert!(
            matches!(err, Error::Authentication { message: ref msg, .. } if msg == "Login failed: No user token received")
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

    // ===================== wrap_as_authentication =====================

    #[test]
    fn wrap_as_authentication_leaves_a_401_unchanged() {
        let original = Error::authentication("some 401 message");
        let wrapped = wrap_as_authentication("Login", true, original);
        assert!(
            matches!(wrapped, Error::Authentication { message, .. } if message == "some 401 message")
        );
    }

    #[test]
    fn wrap_as_authentication_wraps_a_validation_error_with_the_prefix() {
        let original = Error::validation("request", "error.form.errors");
        let wrapped = wrap_as_authentication("Login", true, original);
        match wrapped {
            Error::Authentication { message, .. } => {
                assert_eq!(
                    message,
                    "Login failed: Validation error for 'request': error.form.errors"
                );
            }
            other => panic!("expected Error::Authentication, got {other:?}"),
        }
    }

    #[test]
    fn wrap_as_authentication_leaves_a_rate_limit_unchanged() {
        // `EeroRateLimitException` is a sibling of `EeroAPIException`, so neither `login` nor
        // `verify` catches it (`auth.py:201-217,253-263`).
        let original = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: None,
            envelope: None,
            error_code: Some("error.rate.limit".to_owned()),
        };
        assert!(matches!(
            wrap_as_authentication("Login", true, original),
            Error::RateLimit { .. }
        ));
    }

    #[test]
    fn wrap_as_authentication_leaves_a_validation_error_unchanged_for_verify() {
        // `verify` has no `except EeroValidationException` clause (`auth.py:253-263`).
        let original = Error::validation("request", "error.form.errors");
        assert!(matches!(
            wrap_as_authentication("Verification", false, original),
            Error::Validation { .. }
        ));
    }

    #[test]
    fn wrap_as_authentication_leaves_network_and_timeout_unchanged() {
        assert!(matches!(
            wrap_as_authentication("Verification", false, Error::Timeout),
            Error::Timeout
        ));
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
