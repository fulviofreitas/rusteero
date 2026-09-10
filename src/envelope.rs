//! Lossless view over the raw `{"meta": …, "data": …}` wire envelope.
//!
//! There is no Python original for this module: `eero-api` hands callers a bare `dict` parsed
//! straight from the response body (`api/base.py:208-218`). This crate keeps the same raw
//! contract — every endpoint method still returns `Result<Envelope, crate::error::Error>` over
//! the untouched wire payload — but wraps it in [`Envelope`] so callers get ergonomic access to
//! the common `meta` fields without losing anything: [`Envelope::into_value`] always returns
//! exactly what was parsed in, and unknown `meta` keys survive in [`Meta::extra`]. See
//! `.claude/tasks/rust-port-plan.md` §3.3.
//!
//! # Examples
//!
//! ```
//! use rusteero::envelope::Envelope;
//! use serde_json::json;
//!
//! let env = Envelope::from_value(json!({
//!     "meta": {"code": 200, "server_time": "2026-09-10T00:00:00Z"},
//!     "data": {"networks": []},
//! }));
//!
//! assert_eq!(env.meta().code, Some(200));
//! assert_eq!(env.data()["networks"], json!([]));
//! assert_eq!(env.into_value(), json!({
//!     "meta": {"code": 200, "server_time": "2026-09-10T00:00:00Z"},
//!     "data": {"networks": []},
//! }));
//! ```

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::error::Error;

/// A `serde_json::Value` constant used as the return value of [`Envelope::data`] when the wire
/// payload has no `"data"` key. `'static` so it can be handed out as `&Value` for any borrow of
/// an [`Envelope`], regardless of that envelope's own lifetime.
const NULL_VALUE: Value = Value::Null;

/// A lossless view over a raw `{"meta": …, "data": …}` wire envelope.
///
/// `Envelope` never transforms, reorders or drops anything from the JSON value it was built
/// from: [`Envelope::into_value`] and [`Envelope::as_value`] always expose exactly what
/// [`Envelope::from_value`] was given. [`Envelope::meta`] and [`Envelope::data`] are convenience
/// views computed on demand, not a separate copy of the truth.
///
/// The inner value is private so that nothing outside this module can construct an `Envelope`
/// that doesn't round-trip; the only way in is [`Envelope::from_value`] (or the equivalent
/// [`From<Value>`] impl) and the only way out is [`Envelope::into_value`] / [`Envelope::as_value`].
///
/// `Debug` is implemented by hand (see the impl below) to print a short summary instead of the
/// payload, because a wire body can carry a session token or other secret that must never reach
/// a log line via an incidental `{:?}`.
#[derive(Clone, PartialEq)]
pub struct Envelope {
    raw: Value,
}

impl Envelope {
    /// Wraps a raw JSON value as an `Envelope`, without validating or transforming it in any
    /// way. `value` need not even be a JSON object — [`Envelope::meta`] and [`Envelope::data`]
    /// degrade gracefully (see their docs) for any shape that isn't the expected
    /// `{"meta": …, "data": …}` object.
    #[must_use]
    pub fn from_value(value: Value) -> Self {
        Self { raw: value }
    }

    /// Returns an `Envelope` over an empty JSON object (`{}`).
    ///
    /// This is what an empty or `204 No Content` response body becomes, matching `eero-api`'s
    /// `api/base.py:208-211` (`{}` on an empty body, `meta` and `data` both absent).
    #[must_use]
    pub fn empty() -> Self {
        Self::from_value(Value::Object(Map::new()))
    }

    /// Parses and returns the envelope's `meta` object as a [`Meta`].
    ///
    /// Never panics and never fails: if `"meta"` is absent, or is present but is not a JSON
    /// object (e.g. a bare string or number), this returns [`Meta::default`] — every named
    /// field `None`, `extra` empty. Unknown keys found alongside the four named fields are
    /// preserved verbatim in [`Meta::extra`] so nothing observed on the wire is lost.
    #[must_use]
    pub fn meta(&self) -> Meta {
        let Some(map) = self.raw.get("meta").and_then(Value::as_object) else {
            return Meta::default();
        };

        let mut meta = Meta::default();
        let mut extra = Map::with_capacity(map.len());
        for (key, value) in map {
            match key.as_str() {
                "code" => meta.code = parse_code(value),
                "server_time" => meta.server_time = value.as_str().map(str::to_owned),
                "error" => meta.error = value.as_str().map(str::to_owned),
                "message" => meta.message = value.as_str().map(str::to_owned),
                _ => {
                    extra.insert(key.clone(), value.clone());
                }
            }
        }
        meta.extra = extra;
        meta
    }

    /// Returns the envelope's `data` value.
    ///
    /// Returns `&Value::Null` when `"data"` is absent from the wire payload (or when the
    /// payload isn't a JSON object at all), rather than panicking or requiring callers to
    /// unwrap an `Option`.
    #[must_use]
    pub fn data(&self) -> &Value {
        self.raw.get("data").unwrap_or(&NULL_VALUE)
    }

    /// Deserializes the envelope's `data` value into `T`.
    ///
    /// This is the only place in the crate that produces [`Error::Json`]: an invalid JSON
    /// *body* on a 2xx response is instead mapped to [`Error::Api`] at the transport layer
    /// (matching `eero-api`'s `api/base.py:216-219`), so by the time an `Envelope` exists the
    /// bytes were already valid JSON — a `data_as::<T>()` failure means `T`'s shape didn't
    /// match `data`, not that the response was malformed.
    pub fn data_as<T: DeserializeOwned>(&self) -> Result<T, Error> {
        Ok(serde_json::from_value(self.data().clone())?)
    }

    /// Consumes the envelope and returns the exact JSON value it was built from.
    ///
    /// Named to mirror `into_inner`-style APIs while making the "this is the wire payload,
    /// unmodified" contract explicit at the call site.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.raw
    }

    /// Borrows the exact JSON value the envelope was built from.
    #[must_use]
    pub fn as_value(&self) -> &Value {
        &self.raw
    }
}

impl From<Value> for Envelope {
    fn from(value: Value) -> Self {
        Self::from_value(value)
    }
}

/// A manual `Debug` impl that prints a summary — `meta.code` plus the *kind* of `data` (object,
/// array, string, …) — rather than the raw payload.
///
/// A wire body can carry a session token or other secret (see `crate::redact`); an
/// auto-derived `Debug` that dumped the full `Value` would turn every incidental `{:?}` (a
/// `dbg!()`, an `unwrap_or_else(|e| panic!("{e:?}, {self:?}"))`, a `tracing` field) into a
/// potential credential leak. This impl never touches string contents, only their JSON type.
impl std::fmt::Debug for Envelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Envelope")
            .field("meta_code", &self.meta().code)
            .field("data_kind", &value_kind(self.data()))
            .finish()
    }
}

/// Returns the JSON type name of `value` (`"null"`, `"bool"`, `"number"`, `"string"`,
/// `"array"`, `"object"`), for use in [`Envelope`]'s `Debug` summary. Never inspects a
/// `Value::String`'s contents.
fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Parses a `meta.code`-shaped value into a `u16`, accepting either a JSON number or a numeric
/// string. Both shapes have been observed across `eero-api`'s test fixtures; neither Python nor
/// this crate ever inspects `meta.code` to make a control-flow decision (status mapping is
/// driven entirely by the HTTP status line, per `.claude/docs/architecture.md` §7), so being
/// liberal here costs nothing and avoids `Meta::code` silently going `None` on a server that
/// happens to quote it.
fn parse_code(value: &Value) -> Option<u16> {
    match value {
        Value::Number(n) => n.as_u64().and_then(|code| u16::try_from(code).ok()),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// A parsed, best-effort view over an envelope's `meta` object.
///
/// Every field is independently optional: a real Eero response always sets `code`, but this
/// type makes no assumption about which fields any given response includes, and
/// [`Envelope::meta`] never fails — a missing or malformed `meta` simply yields every field
/// `None` (see that method's docs). Fields are `pub` rather than hidden behind accessors so
/// callers can pattern-match or destructure a `Meta` directly, matching the direct field access
/// shown in `.claude/docs/technical.md` §2 (`env.meta().code`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    /// The response's status code, if `meta.code` was present and parsed as a `u16` (accepting
    /// either a JSON number or a numeric string).
    pub code: Option<u16>,
    /// The server's timestamp for this response, if `meta.server_time` was present and a JSON
    /// string.
    pub server_time: Option<String>,
    /// A machine-readable error code, if `meta.error` was present and a JSON string. This is
    /// the field transport-layer refresh handling matches against
    /// [`crate::consts::REFRESH_ERROR_CODE`].
    pub error: Option<String>,
    /// A human-readable message, if `meta.message` was present and a JSON string.
    pub message: Option<String>,
    /// Any `meta` keys other than `code`, `server_time`, `error` and `message`, preserved
    /// verbatim so a `meta` shape this type doesn't explicitly model is never silently dropped.
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::{Envelope, Meta};
    use crate::error::Error;
    use serde::Deserialize;
    use serde_json::json;

    // ===================== construction / round-trip =====================

    #[test]
    fn empty_envelope_is_over_an_empty_object() {
        let env = Envelope::empty();
        assert_eq!(env.as_value(), &json!({}));
        assert_eq!(env.meta(), Meta::default());
        assert_eq!(env.data(), &json!(null));
    }

    #[test]
    fn from_value_round_trips_byte_identically() {
        let original = json!({
            "meta": {"code": 200, "server_time": "2026-09-10T00:00:00Z", "extra_field": "kept"},
            "data": {"networks": [{"name": "Home", "url": "/2.2/networks/1"}]},
        });
        let env = Envelope::from_value(original.clone());
        assert_eq!(env.as_value(), &original);
        assert_eq!(env.clone().into_value(), original);
    }

    #[test]
    fn from_value_impl_matches_from_value_method() {
        let value = json!({"meta": {"code": 200}, "data": {}});
        let via_from: Envelope = value.clone().into();
        let via_ctor = Envelope::from_value(value);
        assert_eq!(via_from, via_ctor);
    }

    // ===================== meta() =====================

    #[test]
    fn meta_missing_yields_all_none() {
        let env = Envelope::from_value(json!({"data": {}}));
        assert_eq!(env.meta(), Meta::default());
    }

    #[test]
    fn meta_that_is_not_an_object_yields_all_none() {
        for bad_meta in [json!("oops"), json!(42), json!([1, 2]), json!(null)] {
            let env = Envelope::from_value(json!({"meta": bad_meta, "data": {}}));
            assert_eq!(env.meta(), Meta::default(), "meta = {bad_meta:?}");
        }
    }

    #[test]
    fn meta_code_accepts_json_number() {
        let env = Envelope::from_value(json!({"meta": {"code": 200}}));
        assert_eq!(env.meta().code, Some(200));
    }

    #[test]
    fn meta_code_accepts_numeric_string() {
        let env = Envelope::from_value(json!({"meta": {"code": "200"}}));
        assert_eq!(env.meta().code, Some(200));
    }

    #[test]
    fn meta_code_rejects_non_numeric_string() {
        let env = Envelope::from_value(json!({"meta": {"code": "not-a-number"}}));
        assert_eq!(env.meta().code, None);
    }

    #[test]
    fn meta_parses_all_named_fields_and_keeps_unknown_keys_in_extra() {
        let env = Envelope::from_value(json!({
            "meta": {
                "code": 401,
                "server_time": "2026-09-10T00:00:00Z",
                "error": "error.session.refresh",
                "message": "please refresh",
                "request_id": "abc-123",
            },
        }));
        let meta = env.meta();
        assert_eq!(meta.code, Some(401));
        assert_eq!(meta.server_time.as_deref(), Some("2026-09-10T00:00:00Z"));
        assert_eq!(meta.error.as_deref(), Some("error.session.refresh"));
        assert_eq!(meta.message.as_deref(), Some("please refresh"));
        assert_eq!(meta.extra.get("request_id"), Some(&json!("abc-123")));
        assert_eq!(meta.extra.len(), 1);
    }

    // ===================== data() =====================

    #[test]
    fn data_missing_yields_null() {
        let env = Envelope::from_value(json!({"meta": {"code": 200}}));
        assert_eq!(env.data(), &json!(null));
    }

    #[test]
    fn data_present_is_returned_unchanged() {
        let env = Envelope::from_value(json!({"data": {"networks": []}}));
        assert_eq!(env.data(), &json!({"networks": []}));
    }

    // ===================== data_as::<T>() =====================

    #[derive(Debug, Deserialize, PartialEq)]
    struct Network {
        name: String,
    }

    #[test]
    fn data_as_succeeds_on_matching_shape() {
        let env = Envelope::from_value(json!({"data": {"name": "Home"}}));
        let network: Network = env.data_as().expect("shape matches");
        assert_eq!(
            network,
            Network {
                name: "Home".to_owned()
            }
        );
    }

    #[test]
    fn data_as_fails_on_mismatched_shape_with_json_error() {
        let env = Envelope::from_value(json!({"data": {"unexpected": "shape"}}));
        let result: Result<Network, Error> = env.data_as();
        assert!(matches!(result, Err(Error::Json(_))));
    }

    #[test]
    fn data_as_fails_when_data_is_null() {
        let env = Envelope::empty();
        let result: Result<Network, Error> = env.data_as();
        assert!(matches!(result, Err(Error::Json(_))));
    }

    // ===================== Debug redaction =====================

    #[test]
    fn debug_does_not_leak_a_token_embedded_in_the_payload() {
        let env = Envelope::from_value(json!({
            "meta": {"code": 200},
            "data": {"user_token": "super-secret-session-token-value"},
        }));
        let rendered = format!("{env:?}");
        assert!(!rendered.contains("super-secret-session-token-value"));
        assert!(rendered.contains("200"));
        assert!(rendered.contains("object"));
    }

    #[test]
    fn debug_summary_reflects_missing_meta_and_null_data() {
        let env = Envelope::from_value(json!({}));
        let rendered = format!("{env:?}");
        assert!(rendered.contains("null"));
    }
}
