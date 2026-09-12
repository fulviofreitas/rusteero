//! Redaction of sensitive keys in a [`serde_json::Value`].
//!
//! Ported from `eero-api`'s `src/eero/logging.py`: [`redact_sensitive`] mirrors the
//! module-level `redact_sensitive` (`logging.py:125-146`), backed by private helpers ported
//! from `_redact_dict` (`logging.py:91-122`), `_redact_value` (`logging.py:71-88`) and
//! `_is_sensitive_key` (`logging.py:57-68`).
//!
//! `logging.py` also defines `SecureLoggerAdapter` and `get_secure_logger`, a
//! `logging.LoggerAdapter` subclass that transparently redacts every log call's arguments. Those
//! are **not** ported (the port plan §3.7): this crate uses `tracing` for
//! structured logging and `secrecy::SecretString` for token storage, which together remove the
//! need for an ambient auto-redacting logger. Call [`redact_sensitive`] explicitly wherever a raw
//! JSON body must be included in a log line.

use serde_json::{Map, Value};

/// Case-insensitive **substring** patterns that mark a JSON object key as sensitive.
///
/// Verbatim copy of `DEFAULT_SENSITIVE_PATTERNS` (`logging.py:12-32`). Matching is by substring,
/// not whole word, which is a deliberate (if surprising) upstream quirk: `session_expiry`
/// (contains `session`), `author` (contains `auth`) and even `monkey` (contains `key`) are all
/// treated as sensitive keys and redacted, exactly as in the Python original. Do not "fix" this
/// to word-boundary matching — it would silently break parity with `eero-api`.
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
];

/// Number of leading characters of a redacted value that remain visible.
///
/// Matches Python's default `visible_chars = 4` (`logging.py:79`, `logging.py:107`). The Python
/// functions accept `visible_chars` as an override parameter; nothing in `eero-api` ever calls
/// them with a non-default value, so this port fixes it as a constant rather than threading an
/// unused parameter through the public API.
const VISIBLE_CHARS: usize = 4;

/// Returns `true` if `key`, matched case-insensitively as a substring, indicates sensitive data.
///
/// Ported from `_is_sensitive_key` (`logging.py:57-68`).
fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    SENSITIVE_PATTERNS
        .iter()
        .any(|pattern| key_lower.contains(pattern))
}

/// Redacts a single value found under a sensitive key, returning its redacted string form.
///
/// Ported from `_redact_value` (`logging.py:71-88`). Checks are ordered exactly as in Python,
/// where `null`/`bool` must be special-cased before the generic numeric/string handling (in
/// Python, `bool` is a subtype of `int`, so `isinstance(value, bool)` has to run first).
fn redact_value(value: &Value) -> Value {
    let text = match value {
        Value::Null => return Value::String("[NONE]".to_string()),
        Value::Bool(_) => return Value::String("[REDACTED:bool]".to_string()),
        Value::Number(_) => return Value::String("[REDACTED:number]".to_string()),
        Value::String(s) => s.clone(),
        // Python's fallback branch runs `str(value)`, which for a dict/list produces a
        // Python-repr string (e.g. "{'a': 1}"). No Eero wire response ever nests an object or
        // array under a sensitive key, and no upstream test exercises this path, so this port
        // approximates with compact JSON instead of reproducing Python's repr syntax exactly.
        Value::Array(_) | Value::Object(_) => value.to_string(),
    };

    if text.is_empty() {
        return Value::String("[EMPTY]".to_string());
    }

    let length = text.chars().count();
    if length <= VISIBLE_CHARS {
        return Value::String(format!("[REDACTED:{length}chars]"));
    }

    let visible: String = text.chars().take(VISIBLE_CHARS).collect();
    Value::String(format!("{visible}...[REDACTED:{length}chars]"))
}

/// Recursively redacts sensitive keys in a JSON object.
///
/// Ported from `_redact_dict` (`logging.py:91-122`): a sensitive key's value is replaced by
/// [`redact_value`]'s output regardless of its type; a non-sensitive key whose value is itself an
/// object recurses; a non-sensitive key whose value is an array is walked element-wise, and only
/// object elements of that array recurse (non-object items, e.g. plain strings, pass through
/// unchanged, matching Python's `isinstance(item, dict)` guard); every other value is copied
/// through untouched.
fn redact_object(map: &Map<String, Value>) -> Value {
    let mut result = Map::with_capacity(map.len());
    for (key, value) in map {
        let redacted = if is_sensitive_key(key) {
            redact_value(value)
        } else {
            match value {
                Value::Object(nested) => redact_object(nested),
                Value::Array(items) => Value::Array(
                    items
                        .iter()
                        .map(|item| match item {
                            Value::Object(nested) => redact_object(nested),
                            other => other.clone(),
                        })
                        .collect(),
                ),
                other => other.clone(),
            }
        };
        result.insert(key.clone(), redacted);
    }
    Value::Object(result)
}

/// Redacts sensitive data from a JSON value for safe logging.
///
/// Ported from `redact_sensitive` (`logging.py:125-146`). Only [`Value::Object`] is processed
/// (recursively, via the private `redact_object`); every other JSON type is returned unchanged —
/// including a *top-level* array. This asymmetry is a direct, intentional port of the Python
/// behaviour: `redact_sensitive` only special-cases `isinstance(value, dict)`, so an object
/// nested inside another object's array value gets recursed into, but that very same array
/// passed as the top-level input is returned as-is, sensitive keys and all.
///
/// # Examples
///
/// ```
/// use rusteero::redact::redact_sensitive;
/// use serde_json::json;
///
/// let redacted = redact_sensitive(&json!({"user_token": "abc123456", "name": "John"}));
/// assert_eq!(redacted["name"], "John");
/// assert_eq!(redacted["user_token"], "abc1...[REDACTED:9chars]");
///
/// // Non-object top-level values pass through untouched.
/// assert_eq!(redact_sensitive(&json!("hello")), json!("hello"));
/// ```
pub fn redact_sensitive(value: &Value) -> Value {
    match value {
        Value::Object(map) => redact_object(map),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_sensitive_key, redact_sensitive, redact_value};
    use serde_json::json;

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
        assert!(!is_sensitive_key("name"));
        assert!(!is_sensitive_key("email"));
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

    // ===================== _redact_value (TestRedactValue) =====================

    #[test]
    fn redacts_null() {
        assert_eq!(redact_value(&json!(null)), json!("[NONE]"));
    }

    #[test]
    fn redacts_empty_string() {
        assert_eq!(redact_value(&json!("")), json!("[EMPTY]"));
    }

    #[test]
    fn redacts_short_string() {
        assert_eq!(redact_value(&json!("abc")), json!("[REDACTED:3chars]"));
        assert_eq!(redact_value(&json!("abcd")), json!("[REDACTED:4chars]"));
    }

    #[test]
    fn redacts_long_string() {
        let result = redact_value(&json!("secret123456"));
        let text = result.as_str().unwrap();
        assert!(text.starts_with("secr"));
        assert!(text.contains("[REDACTED:12chars]"));
    }

    #[test]
    fn redacts_boolean() {
        assert_eq!(redact_value(&json!(true)), json!("[REDACTED:bool]"));
        assert_eq!(redact_value(&json!(false)), json!("[REDACTED:bool]"));
    }

    #[test]
    fn redacts_numbers() {
        assert_eq!(redact_value(&json!(12345)), json!("[REDACTED:number]"));
        assert_eq!(redact_value(&json!(3.5)), json!("[REDACTED:number]"));
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
    fn top_level_array_is_not_recursed() {
        // Quirk ported verbatim: redact_sensitive only special-cases dict at the top level, so
        // a bare top-level array is returned unchanged even though it contains a sensitive key.
        let input = json!([{"token": "abc123"}]);
        assert_eq!(redact_sensitive(&input), input);
    }
}
