//! `Client` methods for the `SettingsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network settings — returns the raw Eero API response.
    ///
    /// Ported from `get_settings()` (`client.py:819-822`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.settings().get_settings(&network_id).await
    }
}
