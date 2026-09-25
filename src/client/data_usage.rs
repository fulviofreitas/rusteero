//! `Client` methods for the `DataUsageAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;
use serde_json::json;

impl Client {
    /// Gets data usage statistics — returns the raw Eero API response.
    ///
    /// Ported from `get_data_usage()` (`client.py:936-944`), including `payload or {}`
    /// (`client.py:944`): a `None` payload is normalised to an empty JSON object before being
    /// attached as the (unusual, but intentional — see
    /// [`crate::endpoints::DataUsageApi::get_data_usage`]) `GET` request body. `auto_discover =
    /// false` — see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_data_usage(
        &self,
        network_id: Option<&str>,
        payload: Option<Value>,
        resource: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_data_usage(&network_id, payload.unwrap_or_else(|| json!({})), resource)
            .await
    }
}
