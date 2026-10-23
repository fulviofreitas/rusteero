//! Routing API: read-only routing information for a network.
//!
//! Ported from `eero-api src/eero/api/routing.py` (v8.4.1). Python's `RoutingAPI` has exactly
//! one method (`get_routing`) and no mutating counterpart.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::rewrite_version;
use crate::routes::{self, ApiVersion};
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
    /// Ported from `eero-api src/eero/api/routing.py:35-74` (`RoutingAPI.get_routing`). Sends
    /// `GET` [`crate::routes::routing::GET_ROUTING_V8`], preferring `parent`'s own published
    /// `routing` link, and always on API version 2.2: the resolved URL is re-pinned with
    /// [`crate::links::rewrite_version`] whichever form `network_id` takes and whatever version
    /// the published link names. The API publishes the link on 2.3 for a current `User-Agent`;
    /// the known shape is the same on both versions, so the pin is for safety.
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
        let url = routes::routing::GET_ROUTING_V8.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        let url = rewrite_version(&url, ApiVersion::V2_2);
        self.transport
            .request(
                routes::routing::GET_ROUTING_V8.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }
}
