//! `Client` methods for the `PermissionsAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/permissions.py` (v8.0.4) by way of `EeroClient`'s own
//! `permissions`-scoped wrapper in `client.py:2371-2374`.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the current user's permissions on the network — returns the raw Eero API response
    /// (verified read).
    ///
    /// Ported from `get_permissions()` (`client.py:2371-2374`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`).
    /// Never cached.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_permissions(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .permissions()
            .get_permissions(&network_id, parent.as_ref())
            .await
    }
}
