//! `Client` methods for the `ACCompatAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets AC compatibility information — returns the raw Eero API response.
    ///
    /// Ported from `get_ac_compat()` (`client.py:951-954`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_ac_compat(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.ac_compat().get_ac_compat(&network_id).await
    }
}
