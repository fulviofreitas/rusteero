//! Profile-schedule routes (`ScheduleAPI`) — aliases of `profiles::GET_PROFILE`/`profiles::PUT_PROFILE`.

// ---------------------------------- schedule (`ScheduleAPI`) ----------------------------

use super::Route;
use super::profiles::{GET_PROFILE, PUT_PROFILE};

/// Alias of `GET_PROFILE`: `ScheduleAPI.get_profile_schedule` reads the `schedule` field out
/// of the same full profile object.
///
/// Ported from `eero-api src/eero/api/schedule.py:36` (`ScheduleAPI.get_profile_schedule`).
pub const GET_PROFILE_SCHEDULE: Route = GET_PROFILE;

/// Alias of `PUT_PROFILE`: `ScheduleAPI.set_profile_schedule` PUTs
/// `{"schedule": [time_block, ...]}`. Also the target of `ScheduleAPI.clear_profile_schedule`
/// (`schedule.py:101`, delegates with `[]`), `ScheduleAPI.enable_bedtime` (`schedule.py:113`,
/// delegates with a single `{"type": "bedtime", ...}` block),
/// `ScheduleAPI.set_weekday_bedtime` (`schedule.py:169`) and
/// `ScheduleAPI.set_weekend_bedtime` (`schedule.py:190`) — all four delegate to
/// `set_profile_schedule`, so none needs a separate route.
///
/// Ported from `eero-api src/eero/api/schedule.py:62` (`ScheduleAPI.set_profile_schedule`).
pub const SET_PROFILE_SCHEDULE: Route = PUT_PROFILE;
