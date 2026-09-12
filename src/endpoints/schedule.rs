//! Schedule API: the read-only (`GET`) half of `eero-api`'s `ScheduleAPI`.
//!
//! Ported from `eero-api src/eero/api/schedule.py`. Covers every `ScheduleAPI` method,
//! read-only and mutating alike: `get_profile_schedule`, `set_profile_schedule`,
//! `clear_profile_schedule`, `enable_bedtime`, `set_weekday_bedtime` and
//! `set_weekend_bedtime`.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard. `clear_profile_schedule`, `enable_bedtime`,
//! `set_weekday_bedtime` and `set_weekend_bedtime` are pure delegators exactly as in Python
//! (`schedule.py:101,113,169,190`): each builds a time-block list and calls
//! [`ScheduleApi::set_profile_schedule`] with it — none of them talks to
//! [`crate::transport::Transport`] directly or duplicates `set_profile_schedule`'s request.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The seven days of the week, spelled exactly as `eero-api`'s bedtime helpers do
/// (`schedule.py:142-150`), in the order `enable_bedtime`'s own default uses.
const ALL_DAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// Monday through Friday, verbatim from `eero-api src/eero/api/schedule.py:187`
/// (`ScheduleAPI.set_weekday_bedtime`'s `weekdays` list).
const WEEKDAYS: &[&str] = &["monday", "tuesday", "wednesday", "thursday", "friday"];

/// Saturday and Sunday, verbatim from `eero-api src/eero/api/schedule.py:208`
/// (`ScheduleAPI.set_weekend_bedtime`'s `weekend` list).
const WEEKEND: &[&str] = &["saturday", "sunday"];

/// The read-only half of `eero-api`'s `ScheduleAPI` (`src/eero/api/schedule.py`).
///
/// Build one with [`ScheduleApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `ScheduleApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct ScheduleApi {
    transport: Arc<Transport>,
}

impl ScheduleApi {
    /// Wraps `transport` as a `ScheduleApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets a profile's full object — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:36-60`
    /// (`ScheduleAPI.get_profile_schedule`), which sends the exact same request as
    /// `ProfilesApi::get_profile` (`routes::GET_PROFILE_SCHEDULE` is an alias of
    /// `routes::GET_PROFILE`): `GET networks/{network_id}/profiles/{profile_id}`. Python's own
    /// docstring notes "The schedule data is in the `schedule` field of the response"
    /// (`schedule.py:39`), but neither Python nor this port extracts that field — the full,
    /// untouched profile object is returned. Extracting `data.schedule` here would transform the
    /// wire payload, which this crate's raw-envelope contract never does; this method exists
    /// only so a caller searching for "how do I get a profile's schedule" finds a name that says
    /// so, not to narrow the response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_profile_schedule(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_PROFILE_SCHEDULE,
                &[("network_id", network_id), ("profile_id", profile_id)],
                None,
            )
            .await
    }

    /// Sets a profile's internet-access schedule — returns the raw Eero API response.
    ///
    /// `time_blocks` is sent verbatim as the `schedule` array; each element is an arbitrary
    /// JSON object (`{"days": [...], "start": "HH:MM", "end": "HH:MM"}`, optionally with a
    /// `"type"` key such as `"bedtime"`) exactly as Python's own untyped
    /// `List[Dict[str, Any]]` allows (`schedule.py:66`) — this method neither validates nor
    /// reshapes any block's contents.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:62`
    /// (`ScheduleAPI.set_profile_schedule`): sends `PUT` `routes::SET_PROFILE_SCHEDULE` (alias
    /// of `routes::PUT_PROFILE`, the same path as `routes::GET_PROFILE_SCHEDULE`) with body
    /// `{"schedule": time_blocks}`. [`ScheduleApi::clear_profile_schedule`],
    /// [`ScheduleApi::enable_bedtime`], [`ScheduleApi::set_weekday_bedtime`] and
    /// [`ScheduleApi::set_weekend_bedtime`] all funnel through this method rather than sending
    /// their own request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn set_profile_schedule(
        &self,
        network_id: &str,
        profile_id: &str,
        time_blocks: &[Value],
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::SET_PROFILE_SCHEDULE,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ "schedule": time_blocks })),
            )
            .await
    }

    /// Clears every schedule on a profile — returns the raw Eero API response.
    ///
    /// Sends `{"schedule": []}` — an empty JSON **array**, not `null` and not an omitted key.
    /// Ported from `eero-api src/eero/api/schedule.py:101`
    /// (`ScheduleAPI.clear_profile_schedule`), which delegates as
    /// `self.set_profile_schedule(network_id, profile_id, [])` (`schedule.py:111`); this method
    /// calls [`ScheduleApi::set_profile_schedule`] with an empty slice for the same effect,
    /// rather than sending a second, duplicate request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn clear_profile_schedule(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        self.set_profile_schedule(network_id, profile_id, &[]).await
    }

    /// Enables bedtime mode for a profile over `days` (all seven days if `None`) — returns the
    /// raw Eero API response.
    ///
    /// Builds a single time block, `{"days": days, "start": start_time, "end": end_time, "type":
    /// "bedtime"}`, and calls [`ScheduleApi::set_profile_schedule`] with it as the sole element
    /// of the schedule — replacing whatever schedule existed before, exactly as Python's
    /// `bedtime_block` dict and `set_profile_schedule(network_id, profile_id, [bedtime_block])`
    /// call do (`schedule.py:160-167`). `days` defaults to all seven days, verbatim from
    /// Python's own default list (`schedule.py:141-150`), when `None`.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:113` (`ScheduleAPI.enable_bedtime`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn enable_bedtime(
        &self,
        network_id: &str,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        days: Option<&[&str]>,
    ) -> Result<Envelope, Error> {
        let days = days.unwrap_or(ALL_DAYS);
        let bedtime_block = json!({
            "days": days,
            "start": start_time,
            "end": end_time,
            "type": "bedtime",
        });

        self.set_profile_schedule(network_id, profile_id, &[bedtime_block])
            .await
    }

    /// Sets bedtime for weekdays only (Monday through Friday) — returns the raw Eero API
    /// response.
    ///
    /// Delegates to [`ScheduleApi::enable_bedtime`] with `days` fixed to `["monday", "tuesday",
    /// "wednesday", "thursday", "friday"]`, verbatim from
    /// `eero-api src/eero/api/schedule.py:187` (`ScheduleAPI.set_weekday_bedtime`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn set_weekday_bedtime(
        &self,
        network_id: &str,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
    ) -> Result<Envelope, Error> {
        self.enable_bedtime(network_id, profile_id, start_time, end_time, Some(WEEKDAYS))
            .await
    }

    /// Sets bedtime for weekends only (Saturday and Sunday) — returns the raw Eero API
    /// response.
    ///
    /// Delegates to [`ScheduleApi::enable_bedtime`] with `days` fixed to `["saturday",
    /// "sunday"]`, verbatim from `eero-api src/eero/api/schedule.py:208`
    /// (`ScheduleAPI.set_weekend_bedtime`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn set_weekend_bedtime(
        &self,
        network_id: &str,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
    ) -> Result<Envelope, Error> {
        self.enable_bedtime(network_id, profile_id, start_time, end_time, Some(WEEKEND))
            .await
    }
}
