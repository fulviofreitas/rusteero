//! Routing API: read-only routing information for a network.
//!
//! Ported from `eero-api src/eero/api/routing.py` (v8.0.4). Python's `RoutingAPI` has exactly
//! one method (`get_routing`) and no mutating counterpart.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `RoutingAPI` (`src/eero/api/routing.py`): a single read-only method returning a
/// network's routing information.
///
/// Build one with [`RoutingApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `RoutingApi` never constructs or owns a `Transport` itself.
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
    /// Ported from `eero-api src/eero/api/routing.py:35-65` (`RoutingAPI.get_routing`). Sends
    /// `GET` [`crate::routes::routing::GET_ROUTING_V8`], preferring `parent`'s own published
    /// `routing` link — which may resolve to a different API version (2.3) than the 2.2 template
    /// fallback; see that route constant's own doc comment.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_routing(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::routing::GET_ROUTING_V8,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }
}
