//! `Client` methods for the `BackupAPI` domain, rewritten for v8.0.4.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== Backup Internet ====================

    /// Gets cellular-backup-internet configuration — returns the raw Eero API response.
    ///
    /// Ported from `get_backup_internet()` (`client.py:1952-1956`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`]. Passes no `parent` — `client.py` never builds one for
    /// this call, matching [`crate::endpoints::BackupApi::get_backup_internet`] itself never
    /// consulting one either.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_backup_internet(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .get_backup_internet(&network_id, None)
            .await
    }

    /// Gets cellular-backup data usage — returns the raw Eero API response.
    ///
    /// Ported from `get_cellular_backup_usage()` (`client.py:1973-1977`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_cellular_backup_usage(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .get_cellular_backup_usage(&network_id, None)
            .await
    }

    /// Gets cellular-backup events — returns the raw Eero API response.
    ///
    /// Ported from `get_cellular_backup_events()` (`client.py:1978-1982`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_cellular_backup_events(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .get_cellular_backup_events(&network_id, None)
            .await
    }

    // ==================== Backup Internet (mutations) ====================

    /// Enables or disables cellular-backup internet — returns the raw Eero API response.
    ///
    /// Ported from `set_backup_internet` (`eero-api src/eero/client.py:1957-1972`).
    /// `auto_discover = false`. Invalidates `Net{nid}` (`client.py:1972`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_backup_internet(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .backup()
            .set_backup_internet(&network_id, enabled, None)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
