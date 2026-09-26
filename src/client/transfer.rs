//! `Client` methods for the `TransferAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets transfer statistics for a network, or a single device on it — returns the raw Eero
    /// API response.
    ///
    /// Ported from `get_transfer_stats()` (`client.py:1571-1578`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_transfer_stats(
        &self,
        network_id: Option<&str>,
        device_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .transfer()
            .get_transfer_stats(&network_id, device_id, parent.as_ref())
            .await
    }
}
