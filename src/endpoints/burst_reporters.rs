//! Burst Reporters API: `eero-api`'s `BurstReportersAPI`, POST-only since v8.0.0.
//!
//! Ported from `eero-api src/eero/api/burst_reporters.py` at `v8.4.1`:
//! `BurstReportersAPI.get_burst_reporters` was removed upstream in v8.0.0 — the endpoint 404s;
//! the resource is POST-only — and is **not** reproduced here. Only `create_burst_reporter`
//! remains: it requests a live-usage burst.
//!
//! `create_burst_reporter` funnels through `Transport::request`, which already implements the
//! "not authenticated" precondition Python repeats at the top of the method (`get_auth_token()`
//! / `EeroAuthenticationException("Not authenticated")`) and every status-to-error mapping a
//! response can produce — so, unlike the Python source, this method never duplicates that guard.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// The deprecation notice logged whenever a caller supplies `reporter_data`
/// (`burst_reporters.py:90-96` at `v8.4.1`, where it is a `DeprecationWarning`).
const REPORTER_DATA_DEPRECATION: &str = "The reporter_data parameter of create_burst_reporter is \
     deprecated and will be removed in 9.0.0; the API operation takes no body.";

/// The uncharacterised-write operation logged on the deprecated non-empty-body path only
/// (`burst_reporters.py:106-108` at `v8.4.1`).
const DEPRECATED_BODY_OPERATION: &str =
    "request live-usage burst with a deprecated body for network";

/// `eero-api`'s `BurstReportersAPI` (`src/eero/api/burst_reporters.py`).
///
/// Build one with `BurstReportersApi::new`, wrapping a `Transport` already shared with the rest
/// of the `EeroApi` aggregator — `BurstReportersApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct BurstReportersApi {
    transport: Arc<Transport>,
}

impl BurstReportersApi {
    /// Wraps `transport` as a `BurstReportersApi`.
    ///
    /// Ported from `BurstReportersAPI.__init__` (`burst_reporters.py:25-31`), which wraps an
    /// `AuthAPI` rather than a bare transport handle — `Transport` already owns both the current
    /// session and the "not authenticated" precondition every Python method above re-derives by
    /// hand, so wrapping it directly is a strict simplification, not a behaviour change.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `POST /2.2/networks/{network_id}/burst_reporters` — request a live-usage burst.
    ///
    /// Ported from `BurstReportersAPI.create_burst_reporter` (`burst_reporters.py:36-112` at
    /// `v8.4.1`). Sends the two-character JSON string `""` (`Content-Type: application/json`)
    /// to the network's `burst_reporters` link, preferring `parent`'s own published link over
    /// the literal template when supplied (`routes::CREATE_BURST_REPORTER::link`). The observed
    /// response is `{"meta": .., "data": {"next_burst": "<timestamp string>"}}`.
    ///
    /// After a short settle (about 12 seconds) the device list's `usage.down_mbps` and
    /// `usage.up_mbps` values are refreshed, whereas before the POST they are zero or stale. This
    /// does not run a speed test and does not change any setting. At most one burst may be
    /// requested per minute per network.
    ///
    /// Live-verified 2026-10-20: one POST returned 200 with `data.next_burst`; 12 seconds later
    /// 103 of 116 connected devices carried non-zero `usage` values (all were zero before), the
    /// values had largely decayed again by 30 seconds, no WAN-level usage field appeared on the
    /// network envelope, and no node's reboot marker moved in the following five minutes. Read
    /// the device list about 12 seconds after the POST, not later. Never retry it in a loop. The
    /// check covered the `""` body only, so the call logs no uncharacterised-write warning on
    /// that path.
    ///
    /// # Deprecated
    ///
    /// The `reporter_data` parameter is deprecated upstream and slated for removal in `eero-api`
    /// 9.0.0; the API operation takes no body, so pass `None`. Whenever a value is supplied, a
    /// `WARN` line with the deprecation notice is logged first, before the URL is resolved
    /// (Python's `DeprecationWarning`). JSON `null` is the only Rust spelling of Python's
    /// `None`, so `Some(Value::Null)` is treated exactly like `None`: no notice, and the `""`
    /// body.
    ///
    /// Body selection follows Python truthiness (`if reporter_data:`). A falsy value (`null`,
    /// `false`, `0`, `""`, `[]` or `{}`) is ignored and the `""` body is sent; any other value
    /// is still forwarded unchanged as the JSON body, which remains uncharacterised and so
    /// logs the fixed uncharacterised-write warning (`"request live-usage burst with a
    /// deprecated body for network"`) after URL resolution, immediately before the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if the resolved URL is malformed, before any request is
    /// sent. Returns `Error::Authentication("Not authenticated")` if no valid session is
    /// configured, before any request is sent, or whatever other status-mapped error the
    /// request produces — see `Transport::request`.
    pub async fn create_burst_reporter(
        &self,
        network_id: &str,
        reporter_data: Option<Value>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let reporter_data = reporter_data.filter(|data| !data.is_null());
        if reporter_data.is_some() {
            tracing::warn!("{REPORTER_DATA_DEPRECATION}");
        }
        let url =
            routes::CREATE_BURST_REPORTER.resolve(self.transport.api_host(), network_id, parent)?;
        let body = match reporter_data {
            Some(data) if json_truthy(&data) => {
                warn_uncharacterised_write(DEPRECATED_BODY_OPERATION);
                RequestBody::Json(data)
            }
            _ => RequestBody::EmptyJsonString,
        };
        self.transport
            .request(routes::CREATE_BURST_REPORTER.method.clone(), url, &[], body)
            .await
    }
}

/// Python's `bool(value)` for a JSON value: `null`, `false`, `0` (integer or float), `""`, `[]`
/// and `{}` are falsy, everything else truthy.
fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_none_or(|float| float != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::json_truthy;

    #[test]
    fn python_falsy_values_are_falsy() {
        for falsy in [
            json!(null),
            json!(false),
            json!(0),
            json!(0.0),
            json!(-0.0),
            json!(""),
            json!([]),
            json!({}),
        ] {
            assert!(!json_truthy(&falsy), "{falsy} must be falsy");
        }
    }

    #[test]
    fn every_other_value_is_truthy() {
        for truthy in [
            json!(true),
            json!(1),
            json!(-1),
            json!(0.5),
            json!(" "),
            json!("0"),
            json!("false"),
            json!([null]),
            json!([0]),
            json!({"type": "burst"}),
            json!({"": null}),
        ] {
            assert!(json_truthy(&truthy), "{truthy} must be truthy");
        }
        assert!(json_truthy(&Value::from(u64::MAX)));
    }
}
