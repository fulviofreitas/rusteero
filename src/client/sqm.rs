//! `Client` methods for the `SqmAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== SQM ====================

    /// Gets Smart Queue Management settings — returns the raw Eero API response.
    ///
    /// Ported from `get_sqm_settings()` (`client.py:1204-1207`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_sqm_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.sqm().get_sqm_settings(&network_id).await
    }

    // ==================== SQM (mutations) ====================

    /// Enables or disables SQM (Smart Queue Management) — returns the raw Eero API response.
    ///
    /// Ported from `set_sqm_enabled` (`eero-api src/eero/client.py:1209-1214`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm_enabled(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.sqm().set_sqm_enabled(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets SQM upload/download bandwidth limits — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::clear_custom_dns`]'s docs for the reasoning
    /// pattern this follows. `auto_discover = false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm_bandwidth(
        &self,
        upload_mbps: Option<u32>,
        download_mbps: Option<u32>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .sqm()
            .set_sqm_bandwidth(&network_id, upload_mbps, download_mbps)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Configures SQM (enable/disable plus optional bandwidth limits) in one call — returns the
    /// raw Eero API response.
    ///
    /// Ported from `configure_sqm` (`eero-api src/eero/client.py:1216-1225`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn configure_sqm(
        &self,
        enabled: bool,
        upload_mbps: Option<u32>,
        download_mbps: Option<u32>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .sqm()
            .configure_sqm(&network_id, enabled, upload_mbps, download_mbps)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets SQM to automatic mode — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::clear_custom_dns`]'s docs. `auto_discover =
    /// false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm_auto(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.sqm().set_sqm_auto(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }
}
