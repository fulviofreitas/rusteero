//! The session credential value and its on-disk storage representation.
//!
//! Ported from `AuthCredentials` (`eero-api`'s `src/eero/api/auth_storage.py:23-91`) at
//! `v8.0.4`. `v8.0.4` has no client-side session expiry anywhere — `AuthCredentials` has exactly
//! one field, `session_id` — and no refresh token either (`auth.py`'s own docstring: "There is no
//! client-side session expiry -- the server is the sole authority on session validity, signalled
//! via 401 responses."). [`Session`] mirrors that: a session is nothing more than a token.
//!
//! [`Session`] deliberately does **not** implement `Serialize`/`Deserialize` directly: the
//! wire-compatible shape (`{"session_id", "schema_version"}`, shared with `eero-api`'s cookie
//! file and keyring blob per decision D-5) is expressed by the [`StoredSession`] type instead,
//! which has no public constructor and is reached only through `Session::to_json` /
//! [`Session::from_json`]. This keeps the on-disk format defined in exactly one place, so
//! `FileStore` and `KeyringStore` (`src/storage/*`) can both build on it without duplicating the
//! (de)serialization logic.

use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::consts::CREDENTIAL_SCHEMA_VERSION;
use crate::error::{Error, StorageError};

/// A session credential: the long-lived token sent as the `X-User-Token` header (and, unless
/// disabled, a legacy `s=<token>` cookie) on every authenticated request.
///
/// Mirrors `AuthCredentials` (`auth_storage.py:23-38`) at `v8.0.4` field-for-field: one field,
/// `session_id`, represented here as a [`secrecy::SecretString`] rather than a bare `String` so
/// it can never be printed, logged, or otherwise leaked through `Debug`/`Display` without going
/// through [`secrecy::ExposeSecret`] explicitly. There is no "absent token" state distinct from
/// "empty token": Python's `session_id: Optional[str] = None` is represented here as an *empty*
/// `SecretString`, mirrored by [`Session::empty`]. [`Session::is_valid`] treats both the same way
/// Python's `has_valid_session()` does — by truthiness (`auth_storage.py:46-48`), i.e. an empty
/// string is never a valid token. There is no client-side expiry concept at all (`auth.py`'s own
/// docstring, quoted above) — the server is the sole authority on session validity, signalled via
/// `401` responses.
#[derive(Clone)]
pub struct Session {
    token: SecretString,
}

impl Session {
    /// Builds a session from a pre-obtained token, with no validation.
    ///
    /// This is intentionally infallible — the one Rust-shape divergence the port brief calls out
    /// explicitly: token *shape* validation (non-empty, printable ASCII with no CR/LF, since the
    /// token becomes the literal `X-User-Token` header value) happens only at the call sites that
    /// already return a `Result`: [`crate::transport::Transport::set_session`],
    /// [`crate::auth::AuthApi::set_session_token`], and [`Session::from_env`]. A token obtained
    /// from a server response (the login/verify handshake) is trusted as-is, exactly like
    /// `AuthCredentials.session_id` is at `auth.py:198-201,247-250`.
    #[must_use]
    pub fn from_token(token: impl Into<String>) -> Self {
        Session {
            token: SecretString::from(token.into()),
        }
    }

    /// Builds a session from the token held in the environment variable `var_name`, validating
    /// its shape the same way [`crate::auth::AuthApi::set_session_token`] does.
    ///
    /// Added for the Rust port: there is no equivalent in `eero-api`, which has no env-var
    /// support at all. This is a one-liner over [`Session::from_token`] for headless token
    /// injection.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] (mirroring the shape of `EeroValidationException`,
    /// `exceptions.py:81-86`) when the variable is unset, empty, or contains a byte outside the
    /// printable-ASCII range (or a CR/LF) — see `validate_token_shape`.
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
        let token =
            value.ok_or_else(|| Error::validation(var_name, "environment variable not set"))?;
        validate_token_shape(var_name, &token)?;
        Ok(Self::from_token(token))
    }

    /// An empty, "logged out" session: no token.
    ///
    /// Mirrors the all-`None` default of `AuthCredentials()` (`auth_storage.py:36-38`). This is
    /// the value every `CredentialStore::load` returns when nothing has been stored yet.
    #[must_use]
    pub fn empty() -> Self {
        Session {
            token: SecretString::from(String::new()),
        }
    }

    /// Whether this session has a non-empty token.
    ///
    /// Mirrors `has_valid_session()`: `bool(self.session_id)` (`auth_storage.py:46-48`) — a pure
    /// token-presence check, with no expiry of any kind (`v8.0.4` has none, see this type's own
    /// docs).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.token.expose_secret().is_empty()
    }

    /// The session token, exposed only as a [`secrecy::SecretString`].
    ///
    /// There is deliberately no accessor that returns a bare `&str`; the only places the raw
    /// token string is ever exposed as one are two crate-private helpers used to build request
    /// headers, kept private so that exposure stays auditable to those call sites.
    #[must_use]
    pub fn token(&self) -> &SecretString {
        &self.token
    }

    /// Exposes the raw token string.
    ///
    /// One of the crate's two designated places allowed to turn a token back into a bare `&str`
    /// (the other is [`Session::expose_secret_token`]); both exist for exactly one reason:
    /// building the `X-User-Token`/`Cookie: s=<token>` headers has no use for a `SecretString` —
    /// `reqwest` needs a `&str`/`String` to build a header value. This one takes `&self`, for a
    /// caller that already holds a full `Session`.
    // Exercised only by this module's, `auth::mod`'s, and `storage::*`'s tests — inspecting a
    // session's own token value in a test never needs to go through the production header-
    // building call site — which would otherwise make a plain `cargo clippy` (the lib target,
    // built without `cfg(test)`) flag this as dead code.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn expose_token(&self) -> &str {
        self.token.expose_secret()
    }

    /// Exposes a token's raw string value without requiring a `Session`.
    ///
    /// Sibling of [`Session::expose_token`] for the crate's production exposure call sites that
    /// cannot hold a full `Session`: `Transport`'s credential-placement helper (an authenticated
    /// session's own token) and `auth::flow::PendingLogin`'s login token (which predates any
    /// `Session`). Every other consumer of a token must go through [`Session::token`] and
    /// `secrecy`'s `ExposeSecret` trait explicitly.
    #[must_use]
    pub(crate) fn expose_secret_token(token: &SecretString) -> &str {
        token.expose_secret()
    }

    /// Converts to the on-disk representation, ready to serialize.
    ///
    /// Mirrors `AuthCredentials.to_dict()` (`auth_storage.py:50-56`) at `v8.0.4`: an empty token
    /// becomes `session_id: null` (Python never writes an empty string — `session_id` is `None`
    /// in every code path that does not hold a real token), and `schema_version` is always
    /// stamped with the current [`CREDENTIAL_SCHEMA_VERSION`].
    pub(crate) fn to_stored(&self) -> StoredSession {
        StoredSession {
            session_id: non_empty(self.token.expose_secret()),
            schema_version: Some(CREDENTIAL_SCHEMA_VERSION),
            user_token: None,
        }
    }

    /// Converts from the on-disk representation, tolerating malformed or legacy input.
    ///
    /// Mirrors `AuthCredentials.from_dict()` (`auth_storage.py:58-80`) at `v8.0.4`. A record with
    /// no `schema_version` key predates the marker and is treated as a **legacy migration**: the
    /// returned `bool` is `true`, and the session id is resolved as `session_id.or(user_token)`
    /// by truthiness — an empty `session_id` falls through to a present `user_token`, matching
    /// `data.get("session_id") or data.get("user_token")` (`auth_storage.py:74`) — while every
    /// other legacy field (a `refresh_token`, a `session_expiry`) is silently dropped, since
    /// `v8.0.4`'s `AuthCredentials` has no field for either. A record that already carries
    /// `schema_version` is current-shape and never treated as a migration, even if
    /// `session_id` happens to be empty. This never fails and never panics.
    pub(crate) fn from_stored(stored: StoredSession) -> (Self, bool) {
        let migrated = stored.schema_version.is_none();
        let session_id = stored
            .session_id
            .filter(|s| !s.is_empty())
            .or_else(|| stored.user_token.filter(|s| !s.is_empty()));
        let token =
            session_id.map_or_else(|| SecretString::from(String::new()), SecretString::from);
        (Session { token }, migrated)
    }

    /// Serializes this session to the exact `eero-api`-compatible JSON shape:
    /// `{"session_id": <str|null>, "schema_version": 2}`.
    ///
    /// This is the one place the on-disk format is produced; `FileStore` and `KeyringStore`
    /// (`src/storage/*`) call this rather than reimplementing serialization — both live inside
    /// this crate, so `pub(crate)` is all the visibility they need.
    ///
    /// `pub(crate)`, not `pub`: the on-disk JSON shape embeds the raw token string in plain text
    /// (`session_id`). Keeping this crate-private is what makes [`Session::token`]'s doc claim —
    /// that a crate-private helper is the *only* place the raw token string is ever exposed as a
    /// bare string — actually true; a `pub` `to_json` would be a second, external exposure path
    /// that claim did not account for.
    pub(crate) fn to_json(&self) -> Result<String, StorageError> {
        let json = serde_json::to_string(&self.to_stored())?;
        Ok(json)
    }

    /// Parses a session from the `eero-api`-compatible JSON shape, tolerating legacy input the
    /// same way [`Session::from_stored`] does.
    ///
    /// This function never fails on the *content* of a syntactically valid JSON object, only on
    /// JSON that does not parse at all. The returned `bool` mirrors [`Session::from_stored`]'s:
    /// `true` iff the parsed record had no `schema_version` key (a legacy record).
    pub(crate) fn from_json_migrating(json: &str) -> Result<(Self, bool), StorageError> {
        let stored: StoredSession = serde_json::from_str(json)?;
        Ok(Self::from_stored(stored))
    }

    /// Parses a session from the `eero-api`-compatible JSON shape.
    ///
    /// A thin wrapper over `Session::from_json_migrating` that discards the migration flag, for
    /// callers (tests, and any future non-storage consumer) that only care about the resulting
    /// session. `FileStore`/`KeyringStore` use `Session::from_json_migrating` directly so they
    /// can react to a legacy record with a re-save-and-read-back migration.
    pub fn from_json(json: &str) -> Result<Self, StorageError> {
        Self::from_json_migrating(json).map(|(session, _migrated)| session)
    }
}

impl fmt::Debug for Session {
    /// Redacts the token unconditionally.
    ///
    /// `secrecy::SecretString`'s own `Debug` impl already renders as `SecretBox<str>([REDACTED])`,
    /// but this crate implements `Debug` for `Session` explicitly rather than deriving it, so
    /// that the redaction is guaranteed by this type's own contract and does not silently change
    /// if `Session` ever gains a field whose `Debug` impl is not redacted by default.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for Session {
    /// Renders only whether the session is currently valid; never the token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session {{ valid: {} }}", self.is_valid())
    }
}

/// Validates that `token` is safe to use as a session token: non-empty, and printable ASCII with
/// no CR/LF (since the token becomes the literal `X-User-Token` header value verbatim).
///
/// Ported from `set_session_token`'s own check (`auth.py:407-408,530-532`), which reuses
/// `_validate_header_value`'s printable-ASCII/no-CR/LF rule (`base.py:108-121`,
/// `_HEADER_VALUE_RE = re.compile(r"^[\x20-\x7E]*$")`) since the token becomes a header value.
/// Shared by [`Session::from_env`], [`crate::auth::AuthApi::set_session_token`], and
/// [`crate::transport::Transport::set_session`] — the three call sites the port brief identifies
/// as the ones with a `Result` on hand to reject a malformed token.
///
/// # Errors
///
/// Returns [`Error::Validation`] with the exact message `"must be a non-empty string"`
/// (`exceptions.py`'s wording, reused verbatim) if `token` is empty, or
/// `"must contain only printable ASCII characters with no CR or LF"` if it contains any byte
/// outside `0x20..=0x7E`.
pub(crate) fn validate_token_shape(field: &str, token: &str) -> Result<(), Error> {
    if token.is_empty() {
        return Err(Error::validation(field, "must be a non-empty string"));
    }
    if !token.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(Error::validation(
            field,
            "must contain only printable ASCII characters with no CR or LF",
        ));
    }
    Ok(())
}

/// `Some(s.to_owned())` unless `s` is empty, in which case `None`.
fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
}

/// The on-disk / in-keyring JSON representation of a [`Session`].
///
/// This is a **public wire contract** (decision D-5): its JSON shape — `session_id` plus
/// `schema_version` at `v8.0.4` — is shared byte-for-byte with the Python `eero-api` library's
/// `AuthCredentials` (`eero-api` `src/eero/api/auth_storage.py:23-91`), so a cookie file or
/// keyring entry written by either implementation can be read back by the other. Field names and
/// order are therefore load-bearing and must not change independently of a coordinated update to
/// both libraries.
///
/// `user_token` is a read-only compatibility field: it is never emitted when serializing
/// (`#[serde(skip_serializing)]`) and is only consulted as a fallback source for `session_id`
/// when deserializing a *legacy* record (one with no `schema_version`), mirroring the
/// legacy-cookie-file migration path at `auth_storage.py:74`. This type has no public
/// constructor — values of it are produced and consumed only through `Session::to_json`
/// (crate-private) and [`Session::from_json`]/`Session::from_json_migrating`; it is public so
/// that `FileStore` and `KeyringStore` (`src/storage/*`) can name the wire format precisely in
/// their own documentation.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredSession {
    session_id: Option<String>,
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default, skip_serializing)]
    user_token: Option<String>,
}

impl fmt::Debug for StoredSession {
    /// Redacts `session_id` and `user_token` unconditionally; `schema_version` is not a secret
    /// and is rendered as-is.
    ///
    /// Written by hand, like [`Session`]'s own `Debug` impl, rather than derived, so the
    /// redaction is guaranteed by this type's contract and does not silently regress if a field
    /// is ever added.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredSession")
            .field(
                "session_id",
                &self.session_id.as_ref().map(|_| "[REDACTED]"),
            )
            .field("schema_version", &self.schema_version)
            .field(
                "user_token",
                &self.user_token.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Session, StoredSession, validate_token_shape};

    // ===================== Wire-format round trip (D-5, v8.0.4 shape) =====================

    #[test]
    fn round_trip_matches_v8_0_4_wire_shape() {
        let json = r#"{"session_id":"tok-123","schema_version":2}"#;
        let session = Session::from_json(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "tok-123");

        let round_tripped = session.to_json().expect("session serializes");
        assert_eq!(round_tripped, json);
    }

    #[test]
    fn empty_session_round_trips_to_a_null_session_id() {
        let session = Session::empty();
        let json = session.to_json().expect("serializes");
        assert_eq!(json, r#"{"session_id":null,"schema_version":2}"#);
        let restored = Session::from_json(&json).expect("parses");
        assert!(!restored.is_valid());
        assert_eq!(restored.expose_token(), "");
    }

    // ===================== Legacy migration =====================

    #[test]
    fn record_without_schema_version_is_reported_as_migrated() {
        let json = r#"{"session_id":"tok-123"}"#;
        let (session, migrated) = Session::from_json_migrating(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "tok-123");
        assert!(migrated, "no schema_version key means a legacy record");
    }

    #[test]
    fn record_with_schema_version_is_not_migrated() {
        let json = r#"{"session_id":"tok-123","schema_version":2}"#;
        let (_, migrated) = Session::from_json_migrating(json).expect("valid json parses");
        assert!(!migrated);
    }

    #[test]
    fn legacy_user_token_key_is_accepted_as_session_id_alias() {
        let json = r#"{"user_token":"old_token_123","session_id":null}"#;
        let (session, migrated) = Session::from_json_migrating(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "old_token_123");
        assert!(migrated);
    }

    #[test]
    fn empty_session_id_still_falls_back_to_user_token() {
        // Mirrors Python's truthiness-based `or`: an empty string is falsy, so it does not win
        // over a present `user_token` (`auth_storage.py:74`).
        let json = r#"{"session_id":"","user_token":"old_token_123"}"#;
        let (session, _) = Session::from_json_migrating(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "old_token_123");
    }

    #[test]
    fn present_session_id_wins_over_user_token() {
        let json = r#"{"session_id":"new-token","user_token":"old-token"}"#;
        let (session, _) = Session::from_json_migrating(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "new-token");
    }

    #[test]
    fn legacy_refresh_token_and_session_expiry_fields_are_silently_dropped() {
        let json =
            r#"{"session_id":"tok","refresh_token":"rt-1","session_expiry":"2099-01-01T00:00:00"}"#;
        let (session, migrated) = Session::from_json_migrating(json).expect("valid json parses");
        assert_eq!(session.expose_token(), "tok");
        assert!(migrated);
        // The re-serialized shape has no trace of either legacy field.
        let reserialized = session.to_json().expect("serializes");
        assert!(!reserialized.contains("refresh_token"));
        assert!(!reserialized.contains("session_expiry"));
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
    fn is_valid_true_regardless_of_any_wall_clock_state() {
        // v8.0.4 has no client-side expiry concept at all: validity is a pure token-presence
        // check, forever, until the token is explicitly cleared.
        let session = Session::from_token("tok");
        assert!(session.is_valid());
        assert!(session.is_valid(), "still valid on a second check");
    }

    // ===================== Debug / Display redaction =====================

    #[test]
    fn debug_never_prints_the_token() {
        let session = Session::from_token("super-secret-token");
        let debug = format!("{session:?}");
        assert!(!debug.contains("super-secret-token"));
    }

    #[test]
    fn display_never_prints_the_token() {
        let session = Session::from_token("super-secret-token");
        let display = format!("{session}");
        assert!(!display.contains("super-secret-token"));
    }

    // ===================== StoredSession Debug redaction =====================

    #[test]
    fn stored_session_debug_never_prints_session_id_or_user_token() {
        let stored = StoredSession {
            session_id: Some("super-secret-session-id".to_owned()),
            schema_version: Some(2),
            user_token: Some("super-secret-user-token".to_owned()),
        };
        let debug = format!("{stored:?}");
        assert!(!debug.contains("super-secret-session-id"));
        assert!(!debug.contains("super-secret-user-token"));
        assert!(
            debug.contains('2'),
            "schema_version is not a secret: {debug}"
        );
        assert!(debug.contains("REDACTED"));
    }

    // ===================== validate_token_shape =====================

    #[test]
    fn validate_token_shape_rejects_empty() {
        let err = validate_token_shape("token", "").unwrap_err();
        assert!(matches!(err, Error::Validation { field, message, .. }
            if field == "token" && message == "must be a non-empty string"));
    }

    #[test]
    fn validate_token_shape_rejects_control_characters() {
        let err = validate_token_shape("token", "tok\nwith-newline").unwrap_err();
        assert!(matches!(err, Error::Validation { field, .. } if field == "token"));
    }

    #[test]
    fn validate_token_shape_accepts_printable_ascii() {
        validate_token_shape("token", "tok-123_ABC.").expect("printable ascii is accepted");
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
    fn from_env_value_non_ascii_is_validation_error() {
        let err =
            Session::from_env_value("RUSTEERO_TEST_TOKEN", Some("tok\r\n".to_owned())).unwrap_err();
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
