//! `Client` methods for the `RoutingAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network routing information — returns the raw Eero API response.
    ///
    /// Ported from `get_routing()` (`client.py:1413-1418`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`) —
    /// which may resolve to the API's 2.3 `routing` link instead of the 2.2 template fallback;
    /// see [`crate::routes::routing::GET_ROUTING_V8`]'s doc comment.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_routing(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .routing()
            .get_routing(&network_id, parent.as_ref())
            .await
    }
}
