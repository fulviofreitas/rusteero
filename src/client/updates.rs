//! `Client` methods for the `UpdatesAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets firmware-update information — returns the raw Eero API response.
    ///
    /// Ported from `get_updates()` (`client.py:966-969`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_updates(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.updates().get_updates(&network_id).await
    }
}
