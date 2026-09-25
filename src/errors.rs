//! The closed catalogue of `meta.error` strings the Eero cloud API reports, and the single
//! classification point that turns an HTTP status + parsed envelope into an [`Error`].
//!
//! Ported 1:1 from `eero-api`'s `src/eero/errors.py` at `v8.0.4`. The API reports failures as
//! `{"meta": {"code": <http status>, "error": <string>, ...}}`; `meta.error` is drawn from a
//! closed set of dot-separated strings (matched case-insensitively), though some responses omit
//! `meta.error` entirely, or carry a free-text sentence instead of one of these strings.
//!
//! [`crate::transport`] is the only caller of [`error_for_response`] — it is the single place
//! classification happens. Nothing here ever raises or panics; it only builds and returns
//! [`Error`] values.

use std::sync::LazyLock;
use std::time::Duration;

use serde_json::Value;

use crate::error::Error;

/// A group of catalogue error strings that share an [`Error`] variant.
///
/// Ported from `ErrorGroup` (`errors.py:34-45`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorGroup {
    /// `error.session.*` — terminal, credential-clearing 401s.
    Session,
    /// `error.session.refresh` — not terminal; triggers the transparent refresh-and-replay.
    SessionRefresh,
    /// Verification / login-state 401s: token issued, but the account is mid-verification or
    /// otherwise blocked from completing login.
    Verification,
    /// `error.access.denied` — authenticated, but not permitted (HTTP 403 only).
    AccessDenied,
    /// A recognised "resource not found" string (HTTP 404 only; any `error_code`, recognised or
    /// not, also maps to `NotFound` on a 404 — see [`error_for_response`]).
    NotFound,
    /// `error.rate.limit` — reported either via HTTP 429 or this string on another status.
    RateLimit,
    /// A client-side-shaped validation error returned by the server (HTTP 400 only).
    Validation,
    /// Premium/subscription gating — status-independent.
    Premium,
    /// A feature is unavailable on this device/network — status-independent.
    FeatureUnavailable,
    /// The client's app/library version was rejected by the API — status-independent.
    ClientBlocked,
    /// Every other recognised, domain-specific error string. Stays a generic API error; the
    /// sub-code is exposed via `error_code` for callers that want to branch on it.
    Domain,
}

/// Session errors (401): terminal, credential-clearing.
///
/// Ported from `SESSION_ERRORS` (`errors.py:50-56`).
pub const SESSION_ERRORS: &[&str] = &[
    "error.session.expired",
    "error.session.invalid",
    "error.session.revoked",
];

/// Session-refresh signal (401): not terminal, triggers the transparent refresh-and-replay in
/// the transport. Never clears credentials.
///
/// Ported from `SESSION_REFRESH_ERRORS` (`errors.py:58-60`).
pub const SESSION_REFRESH_ERRORS: &[&str] = &["error.session.refresh"];

/// Verification / login-state errors (401): the token has been issued but the account is
/// mid-verification or otherwise blocked from completing login. Never clears credentials.
///
/// Ported from `VERIFICATION_ERRORS` (`errors.py:63-75`).
pub const VERIFICATION_ERRORS: &[&str] = &[
    "error.verification.required",
    "error.verification.invalid",
    "error.verification.expired",
    "error.verification.failure",
    "error.verification.blocked",
    "error.login.unknown",
    "error.login.blocked",
    "error.too.many.resends",
    "error.email.unverified",
];

/// Access denied (403): authenticated, but not permitted. Not an auth error.
///
/// Ported from `ACCESS_DENIED_ERRORS` (`errors.py:77`).
pub const ACCESS_DENIED_ERRORS: &[&str] = &["error.access.denied"];

/// Not found (404). A 404 with no `meta.error` at all, or with a free-text sentence instead of
/// one of these strings, also maps to [`Error::NotFound`] — see [`error_for_response`].
///
/// Ported from `NOT_FOUND_ERRORS` (`errors.py:79-85`).
pub const NOT_FOUND_ERRORS: &[&str] = &[
    "error.network.not.found",
    "error.eero.no_serial_found",
    "error.software_keys.not_found",
];

/// Rate limiting. Reported either via HTTP 429 or this string on another status.
///
/// Ported from `RATE_LIMIT_ERRORS` (`errors.py:87`).
pub const RATE_LIMIT_ERRORS: &[&str] = &["error.rate.limit"];

/// Client-side-shaped validation errors returned by the server (HTTP 400).
///
/// Ported from `VALIDATION_ERRORS` (`errors.py:90-100`).
pub const VALIDATION_ERRORS: &[&str] = &[
    "error.form.errors",
    "error.form.email.unavailable",
    "error.form.phone.unavailable",
    "error.form.email.malformed",
    "error.form.phone.malformed",
    "error.invites.format.faulty",
    "error.invalid.user.role",
    "error.reservation.ip.invalid",
    "error.network.multistaticipv2.wan_ip_not_in_range",
];

/// Premium/subscription gating. Maps to [`Error::PremiumRequired`] regardless of HTTP status.
///
/// Ported from `PREMIUM_ERRORS` (`errors.py:102-108`).
pub const PREMIUM_ERRORS: &[&str] = &[
    "error.premium.user_not_subscribed",
    "error.partner.unavailable",
];

/// Feature unavailable on this device/network. Maps to [`Error::FeatureUnavailable`] regardless
/// of HTTP status.
///
/// Ported from `FEATURE_UNAVAILABLE_ERRORS` (`errors.py:110-121`).
pub const FEATURE_UNAVAILABLE_ERRORS: &[&str] = &[
    "error.eero.offline",
    "error.network.unavailable",
    "error.eero.not.capable",
    "error.eero.deactivated",
    "error.eero.owned_by_organization",
    "error.eero.wifibackup.as.gateway",
    "error.eero.already.owned",
    "error.eero.needs.reset",
];

/// Client version rejected by the API. Maps to [`Error::ClientBlocked`] regardless of HTTP
/// status.
///
/// Ported from `CLIENT_BLOCKED_ERRORS` (`errors.py:123`).
pub const CLIENT_BLOCKED_ERRORS: &[&str] = &["error.app.version.blocked"];

/// Domain-specific errors that stay a generic API error; the sub-code is exposed via
/// `error_code` for callers that want to branch on it.
///
/// Ported from `DOMAIN_ERRORS` (`errors.py:126-158`).
pub const DOMAIN_ERRORS: &[&str] = &[
    "error.reservation.failed",
    "error.assignment.ip.unavailable",
    "error.assignment.port.unavailable",
    "error.forward.failed",
    "error.backup.access.point.ssid.already.exists",
    "error.backup.access.point.ssid.conflict",
    "error.max.number.of.backup.access.points.reached",
    "error.invite.status.accepted",
    "error.invite.status.rejected",
    "error.invite.status.revoked",
    "error.invite.status.expired",
    "error.max.admins.reached",
    "error.public_static_ip.reservation.error",
    "error.network.transfer.recipient.unverified_phone",
    "error.network.transfer.recipient.unverified_email",
    "error.network.transfer.recipient.mismatched_phone",
    "error.network.transfer.recipient.mismatched_email",
    "error.network.transfer.recipient.ambiguous",
    "error.user.amazon_login.exists",
    "error.user.amazon_login.email.unavailable",
    "error.software_keys.already_used",
    "error.stripe.card.incorrect_number",
    "error.stripe.card.expired",
    "error.stripe.card.incorrect_cvc",
    "error.stripe.card.incorrect_zip",
    "error.stripe.card.declined",
    "error.stripe.card.processing_error",
    "error.stripe.coupon.inappropriate",
    "error.stripe.coupon.invalid",
    "error.stripe.coupon.missing",
    "encryptme.error.email.exists",
    "encryptme.error.email.invalid",
    "encryptme.error.creation.failed",
];

/// Every group paired with its member strings, in [`ErrorGroup`] declaration order. The single
/// source of truth [`classify_error_code`]'s reverse lookup is built from.
fn group_members() -> [(ErrorGroup, &'static [&'static str]); 11] {
    [
        (ErrorGroup::Session, SESSION_ERRORS),
        (ErrorGroup::SessionRefresh, SESSION_REFRESH_ERRORS),
        (ErrorGroup::Verification, VERIFICATION_ERRORS),
        (ErrorGroup::AccessDenied, ACCESS_DENIED_ERRORS),
        (ErrorGroup::NotFound, NOT_FOUND_ERRORS),
        (ErrorGroup::RateLimit, RATE_LIMIT_ERRORS),
        (ErrorGroup::Validation, VALIDATION_ERRORS),
        (ErrorGroup::Premium, PREMIUM_ERRORS),
        (ErrorGroup::FeatureUnavailable, FEATURE_UNAVAILABLE_ERRORS),
        (ErrorGroup::ClientBlocked, CLIENT_BLOCKED_ERRORS),
        (ErrorGroup::Domain, DOMAIN_ERRORS),
    ]
}

/// Reverse lookup from a normalized (trimmed, lowercased) catalogue string to its group, built
/// once. Ported from `_STRING_TO_GROUP` (`errors.py:169-172`).
static STRING_TO_GROUP: LazyLock<std::collections::HashMap<&'static str, ErrorGroup>> =
    LazyLock::new(|| {
        let mut map = std::collections::HashMap::new();
        for (group, members) in group_members() {
            for member in members {
                map.insert(*member, group);
            }
        }
        map
    });

/// The groups that select their [`Error`] variant regardless of the HTTP status
/// code that carried them.
///
/// Ported from `_STATUS_INDEPENDENT_GROUPS` (`errors.py:174-181`).
fn is_status_independent(group: ErrorGroup) -> bool {
    matches!(
        group,
        ErrorGroup::Premium
            | ErrorGroup::FeatureUnavailable
            | ErrorGroup::ClientBlocked
            | ErrorGroup::RateLimit
    )
}

/// Returns the catalogue group for an API error code.
///
/// Matching is case-insensitive and trims surrounding whitespace, matching how the transport
/// extracts `meta.error`. Ported from `classify_error_code` (`errors.py:184-201`).
///
/// Returns `None` when `error_code` is `None`, empty, or not present in the catalogue (for
/// example a free-text sentence, or an error string this crate does not yet recognise).
#[must_use]
pub fn classify_error_code(error_code: Option<&str>) -> Option<ErrorGroup> {
    let code = error_code?;
    if code.trim().is_empty() {
        return None;
    }
    let normalized = code.trim().to_lowercase();
    STRING_TO_GROUP.get(normalized.as_str()).copied()
}

/// Builds a leak-safe error message for a `meta.error` value.
///
/// When `error_code` classifies into a known catalogue group, the message is the catalogue
/// string itself (normalized: trimmed and lowercased). Otherwise — `error_code` is `None`,
/// empty, or an unrecognised/free-text value — the message is the fixed label
/// `"unrecognised error string"`. Neither branch ever embeds any other part of the response body
/// (no `data`, no raw body text, no request URL): those stay out of error messages and are only
/// ever available via [`Error::envelope`] or a debug log line.
///
/// Ported from `message_for_error_code` (`errors.py:204-224`).
#[must_use]
pub fn message_for_error_code(error_code: Option<&str>) -> String {
    match error_code {
        Some(code) if classify_error_code(Some(code)).is_some() => code.trim().to_lowercase(),
        _ => "unrecognised error string".to_owned(),
    }
}

/// Extracts `meta.error` from a parsed response envelope.
///
/// Returns the string value of `envelope["meta"]["error"]` when present, else `None`. Ported
/// from `_error_code_from_envelope` (`api/base.py:213-229`).
#[must_use]
pub fn error_code_from_envelope(envelope: &Value) -> Option<String> {
    envelope
        .as_object()?
        .get("meta")?
        .as_object()?
        .get("error")?
        .as_str()
        .map(str::to_owned)
}

/// Best-effort parse of a response body as a JSON object.
///
/// Returns `None` when the body is empty (or all whitespace), not valid JSON, or not a JSON
/// object. Ported from `_parse_envelope` (`api/base.py:191-210`).
///
/// `serde_json`'s own recursion limit stands in for Python's `RecursionError` guard against a
/// pathologically nested body: either way, a body that cannot be parsed becomes `None` rather
/// than propagating an error, since this is a best-effort parse.
#[must_use]
pub fn parse_envelope(text: &str) -> Option<Value> {
    if text.trim().is_empty() {
        return None;
    }
    let parsed = serde_json::from_str::<Value>(text).ok()?;
    if parsed.is_object() {
        Some(parsed)
    } else {
        None
    }
}

/// Chooses and builds the crate's [`Error`] for a non-2xx, non-3xx API response.
///
/// Classification never rejects or alters `envelope` — it is only ever attached to the returned
/// error, verbatim. The error message is always built by [`message_for_error_code`], never from
/// the raw response body or the request URL. Ported from `exception_for_error`
/// (`errors.py:227-346`); precedence:
///
/// 1. HTTP 401 always wins → [`Error::Authentication`], regardless of `meta.error`.
/// 2. Otherwise, a `meta.error` string in one of the status-independent groups
///    (premium/feature-unavailable/client-blocked/rate-limit) always wins, regardless of the
///    HTTP status code.
/// 3. Otherwise the HTTP status code selects the variant: 403 (with `error.access.denied`) →
///    [`Error::AccessDenied`], 404 → [`Error::NotFound`] (regardless of whether `meta.error` is
///    present, recognised, or free text), 429 → [`Error::RateLimit`], 400 (with a recognised
///    validation string) → [`Error::Validation`].
/// 4. Anything else — including every recognised "domain" string, an unrecognised string, and a
///    403/400 that does not match the groups above — is [`Error::Api`].
#[must_use]
pub fn error_for_response(
    status: u16,
    envelope: Option<Value>,
    error_code: Option<String>,
    retry_after: Option<Duration>,
) -> Error {
    let message = message_for_error_code(error_code.as_deref());
    let group = classify_error_code(error_code.as_deref());

    if status == 401 {
        return Error::Authentication {
            message,
            envelope,
            error_code,
        };
    }

    if group.is_some_and(is_status_independent) {
        return match group {
            Some(ErrorGroup::Premium) => Error::PremiumRequired {
                status: Some(status),
                message,
                envelope,
                error_code,
            },
            Some(ErrorGroup::FeatureUnavailable) => Error::FeatureUnavailable {
                status: Some(status),
                message,
                envelope,
                error_code,
            },
            Some(ErrorGroup::ClientBlocked) => Error::ClientBlocked {
                status,
                message,
                envelope,
                error_code,
            },
            // `ErrorGroup::RateLimit` is the only remaining status-independent group.
            _ => Error::RateLimit {
                message,
                retry_after,
                envelope,
                error_code,
            },
        };
    }

    match status {
        403 if group == Some(ErrorGroup::AccessDenied) => Error::AccessDenied {
            status,
            message,
            envelope,
            error_code,
        },
        404 => Error::NotFound {
            status,
            message,
            envelope,
            error_code,
        },
        429 => Error::RateLimit {
            message,
            retry_after,
            envelope,
            error_code,
        },
        400 if group == Some(ErrorGroup::Validation) => Error::Validation {
            field: "request".to_owned(),
            message,
            envelope,
            error_code,
        },
        _ => Error::Api {
            status,
            message,
            envelope,
            error_code,
            url: None,
        },
    }
}

/// Every catalogue string across every group, used only by this module's own consistency tests.
#[cfg(test)]
fn all_catalogue_strings() -> Vec<&'static str> {
    group_members()
        .into_iter()
        .flat_map(|(_, members)| members.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorGroup, classify_error_code, error_code_from_envelope, error_for_response,
        group_members, message_for_error_code, parse_envelope,
    };
    use crate::error::Error;
    use serde_json::json;
    use std::collections::HashSet;
    use std::time::Duration;

    // ===================== TestCatalogueConsistency =====================

    #[test]
    fn no_duplicate_strings_across_groups() {
        let all = super::all_catalogue_strings();
        let unique: HashSet<&str> = all.iter().copied().collect();
        assert_eq!(
            all.len(),
            unique.len(),
            "a catalogue string must belong to exactly one group"
        );
    }

    #[test]
    fn every_group_is_non_empty() {
        for (group, members) in group_members() {
            assert!(!members.is_empty(), "{group:?} must not be empty");
        }
    }

    #[test]
    fn every_catalogue_string_is_already_lowercase_and_trimmed() {
        for string in super::all_catalogue_strings() {
            assert_eq!(string, string.trim());
            assert_eq!(string, string.to_lowercase());
        }
    }

    // ===================== TestClassifyErrorCode =====================

    #[test]
    fn classifies_a_known_string_in_every_group() {
        assert_eq!(
            classify_error_code(Some("error.session.expired")),
            Some(ErrorGroup::Session)
        );
        assert_eq!(
            classify_error_code(Some("error.session.refresh")),
            Some(ErrorGroup::SessionRefresh)
        );
        assert_eq!(
            classify_error_code(Some("error.verification.required")),
            Some(ErrorGroup::Verification)
        );
        assert_eq!(
            classify_error_code(Some("error.access.denied")),
            Some(ErrorGroup::AccessDenied)
        );
        assert_eq!(
            classify_error_code(Some("error.network.not.found")),
            Some(ErrorGroup::NotFound)
        );
        assert_eq!(
            classify_error_code(Some("error.rate.limit")),
            Some(ErrorGroup::RateLimit)
        );
        assert_eq!(
            classify_error_code(Some("error.form.errors")),
            Some(ErrorGroup::Validation)
        );
        assert_eq!(
            classify_error_code(Some("error.premium.user_not_subscribed")),
            Some(ErrorGroup::Premium)
        );
        assert_eq!(
            classify_error_code(Some("error.eero.offline")),
            Some(ErrorGroup::FeatureUnavailable)
        );
        assert_eq!(
            classify_error_code(Some("error.app.version.blocked")),
            Some(ErrorGroup::ClientBlocked)
        );
        assert_eq!(
            classify_error_code(Some("error.reservation.failed")),
            Some(ErrorGroup::Domain)
        );
    }

    #[test]
    fn classify_is_case_insensitive_and_trims_whitespace() {
        assert_eq!(
            classify_error_code(Some("  Error.Session.EXPIRED  ")),
            Some(ErrorGroup::Session)
        );
    }

    #[test]
    fn classify_none_empty_and_unrecognised_all_return_none() {
        assert_eq!(classify_error_code(None), None);
        assert_eq!(classify_error_code(Some("")), None);
        assert_eq!(classify_error_code(Some("   ")), None);
        assert_eq!(classify_error_code(Some("not a catalogue string")), None);
    }

    // ===================== TestMessageForErrorCode =====================

    #[test]
    fn message_for_known_code_is_the_normalized_catalogue_string() {
        assert_eq!(
            message_for_error_code(Some("  Error.Session.Expired  ")),
            "error.session.expired"
        );
    }

    #[test]
    fn message_for_unknown_or_absent_code_is_the_fixed_fallback() {
        assert_eq!(message_for_error_code(None), "unrecognised error string");
        assert_eq!(
            message_for_error_code(Some("")),
            "unrecognised error string"
        );
        assert_eq!(
            message_for_error_code(Some("some free text sentence")),
            "unrecognised error string"
        );
    }

    // ===================== error_code_from_envelope / parse_envelope =====================

    #[test]
    fn error_code_from_envelope_extracts_meta_error() {
        let envelope = json!({"meta": {"code": 401, "error": "error.session.expired"}});
        assert_eq!(
            error_code_from_envelope(&envelope),
            Some("error.session.expired".to_owned())
        );
    }

    #[test]
    fn error_code_from_envelope_none_when_meta_absent_or_not_object() {
        assert_eq!(error_code_from_envelope(&json!({"data": {}})), None);
        assert_eq!(error_code_from_envelope(&json!({"meta": "oops"})), None);
        assert_eq!(
            error_code_from_envelope(&json!({"meta": {"code": 401}})),
            None
        );
    }

    #[test]
    fn parse_envelope_empty_or_whitespace_is_none() {
        assert_eq!(parse_envelope(""), None);
        assert_eq!(parse_envelope("   \n  "), None);
    }

    #[test]
    fn parse_envelope_non_object_json_is_none() {
        assert_eq!(parse_envelope("[1,2,3]"), None);
        assert_eq!(parse_envelope("\"a string\""), None);
        assert_eq!(parse_envelope("42"), None);
    }

    #[test]
    fn parse_envelope_invalid_json_is_none() {
        assert_eq!(parse_envelope("not json at all"), None);
    }

    #[test]
    fn parse_envelope_valid_object_is_some() {
        let parsed = parse_envelope(r#"{"meta":{"code":200},"data":{}}"#).unwrap();
        assert_eq!(parsed["meta"]["code"], 200);
    }

    // ===================== TestExceptionForError401Precedence =====================

    #[test]
    fn a_401_always_wins_even_with_a_premium_error_code() {
        // The 401-beats-everything-else case: a 401 carrying a status-independent group's
        // string (which should not happen per the catalogue, but must never silently downgrade
        // an authentication failure to something else).
        let err = error_for_response(
            401,
            None,
            Some("error.premium.user_not_subscribed".to_owned()),
            None,
        );
        assert!(matches!(err, Error::Authentication { .. }));
    }

    #[test]
    fn status_independent_groups_win_regardless_of_status() {
        let err = error_for_response(
            200,
            None,
            Some("error.premium.user_not_subscribed".to_owned()),
            None,
        );
        assert!(matches!(err, Error::PremiumRequired { .. }));

        let err = error_for_response(500, None, Some("error.eero.offline".to_owned()), None);
        assert!(matches!(err, Error::FeatureUnavailable { .. }));

        let err = error_for_response(
            200,
            None,
            Some("error.app.version.blocked".to_owned()),
            None,
        );
        assert!(matches!(err, Error::ClientBlocked { .. }));

        let err = error_for_response(500, None, Some("error.rate.limit".to_owned()), None);
        assert!(matches!(err, Error::RateLimit { .. }));
    }

    #[test]
    fn a_403_with_access_denied_code_is_access_denied() {
        let err = error_for_response(403, None, Some("error.access.denied".to_owned()), None);
        assert!(matches!(err, Error::AccessDenied { status: 403, .. }));
    }

    #[test]
    fn a_403_without_access_denied_code_is_generic_api() {
        let err = error_for_response(403, None, None, None);
        assert!(matches!(err, Error::Api { status: 403, .. }));
    }

    #[test]
    fn a_404_is_always_not_found_regardless_of_error_code() {
        assert!(matches!(
            error_for_response(404, None, None, None),
            Error::NotFound { status: 404, .. }
        ));
        assert!(matches!(
            error_for_response(404, None, Some("free text".to_owned()), None),
            Error::NotFound { status: 404, .. }
        ));
        assert!(matches!(
            error_for_response(404, None, Some("error.network.not.found".to_owned()), None),
            Error::NotFound { status: 404, .. }
        ));
    }

    #[test]
    fn a_429_is_always_rate_limit() {
        let err = error_for_response(429, None, None, Some(Duration::from_secs(5)));
        match err {
            Error::RateLimit { retry_after, .. } => {
                assert_eq!(retry_after, Some(Duration::from_secs(5)));
            }
            other => panic!("expected Error::RateLimit, got {other:?}"),
        }
    }

    #[test]
    fn a_400_with_validation_code_is_validation_with_request_field() {
        let err = error_for_response(400, None, Some("error.form.errors".to_owned()), None);
        match err {
            Error::Validation { field, .. } => assert_eq!(field, "request"),
            other => panic!("expected Error::Validation, got {other:?}"),
        }
    }

    #[test]
    fn a_400_without_validation_code_is_generic_api() {
        let err = error_for_response(400, None, None, None);
        assert!(matches!(err, Error::Api { status: 400, .. }));
    }

    #[test]
    fn a_domain_string_stays_generic_api_and_carries_error_code() {
        let err = error_for_response(422, None, Some("error.reservation.failed".to_owned()), None);
        match err {
            Error::Api { error_code, .. } => {
                assert_eq!(error_code.as_deref(), Some("error.reservation.failed"));
            }
            other => panic!("expected Error::Api, got {other:?}"),
        }
    }

    #[test]
    fn envelope_is_attached_verbatim() {
        let envelope = json!({"meta": {"code": 404}});
        let err = error_for_response(404, Some(envelope.clone()), None, None);
        match err {
            Error::NotFound { envelope: e, .. } => assert_eq!(e, Some(envelope)),
            other => panic!("expected Error::NotFound, got {other:?}"),
        }
    }
}
