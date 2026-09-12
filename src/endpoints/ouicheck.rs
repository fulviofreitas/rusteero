//! OUI Check API: `eero-api`'s `OUICheckAPI`.
//!
//! Ported from `eero-api src/eero/api/ouicheck.py`: `OUICheckAPI.get_ouicheck` and
//! `OUICheckAPI.run_ouicheck`.
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

/// `eero-api`'s `OUICheckAPI` (`src/eero/api/ouicheck.py`).
///
/// Build one with [`OUICheckApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `OUICheckApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct OUICheckApi {
    transport: Arc<Transport>,
}

impl OUICheckApi {
    /// Wraps `transport` as an `OUICheckApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets OUI (vendor MAC prefix) check results for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/ouicheck.py:33-54` (`OUICheckAPI.get_ouicheck`). Sends
    /// `GET` [`crate::routes::GET_OUICHECK`] (`networks/{network_id}/ouicheck`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_ouicheck(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_OUICHECK, &[("network_id", network_id)], None)
            .await
    }

    /// Runs an OUI (vendor MAC prefix) check for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/ouicheck.py:56-78` (`OUICheckAPI.run_ouicheck`). Sends
    /// `POST` [`crate::routes::RUN_OUICHECK`] (`networks/{network_id}/ouicheck`, the same path
    /// as [`crate::routes::GET_OUICHECK`]) with an empty JSON object `{}` as the body
    /// (`ouicheck.py:77`), not an absent body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn run_ouicheck(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::RUN_OUICHECK,
                &[("network_id", network_id)],
                Some(json!({})),
            )
            .await
    }
}
