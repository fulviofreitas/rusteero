//! Forwards API: the read-only (`GET`) half of `eero-api`'s `ForwardsAPI`.
//!
//! Ported from `eero-api src/eero/api/forwards.py`. This phase (GET-only) covers
//! `ForwardsAPI.get_forwards`; the mutation methods are listed at the bottom of this file for
//! phase 5.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! Port-forward objects carry device MACs and internal IP addresses; nothing in this module
//! logs a response body (see `.claude/rules/security-review.md`).

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `ForwardsAPI` (`src/eero/api/forwards.py`).
///
/// Build one with [`ForwardsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `ForwardsApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct ForwardsApi {
    transport: Arc<Transport>,
}

impl ForwardsApi {
    /// Wraps `transport` as a `ForwardsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the port forwards configured on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:33-54` (`ForwardsAPI.get_forwards`).
    /// Sends `GET` [`crate::routes::GET_FORWARDS`] with `network_id` substituted into the path.
    pub async fn get_forwards(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_FORWARDS, &[("network_id", network_id)], None)
            .await
    }

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): ForwardsAPI's mutation methods go here — `create_forward`
    // (POST, `forwards.py:56-79`) and `delete_forward`
    // (DELETE `networks/{network_id}/forwards/{forward_id}`, `forwards.py:81-103`), both
    // passthrough bodies. Each already has a `Route` constant in `src/routes.rs`
    // (`CREATE_FORWARD`/`DELETE_FORWARD`).
    // ---------------------------------------------------------------------------------------
}
