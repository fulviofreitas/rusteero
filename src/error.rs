//! The crate error type.
//!
//! Ported from `eero-api`'s `src/eero/exceptions.py` at `v8.4.1`. Every variant carries the
//! parsed response `envelope` and `error_code` (`meta.error`) it was classified from, mirroring
//! every Python exception doing the same (`exceptions.py:8-31`); classification itself happens in
//! [`crate::errors`]. [`Error::Authentication`] and [`Error::RateLimit`] also carry the HTTP
//! `status` of the response (`exceptions.py:38-98`), which is `None` for a failure raised before
//! any request was made.
//!
//! | Python | Rust |
//! |---|---|
//! | `EeroAuthenticationException` | [`Error::Authentication`] |
//! | `EeroRateLimitException` | [`Error::RateLimit`] |
//! | `EeroNetworkException` | [`Error::Network`] |
//! | `EeroAPIException(status, msg)` | [`Error::Api`] |
//! | `EeroTimeoutException` | [`Error::Timeout`] |
//! | `EeroAccessDeniedException` | [`Error::AccessDenied`] |
//! | `EeroClientBlockedException` | [`Error::ClientBlocked`] |
//! | `EeroNotFoundException.from_response` | [`Error::NotFound`] |
//! | `EeroPremiumRequiredException.from_response` | [`Error::PremiumRequired`] |
//! | `EeroFeatureUnavailableException.from_response` | [`Error::FeatureUnavailable`] |
//! | `EeroValidationException(field, msg)` | [`Error::Validation`] |
//! | bare `EeroException` (missing network id, `client.py:168`) | [`Error::MissingNetworkId`] |
//! | — (added) | [`Error::Storage`], [`Error::Json`] |
//!
//! `AccessDenied`/`ClientBlocked`/`NotFound` are separate variants rather than folded into
//! [`Error::Api`] (decision D-8): their Python counterparts are `EeroAPIException` *subclasses*
//! purely for `isinstance` ergonomics, but keeping the distinct shapes here means a caller can
//! match on the specific failure without inspecting `error_code` strings.

use std::time::Duration;

use serde_json::Value;

use crate::redact;

/// The crate's error type.
///
/// Mirrors the exception hierarchy of `eero-api`'s `exceptions.py` at `v8.4.1`: every variant
/// here corresponds to exactly one `Eero*Exception` subclass (see the module-level mapping
/// table), plus two variants added for the Rust port ([`Error::Storage`], [`Error::Json`]).
///
/// `Display` strings mirror Python's `str(exc)` character-for-character wherever Python's
/// `__init__` builds a fixed format string. `message` is *never* built from a raw response body
/// or the request URL (`errors.py:237-258`) — only from the catalogue string a recognised
/// `error_code` classifies to, or the fixed fallback `"unrecognised error string"`; the full
/// parsed response is still available, verbatim, via [`Error::envelope`].
///
/// This type is `#[non_exhaustive]`: new variants may be added in a minor release.
///
/// `Debug` is hand-written (see the `impl std::fmt::Debug for Error` below), not derived: a
/// derived `Debug` would print `envelope`/`url` verbatim, and either can carry a credential — a
/// server-echoed `password`/`token` field inside `envelope`, or a `?s=<session-token>` query
/// string appended to `url` for logging/diagnostics before this error was constructed. Every
/// other field is printed exactly as a derived `Debug` would.
#[derive(thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Authentication failed, or no session token is available.
    ///
    /// Ported from `EeroAuthenticationException` (`exceptions.py:38-65`). Covers both the
    /// client-side "no token in memory" pre-flight guard (the literal message
    /// `"Not authenticated"`, built via [`Error::authentication`]) and the server-driven HTTP 401
    /// case, where `message` is always [`crate::errors::message_for_error_code`]'s output — never
    /// raw body text. `envelope`/`error_code`/`status` are `None` for the client-side guard case.
    ///
    /// A 401 is always classified as this variant, whatever `meta.error` it carries — including
    /// `error.rate.limit`, which stays an authentication failure on a 401 (`errors.py:322-325`).
    #[error("{message}")]
    Authentication {
        /// Human-readable message. Either the fixed `"Not authenticated"` guard text, or a
        /// catalogue string / the fixed unrecognised-string fallback.
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
        /// The HTTP status of the response this error was classified from: `Some(401)` for every
        /// server-driven authentication error, which is the only status that selects this
        /// variant. `None` for a failure raised before any request was made (`"Not
        /// authenticated"`, `"No session token available. Login first."`), and for the
        /// `"Login failed: …"` / `"Verification failed: …"` errors that `LoginFlow::start` and
        /// `PendingLogin::verify` build around another failure, which Python raises without a
        /// status code (`api/auth.py:241-255,280,305-308`).
        status: Option<u16>,
    },

    /// The server rejected the request with HTTP 429 (Too Many Requests), or reported
    /// `error.rate.limit` on any other status (status-independent classification).
    ///
    /// Ported from `EeroRateLimitException` (`exceptions.py:68-98`). Never an authentication
    /// error, whatever its `status` — see [`Error::is_auth_error`].
    #[error("{message}")]
    RateLimit {
        /// Human-readable message: a catalogue string, or the fixed unrecognised-string
        /// fallback.
        message: String,
        /// The parsed `Retry-After` header value, if the server sent one and it was parseable
        /// (`api/base.py:366-393`, where Python caps it at one day; see
        /// [`crate::consts::RETRY_AFTER_MAX`]).
        retry_after: Option<Duration>,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
        /// The HTTP status of the response. Not necessarily `429`: `error.rate.limit` is
        /// classified as a rate limit on any status other than `401`, so this can be `400`,
        /// `503` and so on. `None` only for a hand-built error with no status known.
        status: Option<u16>,
    },

    /// A transport-level failure (DNS, TCP, TLS, connect, etc.) below the HTTP status layer.
    ///
    /// Ported from `EeroNetworkException`, wrapping every `reqwest::Error` that isn't itself a
    /// timeout (see [`Error::Timeout`]).
    #[error("Network error: {0}")]
    Network(#[source] reqwest::Error),

    /// The server responded with a non-2xx status that this crate maps to a generic API error,
    /// or a client-side synthetic error modeled on the same shape.
    ///
    /// Ported from `EeroAPIException`. `Display` renders as `"API error {status}: {message}"`,
    /// exactly reproducing Python's `__str__`.
    #[error("API error {status}: {message}")]
    Api {
        /// The HTTP status code returned by the server. In a couple of call sites ported
        /// verbatim from Python this is a client-side fabricated value (e.g. `502` for a
        /// missing MAC address before a blacklist call, `devices.py:174-177`) rather than an
        /// actual response status.
        status: u16,
        /// Human-readable message: a catalogue string, or the fixed unrecognised-string
        /// fallback for every server-driven case; a fixed, hand-written message for every
        /// client-side-synthesized case (redirect refusal, oversized body, invalid JSON, ...).
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
        /// The request URL, when known. An addition on top of the Python contract; deliberately
        /// kept out of `Display` so the rendered message stays identical to Python's, and so a
        /// token embedded in a query string (which should never happen, but is not this type's
        /// job to guarantee) can never surface through `to_string()`.
        url: Option<String>,
    },

    /// The request timed out.
    ///
    /// Ported from `EeroTimeoutException`, raised with the fixed message `"Request timed out"`.
    #[error("Request timed out")]
    Timeout,

    /// The API denied access to a resource (HTTP 403 with `error.access.denied`).
    ///
    /// Ported from `EeroAccessDeniedException`, an `EeroAPIException` subclass distinct from
    /// [`Error::Authentication`]: the caller is authenticated, but not permitted to perform the
    /// operation. Never an auth error — see [`Error::is_auth_error`].
    #[error("API error {status}: {message}")]
    AccessDenied {
        /// Always `403` in practice.
        status: u16,
        /// A catalogue string, or the fixed unrecognised-string fallback.
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
    },

    /// The API rejected requests from this client version (`error.app.version.blocked`,
    /// status-independent).
    ///
    /// Ported from `EeroClientBlockedException`.
    #[error("API error {status}: {message}")]
    ClientBlocked {
        /// The HTTP status code the server used to carry this status-independent code.
        status: u16,
        /// A catalogue string, or the fixed unrecognised-string fallback.
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
    },

    /// A requested resource does not exist (HTTP 404, any `error_code`, recognised or not).
    ///
    /// Ported from `EeroNotFoundException.from_response` — the shape `error_for_response`
    /// actually constructs; the transport never knows a per-resource type/id at classification
    /// time, so both are absent here (unlike Python's original, backward-compatible
    /// two-argument constructor, which this port does not need since nothing in this crate
    /// constructs a `NotFound` client-side).
    #[error("API error {status}: {message}")]
    NotFound {
        /// Always `404` in practice.
        status: u16,
        /// A catalogue string, or the fixed unrecognised-string fallback.
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
    },

    /// A feature requires an Eero Plus subscription (status-independent).
    ///
    /// Ported from `EeroPremiumRequiredException.from_response`.
    #[error("API error {}: {message}", status.map_or_else(|| "None".to_owned(), |s| s.to_string()))]
    PremiumRequired {
        /// The HTTP status code the server used to carry this status-independent code, when
        /// known.
        status: Option<u16>,
        /// A catalogue string, or the fixed unrecognised-string fallback.
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
    },

    /// A feature is unavailable for the reason given (status-independent).
    ///
    /// Ported from `EeroFeatureUnavailableException.from_response`.
    #[error("API error {}: {message}", status.map_or_else(|| "None".to_owned(), |s| s.to_string()))]
    FeatureUnavailable {
        /// The HTTP status code the server used to carry this status-independent code, when
        /// known.
        status: Option<u16>,
        /// A catalogue string, or the fixed unrecognised-string fallback.
        message: String,
        /// The raw, parsed response envelope, when the response body was valid JSON.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present.
        error_code: Option<String>,
    },

    /// A client-side precondition failed before any request was sent, or the server reported a
    /// recognised validation string on HTTP 400.
    ///
    /// Ported from `EeroValidationException`. `field` is `"request"` for the server-driven
    /// (`from_response`) shape — the transport has no per-field context at classification time —
    /// and the specific field name for every client-side validation raised throughout this
    /// crate (built via [`Error::validation`]).
    #[error("Validation error for '{field}': {message}")]
    Validation {
        /// The name of the field that failed validation, or `"request"` for a server-driven
        /// validation error.
        field: String,
        /// The reason validation failed.
        message: String,
        /// The raw, parsed response envelope. `None` for every client-side validation.
        envelope: Option<Value>,
        /// The value of `envelope["meta"]["error"]`, when present. `None` for every client-side
        /// validation.
        error_code: Option<String>,
    },

    /// No network ID was supplied and no preferred network is set.
    ///
    /// Ported from the bare `EeroException` raised at `client.py:168`
    /// (`"No network ID provided and no preferred network set"`) — the only call site in
    /// `eero-api` that raises the base class directly rather than a named subclass. Giving it
    /// its own variant here is more honest than a generic `Error::Other(String)`.
    #[error("No network ID provided and no preferred network set")]
    MissingNetworkId,

    /// A credential-store operation failed.
    ///
    /// Added for the Rust port: Python swallows storage failures internally (logs at DEBUG and
    /// carries on, see `api/auth_storage.py`); this crate surfaces them as a typed error so
    /// callers can decide whether a given failure is fatal.
    #[error(transparent)]
    Storage(#[from] StorageError),

    /// A response body could not be deserialized into the requested type.
    ///
    /// Added for the Rust port. Used only by `Envelope::data_as::<T>()` — invalid JSON on a 2xx
    /// response is instead mapped to `Error::Api` at the transport layer (`"Invalid JSON
    /// response ({n} bytes)"`, byte count only), matching Python's own `base.py:593-597`.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Redacts `envelope` for `Debug`, via [`redact::redact_sensitive`]. `None` stays `None`.
fn debug_envelope(envelope: Option<&Value>) -> Option<Value> {
    envelope.map(redact::redact_sensitive)
}

/// Reduces `url` to its path component for `Debug`, dropping the query string and fragment (and
/// therefore any session token or other sensitive value either could carry). `None` stays `None`.
///
/// Every `url` this crate ever constructs came from [`url::Url`] in the first place, so parsing
/// it back always succeeds in practice; the fallback (splitting on the first `?`/`#` byte by
/// hand) exists only so this can never panic on a hand-constructed `Error` a test or a future
/// caller builds with an arbitrary string.
fn debug_url_path_only(url: Option<&String>) -> Option<String> {
    url.map(|full| {
        url::Url::parse(full).map_or_else(
            |_| full.split(['?', '#']).next().unwrap_or(full).to_owned(),
            |parsed| parsed.path().to_owned(),
        )
    })
}

impl std::fmt::Debug for Error {
    /// See the [`Error`] type's own doc comment for why this is hand-written rather than
    /// derived. Every variant is rendered with the same field names and shape a derived `Debug`
    /// would use, except `envelope` (redacted via `debug_envelope`) and `url` (reduced to its
    /// path via `debug_url_path_only`).
    // This is a single mechanical match over every variant, each arm four lines long and none of
    // them sharing logic worth factoring out further without losing the direct one-arm-per-
    // variant correspondence to a derived `Debug` that makes this easy to audit; splitting it
    // into two functions purely to satisfy a line count would not improve readability.
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Authentication {
                message,
                envelope,
                error_code,
                status,
            } => f
                .debug_struct("Authentication")
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .field("status", status)
                .finish(),
            Error::RateLimit {
                message,
                retry_after,
                envelope,
                error_code,
                status,
            } => f
                .debug_struct("RateLimit")
                .field("message", message)
                .field("retry_after", retry_after)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .field("status", status)
                .finish(),
            Error::Network(err) => f.debug_tuple("Network").field(err).finish(),
            Error::Api {
                status,
                message,
                envelope,
                error_code,
                url,
            } => f
                .debug_struct("Api")
                .field("status", status)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .field("url", &debug_url_path_only(url.as_ref()))
                .finish(),
            Error::Timeout => write!(f, "Timeout"),
            Error::AccessDenied {
                status,
                message,
                envelope,
                error_code,
            } => f
                .debug_struct("AccessDenied")
                .field("status", status)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .finish(),
            Error::ClientBlocked {
                status,
                message,
                envelope,
                error_code,
            } => f
                .debug_struct("ClientBlocked")
                .field("status", status)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .finish(),
            Error::NotFound {
                status,
                message,
                envelope,
                error_code,
            } => f
                .debug_struct("NotFound")
                .field("status", status)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .finish(),
            Error::PremiumRequired {
                status,
                message,
                envelope,
                error_code,
            } => f
                .debug_struct("PremiumRequired")
                .field("status", status)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .finish(),
            Error::FeatureUnavailable {
                status,
                message,
                envelope,
                error_code,
            } => f
                .debug_struct("FeatureUnavailable")
                .field("status", status)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .finish(),
            Error::Validation {
                field,
                message,
                envelope,
                error_code,
            } => f
                .debug_struct("Validation")
                .field("field", field)
                .field("message", message)
                .field("envelope", &debug_envelope(envelope.as_ref()))
                .field("error_code", error_code)
                .finish(),
            Error::MissingNetworkId => write!(f, "MissingNetworkId"),
            Error::Storage(err) => f.debug_tuple("Storage").field(err).finish(),
            Error::Json(err) => f.debug_tuple("Json").field(err).finish(),
        }
    }
}

impl Error {
    /// Builds the client-side "no session token available" guard error.
    ///
    /// The literal message `"Not authenticated"` matches every `EeroAuthenticationException`
    /// call site in `eero-api` that fires before any request was sent (dozens of call sites
    /// across `api/*.py`). `envelope`, `error_code` and `status` are always `None` for this
    /// shape, since no response was ever received — matching Python's `status_code=None`
    /// default (`exceptions.py:41-61`).
    #[must_use]
    pub fn authentication(message: impl Into<String>) -> Self {
        Error::Authentication {
            message: message.into(),
            envelope: None,
            error_code: None,
            status: None,
        }
    }

    /// Builds a client-side validation error: `envelope`/`error_code` are always `None`, since
    /// this shape is raised before any request is sent (matching `EeroValidationException`'s
    /// original, non-`from_response` constructor).
    #[must_use]
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Validation {
            field: field.into(),
            message: message.into(),
            envelope: None,
            error_code: None,
        }
    }

    /// Returns the raw, parsed response envelope this error was classified from, if any.
    ///
    /// `None` for every client-side-synthesized error (a precondition that failed before any
    /// request was sent, [`Error::Network`], [`Error::Timeout`], [`Error::MissingNetworkId`],
    /// [`Error::Storage`], [`Error::Json`]) and for any server-driven error whose response body
    /// was not valid JSON.
    #[must_use]
    pub fn envelope(&self) -> Option<&Value> {
        match self {
            Error::Authentication { envelope, .. }
            | Error::RateLimit { envelope, .. }
            | Error::Api { envelope, .. }
            | Error::AccessDenied { envelope, .. }
            | Error::ClientBlocked { envelope, .. }
            | Error::NotFound { envelope, .. }
            | Error::PremiumRequired { envelope, .. }
            | Error::FeatureUnavailable { envelope, .. }
            | Error::Validation { envelope, .. } => envelope.as_ref(),
            Error::Network(_)
            | Error::Timeout
            | Error::MissingNetworkId
            | Error::Storage(_)
            | Error::Json(_) => None,
        }
    }

    /// Returns the value of `envelope["meta"]["error"]` this error was classified from, if any.
    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        match self {
            Error::Authentication { error_code, .. }
            | Error::RateLimit { error_code, .. }
            | Error::Api { error_code, .. }
            | Error::AccessDenied { error_code, .. }
            | Error::ClientBlocked { error_code, .. }
            | Error::NotFound { error_code, .. }
            | Error::PremiumRequired { error_code, .. }
            | Error::FeatureUnavailable { error_code, .. }
            | Error::Validation { error_code, .. } => error_code.as_deref(),
            Error::Network(_)
            | Error::Timeout
            | Error::MissingNetworkId
            | Error::Storage(_)
            | Error::Json(_) => None,
        }
    }

    /// Returns the HTTP status code this error carries, if any.
    ///
    /// For [`Error::Authentication`] and [`Error::RateLimit`] this is their `status` field: `Some`
    /// for a server-driven error (always `401` for the former, the actual response status for the
    /// latter), `None` for one raised before any request was made. `None` for
    /// [`Error::PremiumRequired`]/[`Error::FeatureUnavailable`] when constructed without a
    /// response (the original, client-side-only Python shape — never produced by this crate's own
    /// transport, kept for parity), and for every variant with no status at all.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api { status, .. }
            | Error::AccessDenied { status, .. }
            | Error::ClientBlocked { status, .. }
            | Error::NotFound { status, .. } => Some(*status),
            Error::Authentication { status, .. }
            | Error::RateLimit { status, .. }
            | Error::PremiumRequired { status, .. }
            | Error::FeatureUnavailable { status, .. } => *status,
            Error::Network(_)
            | Error::Timeout
            | Error::Validation { .. }
            | Error::MissingNetworkId
            | Error::Storage(_)
            | Error::Json(_) => None,
        }
    }

    /// Returns `true` if this error represents an authentication failure.
    ///
    /// Mirrors the Python `is_auth_error()` predicate exactly: `true` for
    /// [`Error::Authentication`] unconditionally, whatever its `status`
    /// (`exceptions.py:63-65`); `false` for [`Error::RateLimit`] whatever its `status`, because
    /// `EeroRateLimitException` keeps the base class's `False` (`exceptions.py:33-35`) even
    /// though it now carries a status; and for every other variant `true` exactly when its
    /// `status` equals `401` (`EeroAPIException.is_auth_error`, inherited by every one of its
    /// subclasses). In practice no variant other than `Authentication` is ever constructed with
    /// `status == 401` by this crate's own classification (401 always routes to
    /// `Authentication` first, per [`crate::errors::error_for_response`]'s precedence), but the
    /// check is written generically so it stays correct for any hand-constructed error too.
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        match self {
            Error::Authentication { .. } => true,
            Error::RateLimit { .. } => false,
            other => other.status() == Some(401),
        }
    }
}

/// Failure modes for a [`crate::storage`] `CredentialStore` backend.
///
/// This lives in `error.rs`, not `storage/`, so that `error.rs` has no dependency on the
/// storage module; `storage::mod` re-exports it as `pub use crate::error::StorageError;`.
///
/// `#[non_exhaustive]` so storage work can add backend-specific variants without a breaking
/// change.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// An I/O failure while reading or writing the credential file.
    #[error("storage I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Stored credentials could not be (de)serialized as JSON.
    #[error("storage serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// A named backend (e.g. the OS keyring) reported a failure.
    #[error("{backend} storage error: {message}")]
    Backend {
        /// The name of the backend that failed, e.g. `"keyring"`.
        backend: String,
        /// A description of the failure. Never includes the credential value itself.
        message: String,
    },

    /// Nothing has been stored yet.
    #[error("no credentials stored")]
    NotFound,

    /// The stored credentials were present but empty or otherwise unusable.
    #[error("stored credentials are empty")]
    Empty,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Error, StorageError};

    // ===================== Display strings =====================

    #[test]
    fn authentication_display_is_verbatim_message() {
        let err = Error::authentication("Not authenticated");
        assert_eq!(err.to_string(), "Not authenticated");
    }

    #[test]
    fn rate_limit_display_is_the_message() {
        let err = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: None,
            envelope: None,
            error_code: Some("error.rate.limit".to_owned()),
            status: Some(429),
        };
        assert_eq!(err.to_string(), "error.rate.limit");

        let err = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: Some(Duration::from_secs(30)),
            envelope: None,
            error_code: None,
            status: None,
        };
        assert_eq!(err.to_string(), "error.rate.limit");
    }

    #[test]
    fn api_display_matches_python_format() {
        let err = Error::Api {
            status: 500,
            message: "boom".to_owned(),
            envelope: None,
            error_code: None,
            url: None,
        };
        assert_eq!(err.to_string(), "API error 500: boom");
    }

    #[test]
    fn api_display_excludes_url_field_even_when_it_carries_a_token() {
        let err = Error::Api {
            status: 404,
            message: "not found".to_owned(),
            envelope: None,
            error_code: None,
            url: Some("https://api-user.e2ro.com/2.2/foo?s=secret-token".to_owned()),
        };
        let rendered = err.to_string();
        assert_eq!(rendered, "API error 404: not found");
        assert!(!rendered.contains("secret-token"));
    }

    #[test]
    fn timeout_display_is_fixed_message() {
        assert_eq!(Error::Timeout.to_string(), "Request timed out");
    }

    #[test]
    fn access_denied_display_matches_python_format() {
        let err = Error::AccessDenied {
            status: 403,
            message: "error.access.denied".to_owned(),
            envelope: None,
            error_code: Some("error.access.denied".to_owned()),
        };
        assert_eq!(err.to_string(), "API error 403: error.access.denied");
    }

    #[test]
    fn client_blocked_display_matches_python_format() {
        let err = Error::ClientBlocked {
            status: 200,
            message: "error.app.version.blocked".to_owned(),
            envelope: None,
            error_code: None,
        };
        assert_eq!(err.to_string(), "API error 200: error.app.version.blocked");
    }

    #[test]
    fn not_found_display_matches_python_format() {
        let err = Error::NotFound {
            status: 404,
            message: "unrecognised error string".to_owned(),
            envelope: None,
            error_code: None,
        };
        assert_eq!(err.to_string(), "API error 404: unrecognised error string");
    }

    #[test]
    fn premium_required_display_matches_python_str_of_optional_status() {
        let err = Error::PremiumRequired {
            status: Some(200),
            message: "error.premium.user_not_subscribed".to_owned(),
            envelope: None,
            error_code: None,
        };
        assert_eq!(
            err.to_string(),
            "API error 200: error.premium.user_not_subscribed"
        );

        let err = Error::PremiumRequired {
            status: None,
            message: "This feature requires an Eero Plus subscription".to_owned(),
            envelope: None,
            error_code: None,
        };
        assert_eq!(
            err.to_string(),
            "API error None: This feature requires an Eero Plus subscription"
        );
    }

    #[test]
    fn feature_unavailable_display_matches_python_format() {
        let err = Error::FeatureUnavailable {
            status: Some(200),
            message: "error.eero.offline".to_owned(),
            envelope: None,
            error_code: None,
        };
        assert_eq!(err.to_string(), "API error 200: error.eero.offline");
    }

    #[test]
    fn validation_display_matches_python_format() {
        let err = Error::validation("field", "msg");
        assert_eq!(err.to_string(), "Validation error for 'field': msg");
    }

    #[test]
    fn missing_network_id_display_is_fixed_message() {
        assert_eq!(
            Error::MissingNetworkId.to_string(),
            "No network ID provided and no preferred network set"
        );
    }

    #[test]
    fn storage_display_is_transparent() {
        let err = Error::Storage(StorageError::NotFound);
        assert_eq!(err.to_string(), "no credentials stored");
    }

    #[test]
    fn json_display_wraps_serde_error() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = Error::Json(serde_err);
        assert!(err.to_string().starts_with("JSON error: "));
    }

    #[test]
    fn network_display_wraps_reqwest_error() {
        // A malformed URL fails at request-build time, with no network access, giving us a
        // real `reqwest::Error` to wrap without needing a live connection.
        let reqwest_err = reqwest::Client::new()
            .get("not a valid url")
            .build()
            .expect_err("malformed URL must fail to build");
        let err = Error::Network(reqwest_err);
        assert!(err.to_string().starts_with("Network error: "));
    }

    #[test]
    fn storage_error_display_strings() {
        assert_eq!(
            StorageError::Backend {
                backend: "keyring".to_owned(),
                message: "entry not found".to_owned(),
            }
            .to_string(),
            "keyring storage error: entry not found"
        );
        assert_eq!(StorageError::NotFound.to_string(), "no credentials stored");
        assert_eq!(
            StorageError::Empty.to_string(),
            "stored credentials are empty"
        );
    }

    // ===================== accessors =====================

    #[test]
    fn envelope_and_error_code_accessors_round_trip() {
        let envelope = serde_json::json!({"meta": {"code": 404}});
        let err = Error::NotFound {
            status: 404,
            message: "unrecognised error string".to_owned(),
            envelope: Some(envelope.clone()),
            error_code: Some("weird".to_owned()),
        };
        assert_eq!(err.envelope(), Some(&envelope));
        assert_eq!(err.error_code(), Some("weird"));
        assert_eq!(err.status(), Some(404));
    }

    #[test]
    fn envelope_and_error_code_are_none_for_client_side_errors() {
        let err = Error::authentication("Not authenticated");
        assert_eq!(err.envelope(), None);
        assert_eq!(err.error_code(), None);
        assert_eq!(err.status(), None);

        assert_eq!(Error::Timeout.envelope(), None);
        assert_eq!(Error::MissingNetworkId.error_code(), None);
    }

    // ===================== is_auth_error() =====================

    #[test]
    fn is_auth_error_matrix() {
        assert!(Error::authentication("Not authenticated").is_auth_error());
        assert!(
            Error::Api {
                status: 401,
                message: String::new(),
                envelope: None,
                error_code: None,
                url: None,
            }
            .is_auth_error()
        );
        assert!(
            Error::AccessDenied {
                status: 401,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );

        assert!(
            !Error::Api {
                status: 403,
                message: String::new(),
                envelope: None,
                error_code: None,
                url: None,
            }
            .is_auth_error()
        );
        assert!(
            !Error::AccessDenied {
                status: 403,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(
            !Error::NotFound {
                status: 404,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(
            !Error::RateLimit {
                message: String::new(),
                retry_after: None,
                envelope: None,
                error_code: None,
                status: Some(429),
            }
            .is_auth_error()
        );
        assert!(
            !Error::RateLimit {
                message: String::new(),
                retry_after: None,
                envelope: None,
                error_code: None,
                status: None,
            }
            .is_auth_error()
        );
        assert!(!Error::Timeout.is_auth_error());
        assert!(
            !Error::PremiumRequired {
                status: None,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(
            !Error::FeatureUnavailable {
                status: None,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(!Error::validation("x", "y").is_auth_error());
        assert!(!Error::MissingNetworkId.is_auth_error());
        assert!(!Error::Storage(StorageError::NotFound).is_auth_error());
        assert!(
            !Error::Json(serde_json::from_str::<serde_json::Value>("bad").unwrap_err())
                .is_auth_error()
        );
    }

    #[test]
    fn is_auth_error_authentication_is_true_whatever_its_status() {
        for status in [None, Some(401), Some(200), Some(403)] {
            let err = Error::Authentication {
                message: String::new(),
                envelope: None,
                error_code: None,
                status,
            };
            assert!(err.is_auth_error(), "status {status:?}");
        }
    }

    #[test]
    fn is_auth_error_rate_limit_is_false_whatever_its_status() {
        // `EeroRateLimitException` keeps the base class's `False`, even though it now carries a
        // status code; a hand-built one with a 401 must not turn into an auth error.
        for status in [None, Some(401), Some(429), Some(503)] {
            let err = Error::RateLimit {
                message: String::new(),
                retry_after: None,
                envelope: None,
                error_code: None,
                status,
            };
            assert!(!err.is_auth_error(), "status {status:?}");
        }
    }

    #[test]
    fn is_auth_error_status_carrying_variants_follow_a_401() {
        assert!(
            Error::PremiumRequired {
                status: Some(401),
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(
            Error::FeatureUnavailable {
                status: Some(401),
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(
            Error::ClientBlocked {
                status: 401,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
        assert!(
            Error::NotFound {
                status: 401,
                message: String::new(),
                envelope: None,
                error_code: None,
            }
            .is_auth_error()
        );
    }

    // ===================== status on Authentication / RateLimit =====================

    #[test]
    fn authentication_helper_builds_a_client_side_error_without_a_status() {
        let err = Error::authentication("session expired");
        assert_eq!(err.to_string(), "session expired");
        assert_eq!(err.envelope(), None);
        assert_eq!(err.error_code(), None);
        assert_eq!(err.status(), None);
        assert!(err.is_auth_error());
    }

    #[test]
    fn status_returns_the_field_of_authentication_and_rate_limit() {
        let auth = Error::Authentication {
            message: "error.session.expired".to_owned(),
            envelope: None,
            error_code: Some("error.session.expired".to_owned()),
            status: Some(401),
        };
        assert_eq!(auth.status(), Some(401));

        let rate_limit = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: None,
            envelope: None,
            error_code: Some("error.rate.limit".to_owned()),
            status: Some(503),
        };
        assert_eq!(rate_limit.status(), Some(503));

        let rate_limit = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: None,
            envelope: None,
            error_code: None,
            status: None,
        };
        assert_eq!(rate_limit.status(), None);
    }

    #[test]
    fn validation_carries_no_status() {
        assert_eq!(Error::validation("f", "m").status(), None);
    }

    #[test]
    fn display_is_unchanged_by_the_status_field() {
        let with_status = Error::Authentication {
            message: "error.session.expired".to_owned(),
            envelope: None,
            error_code: None,
            status: Some(401),
        };
        let without_status = Error::Authentication {
            message: "error.session.expired".to_owned(),
            envelope: None,
            error_code: None,
            status: None,
        };
        assert_eq!(with_status.to_string(), "error.session.expired");
        assert_eq!(with_status.to_string(), without_status.to_string());

        let rate_limit = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: None,
            envelope: None,
            error_code: None,
            status: Some(429),
        };
        assert_eq!(rate_limit.to_string(), "error.rate.limit");
    }

    // ===================== #[from] conversions =====================

    #[test]
    fn storage_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let storage_err: StorageError = io_err.into();
        assert!(matches!(storage_err, StorageError::Io(_)));
    }

    #[test]
    fn storage_error_from_serde_error() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let storage_err: StorageError = serde_err.into();
        assert!(matches!(storage_err, StorageError::Serde(_)));
    }

    #[test]
    fn error_from_storage_error() {
        let err: Error = StorageError::NotFound.into();
        assert!(matches!(err, Error::Storage(StorageError::NotFound)));
    }

    #[test]
    fn error_from_serde_error() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: Error = serde_err.into();
        assert!(matches!(err, Error::Json(_)));
    }

    // ===================== Debug redaction (security finding 4) =====================

    #[test]
    fn api_debug_never_prints_the_envelope_password_or_the_url_query() {
        let err = Error::Api {
            status: 200,
            message: "boom".to_owned(),
            envelope: Some(serde_json::json!({"password": "hunter2"})),
            error_code: None,
            url: Some("https://api-user.e2ro.com/2.2/account?serial=SECRET".to_owned()),
        };
        let debug = format!("{err:?}");
        assert!(
            !debug.contains("hunter2"),
            "envelope value must be redacted, got {debug:?}"
        );
        assert!(
            !debug.contains("SECRET"),
            "url query string must be stripped, got {debug:?}"
        );
        assert!(
            debug.contains("[REDACTED"),
            "the redacted envelope's marker must still be visible, got {debug:?}"
        );
        assert!(
            debug.contains("/2.2/account"),
            "the url path itself is not sensitive and should still be visible, got {debug:?}"
        );
    }

    #[test]
    fn authentication_debug_never_prints_the_envelope_password() {
        let err = Error::Authentication {
            message: "failed".to_owned(),
            envelope: Some(serde_json::json!({"token": "abc123456"})),
            error_code: None,
            status: Some(401),
        };
        let debug = format!("{err:?}");
        assert!(!debug.contains("abc123456"));
        assert!(debug.contains("[REDACTED"));
    }

    #[test]
    fn authentication_and_rate_limit_debug_show_the_status() {
        let err = Error::Authentication {
            message: "error.session.expired".to_owned(),
            envelope: None,
            error_code: None,
            status: Some(401),
        };
        assert!(
            format!("{err:?}").contains("status: Some(401)"),
            "got {err:?}"
        );
        assert!(format!("{:?}", Error::authentication("x")).contains("status: None"));

        let err = Error::RateLimit {
            message: "error.rate.limit".to_owned(),
            retry_after: Some(Duration::from_secs(5)),
            envelope: None,
            error_code: None,
            status: Some(503),
        };
        let debug = format!("{err:?}");
        assert!(debug.contains("status: Some(503)"), "got {debug}");
        assert!(debug.contains("retry_after: Some(5s)"), "got {debug}");
    }

    #[test]
    fn debug_with_no_envelope_or_url_still_renders() {
        let err = Error::Timeout;
        assert_eq!(format!("{err:?}"), "Timeout");

        let err = Error::MissingNetworkId;
        assert_eq!(format!("{err:?}"), "MissingNetworkId");
    }
}
