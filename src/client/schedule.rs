//! `Client` methods for the `ScheduleAPI` domain at v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py`'s schedule-scoped wrappers
//! (`.claude/tasks/briefs/v8/client.md` §4 "schedule"). `get_profile_schedule`/
//! `set_profile_schedule` are **removed** (client.md §5): scheduled pauses are sub-resources of a
//! profile at v8.0.4, not a `schedule` field on the profile object — replaced by
//! [`Client::get_schedules`]/[`Client::create_schedule`]/[`Client::update_schedule`]/
//! [`Client::delete_schedule`]/[`Client::clear_profile_schedule`].
//!
//! **Cache-invalidation divergence retired.** The pre-v8 `Client` invalidated the profile cache
//! after every schedule write (security review finding F3), because a cached profile object used
//! to embed its own `schedule` array. At v8.0.4 a profile's `GET` response carries no `schedule`
//! field at all (schedules are sub-resources, never folded into the cached profile entry, and
//! never cached themselves — `.claude/tasks/briefs/v8/g4-profiles.md` §1.2's fixture notes), so
//! that invalidation no longer has anything stale to correct. None of the methods below
//! invalidates any cache entry, matching `client.py` exactly (`client.md` §4's "none invalidated"
//! for every row in this table).
//!
//! `set_weekday_bedtime`/`set_weekend_bedtime` have no v8.0.4 `client.py` equivalent (`client.md`
//! §5) — reach the domain methods directly via `client.api().schedule()` if needed.

use super::Client;
use crate::endpoints::schedule::UpdateScheduleOptions;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets the scheduled pauses for a profile — returns the raw Eero API response.
    ///
    /// Ported from `get_schedules` (`eero-api src/eero/client.py:1985-1992`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Not cached: schedules are never one of the eight
    /// cached buckets. No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_schedules(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .get_schedules(&network_id, profile_id, None)
            .await
    }

    /// Creates a scheduled pause for a profile — returns the raw Eero API response.
    ///
    /// Ported from `create_schedule` (`eero-api src/eero/client.py:1992-2015`). `auto_discover =
    /// false`. `enabled` defaults to `true` in Python (`enabled: bool = True`); callers that want
    /// that default pass `true` explicitly. No cache invalidation (see the module docs).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    #[allow(clippy::too_many_arguments)] // mirrors client.py:1992-2001's own signature
    pub async fn create_schedule(
        &self,
        profile_id: &str,
        name: &str,
        days: &[&str],
        start: &str,
        end: &str,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .create_schedule(
                &network_id,
                profile_id,
                name,
                days,
                start,
                end,
                enabled,
                None,
            )
            .await
    }

    /// Updates a scheduled pause via its own path/URL, or its own cached envelope — returns the
    /// raw Eero API response.
    ///
    /// Ported from `update_schedule` (`eero-api src/eero/client.py:2015-2044`). No `network_id`
    /// parameter at all — `schedule` alone resolves the pause's URL, matching Python's own
    /// signature. `parent` accepts the pause's own cached envelope (Python's `schedule: Any`
    /// accepts either a path/URL string or a mapping; this port always splits that into an
    /// `id_or_url` plus a `parent`, per phase-G fix list item 19), placed right after `schedule`
    /// since this method has no `network_id` to place it before. No cache invalidation.
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::schedule::ScheduleApi::update_schedule`].
    pub async fn update_schedule(
        &self,
        schedule: &str,
        options: &UpdateScheduleOptions<'_>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.api
            .schedule()
            .update_schedule(schedule, options, parent)
            .await
    }

    /// Deletes a scheduled pause via its own path/URL, or its own cached envelope — returns the
    /// raw Eero API response.
    ///
    /// Ported from `delete_schedule` (`eero-api src/eero/client.py:2044-2053`). No `network_id`
    /// parameter. `parent` accepts the pause's own cached envelope, for the same reason as
    /// [`Client::update_schedule`] (phase-G fix list item 19). No cache invalidation.
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::schedule::ScheduleApi::delete_schedule`].
    pub async fn delete_schedule(
        &self,
        schedule: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.api.schedule().delete_schedule(schedule, parent).await
    }

    /// Deletes every scheduled pause currently set on a profile — returns one raw response per
    /// deleted pause.
    ///
    /// Ported from `clear_profile_schedule` (`eero-api src/eero/client.py:2053-2064`).
    /// `auto_discover = false`. No cache invalidation.
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::schedule::ScheduleApi::clear_profile_schedule`].
    pub async fn clear_profile_schedule(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Vec<Envelope>, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .clear_profile_schedule(&network_id, profile_id, None)
            .await
    }

    /// Creates a single bedtime scheduled pause for a profile — returns the raw Eero API
    /// response.
    ///
    /// Ported from `enable_bedtime` (`eero-api src/eero/client.py:2064-2081`). `auto_discover =
    /// false`. No cache invalidation (see the module docs — this drops the pre-v8 `Client`'s
    /// deliberate F3 divergence, which no longer applies).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn enable_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        days: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .enable_bedtime(&network_id, profile_id, start_time, end_time, days, None)
            .await
    }
}
