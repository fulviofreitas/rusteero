//! `Client` methods for the `UpdatesAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets firmware-update information — returns the raw Eero API response.
    ///
    /// Ported from `get_updates()` (`client.py:1827-1831`). `auto_discover = false` — see
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
    /// Ported from `apply_update()` (`client.py:1834-1849`). `auto_discover = false`. Passes the
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
}
