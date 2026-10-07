//! Burst Reporters API: `eero-api`'s `BurstReportersAPI`, POST-only since v8.0.0.
//!
//! Ported from `eero-api src/eero/api/burst_reporters.py`: `BurstReportersAPI.get_burst_reporters`
//! was removed upstream in v8.0.0 — the endpoint 404s; the resource is POST-only — and is
//! **not** reproduced here. Only `create_burst_reporter` remains.
//!
//! `create_burst_reporter` funnels through `Transport::resource`, which already implements the
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

    /// `POST /2.2/networks/{network_id}/burst_reporters` — create a burst reporter.
    ///
    /// Ported from `BurstReportersAPI.create_burst_reporter` (`burst_reporters.py:56-81`).
    /// Prefers `parent`'s own published `burst_reporters` link over the literal template when
    /// supplied (`routes::CREATE_BURST_REPORTER::link`) — new in this port; the pre-v8.0.4
    /// revision of this method had no `parent=`/published-link concept at all. `reporter_data`
    /// is forwarded verbatim as the request's JSON body; Python accepts an arbitrary
    /// `reporter_data: Dict[str, Any]` with no client-side key allowlist or shape validation
    /// (`burst_reporters.py:74-79`), and this port does the same — a pure passthrough.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::resource`.
    pub async fn create_burst_reporter(
        &self,
        network_id: &str,
        reporter_data: Value,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url =
            routes::CREATE_BURST_REPORTER.resolve(self.transport.api_host(), network_id, parent)?;
        warn_uncharacterised_write("create burst reporter for network");
        self.transport
            .request(
                routes::CREATE_BURST_REPORTER.method.clone(),
                url,
                &[],
                RequestBody::Json(reporter_data),
            )
            .await
    }
}
