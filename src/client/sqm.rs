//! `Client` methods for the `SqmAPI` domain, v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py:2169-2183` (the SQM group of `EeroClient`). Both
//! methods pass `**self._network_parent_kwargs(network_id)` in Python — this port's
//! [`Client::network_parent`] equivalent. `Client::set_sqm` replaces the four pre-v7.0.0
//! `Client` methods this file used to have (`set_sqm_enabled`/`set_sqm_bandwidth`/
//! `configure_sqm`/`set_sqm_auto`) — see [`crate::endpoints::sqm::SqmApi`]'s own module docs.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== SQM ====================

    /// Gets Smart Queue Management settings — returns the raw Eero API response.
    ///
    /// Ported from `get_sqm_settings()` (`client.py:2169-2174`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_sqm_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .sqm()
            .get_sqm_settings(&network_id, parent.as_ref())
            .await
    }

    /// Enables or disables SQM (Smart Queue Management) — returns the raw Eero API response
    /// (settings-class write; unconfirmed against a live network; like other writes to this
    /// endpoint, may trigger a mesh reboot).
    ///
    /// Ported from `set_sqm` (`client.py:2176-2183`). `auto_discover = false`. On success,
    /// invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .sqm()
            .set_sqm(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
