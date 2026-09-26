//! `Client` methods for the `WanAPI` domain (new in v8.0.0).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    // ==================== Multi-static IP & Secondary WAN ====================

    /// Gets the multi-static-IP configuration — returns the raw Eero API response.
    ///
    /// Ported from `get_multistaticip` (`eero-api src/eero/client.py:3034-3043`).
    /// `auto_discover = false`. Passes the network's cached envelope (if fresh) as `parent`. The
    /// API answers 404 with `error.network.multistaticip_not_found` on a network without the
    /// feature.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_multistaticip(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .wan()
            .get_multistaticip(&network_id, parent.as_ref())
            .await
    }

    /// Sets the multi-static-IP configuration — returns the raw Eero API response.
    ///
    /// Ported from `set_multistaticip` (`eero-api src/eero/client.py:3045-3052`).
    /// `auto_discover = false`. No `parent=` forwarded — matches
    /// [`crate::endpoints::wan::WanApi::set_multistaticip`], which never accepts one.
    /// Invalidates `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_multistaticip(
        &self,
        config: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .wan()
            .set_multistaticip(&network_id, config)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets the secondary WAN configuration for every device in one call — returns the raw Eero
    /// API response.
    ///
    /// Ported from `set_secondary_wan_config` (`eero-api src/eero/client.py:3054-3061`).
    /// `auto_discover = false`. No `parent=` forwarded. Invalidates `network[{nid}]`
    /// unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_secondary_wan_config(
        &self,
        config: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .wan()
            .set_secondary_wan_config(&network_id, config)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Allows or denies a single device's secondary-WAN access — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_device_secondary_wan_access` (`eero-api src/eero/client.py:3063-3070`).
    /// `auto_discover = false`. Invalidates the **device** cache (`Dev{nid,mac}` +
    /// `DevList{nid}`, via `Client::invalidate_device_cache`) after the write, not the network
    /// cache — matches Python's `self._invalidate_device_cache(network_id, mac)` exactly, even
    /// though the domain call lives on `WanApi`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_device_secondary_wan_access(
        &self,
        mac: &str,
        deny: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .wan()
            .set_device_secondary_wan_access(&network_id, mac, deny)
            .await?;
        self.invalidate_device_cache(&network_id, mac);
        Ok(response)
    }
}
