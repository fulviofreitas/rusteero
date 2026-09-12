//! Diagnostics API: `eero-api`'s `DiagnosticsAPI`.
//!
//! Ported from `eero-api src/eero/api/diagnostics.py`: `DiagnosticsAPI.get_diagnostics` and
//! `DiagnosticsAPI.run_diagnostics`.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::json;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `DiagnosticsAPI` (`src/eero/api/diagnostics.py`).
///
/// Build one with [`DiagnosticsApi::new`], wrapping a [`Transport`] already shared with the rest
/// of the (not-yet-built) `EeroApi` aggregator — `DiagnosticsApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct DiagnosticsApi {
    transport: Arc<Transport>,
}

impl DiagnosticsApi {
    /// Wraps `transport` as a `DiagnosticsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets network diagnostics information — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/diagnostics.py:33-54`
    /// (`DiagnosticsAPI.get_diagnostics`). Sends `GET` [`crate::routes::GET_DIAGNOSTICS`]
    /// (`networks/{network_id}/diagnostics`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_diagnostics(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_DIAGNOSTICS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// Runs network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/diagnostics.py:56-78` (`DiagnosticsAPI.run_diagnostics`).
    /// Sends `POST` [`crate::routes::RUN_DIAGNOSTICS`] (`networks/{network_id}/diagnostics`,
    /// the same path as [`crate::routes::GET_DIAGNOSTICS`]) with an empty JSON object `{}` as
    /// the body (`diagnostics.py:77`), not an absent body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn run_diagnostics(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::RUN_DIAGNOSTICS,
                &[("network_id", network_id)],
                Some(json!({})),
            )
            .await
    }
}
