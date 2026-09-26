//! `Client` methods for the `DdnsAPI` domain (new in v8.0.0).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== Dynamic DNS ====================

    /// Enables dynamic DNS — returns the raw Eero API response.
    ///
    /// Ported from `enable_ddns` (`eero-api src/eero/client.py:2892-2899`). `auto_discover =
    /// false`. Passes the network's cached envelope (if fresh) as `parent`. Invalidates
    /// `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn enable_ddns(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self.api.ddns().enable(&network_id, parent.as_ref()).await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Disables dynamic DNS — returns the raw Eero API response.
    ///
    /// Ported from `disable_ddns` (`eero-api src/eero/client.py:2901-2908`). `auto_discover =
    /// false`. Passes the network's cached envelope (if fresh) as `parent`. Invalidates
    /// `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn disable_ddns(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .ddns()
            .disable(&network_id, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
