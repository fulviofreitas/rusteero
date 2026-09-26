//! `Client` methods for the `InsightsAPI` domain.
//!
//! # `run_insights`: removed
//!
//! `InsightsAPI.run_insights` (v6.2.0 `insights.py:115-137`) was removed entirely in `05a2b07`
//! (v8.0.0) — no v8.0.4 API operation corresponds to it. `Client::run_insights`, which this crate
//! previously shipped, is removed along with the endpoint method and route it called.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Queries network-level insights time-series data — returns the raw Eero API response.
    ///
    /// Ported from `get_insights()` (`eero-api src/eero/client.py:1275-1309` at `v8.0.4`):
    /// `start`, `end` and `insight_type` have been required since v6.0.0. `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. No `parent=`. Unlike Python (which
    /// SDK-defaults `cadence` to `"daily"`), `cadence` has no default here — see
    /// [`crate::endpoints::insights::InsightsApi::get_insights`]'s own docs for why this
    /// divergence was kept rather than closed in this phase.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_insights(
        &self,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        insight_type: &str,
        cadence: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .insights()
            .get_insights(&network_id, start, end, insight_type, cadence)
            .await
    }

    /// Queries insights for every device on a network — returns the raw Eero API response.
    ///
    /// Ported from `get_devices_insights()` (`eero-api src/eero/client.py:1310-1329`).
    /// `auto_discover = false`. Passes the cached network envelope as `parent=` (`+net`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_devices_insights(
        &self,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .insights()
            .get_devices_insights(
                &network_id,
                start,
                end,
                cadence,
                insight_type,
                parent.as_ref(),
            )
            .await
    }

    /// Queries insights for a single device — returns the raw Eero API response.
    ///
    /// Ported from `get_device_insights()` (`eero-api src/eero/client.py:1330-1350`).
    /// `auto_discover = false`. No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_device_insights(
        &self,
        device_id: &str,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .insights()
            .get_device_insights(&network_id, device_id, start, end, cadence, insight_type)
            .await
    }

    /// Queries insights for every profile on a network — returns the raw Eero API response.
    ///
    /// Ported from `get_profiles_insights()` (`eero-api src/eero/client.py:1351-1370`).
    /// `auto_discover = false`. Passes the cached network envelope as `parent=` (`+net`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_profiles_insights(
        &self,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .insights()
            .get_profiles_insights(
                &network_id,
                start,
                end,
                cadence,
                insight_type,
                parent.as_ref(),
            )
            .await
    }

    /// Queries insights for a single profile — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_insights()` (`eero-api src/eero/client.py:1371-1391`).
    /// `auto_discover = false`. No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_profile_insights(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .insights()
            .get_profile_insights(&network_id, profile_id, start, end, cadence, insight_type)
            .await
    }

    /// Queries insights for every device belonging to a single profile — returns the raw Eero
    /// API response.
    ///
    /// Ported from `get_profile_devices_insights()` (`eero-api src/eero/client.py:1392-1412`).
    /// `auto_discover = false`. No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_profile_devices_insights(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .insights()
            .get_profile_devices_insights(
                &network_id,
                profile_id,
                start,
                end,
                cadence,
                insight_type,
            )
            .await
    }
}
