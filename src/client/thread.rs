//! `Client` methods for the `ThreadAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets Thread protocol status — returns the raw Eero API response.
    ///
    /// Ported from `get_thread()` (`client.py:864-867`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_thread(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.thread().get_thread(&network_id).await
    }
}
