//! `Client` methods for the `DataUsageAPI` domain.
//!
//! None of these eleven methods is ever cached (behaviour brief §2.1: the whole family is
//! time-windowed reads, not one of the eight cached getters) and every one passes
//! `auto_discover = false`. None of them passes `parent=` at this layer either — `EeroClient`
//! never supplies one for this family (`.claude/tasks/briefs/v8/client.md`'s `data_usage` table
//! has no `+net` annotation on any row); the underlying domain methods still accept `parent=`
//! for a caller reaching `client.api().data_usage()` directly with its own cached envelope.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets network-wide data-usage statistics — returns the raw Eero API response.
    ///
    /// Ported from `get_data_usage()` (`eero-api src/eero/client.py:1580-1606` at `v8.0.4`).
    /// **Breaking shape change** from the pre-8.0.0 port this crate previously shipped: the old
    /// `payload`/`resource` parameters are gone, replaced by required `start`/`end`/`cadence` and
    /// optional `timezone`. `auto_discover = false`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_data_usage(
        &self,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_data_usage(&network_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets a data-usage breakdown by category — returns the raw Eero API response.
    ///
    /// Ported from `get_data_usage_breakdown()` (`eero-api src/eero/client.py:1607-1625`).
    /// `auto_discover = false`. `cadence` is optional.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_data_usage_breakdown(
        &self,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_breakdown(&network_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets data-usage statistics for every device on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_devices_data_usage()` (`eero-api src/eero/client.py:1626-1650`).
    /// `auto_discover = false`. `cadence`/`profile_id` are optional.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_devices_data_usage(
        &self,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        profile_id: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_devices_usage(&network_id, start, end, cadence, timezone, profile_id, None)
            .await
    }

    /// Gets data-usage statistics for a single device — returns the raw Eero API response.
    ///
    /// Ported from `get_device_data_usage()` (`eero-api src/eero/client.py:1651-1670`).
    /// `auto_discover = false`. `cadence` is required.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_device_data_usage(
        &self,
        device_mac: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_device_usage(&network_id, device_mac, start, end, cadence, timezone, None)
            .await
    }

    /// Gets an aggregated data-usage summary across every eero on a network — returns the raw
    /// Eero API response.
    ///
    /// Ported from `get_eeros_data_usage_summary()` (`eero-api src/eero/client.py:1671-1689`).
    /// `auto_discover = false`. `cadence` is required.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_eeros_data_usage_summary(
        &self,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_eeros_summary(&network_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets data-usage statistics for a single eero — returns the raw Eero API response.
    ///
    /// Ported from `get_eero_data_usage()` (`eero-api src/eero/client.py:1690-1709`).
    /// `auto_discover = false`. `cadence` is required.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_eero_data_usage(
        &self,
        eero_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_eero_usage(&network_id, eero_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets data-usage statistics for a single profile — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_data_usage()` (`eero-api src/eero/client.py:1710-1729`).
    /// `auto_discover = false`. `cadence` is required.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_profile_data_usage(
        &self,
        profile_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_profile_usage(&network_id, profile_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets data-usage statistics for devices with no profile — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_unprofiled_devices_data_usage()` (`eero-api src/eero/client.py:1730-1748`).
    /// `auto_discover = false`. `cadence` is optional.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_unprofiled_devices_data_usage(
        &self,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_unprofiled_devices(&network_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets an aggregated data-usage summary for unprofiled devices — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_unprofiled_data_usage_summary()` (`eero-api src/eero/client.py:1749-1767`).
    /// `auto_discover = false`. `cadence` is required.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_unprofiled_data_usage_summary(
        &self,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_unprofiled_summary(&network_id, start, end, cadence, timezone, None)
            .await
    }

    /// Gets the network's data-usage report settings — returns the raw Eero API response.
    ///
    /// Ported from `get_data_usage_report_settings()` (`eero-api src/eero/client.py:1768-1777`).
    /// `auto_discover = false`. No query parameters at all.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_data_usage_report_settings(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_report_settings(&network_id, None)
            .await
    }

    /// Sets the network's data-usage report settings — returns the raw Eero API response.
    ///
    /// Ported from `set_data_usage_report_settings()` (`eero-api src/eero/client.py:1778-1799`),
    /// the only write in this family. `auto_discover = false`. On success, invalidates
    /// `network[{nid}]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_data_usage_report_settings(
        &self,
        cadence: &str,
        notification_day: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .data_usage()
            .set_report_settings(&network_id, cadence, notification_day, None)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }
}
