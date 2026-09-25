//! `Client` methods for the `RoutingAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network routing information — returns the raw Eero API response.
    ///
    /// Ported from `get_routing()` (`client.py:859-862`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_routing(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.routing().get_routing(&network_id).await
    }
}
