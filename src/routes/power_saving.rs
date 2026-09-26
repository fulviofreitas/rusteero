//! `power_saving` routes (`PowerSavingAPI`, new in v8.0.0), ported from
//! `eero-api src/eero/api/power_saving.py` at v8.0.4.

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `PUT /2.2/networks/{network_id}/power_saving` — enable/disable power saving, or its schedule.
///
/// Ported from `eero-api src/eero/api/power_saving.py:38-96` (`PowerSavingAPI.set_power_saving`).
/// Resolved via [`Resource::resolve`]: prefers a parent envelope's `resources.power_saving`
/// link, falls back to the `networks/{id}/power_saving` template.
pub const SET_POWER_SAVING: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/power_saving",
    link: Some("power_saving"),
};

/// `GET /2.2/networks/{network_id}/power_saving/schedules` — list power-saving schedules.
///
/// Ported from `eero-api src/eero/api/power_saving.py:98-121` (`PowerSavingAPI.get_schedules`).
/// `link: None` deliberately: unlike every other `power_saving.py` method that accepts a
/// `parent=`, this one never consults it for URL resolution at all (verified v8.0.4 behaviour,
/// not an oversight — see `src/endpoints/power_saving.rs`'s own docs). Resolving via
/// [`Resource::resolve`] with `link: None` reproduces that exactly: [`Resource::resolve`] only
/// consults `parent` when `link` is `Some`, so a caller-supplied `parent` here is structurally
/// inert regardless of what it carries.
pub const GET_SCHEDULES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/power_saving/schedules",
    link: None,
};

/// Alias of [`GET_SCHEDULES`], `POST`: create a power-saving schedule.
///
/// Ported from `eero-api src/eero/api/power_saving.py:123-165` (`PowerSavingAPI.create_schedule`).
/// No `parent=` parameter in Python at all (unlike [`GET_SCHEDULES`], which at least accepts one
/// even though it goes unused) — resolved from `network_id` alone.
pub const CREATE_SCHEDULE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/power_saving/schedules",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/power_saving/schedules/{schedule_id}` — update a power-saving
/// schedule.
///
/// Ported from `eero-api src/eero/api/power_saving.py:167-224` (`PowerSavingAPI.update_schedule`).
/// Resolved via [`Nested::resolve`] (`crate::params::resolve_nested_url`); no `link` (Python
/// calls `resolve_nested_url` with no `link=` at all) and no `parent=` parameter in Python.
pub const UPDATE_SCHEDULE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "power_saving/schedules",
    suffix: "",
    link: None,
};

/// Alias of [`UPDATE_SCHEDULE`], `DELETE`: delete a power-saving schedule.
///
/// Ported from `eero-api src/eero/api/power_saving.py:226-248` (`PowerSavingAPI.delete_schedule`).
pub const DELETE_SCHEDULE: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "power_saving/schedules",
    suffix: "",
    link: None,
};
