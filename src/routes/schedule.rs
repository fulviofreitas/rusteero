//! Profile-schedule routes (`ScheduleAPI`) at v8.0.4.
//!
//! Ported from `eero-api src/eero/api/schedule.py` (v8.0.4). Scheduled pauses are sub-resources
//! of a profile (`networks/{network}/profiles/{profile}/schedules`), not a field on the profile
//! object itself — replacing the pre-v8 design this domain's legacy `Route` aliases
//! (`GET_PROFILE_SCHEDULE`/`SET_PROFILE_SCHEDULE`, both aliases of `profiles::GET_PROFILE`/
//! `profiles::PUT_PROFILE`) modelled. Those aliases are deleted along with the endpoint methods
//! that used them.
//!
//! [`SCHEDULE_UPDATE`]/[`SCHEDULE_DELETE`] address one pause by its own path/URL/envelope, not by
//! network+profile — see `resolve_schedule_url` (`crate::endpoints::schedule::ScheduleApi`) for how
//! that single-argument resolution (`_resolve_schedule_url`, `schedule.py:48-74`) is reproduced.

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/profiles/{profile_id}/schedules` — list a profile's scheduled
/// pauses.
///
/// Prefers the profile's own published `schedules` link when a `parent` envelope is supplied.
///
/// Ported from `eero-api src/eero/api/schedule.py:100-121,123-150` (`ScheduleAPI.get_schedules`,
/// via `_schedules_url`).
pub const SCHEDULE_GET_SCHEDULES: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "/schedules",
    link: Some("schedules"),
};

/// `POST /2.2/networks/{network_id}/profiles/{profile_id}/schedules` — create a scheduled pause.
///
/// Same URL resolution as [`SCHEDULE_GET_SCHEDULES`].
///
/// Ported from `eero-api src/eero/api/schedule.py:152-192` (`ScheduleAPI.create_schedule`).
pub const SCHEDULE_CREATE_SCHEDULE: Nested = Nested {
    method: Method::POST,
    version: ApiVersion::V2_2,
    prefix: "profiles",
    suffix: "/schedules",
    link: Some("schedules"),
};

/// `PUT {schedule's own URL}` — update one scheduled pause.
///
/// `template: "{id}"` (no other text) matches `resource_url(schedule, "{id}")`
/// (`schedule.py:70`) exactly: the caller-supplied `schedule` id-or-path-or-URL is resolved with
/// no literal prefix or suffix appended. `link: None` — pause resolution never consults a
/// named-link parent; see `resolve_schedule_url` (`crate::endpoints::schedule::ScheduleApi`).
///
/// Ported from `eero-api src/eero/api/schedule.py:194-247` (`ScheduleAPI.update_schedule`).
pub const SCHEDULE_UPDATE: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "{id}",
    link: None,
};

/// `DELETE {schedule's own URL}` — delete one scheduled pause.
///
/// Same URL resolution as [`SCHEDULE_UPDATE`].
///
/// Ported from `eero-api src/eero/api/schedule.py:250-270` (`ScheduleAPI.delete_schedule`).
pub const SCHEDULE_DELETE: Resource = Resource {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    template: "{id}",
    link: None,
};
