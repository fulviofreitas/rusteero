//! Data-usage routes (`DataUsageAPI`).

// ------------------------------- data_usage (`DataUsageAPI`) ---------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

// ============================================================================================
// v8.0.4 constants. Every `DataUsageApi` method in
// `src/endpoints/data_usage.rs` is built on one of these. All eleven Python methods this group
// ports resolve their network segment via `resolve_network_url(network_id, parent)`
// (`data_usage.py:97-153`'s shared `_get_usage`) — i.e. they prefer `parent`'s own published
// `url` over the bare `network_id` template, exactly like [`crate::params::resolve_network_url`]
// — then literal-append a fixed suffix. `Resource::resolve`'s own `parent` handling only
// supports a *named* link inside a *different* resource's `resources` map (`sub_resource_url`),
// not "prefer this same resource's own `url`" (`resolve_network_url`), so every endpoint method
// below performs that preference itself (via `crate::links::self_url`) and then calls
// `Resource::resolve` with the *resolved* id-or-url string and `parent: None` — see
// `DataUsageApi`'s private `network_id_or_self_url` helper.
// ============================================================================================

/// `GET /2.2/networks/{id}/data_usage` — network-wide data-usage statistics.
///
/// Ported from `DataUsageAPI.get_data_usage` (`eero-api src/eero/api/data_usage.py:155-197` at
/// `v8.0.4`). `cadence` is **required** (`cadence_required=True`). API-Reference.md:987.
pub const V8_GET_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/breakdown` — data-usage breakdown by category.
///
/// Ported from `DataUsageAPI.get_breakdown` (`data_usage.py:199-236`). `cadence` is optional.
/// API-Reference.md:988.
pub const GET_DATA_USAGE_BREAKDOWN: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/breakdown",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/devices` — data-usage statistics for every device on a
/// network (optionally filtered by `profile_id`), and the collection
/// `DataUsageApi::get_device_usage` appends a single validated device id onto.
///
/// Ported from `DataUsageAPI.get_devices_usage` (`data_usage.py:238-280`) and
/// `DataUsageAPI.get_device_usage` (`data_usage.py:282-325`, which literal-appends
/// `/{_validate_child_id(device_mac)}` onto this same collection URL rather than a second
/// `.format()` call — `data_usage.py:317`). `cadence` is optional on the collection form,
/// required on the single-device form. API-Reference.md:989, :990.
pub const GET_DEVICES_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/devices",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/eeros/summary` — aggregated data-usage summary across
/// every eero on a network.
///
/// Ported from `DataUsageAPI.get_eeros_summary` (`data_usage.py:327-368`). `cadence` is
/// required. API-Reference.md:991.
pub const GET_EEROS_DATA_USAGE_SUMMARY: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/eeros/summary",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/eeros` — the collection
/// `DataUsageApi::get_eero_usage` appends a single validated eero id onto.
///
/// Ported from `DataUsageAPI.get_eero_usage` (`data_usage.py:370-413`):
/// `.../data_usage/eeros/{_validate_child_id(eero_id)}`. `cadence` is required.
/// API-Reference.md:992.
pub const GET_EEROS_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/eeros",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/profiles` — the collection
/// `DataUsageApi::get_profile_usage` appends a single validated profile id onto.
///
/// Ported from `DataUsageAPI.get_profile_usage` (`data_usage.py:415-458`):
/// `.../data_usage/profiles/{_validate_child_id(profile_id)}`. `cadence` is required.
/// API-Reference.md:993.
pub const GET_PROFILES_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/profiles",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/unprofiled/devices` — data-usage statistics for devices
/// with no profile.
///
/// Ported from `DataUsageAPI.get_unprofiled_devices` (`data_usage.py:460-497`). `cadence` is
/// optional. API-Reference.md:994.
pub const GET_UNPROFILED_DEVICES_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/unprofiled/devices",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/unprofiled/summary` — aggregated data-usage summary for
/// unprofiled devices.
///
/// Ported from `DataUsageAPI.get_unprofiled_summary` (`data_usage.py:499-540`). `cadence` is
/// required. Listed as part of the family in `wiki/API-Reference.md`'s `DataUsageAPI` table (no
/// dedicated numbered bullet).
pub const GET_UNPROFILED_DATA_USAGE_SUMMARY: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/unprofiled/summary",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/report_settings` — the network's data-usage report
/// settings. No query parameters at all.
///
/// Ported from `DataUsageAPI.get_report_settings` (`data_usage.py:542-566`).
/// API-Reference.md:995 ("No query parameters").
pub const GET_DATA_USAGE_REPORT_SETTINGS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/report_settings",
    link: None,
};

/// `PUT /2.2/networks/{id}/data_usage/report_settings` — set the network's data-usage report
/// settings.
///
/// Ported from `DataUsageAPI.set_report_settings` (`data_usage.py:568-621`), the only write in
/// this module: JSON body `{"cadence": cadence, "notification_day": notification_day}`
/// (`data_usage.py:620`); `cadence` is validated before this crate's own "not authenticated"
/// precondition even runs (`data_usage.py:614`, matched here by validating it before the
/// `Transport::request` call rather than inside it). API-Reference.md:996, unverified write.
pub const SET_DATA_USAGE_REPORT_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/report_settings",
    link: None,
};
