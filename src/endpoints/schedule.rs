//! Schedule API: the read-only (`GET`) half of `eero-api`'s `ScheduleAPI`.
//!
//! Ported from `eero-api src/eero/api/schedule.py`. This phase (3, GET-only) covers
//! `ScheduleAPI.get_profile_schedule`; the mutation methods (`set_profile_schedule`,
//! `clear_profile_schedule`, `enable_bedtime`, `set_weekday_bedtime`, `set_weekend_bedtime`) are
//! phase 5 — see the marker comment at the bottom of this file.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

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

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): `ScheduleAPI`'s mutation methods go here, in the same order as
    // `eero-api src/eero/api/schedule.py`:
    //   - `set_profile_schedule` (`schedule.py:62-99`) — PUT `routes::SET_PROFILE_SCHEDULE`
    //     (alias of `routes::PUT_PROFILE`, same path as `GET_PROFILE_SCHEDULE`) with
    //     `{"schedule": time_blocks}`.
    //   - `clear_profile_schedule` (`schedule.py:101-111`), `enable_bedtime`
    //     (`schedule.py:113-167`), `set_weekday_bedtime` (`schedule.py:169-188`) and
    //     `set_weekend_bedtime` (`schedule.py:190-209`) are pure delegators: each builds a list
    //     of time-block dicts (an empty list for `clear_profile_schedule`; a single
    //     `{"days": [...], "start": ..., "end": ..., "type": "bedtime"}` block — with `days`
    //     defaulted to all seven, weekdays only, or the weekend only, respectively — for the
    //     other three) and calls `set_profile_schedule` with it. None needs a distinct `Route`.
    // ---------------------------------------------------------------------------------------
}
