//! `Client` methods for the `RoutingAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network routing information — returns the raw Eero API response.
    ///
    /// Ported from `get_routing()` (`client.py:1413-1418`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`). The
    /// request always goes to API version 2.2, even when that envelope publishes its `routing`
    /// link on 2.3; see [`crate::endpoints::routing::RoutingApi::get_routing`].
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
