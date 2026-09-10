//! The session credential value and its on-disk storage representation.
//!
//! Ported from `AuthCredentials` (`eero-api`'s `src/eero/api/auth_storage.py:23-91`) and the
//! naive-local, `+30`-day expiry convention applied at `login/verify`, `refresh_session()` and
//! `set_session_token()` (`src/eero/api/auth.py:180-182,323-325,411-413`). See
//! `rusteero-context/claude/tasks/briefs/const.md` §2 and
//! `rusteero-context/claude/tasks/briefs/auth.md` for the full behaviour briefs this module
//! implements, and the port plan §3.4/§3.5 (decisions D-5, D-13, D-17) for the design
//! rationale.
//!
//! [`Session`] deliberately does **not** implement `Serialize`/`Deserialize` directly: the
//! wire-compatible shape (`{"session_id", "refresh_token", "session_expiry"}`, shared with
//! `eero-api`'s cookie file and keyring blob per decision D-5) is expressed by the
//! [`StoredSession`] type instead, which has no public constructor and is reached only through
//! [`Session::to_json`] / [`Session::from_json`]. This keeps the on-disk format defined in
//! exactly one place, so Phase 2's `FileStore` and `KeyringStore` (`src/storage/*`) can both
//! build on it without duplicating the (de)serialization logic.

use std::fmt;
use std::time::{Duration, SystemTime};

use jiff::Zoned;
use jiff::civil::DateTime as CivilDateTime;
use jiff::tz::TimeZone;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::consts::SESSION_LIFETIME_DAYS;
use crate::error::{Error, StorageError};

/// A session credential: the long-lived token sent as the `s` cookie on every authenticated
/// request, plus the (practically dead, per the port plan §7.2/D-16) refresh token, and a
/// client-fabricated expiry.
///
/// Mirrors `AuthCredentials` (`auth_storage.py:23-38`) field-for-field, with two differences
/// forced by the Rust type system:
///
/// - The token is a [`secrecy::SecretString`], not a bare `String` — it can never be printed,
///   logged, or otherwise leaked through `Debug`/`Display` without going through
///   [`secrecy::ExposeSecret`] explicitly.
/// - There is no "absent token" state distinct from "empty token": Python's `session_id:
///   Optional[str] = None` is represented here as an *empty* `SecretString`, mirrored by
///   [`Session::empty`]. [`Session::is_valid`] treats both the same way Python's
///   `has_valid_session()` does — by truthiness (`auth_storage.py:46-48`), i.e. an empty
///   string is never a valid token.
#[derive(Clone)]
pub struct Session {
    token: SecretString,
    refresh_token: Option<SecretString>,
    expiry: Option<SystemTime>,
}

impl Session {
    /// Builds a session from a pre-obtained token, fabricating an expiry `30` days from now.
    ///
    /// Mirrors `set_session_token()` (`auth.py:390-418`) minus the validation and the
    /// storage/cookie side effects, which belong to the (not-yet-ported) `AuthApi`/`Client`
    /// layer, not to this value type. The expiry uses [`crate::consts::SESSION_LIFETIME_DAYS`]
    /// and is truncated to whole seconds *before* the offset is added, matching Python's
    /// `datetime.now().replace(microsecond=0) + timedelta(days=30)`
    /// (`auth.py:180-182,323-325,411-413`).
    #[must_use]
    pub fn from_token(token: impl Into<String>) -> Self {
        Session {
            token: SecretString::from(token.into()),
            refresh_token: None,
            expiry: Some(fabricated_expiry()),
        }
    }

    /// Builds a session from the token held in the environment variable `var_name`.
    ///
    /// Added for the Rust port (port plan §3.4): there is no equivalent in `eero-api`, which
    /// has no env-var support at all. This is a one-liner over [`Session::from_token`] for
    /// headless token injection. Returns [`Error::Validation`] (mirroring the shape of
    /// `EeroValidationException`, `exceptions.py:81-86`, and the exact wording used by
    /// `set_session_token`'s own check, `auth.py:407-408`) when the variable is unset or set to
    /// an empty string.
    pub fn from_env(var_name: &str) -> Result<Self, Error> {
        // `std::env::var` is the only fallible part of this function; delegating to a helper
        // that takes an already-read `Option<String>` keeps the validation logic unit-testable
        // without ever mutating real process environment variables. Mutating them
        // (`std::env::set_var`) has required an `unsafe` block since Rust 1.83, which this
        // crate's `#![forbid(unsafe_code)]` disallows even inside `#[cfg(test)]`.
        Self::from_env_value(var_name, std::env::var(var_name).ok())
    }

    /// The testable half of [`Session::from_env`]: validates an already-read environment value.
    fn from_env_value(var_name: &str, value: Option<String>) -> Result<Self, Error> {
        let token = value.ok_or_else(|| Error::Validation {
            field: var_name.to_owned(),
            message: "environment variable not set".to_owned(),
        })?;
        if token.is_empty() {
            return Err(Error::Validation {
                field: var_name.to_owned(),
                message: "must be a non-empty string".to_owned(),
            });
        }
        Ok(Self::from_token(token))
    }

    /// An empty, "logged out" session: no token, no refresh token, no expiry.
    ///
    /// Mirrors the all-`None` default of `AuthCredentials()` (`auth_storage.py:36-38`). This is
    /// the value every `CredentialStore::load` returns when nothing has been stored yet, and
    /// what a fresh `login()` clears everything to before the network call
    /// (`clear_all()`, `auth.py:101`).
    #[must_use]
    pub fn empty() -> Self {
        Session {
            token: SecretString::from(String::new()),
            refresh_token: None,
            expiry: None,
        }
    }

    /// Whether this session has a non-empty token that has not expired.
    ///
    /// Mirrors `has_valid_session()` exactly: `bool(self.session_id and not
    /// self.is_session_expired())` (`auth_storage.py:46-48`), where `is_session_expired()`
    /// treats a missing expiry as expired and otherwise compares with a **strict** `>`
    /// (`auth_storage.py:40-44`) — so a session is valid iff the token is non-empty *and* an
    /// expiry is set *and* `now <= expiry`.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.token.expose_secret().is_empty()
            && self
                .expiry
                .is_some_and(|expiry| SystemTime::now() <= expiry)
    }

    /// The session token, exposed only as a [`secrecy::SecretString`].
    ///
    /// There is deliberately no accessor that returns a bare `&str`; the only place the raw
    /// token string is ever exposed as one is a crate-private helper that builds the
    /// `Cookie: s=<token>` header, kept private so that exposure stays auditable to that single
    /// call site.
    #[must_use]
    pub fn token(&self) -> &SecretString {
        &self.token
    }

    /// The refresh token, if one was ever assigned.
    ///
    /// Ported from `AuthCredentials.refresh_token` (`auth_storage.py:37`). Per the port plan
    /// §7.2/D-16 and the `auth.py` behaviour brief, `eero-api` never receives a refresh token
    /// at login/verify, so in practice this is `None` for the lifetime of a normal session.
    #[must_use]
    pub fn refresh_token(&self) -> Option<&SecretString> {
        self.refresh_token.as_ref()
    }

    /// The client-fabricated expiry, if one has been set.
    ///
    /// Ported from `AuthCredentials.session_expiry` (`auth_storage.py:38`). `None` before
    /// verification succeeds (Python's comment: "`session_expiry` is None until verified",
    /// `auth.py:133`) and for [`Session::empty`].
    #[must_use]
    pub fn expiry(&self) -> Option<SystemTime> {
        self.expiry
    }

    /// Exposes the raw token string.
    ///
    /// This is the **only** place in the crate allowed to turn the token back into a bare
    /// `&str`, and exists for exactly one reason: building the `Cookie: s=<token>` header
    /// (`api/base.py:148-152`) has no use for a `SecretString` — `reqwest` needs a `&str`/
    /// `String` to build the header value. Every other consumer of a session must go through
    /// [`Session::token`] and `secrecy`'s `ExposeSecret` trait explicitly, keeping accidental
    /// exposure auditable to this one call site.
    // `transport.rs` (owned by a concurrent task, not yet implemented) is the intended non-test
    // caller, for building the `Cookie: s=<token>` header; until it lands this method is only
    // exercised by this module's and `storage::memory`'s tests, which would otherwise make a
    // plain `cargo clippy` flag it as dead code.
    #[allow(dead_code)]
    pub(crate) fn expose_token(&self) -> &str {
        self.token.expose_secret()
    }

    /// Converts to the on-disk representation, ready to serialize.
    ///
    /// Mirrors `AuthCredentials.to_dict()` (`auth_storage.py:50-56`): an empty token becomes
    /// `session_id: null` (Python never writes an empty string — `session_id` is `None` in
    /// every code path that does not hold a real token), and the expiry is rendered as a naive
    /// local ISO 8601 string with second precision (no fractional seconds, no offset) via
    /// [`format_naive_local`].
    pub(crate) fn to_stored(&self) -> StoredSession {
        StoredSession {
            session_id: non_empty(self.token.expose_secret()),
            refresh_token: self
                .refresh_token
                .as_ref()
                .map(|rt| rt.expose_secret().to_owned()),
            session_expiry: self.expiry.and_then(format_naive_local),
            user_token: None,
        }
    }

    /// Converts from the on-disk representation, tolerating malformed input.
    ///
    /// Mirrors `AuthCredentials.from_dict()` (`auth_storage.py:58-80`) exactly:
    ///
    /// - The legacy `user_token` key is accepted as a fallback for `session_id`, selected by
    ///   truthiness (an empty string does *not* count), matching
    ///   `data.get("session_id") or data.get("user_token")` (`auth_storage.py:74`).
    /// - `refresh_token` has no legacy alias and is read as-is (`auth_storage.py:37,54,78`).
    /// - A missing, `null`, or empty `session_expiry` becomes `None` silently. An expiry that
    ///   is present but fails to parse also becomes `None`, but logs a `WARNING` with the exact
    ///   fixed message Python uses (`"Invalid session_expiry date format in stored
    ///   credentials"`, `auth_storage.py:71`, deliberately not interpolating the bad string).
    ///   Either way, `from_stored` never fails and never panics — a corrupt or legacy on-disk
    ///   blob degrades to *some* [`Session`], never an error, matching Python's tolerance here.
    pub(crate) fn from_stored(stored: StoredSession) -> Self {
        let session_id = stored
            .session_id
            .filter(|s| !s.is_empty())
            .or_else(|| stored.user_token.filter(|s| !s.is_empty()));
        let token =
            session_id.map_or_else(|| SecretString::from(String::new()), SecretString::from);
        let refresh_token = stored.refresh_token.map(SecretString::from);
        let expiry = match stored.session_expiry.filter(|s| !s.is_empty()) {
            Some(raw) => parse_naive_local(&raw).or_else(|| {
                tracing::warn!("Invalid session_expiry date format in stored credentials");
                None
            }),
            None => None,
        };
        Session {
            token,
            refresh_token,
            expiry,
        }
    }

    /// Serializes this session to the exact `eero-api`-compatible JSON shape.
    ///
    /// This is the one place the on-disk format is produced; Phase 2's `FileStore` and
    /// `KeyringStore` (`src/storage/*`) call this rather than reimplementing serialization.
    pub fn to_json(&self) -> Result<String, StorageError> {
        let json = serde_json::to_string(&self.to_stored())?;
        Ok(json)
    }

    /// Parses a session from the `eero-api`-compatible JSON shape.
    ///
    /// Tolerates malformed or legacy input the same way Python's `AuthCredentials.from_dict()`
    /// does: the legacy `user_token` key is accepted as a fallback for a missing or empty
    /// `session_id`, and a missing, empty, or unparseable `session_expiry` degrades the session
    /// to "no expiry" rather than failing. This function never fails on the *content* of a
    /// syntactically valid JSON object, only on JSON that does not parse at all.
    pub fn from_json(json: &str) -> Result<Self, StorageError> {
        let stored: StoredSession = serde_json::from_str(json)?;
        Ok(Self::from_stored(stored))
    }
}

impl fmt::Debug for Session {
    /// Redacts the token and refresh token unconditionally.
    ///
    /// `secrecy::SecretString`'s own `Debug` impl already renders as `SecretBox<str>([REDACTED])`,
    /// but this crate implements `Debug` for `Session` explicitly rather than deriving it, so
    /// that the redaction is guaranteed by this type's own contract and does not silently
    /// change if `Session` ever gains a field whose `Debug` impl is not redacted by default.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expiry", &self.expiry)
            .finish()
    }
}

impl fmt::Display for Session {
    /// Renders only whether the session is currently valid; never the token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session {{ valid: {} }}", self.is_valid())
    }
}

/// Returns `now + `[`SESSION_LIFETIME_DAYS`]` days`, with `now` truncated to whole seconds.
///
/// The truncation happens *before* the offset is added, matching Python's
/// `datetime.now().replace(microsecond=0) + timedelta(days=30)` (`auth.py:180-182`) rather than
/// truncating only at serialization time — the two only differ by sub-second noise, but this
/// keeps the in-memory expiry consistent with what will eventually be persisted.
fn fabricated_expiry() -> SystemTime {
    // `SESSION_LIFETIME_DAYS` is a positive `i64` constant (30); `unwrap_or(30)` is unreachable
    // in practice and chosen over `.expect(...)` purely so this function has no panicking path
    // to document.
    let days = u64::try_from(SESSION_LIFETIME_DAYS).unwrap_or(30);
    now_truncated_to_secs() + Duration::from_secs(days * 24 * 60 * 60)
}

/// The current time, truncated to whole seconds (sub-second component dropped, not rounded).
fn now_truncated_to_secs() -> SystemTime {
    // `unwrap_or_default()` only matters if the system clock is set before the Unix epoch; it
    // yields `Duration::ZERO` rather than panicking, which is a harmless (if wrong) answer in
    // that unreachable-in-practice case.
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

/// `Some(s.to_owned())` unless `s` is empty, in which case `None`.
fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}

/// Formats `time` as a naive local ISO 8601 string with second precision: exactly
/// `YYYY-MM-DDTHH:MM:SS`, no fractional seconds, no UTC offset, no timezone designator.
///
/// Uses `jiff` per decision D-17. `{:.0}` on a `jiff::civil::DateTime` always drops the
/// fractional-seconds component regardless of its value, which is what makes the output shape
/// exact rather than merely "usually so" (the underlying `SystemTime` is expected to already be
/// truncated to whole seconds by [`fabricated_expiry`], but this function does not rely on
/// that). Returns `None` only if `time` falls outside the range `jiff` can represent — not
/// reachable for any expiry this crate computes, but handled without panicking regardless.
fn format_naive_local(time: SystemTime) -> Option<String> {
    let zoned = Zoned::try_from(time).ok()?;
    Some(format!("{:.0}", zoned.datetime()))
}

/// Parses a naive local ISO 8601 string (as produced by [`format_naive_local`]) back into a
/// [`SystemTime`], interpreting it in the system's local timezone.
///
/// Returns `None` on any parse failure instead of propagating an error, so that callers (in
/// particular [`Session::from_stored`]) can apply Python's exact tolerance: a bad
/// `session_expiry` degrades the session to "not expiring", it never fails the whole load.
fn parse_naive_local(s: &str) -> Option<SystemTime> {
    let naive: CivilDateTime = s.parse().ok()?;
    let zoned = naive.to_zoned(TimeZone::system()).ok()?;
    Some(SystemTime::from(zoned))
}

/// The on-disk / in-keyring JSON representation of a [`Session`].
///
/// This is a **public wire contract** (decision D-5): its JSON shape — `session_id`,
/// `refresh_token`, `session_expiry`, plus the legacy read-only `user_token` alias — is shared
/// byte-for-byte with the Python `eero-api` library's `AuthCredentials`
/// (`eero-api` `src/eero/api/auth_storage.py:23-91`), so a cookie file or keyring entry written
/// by either implementation can be read back by the other. Field names and order are therefore
/// load-bearing and must not change independently of a coordinated update to both libraries.
///
/// `user_token` is a read-only compatibility field: it is never emitted when serializing
/// (`#[serde(skip_serializing)]`) and is only consulted as a fallback source for `session_id`
/// when deserializing, mirroring the legacy-cookie-file migration path at
/// `auth_storage.py:74`. This type has no public constructor — values of it are produced and
/// consumed only through [`Session::to_json`] and [`Session::from_json`]; it is public so that
/// Phase 2's `FileStore` and `KeyringStore` (`src/storage/*`) can name the wire format precisely
/// in their own documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    session_id: Option<String>,
    refresh_token: Option<String>,
    session_expiry: Option<String>,
    #[serde(default, skip_serializing)]
    user_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use secrecy::{ExposeSecret, SecretString};

    use super::{Error, Session};

    // ===================== Wire-format round trip (D-5) =====================

    #[test]
    fn round_trip_matches_python_wire_shape() {
        let json = r#"{"session_id":"tok-123","refresh_token":null,"session_expiry":"2026-10-10T14:32:07"}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "tok-123");
        assert!(session.refresh_token().is_none());
        assert!(session.expiry().is_some());

        let round_tripped = session.to_json().expect("session serializes");
        assert_eq!(round_tripped, json);
    }

    #[test]
    fn round_trip_preserves_a_present_refresh_token() {
        let json = r#"{"session_id":"tok-123","refresh_token":"rt-456","session_expiry":null}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert_eq!(
            session
                .refresh_token()
                .expect("refresh token present")
                .expose_secret(),
            "rt-456"
        );
        assert!(session.expiry().is_none());
        assert_eq!(session.to_json().expect("serializes"), json);
    }

    #[test]
    fn empty_session_round_trips_to_all_null_fields() {
        let session = Session::empty();
        let json = session.to_json().expect("serializes");
        assert_eq!(
            json,
            r#"{"session_id":null,"refresh_token":null,"session_expiry":null}"#
        );
        let restored = Session::from_json(&json).expect("parses");
        assert!(!restored.is_valid());
        assert_eq!(restored.expose_token(), "");
    }

    // ===================== Legacy `user_token` alias =====================

    #[test]
    fn legacy_user_token_key_is_accepted_as_session_id_alias() {
        let json = r#"{"user_token":"old_token_123","session_id":null}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "old_token_123");
    }

    #[test]
    fn empty_session_id_still_falls_back_to_user_token() {
        // Mirrors Python's truthiness-based `or`: an empty string is falsy, so it does not win
        // over a present `user_token` (`auth_storage.py:74`).
        let json = r#"{"session_id":"","user_token":"old_token_123"}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "old_token_123");
    }

    #[test]
    fn present_session_id_wins_over_user_token() {
        let json = r#"{"session_id":"new-token","user_token":"old-token"}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "new-token");
    }

    // ===================== Naive ISO 8601 shape =====================

    #[test]
    fn session_expiry_serializes_as_naive_nineteen_char_iso8601() {
        let session = Session::from_token("tok");
        let json = session.to_json().expect("serializes");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let expiry = value["session_expiry"].as_str().expect("string expiry");

        assert_eq!(expiry.len(), 19, "expected exactly 19 characters: {expiry}");
        let bytes = expiry.as_bytes();
        assert_eq!(bytes[4], b'-');
        assert_eq!(bytes[7], b'-');
        assert_eq!(bytes[10], b'T');
        assert_eq!(bytes[13], b':');
        assert_eq!(bytes[16], b':');
        assert!(expiry[0..4].bytes().all(|b| b.is_ascii_digit()));
        assert!(expiry[5..7].bytes().all(|b| b.is_ascii_digit()));
        assert!(expiry[8..10].bytes().all(|b| b.is_ascii_digit()));
        assert!(expiry[11..13].bytes().all(|b| b.is_ascii_digit()));
        assert!(expiry[14..16].bytes().all(|b| b.is_ascii_digit()));
        assert!(expiry[17..19].bytes().all(|b| b.is_ascii_digit()));
        assert!(!expiry.contains('.'), "must have no fractional seconds");
        assert!(!expiry.contains('Z'), "must have no UTC designator");
        assert!(!expiry.contains('+'), "must have no UTC offset");
    }

    #[test]
    fn unparseable_session_expiry_becomes_none_not_an_error() {
        let json = r#"{"session_id":"tok","refresh_token":null,"session_expiry":"not-a-date"}"#;
        let session = Session::from_json(json).expect("load never fails on a bad expiry");
        assert!(session.expiry().is_none());
        assert!(!session.is_valid());
    }

    #[test]
    fn missing_session_expiry_becomes_none() {
        let json = r#"{"session_id":"tok"}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert!(session.expiry().is_none());
    }

    // ===================== is_valid() / has_valid_session() parity =====================

    #[test]
    fn empty_session_is_never_valid() {
        assert!(!Session::empty().is_valid());
    }

    #[test]
    fn from_token_produces_a_valid_session() {
        let session = Session::from_token("tok");
        assert!(session.is_valid());
    }

    #[test]
    fn is_valid_false_when_expiry_in_the_past() {
        let session = Session {
            token: SecretString::from("tok"),
            refresh_token: None,
            expiry: Some(SystemTime::now() - Duration::from_secs(3600)),
        };
        assert!(!session.is_valid());
    }

    #[test]
    fn is_valid_false_when_token_is_empty_even_with_future_expiry() {
        let session = Session {
            token: SecretString::from(String::new()),
            refresh_token: None,
            expiry: Some(SystemTime::now() + Duration::from_secs(3600)),
        };
        assert!(!session.is_valid());
    }

    #[test]
    fn is_valid_false_when_expiry_is_none() {
        let session = Session {
            token: SecretString::from("tok"),
            refresh_token: None,
            expiry: None,
        };
        assert!(!session.is_valid());
    }

    // ===================== Debug / Display redaction =====================

    #[test]
    fn debug_never_prints_token_or_refresh_token() {
        let mut session = Session::from_token("super-secret-token");
        session.refresh_token = Some(SecretString::from("super-secret-refresh"));
        let debug = format!("{session:?}");
        assert!(!debug.contains("super-secret-token"));
        assert!(!debug.contains("super-secret-refresh"));
    }

    #[test]
    fn display_never_prints_token_or_refresh_token() {
        let session = Session::from_token("super-secret-token");
        let display = format!("{session}");
        assert!(!display.contains("super-secret-token"));
    }

    // ===================== from_env =====================

    #[test]
    fn from_env_value_missing_is_validation_error() {
        let err = Session::from_env_value("RUSTEERO_TEST_TOKEN", None).unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
    }

    #[test]
    fn from_env_value_empty_is_validation_error() {
        let err = Session::from_env_value("RUSTEERO_TEST_TOKEN", Some(String::new())).unwrap_err();
        assert!(matches!(err, Error::Validation { .. }));
    }

    #[test]
    fn from_env_value_success_builds_a_valid_session() {
        let session = Session::from_env_value("RUSTEERO_TEST_TOKEN", Some("tok".to_owned()))
            .expect("non-empty value is accepted");
        assert!(session.is_valid());
        assert_eq!(session.expose_token(), "tok");
    }

    #[test]
    fn from_env_missing_variable_returns_validation_error() {
        // No env var is ever mutated (see the comment on `Session::from_env`); this variable
        // name is chosen to be vanishingly unlikely to be set in any real environment.
        let err = Session::from_env("RUSTEERO_SESSION_STORE_TEST_VAR_DOES_NOT_EXIST")
            .expect_err("variable is not set");
        assert!(
            matches!(err, Error::Validation { field, .. } if field == "RUSTEERO_SESSION_STORE_TEST_VAR_DOES_NOT_EXIST")
        );
    }
}
