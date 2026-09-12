//! The crate error type.
//!
//! Ported 1:1 from `eero-api`'s `src/eero/exceptions.py`. See
//! `rusteero-context/claude/tasks/briefs/exceptions.md` for the full behaviour brief this
//! module implements, and `rusteero-context/claude/docs/architecture.md` §7 for the mapping
//! table reproduced below.
//!
//! | Python | Rust |
//! |---|---|
//! | `EeroAuthenticationException` | [`Error::Authentication`] |
//! | `EeroRateLimitException` | [`Error::RateLimit`] |
//! | `EeroNetworkException` | [`Error::Network`] |
//! | `EeroAPIException(status, msg)` | [`Error::Api`] |
//! | `EeroTimeoutException` | [`Error::Timeout`] |
//! | `EeroNotFoundException` | [`Error::NotFound`] (kept, D-8; never constructed) |
//! | `EeroPremiumRequiredException` | [`Error::PremiumRequired`] (kept, D-8; never constructed) |
//! | `EeroFeatureUnavailableException` | [`Error::FeatureUnavailable`] (kept, D-8; never constructed) |
//! | `EeroValidationException(field, msg)` | [`Error::Validation`] |
//! | bare `EeroException` (missing network id, `client.py:168`) | [`Error::MissingNetworkId`] |
//! | — (added) | [`Error::Storage`], [`Error::Json`] |

use std::time::Duration;

use serde_json::Value;

use crate::consts::MAX_ERROR_BODY_CHARS;
use crate::redact;

/// The crate's error type.
///
/// Mirrors the flat exception hierarchy of `eero-api`'s `exceptions.py` one-to-one: every
/// variant here corresponds to exactly one `Eero*Exception` subclass (see the module-level
/// mapping table), plus two variants added for the Rust port ([`Error::Storage`],
/// [`Error::Json`]) and one added field ([`Error::RateLimit::retry_after`]).
///
/// `Display` strings mirror Python's `str(exc)` character-for-character wherever Python's
/// `__init__` builds a fixed format string; fields that are additions on top of the Python
/// contract (such as [`Error::Api::url`]) are deliberately excluded from `Display` so the
/// rendered message never gains information (or leaks anything) Python's would not have shown.
///
/// This type is `#[non_exhaustive]`: new variants may be added in a minor release.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Authentication failed, or no session token is available.
    ///
    /// Ported from `EeroAuthenticationException` (`exceptions.py:16-21`). Covers both the
    /// client-side "no token in memory" pre-flight guard (always the literal message
    /// `"Not authenticated"`, dozens of call sites across `api/*.py`) and the server-driven
    /// HTTP 401 case (`"Authentication failed: {truncated body}"`, `api/base.py:254-256`) —
    /// Python uses the same exception type for both, distinguished only by message text; this
    /// port does the same rather than fragmenting into more variants (see the brief's gotcha
    /// #4: collapsing further, e.g. matching on message text, would be more fragile, not less).
    #[error("{0}")]
    Authentication(String),

    /// The server rejected the request with HTTP 429 (Too Many Requests).
    ///
    /// Ported from `EeroRateLimitException` (`exceptions.py:24-27`), raised at
    /// `api/base.py:265` with the fixed message `"Rate limit exceeded"`. `retry_after` is an
    /// addition on top of the Python contract: Python discards the response's `Retry-After`
    /// header entirely (never even reads it); this port surfaces it when the server sends one.
    #[error("Rate limit exceeded")]
    RateLimit {
        /// The parsed `Retry-After` header value, if the server sent one. `None` when the
        /// header was absent, matching Python's behaviour of never having this information at
        /// all.
        retry_after: Option<Duration>,
    },

    /// A transport-level failure (DNS, TCP, TLS, connect, etc.) below the HTTP status layer.
    ///
    /// Ported from `EeroNetworkException` (`exceptions.py:30-33`), raised at five call sites in
    /// `api/base.py` and `api/auth.py`, all wrapping `aiohttp.ClientError` with `from err`.
    #[error("Network error: {0}")]
    Network(#[source] reqwest::Error),

    /// The server responded with a non-2xx status that this crate maps to a generic API error,
    /// or a client-side synthetic error modeled on the same shape.
    ///
    /// Ported from `EeroAPIException` (`exceptions.py:36-46`). `Display` renders as
    /// `"API error {status}: {message}"`, exactly reproducing Python's `__str__`
    /// (`exceptions.py:42`).
    #[error("API error {status}: {message}")]
    Api {
        /// The HTTP status code returned by the server. In a couple of call sites ported
        /// verbatim from Python this is a client-side fabricated value (e.g. `502` for a
        /// missing MAC address before a blacklist call, `devices.py:174-177`) rather than an
        /// actual response status.
        status: u16,
        /// The error message or response body. Bodies longer than the crate's fixed character
        /// limit are truncated before being stored here, with a trailing marker noting the
        /// original (pre-truncation) length.
        message: String,
        /// The request URL, when known. An addition on top of the Python contract; deliberately
        /// kept out of `Display` so the rendered message stays identical to Python's, and so a
        /// token embedded in a query string (which should never happen, but is not this type's
        /// job to guarantee) can never surface through `to_string()`.
        url: Option<String>,
    },

    /// The request timed out.
    ///
    /// Ported from `EeroTimeoutException` (`exceptions.py:49-52`), raised at `api/base.py:272`
    /// with the fixed message `"Request timed out"`.
    #[error("Request timed out")]
    Timeout,

    /// A requested resource does not exist.
    ///
    /// Ported from `EeroNotFoundException` (`exceptions.py:55-61`) for 1:1 parity with the
    /// Python exception hierarchy. Kept per decision D-8 even though Python never actually
    /// raises it — every real "not found" surfaces as `Error::Api { status: 404, .. }` instead
    /// (`api/base.py:260-263`) — so this variant is never constructed anywhere in this crate.
    #[error("{resource_type} '{resource_id}' not found")]
    NotFound {
        /// The kind of resource that was not found (e.g. `"network"`).
        resource_type: String,
        /// The identifier that was looked up.
        resource_id: String,
    },

    /// A feature requires an Eero Plus subscription.
    ///
    /// Ported from `EeroPremiumRequiredException` (`exceptions.py:64-69`) for 1:1 parity. Kept
    /// per decision D-8; like [`Error::NotFound`], never constructed anywhere in this crate
    /// because Python never raises the exception it mirrors.
    #[error("{feature} requires an Eero Plus subscription")]
    PremiumRequired {
        /// The name of the gated feature.
        feature: String,
    },

    /// A feature is unavailable for the reason given.
    ///
    /// Ported from `EeroFeatureUnavailableException` (`exceptions.py:72-78`) for 1:1 parity.
    /// Kept per decision D-8; like [`Error::NotFound`], never constructed anywhere in this
    /// crate because Python never raises the exception it mirrors.
    #[error("{feature} is {reason}")]
    FeatureUnavailable {
        /// The name of the unavailable feature.
        feature: String,
        /// Why the feature is unavailable.
        reason: String,
    },

    /// A client-side precondition failed before any request was sent.
    ///
    /// Ported from `EeroValidationException` (`exceptions.py:81-86`), raised for e.g.
    /// `id_from_url` on a non-string/empty id (`api/base.py:54`,
    /// `"Validation error for 'id_or_url': must be a non-empty string"`) or
    /// `set_session_token` on an empty token (`api/auth.py:408`,
    /// `"Validation error for 'token': must be a non-empty string"`).
    #[error("Validation error for '{field}': {message}")]
    Validation {
        /// The name of the field that failed validation.
        field: String,
        /// The reason validation failed.
        message: String,
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
    /// response is instead mapped to `Error::Api` at the transport layer, matching Python's
    /// `api/base.py:216-219` (`"Invalid JSON response: {truncated body}"`).
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Returns `true` if this error represents an authentication failure.
    ///
    /// Mirrors the Python `is_auth_error()` predicate exactly: `true` for
    /// [`Error::Authentication`] unconditionally (`exceptions.py:19-21`), and for
    /// [`Error::Api`] iff `status == 401` (`exceptions.py:44-46`). No other variant is ever an
    /// auth error — in particular a `403` (likely "insufficient subscription", per
    /// `api/insights.py:88-89`) is deliberately *not* treated as one, matching Python's
    /// exact-equality-against-401 semantics.
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Error::Authentication(_)) || matches!(self, Error::Api { status: 401, .. })
    }
}

/// Failure modes for a [`crate::storage`] `CredentialStore` backend.
///
/// This lives in `error.rs`, not `storage/`, so that `error.rs` has no dependency on the
/// storage module; `storage::mod` re-exports it as `pub use crate::error::StorageError;`.
///
/// `#[non_exhaustive]` so Phase 2 storage work can add backend-specific variants without a
/// breaking change.
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

/// Truncates `text` to at most [`MAX_ERROR_BODY_CHARS`] Unicode scalar values, appending a
/// marker that reports the *original* length.
///
/// Mirrors `_truncate_for_error` (`api/base.py:29-33`) exactly: if `text` is at most
/// `MAX_ERROR_BODY_CHARS` characters long it is returned unchanged; otherwise the first
/// `MAX_ERROR_BODY_CHARS` characters are kept and
/// `"... [truncated, {original_length} chars total]"` is appended, where `original_length` is
/// the *pre-truncation* character count — not the truncated length and not the byte length.
///
/// Truncation happens on `char` (Unicode scalar value) boundaries, matching Python's `str`
/// slicing by code point, so this never panics on a multi-byte UTF-8 boundary the way a naive
/// byte-index slice (`&text[..MAX_ERROR_BODY_CHARS]`) could.
/// Called from `transport.rs`'s `status_to_envelope` at every site that embeds a response body
/// in an `Error::Api`/`Error::Authentication` message (four call sites as of this writing: the
/// invalid-JSON-on-2xx, `401`, `404`, and generic non-`2xx` arms).
pub(crate) fn truncate_for_error(text: &str) -> String {
    let char_count = text.chars().count();
    if char_count <= MAX_ERROR_BODY_CHARS {
        return text.to_owned();
    }
    let truncated: String = text.chars().take(MAX_ERROR_BODY_CHARS).collect();
    format!("{truncated}... [truncated, {char_count} chars total]")
}

/// Case-insensitive substring markers used by [`sanitize_body_for_error`]'s fallback path for a
/// response body that does not parse as JSON at all.
///
/// A deliberately small, independently-maintained list rather than a re-export of
/// [`crate::redact`]'s own (private) `SENSITIVE_PATTERNS`: `redact.rs` is owned by a different
/// task in this port and out of scope here, and its list is tuned for *structured* (parsed)
/// JSON keys, not for scanning raw, possibly-truncated text. Keeping this list conservative and
/// separate means a body that merely fails to parse is still protected without reaching into
/// another module's private surface.
const RAW_TEXT_SENSITIVE_MARKERS: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "session_id",
    "session_token",
    "credential",
    "cookie",
    "authorization",
    "bearer",
    "private",
];

/// Returns `true` if `text`, matched case-insensitively as a substring, contains anything that
/// looks like a credential — the fallback heuristic [`sanitize_body_for_error`] applies to a body
/// it cannot structurally parse, and (security finding F5) to every string *value* left visible
/// after [`crate::redact::redact_sensitive`] has run.
fn looks_sensitive(text: &str) -> bool {
    let lower = text.to_lowercase();
    RAW_TEXT_SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Returns `true` if any string *value* (never a key) reachable from `value` looks sensitive per
/// [`looks_sensitive`].
///
/// Deliberately walks only values, not keys: [`crate::redact::redact_sensitive`] already replaced
/// every value found under a sensitive *key* with a fixed, non-sensitive-looking marker (e.g.
/// `"eyJs...[REDACTED:32chars]"`), but it leaves the key name itself — e.g. the literal text
/// `"session_token"` — in the serialized JSON. [`RAW_TEXT_SENSITIVE_MARKERS`] contains several
/// strings that are also completely ordinary JSON key names (`"token"`, `"password"`, `"cookie"`,
/// ...), so scanning the *whole* redacted rendering as one string (keys included) would flag
/// every body that ever carried a sensitive key, even after that key's value was safely redacted
/// — defeating the entire point of doing structured, per-key redaction first. Scanning only
/// values catches the case redaction cannot: a secret sitting under a key that is not on
/// `redact_sensitive`'s own key-pattern list (e.g. a password echoed back under `"error"`).
fn value_contains_sensitive_text(value: &Value) -> bool {
    match value {
        Value::String(text) => looks_sensitive(text),
        Value::Array(items) => items.iter().any(value_contains_sensitive_text),
        Value::Object(map) => map.values().any(value_contains_sensitive_text),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Prepares a response body for embedding in an [`Error::Api`] or [`Error::Authentication`]
/// message (security finding F5): unlike [`truncate_for_error`], this never embeds a credential
/// verbatim.
///
/// - If `body` parses as JSON, it is run through [`crate::redact::redact_sensitive`] first. If
///   [`value_contains_sensitive_text`] still finds a sensitive-looking string *value* anywhere in
///   the redacted tree — i.e. a secret that survived because it was submitted under a key
///   `redact_sensitive` does not recognise as sensitive (the scenario this finding closes: a
///   server echoing a submitted Wi-Fi password back under a plain `"error"` key) — the entire
///   body is replaced with the same fixed marker the raw-text fallback below uses, rather than
///   truncated. Otherwise the redacted rendering is truncated and returned; a typical error body
///   (e.g. `{"meta":{"code":404,"error":"..."}}`) contains no such value, so it comes back
///   unchanged in substance — only its exact byte layout changes (compact re-serialization, and
///   `serde_json::Map`'s default key ordering), which is a deliberate, documented divergence from
///   Python's verbatim-body messages (see this crate's `notes` for this change, recorded for
///   `PARITY.md`).
/// - If `body` does not parse as JSON at all (the only way `status_to_envelope`'s
///   invalid-JSON-on-2xx arm can be reached in the first place, since any syntactically valid
///   JSON on a 2xx succeeds as an `Envelope` instead of becoming an error), a body that is
///   irrecoverably malformed cannot be redacted field-by-field. [`looks_sensitive`] is a
///   conservative substring scan of the *raw* text as a fallback: if it fires, the entire body is
///   replaced with a fixed marker rather than truncated verbatim, since a truncated prefix of a
///   credential-carrying body can still itself carry the credential (see the `login`/`refresh`
///   scenario this guards against). Otherwise the raw text is truncated exactly as before.
///
/// [`truncate_for_error`]'s truncation still applies in every branch that does not suppress the
/// body outright.
pub(crate) fn sanitize_body_for_error(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        let redacted = redact::redact_sensitive(&value);
        if value_contains_sensitive_text(&redacted) {
            return "[response body omitted: contains data that looks sensitive]".to_owned();
        }
        return truncate_for_error(&redacted.to_string());
    }
    if looks_sensitive(body) {
        return "[response body omitted: contains data that looks sensitive]".to_owned();
    }
    truncate_for_error(body)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Error, StorageError, truncate_for_error};

    // ===================== Display strings =====================

    #[test]
    fn authentication_display_is_verbatim_message() {
        let err = Error::Authentication("Not authenticated".to_owned());
        assert_eq!(err.to_string(), "Not authenticated");
    }

    #[test]
    fn rate_limit_display_is_fixed_message() {
        let err = Error::RateLimit { retry_after: None };
        assert_eq!(err.to_string(), "Rate limit exceeded");

        let err = Error::RateLimit {
            retry_after: Some(Duration::from_secs(30)),
        };
        assert_eq!(err.to_string(), "Rate limit exceeded");
    }

    #[test]
    fn api_display_matches_python_format() {
        let err = Error::Api {
            status: 500,
            message: "boom".to_owned(),
            url: None,
        };
        assert_eq!(err.to_string(), "API error 500: boom");
    }

    #[test]
    fn api_display_excludes_url_field_even_when_it_carries_a_token() {
        let err = Error::Api {
            status: 404,
            message: "not found".to_owned(),
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
    fn not_found_display_matches_python_format() {
        let err = Error::NotFound {
            resource_type: "network".to_owned(),
            resource_id: "abc".to_owned(),
        };
        assert_eq!(err.to_string(), "network 'abc' not found");
    }

    #[test]
    fn premium_required_display_matches_python_format() {
        let err = Error::PremiumRequired {
            feature: "This feature".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "This feature requires an Eero Plus subscription"
        );
    }

    #[test]
    fn feature_unavailable_display_matches_python_format() {
        let err = Error::FeatureUnavailable {
            feature: "Thread".to_owned(),
            reason: "not supported on this device".to_owned(),
        };
        assert_eq!(err.to_string(), "Thread is not supported on this device");
    }

    #[test]
    fn validation_display_matches_python_format() {
        let err = Error::Validation {
            field: "field".to_owned(),
            message: "msg".to_owned(),
        };
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

    // ===================== is_auth_error() =====================

    #[test]
    fn is_auth_error_matrix() {
        assert!(Error::Authentication("Not authenticated".to_owned()).is_auth_error());
        assert!(
            Error::Api {
                status: 401,
                message: String::new(),
                url: None,
            }
            .is_auth_error()
        );

        assert!(
            !Error::Api {
                status: 403,
                message: String::new(),
                url: None,
            }
            .is_auth_error()
        );
        assert!(
            !Error::Api {
                status: 404,
                message: String::new(),
                url: None,
            }
            .is_auth_error()
        );
        assert!(!Error::RateLimit { retry_after: None }.is_auth_error());
        assert!(!Error::Timeout.is_auth_error());
        assert!(
            !Error::NotFound {
                resource_type: "network".to_owned(),
                resource_id: "x".to_owned(),
            }
            .is_auth_error()
        );
        assert!(
            !Error::PremiumRequired {
                feature: "x".to_owned(),
            }
            .is_auth_error()
        );
        assert!(
            !Error::FeatureUnavailable {
                feature: "x".to_owned(),
                reason: "y".to_owned(),
            }
            .is_auth_error()
        );
        assert!(
            !Error::Validation {
                field: "x".to_owned(),
                message: "y".to_owned(),
            }
            .is_auth_error()
        );
        assert!(!Error::MissingNetworkId.is_auth_error());
        assert!(!Error::Storage(StorageError::NotFound).is_auth_error());
        assert!(
            !Error::Json(serde_json::from_str::<serde_json::Value>("bad").unwrap_err())
                .is_auth_error()
        );
    }

    // ===================== truncate_for_error =====================

    #[test]
    fn truncate_for_error_passes_short_body_through_unchanged() {
        let body = r#"{"error":"s=secret-token"}"#;
        assert_eq!(truncate_for_error(body), body);
    }

    #[test]
    fn truncate_for_error_truncates_long_ascii_body_and_reports_original_length() {
        let body = "a".repeat(600);
        let truncated = truncate_for_error(&body);
        let expected = format!("{}... [truncated, 600 chars total]", "a".repeat(512));
        assert_eq!(truncated, expected);
    }

    #[test]
    fn truncate_for_error_does_not_panic_on_multibyte_boundary() {
        // 600 multi-byte characters (each "é" is 2 bytes in UTF-8): a naive byte-index slice
        // at index 512 would either panic or split a character in half. Truncating by `char`
        // must land exactly on the 512th character and report 600 as the original length,
        // counted in characters, not bytes.
        let body = "é".repeat(600);
        let truncated = truncate_for_error(&body);
        let expected = format!("{}... [truncated, 600 chars total]", "é".repeat(512));
        assert_eq!(truncated, expected);
        // Sanity: the byte length is double the char length, confirming this body would have
        // panicked under a byte-index slice at an odd offset.
        assert_eq!(body.len(), 1200);
    }

    #[test]
    fn truncate_for_error_boundary_exactly_at_limit_is_unchanged() {
        let body = "a".repeat(512);
        assert_eq!(truncate_for_error(&body), body);
    }

    // ===================== sanitize_body_for_error (finding F5) =====================

    #[test]
    fn sanitize_body_for_error_redacts_a_parseable_json_body() {
        let body = r#"{"meta":{"code":200},"data":{"session_token":"eyJsecret-value"}}"#;
        let sanitized = super::sanitize_body_for_error(body);
        // `redact::redact_sensitive` keeps a short, fixed-length visible prefix (matching
        // Python's `_redact_value`); the point of this test is that the *full* credential is
        // gone, not that every leading character is.
        assert!(!sanitized.contains("eyJsecret-value"));
        assert!(sanitized.contains("REDACTED"));
    }

    #[test]
    fn sanitize_body_for_error_leaves_a_typical_error_body_unaffected_in_substance() {
        let body = r#"{"meta":{"code":404,"error":"resource not found"}}"#;
        let sanitized = super::sanitize_body_for_error(body);
        let value: serde_json::Value = serde_json::from_str(&sanitized).expect("still valid json");
        assert_eq!(value["meta"]["code"], 404);
        assert_eq!(value["meta"]["error"], "resource not found");
    }

    #[test]
    fn sanitize_body_for_error_omits_an_unparseable_body_that_looks_sensitive() {
        // Deliberately truncated/malformed, exactly like a cut-off `login/refresh` response —
        // the scenario security finding F5 exists to close.
        let body = concat!(
            r#"{"meta":{"code":200},"data":{"session_token":"eyJsecret-session-value","#,
            r#""refresh_token":"eyJsecret-refresh-value"#,
        );
        assert!(serde_json::from_str::<serde_json::Value>(body).is_err());
        let sanitized = super::sanitize_body_for_error(body);
        assert!(!sanitized.contains("eyJsecret"));
        assert!(!sanitized.contains("session-value"));
        assert!(!sanitized.contains("refresh-value"));
    }

    #[test]
    fn sanitize_body_for_error_truncates_an_unparseable_body_with_nothing_sensitive() {
        let body = "no such account";
        assert_eq!(super::sanitize_body_for_error(body), body);
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
}
