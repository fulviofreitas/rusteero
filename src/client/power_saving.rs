//! `Client` methods for the `PowerSavingAPI` domain (new in v8.0.0), ported from `eero-api
//! src/eero/client.py`'s power-saving section (`client.py:2814-2888`).
//!
//! Every method here calls `_ensure_network_id(network_id, auto_discover=False)`. Only
//! [`Client::set_power_saving`] and [`Client::get_power_saving_schedules`] forward a `parent=` —
//! [`Client::create_power_saving_schedule`]/[`Client::update_power_saving_schedule`]/
//! [`Client::delete_power_saving_schedule`] do not, matching the domain methods themselves,
//! which take no `parent` parameter at all (`.claude/tasks/briefs/v8/g2-eeros.md` §3).

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Enables/disables power saving, or its schedule, for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_power_saving()` (`eero-api src/eero/client.py:2814-2830`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. On success, invalidates
    /// `network[{nid}]` (`client.py:2827`, `_invalidate_network_cache`). **May reboot the mesh;
    /// unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "power_saving", .. }` if both `enable` and
    /// `power_saving_schedule_enabled` are `None`. Otherwise see [`Client::get_diagnostics`]. The
    /// cache is left untouched on any `Err`.
    pub async fn set_power_saving(
        &self,
        network_id: Option<&str>,
        enable: Option<bool>,
        power_saving_schedule_enabled: Option<bool>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .power_saving()
            .set_power_saving(
                &network_id,
                enable,
                power_saving_schedule_enabled,
                parent.as_ref(),
            )
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Lists power-saving schedules for a network — returns the raw Eero API response.
    ///
    /// Ported from `get_power_saving_schedules()` (`eero-api src/eero/client.py:2832-2837`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. Passes the cached network
    /// envelope (if any) as `parent=`, matching the domain method's own signature — even though
    /// [`crate::endpoints::PowerSavingApi::get_schedules`] never actually consults it (see that
    /// method's own docs). Not cached. **Live-verified.**
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_power_saving_schedules(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .power_saving()
            .get_schedules(&network_id, parent.as_ref())
            .await
    }

    /// Creates a power-saving schedule for a network — returns the raw Eero API response.
    ///
    /// Ported from `create_power_saving_schedule()` (`eero-api src/eero/client.py:2839-2858`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. No `parent=` — the domain method
    /// takes none. `days` is forwarded unchanged as an opaque JSON value. Not cached (no
    /// `power_saving` cache bucket exists). **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    #[allow(clippy::too_many_arguments)] // mirrors client.py:2839-2848's own five keyword-only fields
    pub async fn create_power_saving_schedule(
        &self,
        network_id: Option<&str>,
        name: &str,
        days: Value,
        start_time: &str,
        end_time: &str,
        enabled: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .power_saving()
            .create_schedule(&network_id, name, days, start_time, end_time, enabled)
            .await
    }

    /// Updates a power-saving schedule — returns the raw Eero API response.
    ///
    /// Ported from `update_power_saving_schedule()` (`eero-api src/eero/client.py:2860-2881`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. No `parent=` — the domain method
    /// takes none. Not cached. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "schedule", .. }` if every one of `name`, `days`,
    /// `start_time`, `end_time`, `enabled` is `None`. Otherwise see [`Client::get_diagnostics`].
    #[allow(clippy::too_many_arguments)] // mirrors client.py:2860-2870's own six keyword-only fields
    pub async fn update_power_saving_schedule(
        &self,
        schedule_id: &str,
        network_id: Option<&str>,
        name: Option<&str>,
        days: Option<Value>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .power_saving()
            .update_schedule(
                &network_id,
                schedule_id,
                name,
                days,
                start_time,
                end_time,
                enabled,
            )
            .await
    }

    /// Deletes a power-saving schedule — returns the raw Eero API response.
    ///
    /// Ported from `delete_power_saving_schedule()` (`eero-api src/eero/client.py:2883-2888`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. No `parent=` — the domain method
    /// takes none. Not cached. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_power_saving_schedule(
        &self,
        schedule_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .power_saving()
            .delete_schedule(&network_id, schedule_id)
            .await
    }
}
