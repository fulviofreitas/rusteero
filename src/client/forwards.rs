//! `Client` methods for the `ForwardsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets port forwards — returns the raw Eero API response.
    ///
    /// Ported from `get_forwards()` (`eero-api src/eero/client.py:1538-1543`). `auto_discover =
    /// false`. Passes the network's cached envelope (if fresh) as `parent`, matching
    /// `_network_parent_kwargs`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_forwards(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .forwards()
            .get_forwards(&network_id, parent.as_ref())
            .await
    }

    // ==================== Forwards ====================

    /// Creates a port forward on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_forward` (`eero-api src/eero/client.py:1545-1552`). `auto_discover =
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
        let parent = self.network_parent(&network_id);
        self.api
            .forwards()
            .create_forward(&network_id, forward_data, parent.as_ref())
            .await
    }

    /// Updates a port forward on the network — returns the raw Eero API response.
    ///
    /// Ported from `update_forward` (`eero-api src/eero/client.py:1554-1563`). `auto_discover =
    /// false` — unlike every other row in this file, `network_id` is resolved and forwarded to
    /// the domain call as `network=Some(..)` unconditionally (Python's `network=network_id`
    /// after `_ensure_network_id`), not passed straight through as `Option`. No `parent=`
    /// forwarded, matching Python exactly. Invalidates nothing — see
    /// [`Client::create_forward`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn update_forward(
        &self,
        forward_id: &str,
        forward_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .forwards()
            .update_forward(forward_id, forward_data, Some(&network_id), None)
            .await
    }

    /// Deletes a port forward from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_forward` (`eero-api src/eero/client.py:1564-1569`). `auto_discover =
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
