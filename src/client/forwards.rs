//! `Client` methods for the `ForwardsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets port forwards — returns the raw Eero API response.
    ///
    /// Ported from `get_forwards()` (`client.py:910-913`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_forwards(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.forwards().get_forwards(&network_id).await
    }

    // ==================== Forwards ====================

    /// Creates a port forward on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_forward` (`eero-api src/eero/client.py:915-920`). `auto_discover =
    /// false`. Invalidates nothing: there is no `forwards` cache bucket (behaviour brief §2.1).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn create_forward(
        &self,
        forward_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .forwards()
            .create_forward(&network_id, forward_data)
            .await
    }

    /// Deletes a port forward from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_forward` (`eero-api src/eero/client.py:922-927`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::create_forward`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_forward(
        &self,
        forward_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .forwards()
            .delete_forward(&network_id, forward_id)
            .await
    }
}
