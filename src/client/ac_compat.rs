//! `Client` methods for the `ACCompatAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets AC compatibility information — returns the raw Eero API response.
    ///
    /// Ported from `get_ac_compat()` (`client.py:1800-1804`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_ac_compat(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .ac_compat()
            .get_ac_compat(&network_id, parent.as_ref())
            .await
    }
}
