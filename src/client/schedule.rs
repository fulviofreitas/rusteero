//! `Client` methods for the `ScheduleAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    // ==================== Schedule ====================

    /// Gets a profile's schedule — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_schedule()` (`client.py:1129-1134`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`]. Note the parameter order: `profile_id` first,
    /// `network_id` second, matching `client.py`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_profile_schedule(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .get_profile_schedule(&network_id, profile_id)
            .await
    }

    // ==================== Schedule (mutations) ====================

    /// Sets a profile's internet-access schedule — returns the raw Eero API response.
    ///
    /// Ported from `set_profile_schedule` (`eero-api src/eero/client.py:1136-1148`).
    /// `auto_discover = false`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` (`client.py:1147`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_profile_schedule(
        &self,
        profile_id: &str,
        time_blocks: &[Value],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .schedule()
            .set_profile_schedule(&network_id, profile_id, time_blocks)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Enables bedtime mode for a profile — returns the raw Eero API response.
    ///
    /// Ported from `enable_bedtime` (`eero-api src/eero/client.py:1150-1162`). `auto_discover =
    /// false`.
    ///
    /// **Deliberate divergence from Python (security review finding F3).** This delegates to
    /// [`crate::endpoints::ScheduleApi::enable_bedtime`], which itself calls the exact same
    /// `PUT .../profiles/{pid}` [`Client::set_profile_schedule`] uses — the method that *does*
    /// invalidate the profile cache. Python's own `client.py` leaves this asymmetric
    /// (`enable_bedtime` invalidates nothing, despite delegating to the same wire call
    /// `set_profile_schedule` invalidates for), which meant a profile fetched via
    /// [`Client::get_profile`] right after this call could report a stale (or entirely absent)
    /// bedtime schedule for the rest of the TTL — a parental-control setting a caller believes is
    /// active/cleared may not be, as far as any cached read can tell. Unlike the schedule
    /// pass-throughs' previous "faithful no-op" framing, this asymmetry is fixed here rather than
    /// reproduced: on success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`,
    /// matching [`Client::set_profile_schedule`] exactly.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn enable_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        days: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .schedule()
            .enable_bedtime(&network_id, profile_id, start_time, end_time, days)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Clears every schedule on a profile — returns the raw Eero API response.
    ///
    /// Ported from `clear_profile_schedule` (`eero-api src/eero/client.py:1164-1169`).
    /// `auto_discover = false`. Same divergence as [`Client::enable_bedtime`] (security review
    /// finding F3): on success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn clear_profile_schedule(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .schedule()
            .clear_profile_schedule(&network_id, profile_id)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Sets bedtime for weekdays only (Monday through Friday) — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped `ScheduleAPI.set_weekday_bedtime`
    /// (`api/schedule.py:169`) on `EeroClient`.
    /// [`crate::endpoints::ScheduleApi::set_weekday_bedtime`] is a pure delegator to
    /// `ScheduleApi::enable_bedtime`, so this method's cache behaviour matches
    /// [`Client::enable_bedtime`] exactly (security review finding F3): on success, invalidates
    /// `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_weekday_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .schedule()
            .set_weekday_bedtime(&network_id, profile_id, start_time, end_time)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Sets bedtime for weekends only (Saturday and Sunday) — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::set_weekday_bedtime`]'s docs, which apply
    /// identically here (security review finding F3): on success, invalidates
    /// `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_weekend_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .schedule()
            .set_weekend_bedtime(&network_id, profile_id, start_time, end_time)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }
}
