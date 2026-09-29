//! `Client` methods for the `BackupAccessPointsAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/backup_access_points.py` (v8.0.4) by way of `EeroClient`'s
//! own `backup_access_points`-scoped wrappers in `client.py`.

use super::Client;
use crate::endpoints::backup_access_points::UpdateBackupAccessPointOptions;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Lists backup access points — returns the raw Eero API response.
    ///
    /// Ported from `list_backup_access_points()` (`client.py:2912-2918`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Passes the cached network envelope as `parent`
    /// (`+net`, `client.py:2917`), so a published `backup_access_points` link on it wins over
    /// the literal template — see [`crate::endpoints::backup_access_points::BackupAccessPointsApi::list`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn list_backup_access_points(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .backup_access_points()
            .list(&network_id, parent.as_ref())
            .await
    }

    /// Adds a backup access point — returns the raw Eero API response.
    ///
    /// Ported from `add_backup_access_point()` (`client.py:2919-2932`). `auto_discover = false`.
    /// Invalidates nothing: `client.py`'s own method body has no `del self._cache[...]` call to
    /// reproduce.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn add_backup_access_point(
        &self,
        ssid: &str,
        password: &str,
        uuid: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .add(&network_id, ssid, password, uuid)
            .await
    }

    /// Updates a backup access point — returns the raw Eero API response.
    ///
    /// Ported from `update_backup_access_point()` (`client.py:2933-2959`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::add_backup_access_point`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn update_backup_access_point(
        &self,
        backup_network_id: &str,
        options: &UpdateBackupAccessPointOptions<'_>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .update(&network_id, backup_network_id, options)
            .await
    }

    /// Deletes a backup access point — returns the raw Eero API response.
    ///
    /// Ported from `delete_backup_access_point()` (`client.py:2960-2968`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::add_backup_access_point`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_backup_access_point(
        &self,
        backup_network_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .delete_backup_access_point(&network_id, backup_network_id)
            .await
    }

    /// Reorders backup access points — returns the raw Eero API response.
    ///
    /// Ported from `rearrange_backup_access_points()` (`client.py:2969-2975`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::add_backup_access_point`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn rearrange_backup_access_points(
        &self,
        order: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .rearrange(&network_id, order)
            .await
    }

    /// Reads discovered backup-access-point SSIDs — returns the raw Eero API response.
    ///
    /// Ported from `discover_backup_ssids()` (`client.py:2976-2980`). `auto_discover = false`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn discover_backup_ssids(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .discover_ssids(&network_id)
            .await
    }

    /// Starts backup-access-point SSID discovery — returns the raw Eero API response.
    ///
    /// Ported from `start_backup_ssid_discovery()` (`client.py:2981-2985`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::add_backup_access_point`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn start_backup_ssid_discovery(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .start_ssid_discovery(&network_id)
            .await
    }

    /// Starts a backup connectivity check — returns the raw Eero API response.
    ///
    /// Ported from `backup_connectivity_check()` (`client.py:2986-2992`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::add_backup_access_point`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn backup_connectivity_check(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup_access_points()
            .connectivity_check(&network_id)
            .await
    }
}
