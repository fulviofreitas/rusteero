//! `Client` methods for the `SupportAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets support information — returns the raw Eero API response.
    ///
    /// Ported from `get_support()` (`client.py:1852-1857`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`). The
    /// request always goes to API version 2.2, even when that envelope publishes its `support`
    /// link on 2.3 (2.3 answers with a `contacts` array where 2.2 answers with a flat object); see
    /// [`crate::endpoints::support::SupportApi::get_support`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_support(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .support()
            .get_support(&network_id, parent.as_ref())
            .await
    }

    /// Files a support request for the network — returns the raw Eero API response.
    ///
    /// Ported from `request_support` (`eero-api src/eero/client.py:1859-1891`). `auto_discover =
    /// false`. `request_data` is forwarded unchanged and unvalidated: its fields are not
    /// documented, so the caller owns them. `parent` is the network's own envelope; when `None`,
    /// the cached network envelope is used if one is fresh, and a `parent` the caller supplies is
    /// forwarded unchanged and wins over the cache. Invalidates no cache entry. Unlike
    /// [`Client::get_support`], the request is not pinned to API version 2.2: it follows the
    /// network's published `support` link on whatever version that names.
    ///
    /// # Warning
    ///
    /// This write has not been confirmed against a live network, and logs one uncharacterised-
    /// write warning. Never retry it in a loop.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn request_support(
        &self,
        request_data: Value,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent_or(network_id.as_str(), parent);
        self.api
            .support()
            .request_support(&network_id, request_data, parent.as_ref())
            .await
    }
}
