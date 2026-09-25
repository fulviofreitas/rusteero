//! `Client` methods for the `BackupAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== Backup Network ====================

    /// Gets backup-network configuration — returns the raw Eero API response.
    ///
    /// Ported from `get_backup_network()` (`client.py:1098-1101`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_backup_network(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.backup().get_backup_network(&network_id).await
    }

    /// Gets backup-network status — returns the raw Eero API response.
    ///
    /// Ported from `get_backup_status()` (`client.py:1103-1106`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_backup_status(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.backup().get_backup_status(&network_id).await
    }

    // ==================== Backup Network (mutations) ====================

    /// Enables or disables the backup network — returns the raw Eero API response.
    ///
    /// Ported from `set_backup_network` (`eero-api src/eero/client.py:1108-1113`).
    /// `auto_discover = false`. Invalidates nothing: Python's method body has no
    /// `del self._cache[...]` call (behaviour brief §2, "Verified to invalidate nothing at
    /// all"), and there is no `backup` cache bucket to keep consistent with
    /// `get_backup_network`/`get_backup_status` either way, since neither of those is one of the
    /// eight cached getters.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_backup_network(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .set_backup_network(&network_id, enabled)
            .await
    }

    /// Configures the backup network (enable/disable and/or a phone number) — returns the raw
    /// Eero API response.
    ///
    /// Ported from `configure_backup_network` (`eero-api src/eero/client.py:1115-1125`).
    /// `auto_discover = false`. Invalidates nothing — see [`Client::set_backup_network`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if both `enabled` and `phone_number` are `None` — see
    /// [`crate::endpoints::BackupApi::configure_backup_network`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`].
    pub async fn configure_backup_network(
        &self,
        enabled: Option<bool>,
        phone_number: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .configure_backup_network(&network_id, enabled, phone_number)
            .await
    }
}
