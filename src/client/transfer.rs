//! `Client` methods for the `TransferAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets transfer statistics for a network, or a single device on it — returns the raw Eero
    /// API response.
    ///
    /// Ported from `get_transfer_stats()` (`client.py:929-934`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
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
        self.api
            .transfer()
            .get_transfer_stats(&network_id, device_id)
            .await
    }
}
