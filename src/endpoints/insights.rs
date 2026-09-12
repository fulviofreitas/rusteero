//! Insights API: `eero-api`'s `InsightsAPI`.
//!
//! Ported from `eero-api src/eero/api/insights.py`: `InsightsAPI.get_insights` and
//! `InsightsAPI.run_insights`.
//!
//! Every method here funnels through [`crate::transport::Transport`], which already implements
//! the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::json;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `InsightsAPI` (`src/eero/api/insights.py`).
///
/// Build one with [`InsightsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `InsightsApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct InsightsApi {
    transport: Arc<Transport>,
}

impl InsightsApi {
    /// Wraps `transport` as an `InsightsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Queries insights time-series data for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/insights.py:36-113` (`InsightsAPI.get_insights`).
    /// Sends `GET` [`crate::routes::GET_INSIGHTS`] (`networks/{network_id}/insights`) with
    /// `start`, `end`, `insight_type` and `cadence` attached as query parameters (via
    /// [`Transport::send_with_query`]), never as a request body — the four parameters map
    /// directly onto the `params` dict Python builds at `insights.py:95-100`.
    ///
    /// All four parameters are required, matching Python: `insights.py:52-68`'s signature makes
    /// `start`, `end` and `insight_type` mandatory keyword arguments, and although `cadence` has
    /// an SDK-supplied default of `"daily"` in Python (`insights.py:43`), this port takes it as a
    /// plain required argument rather than reproducing that default — callers wanting `"daily"`
    /// pass it explicitly.
    ///
    /// Python declares a module-level `INSIGHTS_CADENCES = ("hourly", "daily", "weekly")`
    /// (`insights.py:18`) but never enforces it anywhere in `get_insights`; this port
    /// deliberately does not add validation Python does not have. Any string is forwarded to the
    /// server as-is, which will reject an invalid `cadence` itself (via
    /// [`Error::Api`]).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send_with_query`]).
    pub async fn get_insights(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        insight_type: &str,
        cadence: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send_with_query(
                &routes::GET_INSIGHTS,
                &[("network_id", network_id)],
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                None,
            )
            .await
    }

    /// Runs insights analysis for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/insights.py:115-137` (`InsightsAPI.run_insights`).
    /// Sends `POST` [`crate::routes::RUN_INSIGHTS`] (`networks/{network_id}/insights`, the same
    /// path as [`crate::routes::GET_INSIGHTS`]) with an empty JSON object `{}` as the body
    /// (`insights.py:136`), not an absent body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn run_insights(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::RUN_INSIGHTS,
                &[("network_id", network_id)],
                Some(json!({})),
            )
            .await
    }
}
