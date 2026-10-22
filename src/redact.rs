//! Redaction of sensitive keys in a [`serde_json::Value`].
//!
//! Ported from `eero-api`'s `src/eero/logging.py` at `v8.4.1`: [`redact_sensitive`] mirrors the
//! module-level `redact_sensitive`, backed by private helpers ported from `_redact`,
//! `_redact_value` and `_is_sensitive_key`.
//!
//! `logging.py` also defines `SecureLoggerAdapter` and `get_secure_logger`, a
//! `logging.LoggerAdapter` subclass that transparently redacts every log call's arguments. Those
//! are **not** ported: this crate uses `tracing` for structured logging and
//! `secrecy::SecretString` for token storage, which together remove the need for an ambient
//! auto-redacting logger. Call [`redact_sensitive`] explicitly wherever a raw JSON body must be
//! included in a log line.

use serde_json::{Map, Value};

/// Case-insensitive **substring** patterns that mark a JSON object key as sensitive.
///
/// Ported from `DEFAULT_SENSITIVE_PATTERNS`. Matching is by substring, not whole word, which is
/// a deliberate (if surprising) upstream quirk: `session_expiry` (contains `session`), `author`
/// (contains `auth`) and even `monkey` (contains `key`) are all treated as sensitive keys and
/// redacted, exactly as in the Python original. Do not "fix" this to word-boundary matching — it
/// would silently break parity with `eero-api`.
///
/// `v8.0.4` adds eight identifier-shaped patterns (`login`, `email`, `phone`, `sms`, `serial`,
/// `mac`, `mac_address`, `ssid`) on top of the pre-existing credential-shaped set, to keep a
/// user-submitted identifier (e.g. the value passed to `login()`) from leaking into a log record
/// via an echoed API response or a caller-provided extra/args dict.
const SENSITIVE_PATTERNS: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "key",
    "credential",
    "session_id",
    "session",
    "cookie",
    "auth",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "user_token",
    "bearer",
    "authorization",
    "private",
    "login",
    "email",
    "phone",
    "sms",
    "serial",
    "mac",
    "mac_address",
    "ssid",
];

/// The subset of [`SENSITIVE_PATTERNS`] considered credential-shaped rather than merely
/// identifier-shaped.
///
/// Ported from `_ZERO_VISIBILITY_PATTERNS` (new at `v8.0.4`). A key matching one of these must
/// never have any of its value's leading characters logged — only its length (see
/// [`redact_value`]/[`is_zero_visibility_key`]). Identifier-shaped fields (`email`, `phone`, ...)
/// still show a short prefix for debugging usability.
const ZERO_VISIBILITY_PATTERNS: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "key",
    "credential",
    "session_id",
    "session",
    "cookie",
    "auth",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "user_token",
    "bearer",
    "authorization",
    "private",
];

/// Number of leading characters of a redacted value that remain visible for a non-zero-visibility
/// (identifier-shaped) key.
///
/// Matches Python's default `visible_chars = 4`. The Python functions accept `visible_chars` as
/// an override parameter; nothing in `eero-api` ever calls them with a non-default value, so this
/// port fixes it as a constant rather than threading an unused parameter through the public API.
const VISIBLE_CHARS: usize = 4;

/// Containers nested this deep or deeper are replaced wholesale by [`UNTRAVERSABLE_PLACEHOLDER`]
/// instead of being walked.
///
/// Ported from `_MAX_REDACTION_DEPTH` (`logging.py:79-83`). The top-level argument is depth `0`
/// and every child is one deeper, so a container at depth `16` is the first one replaced. The cap
/// keeps a pathological value from exhausting the stack and keeps a sensitive key from being
/// smuggled past the redactor by sitting below the point where it stops looking.
const MAX_REDACTION_DEPTH: usize = 16;

/// Fixed text that replaces a container nested at or beyond [`MAX_REDACTION_DEPTH`].
///
/// Ported from `_UNTRAVERSABLE_PLACEHOLDER` (`logging.py:85-88`). It never depends on the content
/// it replaces. Python uses the same text for a reference cycle, which a [`Value`] cannot form.
const UNTRAVERSABLE_PLACEHOLDER: &str = "[REDACTED:cyclic-or-too-deep]";

/// Returns `true` if `key`, matched case-insensitively as a substring, indicates sensitive data.
///
/// Ported from `_is_sensitive_key`.
fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    SENSITIVE_PATTERNS
        .iter()
        .any(|pattern| key_lower.contains(pattern))
}

/// Returns `true` if `key`, matched case-insensitively as a substring, is credential-shaped and
/// must never show a value prefix.
///
/// Ported from `_is_zero_visibility_key` (new at `v8.0.4`).
fn is_zero_visibility_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    ZERO_VISIBILITY_PATTERNS
        .iter()
        .any(|pattern| key_lower.contains(pattern))
}

/// Redacts a single value found under a sensitive key, returning its redacted string form.
///
/// Ported from `_redact_value`. Checks are ordered exactly as in Python, where `null`/`bool`
/// must be special-cased before the generic numeric/string handling (in Python, `bool` is a
/// subtype of `int`, so `isinstance(value, bool)` has to run first). `visible_chars` is `0` for a
/// zero-visibility (credential-shaped) key and [`VISIBLE_CHARS`] for every other sensitive key —
/// see [`is_zero_visibility_key`].
fn redact_value(value: &Value, visible_chars: usize) -> Value {
    let text = match value {
        Value::Null => return Value::String("[NONE]".to_string()),
        Value::Bool(_) => return Value::String("[REDACTED:bool]".to_string()),
        Value::Number(_) => return Value::String("[REDACTED:number]".to_string()),
        Value::String(s) => s.clone(),
        // Python's fallback branch runs `str(value)`, which for a dict/list produces a
        // Python-repr string (e.g. "{'a': 1}"). A container under a sensitive key does occur on
        // the wire: the account envelope's `email` is `{"address": ..., "verified": true}`. This
        // port measures compact JSON instead of reproducing Python's repr syntax, so for a
        // container value the reported length (and, for an identifier-shaped key, the visible
        // prefix) can differ from Python: that `email` object is `46chars` here and `49chars`
        // in Python, and `{"token": {"a": 1}}` is `7chars` here and `8chars` in Python. How much
        // of the container reaches the output is what Python shows: under a credential-shaped key
        // (`visible_chars == 0`) nothing of it; under an identifier-shaped key the first
        // `VISIBLE_CHARS` characters of its text (`str(value)[:4]` in Python), so
        // `{"emails": ["ab@x.io"]}` shows `["ab...`, which is part of the address.
        Value::Array(_) | Value::Object(_) => value.to_string(),
    };

    if text.is_empty() {
        return Value::String("[EMPTY]".to_string());
    }

    let length = text.chars().count();
    if visible_chars == 0 || length <= visible_chars {
        return Value::String(format!("[REDACTED:{length}chars]"));
    }

    let visible: String = text.chars().take(visible_chars).collect();
    Value::String(format!("{visible}...[REDACTED:{length}chars]"))
}

/// Recursively redacts sensitive keys in a JSON object that sits at `depth`.
///
/// Ported from the mapping branch of `_redact` (`logging.py:205-217`): a sensitive key's value is
/// replaced by [`redact_value`]'s output (with `visible_chars` set to `0` for a zero-visibility
/// key, [`VISIBLE_CHARS`] otherwise) regardless of its type, and **before** any depth check, so a
/// deep container under a sensitive key is length-redacted rather than replaced by the
/// placeholder; every other value goes through [`redact_nested`] one level deeper.
fn redact_object(map: &Map<String, Value>, depth: usize) -> Value {
    let mut result = Map::with_capacity(map.len());
    for (key, value) in map {
        let redacted = if is_sensitive_key(key) {
            let visible_chars = if is_zero_visibility_key(key) {
                0
            } else {
                VISIBLE_CHARS
            };
            redact_value(value, visible_chars)
        } else {
            redact_nested(value, depth + 1)
        };
        result.insert(key.clone(), redacted);
    }
    Value::Object(result)
}

/// Builds a redacted copy of `value`, which sits at `depth` (`0` for the top-level argument).
///
/// Ported from `_redact` (`logging.py:171-220`). A scalar is cloned unchanged at any depth: the
/// container check runs before the depth check. An object or array (an empty one included) at
/// [`MAX_REDACTION_DEPTH`] or deeper is replaced wholesale by [`UNTRAVERSABLE_PLACEHOLDER`], which
/// hides its non-sensitive siblings as well. Anything shallower is rebuilt, keeping the
/// container kind, with every sensitive key found on the way redacted.
fn redact_nested(value: &Value, depth: usize) -> Value {
    match value {
        Value::Object(_) | Value::Array(_) if depth >= MAX_REDACTION_DEPTH => {
            Value::String(UNTRAVERSABLE_PLACEHOLDER.to_string())
        }
        Value::Object(map) => redact_object(map, depth),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_nested(item, depth + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Redacts sensitive data from a JSON value for safe logging.
///
/// Ported from `redact_sensitive` (`logging.py:223-248`). Objects and arrays are walked to any
/// mix of nesting, a top-level array included, and come back as the same kind of container; a
/// sensitive key's whole value is replaced by its redacted form whatever the value's type, using
/// its text form for the length (and, for an identifier-shaped key, the four-character prefix).
/// Every other JSON value (a string, number, bool or null) is returned unchanged, because there
/// is no key to match.
///
/// Recursion stops at 16 levels of nesting. The top-level argument is level 0, and a container
/// at level 16 or deeper (an empty one too) is replaced by the fixed string
/// `"[REDACTED:cyclic-or-too-deep]"` instead of being walked, together with everything inside it.
/// A scalar below that depth still passes through, and so does a sensitive key's value, which is
/// redacted by length before any depth is checked.
///
/// The argument is never modified; the result is always a new value.
///
/// # Examples
///
/// ```
/// use rusteero::redact::redact_sensitive;
/// use serde_json::json;
///
/// let redacted = redact_sensitive(&json!({"user_token": "abc123456", "name": "John"}));
/// assert_eq!(redacted["name"], "John");
/// // "user_token" is credential-shaped (zero-visibility): no leading characters survive.
/// assert_eq!(redacted["user_token"], "[REDACTED:9chars]");
///
/// // A top-level array is walked element by element.
/// let redacted = redact_sensitive(&json!([{"token": "abc123456", "name": "John"}]));
/// assert_eq!(redacted[0]["name"], "John");
/// assert_eq!(redacted[0]["token"], "[REDACTED:9chars]");
///
/// // Non-container top-level values pass through untouched.
/// assert_eq!(redact_sensitive(&json!("hello")), json!("hello"));
/// ```
pub fn redact_sensitive(value: &Value) -> Value {
    redact_nested(value, 0)
}

#[cfg(test)]
mod tests {
    use super::{is_sensitive_key, is_zero_visibility_key, redact_sensitive, redact_value};
    use serde_json::{Value, json};

    // ===================== _is_sensitive_key (TestIsSensitiveKey) =====================

    #[test]
    fn detects_token_variations() {
        assert!(is_sensitive_key("token"));
        assert!(is_sensitive_key("user_token"));
        assert!(is_sensitive_key("access_token"));
        assert!(is_sensitive_key("refresh_token"));
        assert!(is_sensitive_key("TOKEN"));
        assert!(is_sensitive_key("User_Token"));
    }

    #[test]
    fn detects_password_variations() {
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("passwd"));
        assert!(is_sensitive_key("user_password"));
        assert!(is_sensitive_key("PASSWORD"));
    }

    #[test]
    fn detects_secret_variations() {
        assert!(is_sensitive_key("secret"));
        assert!(is_sensitive_key("client_secret"));
        assert!(is_sensitive_key("api_secret"));
    }

    #[test]
    fn detects_key_variations() {
        assert!(is_sensitive_key("key"));
        assert!(is_sensitive_key("api_key"));
        assert!(is_sensitive_key("apikey"));
        assert!(is_sensitive_key("private_key"));
    }

    #[test]
    fn detects_session_variations() {
        assert!(is_sensitive_key("session"));
        assert!(is_sensitive_key("session_id"));
        assert!(is_sensitive_key("cookie"));
    }

    #[test]
    fn detects_auth_variations() {
        assert!(is_sensitive_key("auth"));
        assert!(is_sensitive_key("authorization"));
        assert!(is_sensitive_key("bearer"));
    }

    #[test]
    fn non_sensitive_keys_are_not_flagged() {
        // "email" moved from this list to `detects_identifier_shaped_variations_new_at_v8_0_4`
        // at v8.0.4: it is now one of the eight new identifier-shaped patterns
        // (`DEFAULT_SENSITIVE_PATTERNS`), a deliberate, intentional classification change, not a
        // regression.
        assert!(!is_sensitive_key("name"));
        assert!(!is_sensitive_key("status"));
        assert!(!is_sensitive_key("id"));
        assert!(!is_sensitive_key("count"));
        assert!(!is_sensitive_key("network_id"));
    }

    #[test]
    fn substring_quirk_matches_incidental_words() {
        // Deliberate upstream quirk: substring, not whole-word, matching.
        assert!(is_sensitive_key("session_expiry")); // contains "session"
        assert!(is_sensitive_key("author")); // contains "auth"
        assert!(is_sensitive_key("monkey")); // contains "key"
    }

    #[test]
    fn detects_identifier_shaped_variations_new_at_v8_0_4() {
        assert!(is_sensitive_key("login"));
        assert!(is_sensitive_key("email"));
        assert!(is_sensitive_key("phone"));
        assert!(is_sensitive_key("sms"));
        assert!(is_sensitive_key("serial"));
        assert!(is_sensitive_key("mac"));
        assert!(is_sensitive_key("mac_address"));
        assert!(is_sensitive_key("ssid"));
    }

    // ===================== _is_zero_visibility_key =====================

    #[test]
    fn credential_shaped_keys_are_zero_visibility() {
        assert!(is_zero_visibility_key("token"));
        assert!(is_zero_visibility_key("password"));
        assert!(is_zero_visibility_key("session_id"));
        assert!(is_zero_visibility_key("cookie"));
    }

    #[test]
    fn identifier_shaped_keys_are_not_zero_visibility() {
        assert!(!is_zero_visibility_key("login"));
        assert!(!is_zero_visibility_key("email"));
        assert!(!is_zero_visibility_key("phone"));
        assert!(!is_zero_visibility_key("ssid"));
    }

    // ===================== _redact_value (TestRedactValue) =====================

    #[test]
    fn redacts_null() {
        assert_eq!(
            redact_value(&json!(null), super::VISIBLE_CHARS),
            json!("[NONE]")
        );
    }

    #[test]
    fn redacts_empty_string() {
        assert_eq!(
            redact_value(&json!(""), super::VISIBLE_CHARS),
            json!("[EMPTY]")
        );
    }

    #[test]
    fn redacts_short_string() {
        assert_eq!(
            redact_value(&json!("abc"), super::VISIBLE_CHARS),
            json!("[REDACTED:3chars]")
        );
        assert_eq!(
            redact_value(&json!("abcd"), super::VISIBLE_CHARS),
            json!("[REDACTED:4chars]")
        );
    }

    #[test]
    fn redacts_long_string() {
        let result = redact_value(&json!("secret123456"), super::VISIBLE_CHARS);
        let text = result.as_str().unwrap();
        assert!(text.starts_with("secr"));
        assert!(text.contains("[REDACTED:12chars]"));
    }

    #[test]
    fn redacts_boolean() {
        assert_eq!(
            redact_value(&json!(true), super::VISIBLE_CHARS),
            json!("[REDACTED:bool]")
        );
        assert_eq!(
            redact_value(&json!(false), super::VISIBLE_CHARS),
            json!("[REDACTED:bool]")
        );
    }

    #[test]
    fn redacts_numbers() {
        assert_eq!(
            redact_value(&json!(12345), super::VISIBLE_CHARS),
            json!("[REDACTED:number]")
        );
        assert_eq!(
            redact_value(&json!(3.5), super::VISIBLE_CHARS),
            json!("[REDACTED:number]")
        );
    }

    // ===================== _redact_dict (TestRedactDict, via redact_sensitive) =====================

    #[test]
    fn redacts_sensitive_keys() {
        let result = redact_sensitive(&json!({"user_token": "abc123", "name": "John"}));
        assert!(!result["user_token"].as_str().unwrap().contains("abc123"));
        assert!(result["user_token"].as_str().unwrap().contains("[REDACTED"));
        assert_eq!(result["name"], "John");
    }

    #[test]
    fn preserves_non_sensitive_keys() {
        let result = redact_sensitive(&json!({"status": "ok", "count": 5, "items": ["a", "b"]}));
        assert_eq!(result["status"], "ok");
        assert_eq!(result["count"], 5);
        assert_eq!(result["items"], json!(["a", "b"]));
    }

    // ===================== TestRedactDictZeroVisibilityForCredentials =====================

    #[test]
    fn zero_visibility_keys_never_show_a_leading_character() {
        let result = redact_sensitive(&json!({
            "password": "super-secret-value",
            "session_id": "abcdef123456",
            "token": "eyJhbGciOiJIUzI1NiJ9",
        }));
        for key in ["password", "session_id", "token"] {
            let redacted = result[key].as_str().unwrap();
            assert!(
                redacted.starts_with("[REDACTED:"),
                "{key} must show no leading characters of its value, got {redacted:?}"
            );
        }
    }

    #[test]
    fn identifier_shaped_keys_still_show_a_short_prefix() {
        let result = redact_sensitive(&json!({"email": "user@example.com"}));
        let redacted = result["email"].as_str().unwrap();
        assert!(
            redacted.starts_with("user"),
            "identifier-shaped keys keep the normal partial-prefix redaction, got {redacted:?}"
        );
        assert!(redacted.contains("[REDACTED"));
    }

    #[test]
    fn handles_nested_dicts() {
        let result = redact_sensitive(&json!({
            "user": {"name": "John", "password": "secret123"},
            "session_id": "xyz789",
        }));
        assert_eq!(result["user"]["name"], "John");
        assert!(
            result["user"]["password"]
                .as_str()
                .unwrap()
                .contains("[REDACTED")
        );
        assert!(result["session_id"].as_str().unwrap().contains("[REDACTED"));
    }

    #[test]
    fn handles_nested_list_of_lists_of_dicts() {
        // Recursion through an array must not stop after one level.
        let result = redact_sensitive(&json!({
            "outer": [[{"token": "abc123"}]]
        }));
        let redacted = result["outer"][0][0]["token"].as_str().unwrap();
        assert!(
            redacted.contains("[REDACTED"),
            "a token nested two array levels deep must still be redacted, got {redacted:?}"
        );
    }

    #[test]
    fn handles_list_of_dicts() {
        let result = redact_sensitive(&json!({
            "users": [
                {"name": "John", "token": "abc"},
                {"name": "Jane", "token": "xyz"},
            ]
        }));
        assert_eq!(result["users"][0]["name"], "John");
        assert!(
            result["users"][0]["token"]
                .as_str()
                .unwrap()
                .contains("[REDACTED")
        );
        assert_eq!(result["users"][1]["name"], "Jane");
        assert!(
            result["users"][1]["token"]
                .as_str()
                .unwrap()
                .contains("[REDACTED")
        );
    }

    #[test]
    fn handles_empty_object() {
        assert_eq!(redact_sensitive(&json!({})), json!({}));
    }

    // ===================== redact_sensitive (TestRedactSensitive) =====================

    #[test]
    fn handles_dict() {
        let result = redact_sensitive(&json!({"token": "secret"}));
        assert!(result["token"].as_str().unwrap().contains("[REDACTED"));
    }

    #[test]
    fn passes_through_non_dict() {
        assert_eq!(redact_sensitive(&json!("hello")), json!("hello"));
        assert_eq!(redact_sensitive(&json!(123)), json!(123));
        assert_eq!(redact_sensitive(&json!(null)), json!(null));
    }

    #[test]
    fn passes_through_scalars_and_strings_unchanged() {
        // A bare string has no key to match, so it is not redacted even when it looks like a
        // credential.
        assert_eq!(redact_sensitive(&json!("s=abc")), json!("s=abc"));
        assert_eq!(redact_sensitive(&json!(true)), json!(true));
    }

    const CANARY: &str = "CANARY-tok-5b2e";

    /// Wraps `leaf` in `levels` single-element arrays, built iteratively so the helper itself
    /// cannot overflow the stack.
    fn nest(levels: usize, leaf: Value) -> Value {
        (0..levels).fold(leaf, |inner, _| Value::Array(vec![inner]))
    }

    /// The value reached by indexing `[0]` `levels` times.
    fn descend(value: &Value, levels: usize) -> &Value {
        (0..levels).fold(value, |current, _| &current[0])
    }

    #[test]
    fn top_level_array_of_objects_is_redacted() {
        let result = redact_sensitive(&json!([{"token": CANARY, "name": "n"}]));
        assert!(result.is_array());
        assert_eq!(result[0]["name"], "n");
        assert_eq!(result[0]["token"], "[REDACTED:15chars]");
        assert!(!result.to_string().contains(CANARY));
    }

    #[test]
    fn top_level_array_of_arrays_is_redacted() {
        let result = redact_sensitive(&json!([[{"password": CANARY}]]));
        assert!(result.is_array());
        assert!(result[0].is_array());
        assert!(result[0][0].is_object());
        assert!(!result.to_string().contains(CANARY));
    }

    #[test]
    fn array_in_array_in_object_keeps_every_container_kind() {
        let result = redact_sensitive(&json!({"outer": [[{"token": CANARY}]]}));
        assert!(result["outer"].is_array());
        assert!(result["outer"][0].is_array());
        assert!(result["outer"][0][0].is_object());
        assert!(!result.to_string().contains(CANARY));
    }

    #[test]
    fn argument_is_unchanged_after_redaction() {
        let input = json!({"token": CANARY, "items": [{"password": CANARY}]});
        let snapshot = input.clone();
        let _ = redact_sensitive(&input);
        assert_eq!(input, snapshot);
        assert_eq!(input["token"], CANARY);
    }

    #[test]
    fn sensitive_key_with_container_value_is_fully_redacted() {
        let result = redact_sensitive(&json!({"token": {"a": 1}}));
        let redacted = result["token"].as_str().unwrap();
        assert!(redacted.starts_with("[REDACTED:"));
        assert!(!redacted.contains('{'));
        // Compact JSON is measured, not Python's `str()` (which is one character longer here).
        assert_eq!(redacted, "[REDACTED:7chars]");
    }

    #[test]
    fn account_email_object_is_replaced_with_its_json_length() {
        let result = redact_sensitive(&json!({
            "email": {"address": "user@example.com", "verified": true}
        }));
        assert_eq!(result["email"], "{\"ad...[REDACTED:46chars]");
    }

    /// An identifier-shaped key shows the first four characters of a container's text, as
    /// Python's `str(value)[:4]` does, which can be part of what the container holds; a
    /// credential-shaped key shows none.
    #[test]
    fn a_container_shows_a_prefix_only_under_an_identifier_shaped_key() {
        let result = redact_sensitive(&json!({
            "emails": ["ab@x.io"],
            "token": ["abcdefgh"],
        }));
        assert_eq!(result["emails"], "[\"ab...[REDACTED:11chars]");
        assert_eq!(result["token"], "[REDACTED:12chars]");
    }

    // ===================== depth cap (_MAX_REDACTION_DEPTH) =====================

    #[test]
    fn nesting_at_the_cap_is_still_walked() {
        let result = redact_sensitive(&nest(15, json!({"token": CANARY, "name": "n"})));
        let text = result.to_string();
        assert!(text.contains("\"name\":\"n\""));
        assert!(!text.contains("cyclic-or-too-deep"));
        assert!(!text.contains(CANARY));
        assert_eq!(descend(&result, 15)["token"], "[REDACTED:15chars]");
    }

    #[test]
    fn nesting_beyond_the_cap_is_replaced_by_placeholder() {
        let result = redact_sensitive(&nest(16, json!({"token": CANARY, "note": CANARY})));
        assert_eq!(descend(&result, 16), "[REDACTED:cyclic-or-too-deep]");
        assert!(!result.to_string().contains(CANARY));
    }

    #[test]
    fn empty_container_beyond_the_cap_is_replaced() {
        for leaf in [json!({}), json!([])] {
            let result = redact_sensitive(&nest(16, leaf));
            assert_eq!(descend(&result, 16), "[REDACTED:cyclic-or-too-deep]");
        }
    }

    #[test]
    fn scalar_beyond_the_cap_passes_through() {
        let result = redact_sensitive(&nest(16, json!("plain")));
        assert_eq!(descend(&result, 16), "plain");
    }

    #[test]
    fn sensitive_key_value_at_the_cap_is_length_redacted_not_replaced() {
        // The object sits at depth 15 and the token's container value at depth 16: the sensitive
        // key is handled before any depth check, so the value is measured, not replaced.
        let result = redact_sensitive(&nest(15, json!({"token": [[1]]})));
        assert_eq!(descend(&result, 15)["token"], "[REDACTED:5chars]");
    }

    #[test]
    fn mixed_object_array_nesting_beyond_the_cap_hides_the_canary() {
        let mut value = json!({"token": CANARY, "note": CANARY});
        for _ in 0..64 {
            value = json!({ "n": [value] });
        }
        let text = redact_sensitive(&value).to_string();
        assert!(text.contains("[REDACTED:cyclic-or-too-deep]"));
        assert!(!text.contains(CANARY));
    }

    #[test]
    fn very_deep_nesting_terminates() {
        // 2000 levels, not more: dropping the input is itself recursive, and the test thread
        // has a 2 MiB stack.
        let value = nest(2000, json!({"token": CANARY}));
        let text = redact_sensitive(&value).to_string();
        assert!(text.contains("[REDACTED:cyclic-or-too-deep]"));
        assert!(!text.contains(CANARY));
    }
}
