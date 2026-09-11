//! OUI Check API: the read-only (`GET`) half of `eero-api`'s `OUICheckAPI`.
//!
//! Ported from `eero-api src/eero/api/ouicheck.py`. This phase (3, GET-only) covers
//! `OUICheckAPI.get_ouicheck`; `run_ouicheck` (`ouicheck.py:56-78`, `POST` with an empty `{}`
//! body) is phase 5.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `OUICheckAPI` (`src/eero/api/ouicheck.py`).
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

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): `OUICheckAPI.run_ouicheck` (`ouicheck.py:56-78`) — `POST`
    // `routes::RUN_OUICHECK` (`networks/{network_id}/ouicheck`, same path as `GET_OUICHECK`)
    // with an empty `{}` body.
    // ---------------------------------------------------------------------------------------
}
