//! Routing API: read-only routing information for a network.
//!
//! Ported from `eero-api src/eero/api/routing.py`. Python's `RoutingAPI` has exactly one method
//! (`get_routing`) and no mutating counterpart — there is no phase-5 work for this module; a
//! later reader need not look for one.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `RoutingAPI` (`src/eero/api/routing.py`): a single read-only method returning a
/// network's routing information.
///
/// Build one with [`RoutingApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `RoutingApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct RoutingApi {
    transport: Arc<Transport>,
}

impl RoutingApi {
    /// Wraps `transport` as a `RoutingApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets routing information for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/routing.py:33-54` (`RoutingAPI.get_routing`). Sends
    /// `GET` [`crate::routes::GET_ROUTING`] with `network_id` substituted into the path.
    pub async fn get_routing(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_ROUTING, &[("network_id", network_id)], None)
            .await
    }
}
