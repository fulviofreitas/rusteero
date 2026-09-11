//! Support API: the read-only (`GET`) half of `eero-api`'s `SupportAPI`.
//!
//! Ported from `eero-api src/eero/api/support.py`. This phase (GET-only) covers
//! `SupportAPI.get_support`; the mutation method is listed at the bottom of this file for
//! phase 5.
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

/// The read-only half of `eero-api`'s `SupportAPI` (`src/eero/api/support.py`).
///
/// Build one with [`SupportApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `SupportApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct SupportApi {
    transport: Arc<Transport>,
}

impl SupportApi {
    /// Wraps `transport` as a `SupportApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets support information for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/support.py:33-54` (`SupportAPI.get_support`). Sends
    /// `GET` [`crate::routes::GET_SUPPORT`] with `network_id` substituted into the path.
    pub async fn get_support(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_SUPPORT, &[("network_id", network_id)], None)
            .await
    }

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): SupportAPI's mutation method goes here — `request_support`
    // (POST, passthrough body, `support.py:56-81`). Already has a `Route` constant in
    // `src/routes.rs` (`REQUEST_SUPPORT`).
    // ---------------------------------------------------------------------------------------
}
