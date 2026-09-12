//! Burst Reporters API: `eero-api`'s `BurstReportersAPI`.
//!
//! Ported from `eero-api src/eero/api/burst_reporters.py`:
//! `BurstReportersAPI.get_burst_reporters` and `BurstReportersAPI.create_burst_reporter`.
//!
//! Every method here funnels through `Transport::send`, which already implements the "not
//! authenticated" precondition Python repeats at the top of each method (`get_auth_token()` /
//! `EeroAuthenticationException("Not authenticated")`) and every status-to-error mapping a
//! response can produce — so, unlike the Python source, no method below duplicates that guard.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `BurstReportersAPI` (`src/eero/api/burst_reporters.py`).
///
/// Build one with `BurstReportersApi::new`, wrapping a `Transport` already shared with the rest
/// of the (not-yet-built) `EeroApi` aggregator — `BurstReportersApi` never constructs or owns a
/// `Transport` itself.
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

    /// `GET /2.2/networks/{network_id}/burst_reporters` — list burst reporters.
    ///
    /// Ported from `BurstReportersAPI.get_burst_reporters` (`burst_reporters.py:33-54`).
    /// Returns the raw `{"meta": …, "data": [...]}` envelope; this method never inspects or
    /// reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn get_burst_reporters(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_BURST_REPORTERS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/burst_reporters` — create a burst reporter.
    ///
    /// Ported from `BurstReportersAPI.create_burst_reporter` (`burst_reporters.py:56-81`).
    /// Sends `routes::CREATE_BURST_REPORTER` with `reporter_data` attached as the request's JSON
    /// body exactly as given. Python accepts an arbitrary `reporter_data: Dict[str, Any]` and
    /// forwards it verbatim as the request JSON with no client-side key allowlist or shape
    /// validation; this port does the same — a pure passthrough.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn create_burst_reporter(
        &self,
        network_id: &str,
        reporter_data: Value,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::CREATE_BURST_REPORTER,
                &[("network_id", network_id)],
                Some(reporter_data),
            )
            .await
    }
}
