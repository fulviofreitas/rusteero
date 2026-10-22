//! `Client` methods for the `UpdatesAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::params::require_hour;

impl Client {
    /// Gets firmware-update information — returns the raw Eero API response.
    ///
    /// Ported from `get_updates()` (`client.py:2347-2352`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_updates(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .updates()
            .get_updates(&network_id, parent.as_ref())
            .await
    }

    /// Applies a pending update — returns the raw Eero API response.
    ///
    /// Ported from `apply_update()` (`client.py:2354-2370`). `auto_discover = false`. Passes the
    /// cached network envelope as `parent=` (`+net`). **Reboot-class write**: applying an update
    /// reboots every node on the network. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn apply_update(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .updates()
            .apply_update(&network_id, parent.as_ref())
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets the hour of day the network applies firmware updates — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_preferred_update_hour()` (`client.py:2372-2403`). `hour` (`0..=23`) is
    /// validated first (`client.py:2397`), so an invalid hour never triggers network resolution
    /// and is reported even when no network can be resolved. `auto_discover = false`. **Passes no
    /// `parent=`**: the resource has no published link, so Python builds the path from the
    /// template. The current value lives in the network envelope's
    /// `updates.preferred_update_hour`, so a successful write drops `network[nid]`.
    ///
    /// **Unverified against a live account**: read the current value first, skip the write when
    /// it already matches, and never retry a failed write in a loop.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `hour` if `hour` is outside `0..=23`, before the
    /// network id is resolved and before any request. [`Error::MissingNetworkId`] if `network_id`
    /// is absent and no preferred network is set. Otherwise, whatever status-mapped [`Error`] the
    /// request produces. The cache is left untouched on any `Err`.
    pub async fn set_preferred_update_hour(
        &self,
        hour: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        require_hour("hour", hour)?;
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .updates()
            .set_preferred_update_hour(&network_id, hour)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
