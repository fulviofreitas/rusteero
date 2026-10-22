//! Data-usage routes (`DataUsageAPI`).

// ------------------------------- data_usage (`DataUsageAPI`) ---------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

// ============================================================================================
// Every `DataUsageApi` method in `src/endpoints/data_usage.rs` is built on one of these. All
// twelve Python methods this group ports resolve their network base via
// `resolve_network_url(network_id, parent)` (`data_usage.py:156-157`'s shared `_get_usage`) —
// i.e. they prefer `parent`'s own published `url` over the bare `network_id` template, exactly
// like [`crate::params::resolve_network_url`] — then literal-append a fixed suffix. Each
// constant's `template` is the single source of that suffix: `DataUsageApi`'s private
// `route_url` appends the template text after `{id}` to the resolved network base, so a
// published `url` is never fed back through `Resource::resolve` (which would apply the
// caller-path family check to a link Python only shape-checks).
// ============================================================================================

/// `GET /2.2/networks/{id}/data_usage` — network-wide data-usage statistics.
///
/// Ported from `DataUsageAPI.get_data_usage` (`eero-api src/eero/api/data_usage.py:162-209` at
/// `v8.4.1`; every citation in this file is to that version). `cadence` is **required**
/// (`cadence_required=True`). API-Reference.md:987.
pub const V8_GET_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/breakdown` — data-usage breakdown by category.
///
/// Ported from `DataUsageAPI.get_breakdown` (`data_usage.py:211-253`). `cadence` is optional.
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
/// Ported from `DataUsageAPI.get_devices_usage` (`data_usage.py:255-302`) and
/// `DataUsageAPI.get_device_usage` (`data_usage.py:304-352`, which literal-appends
/// `/{_validate_child_id(device_mac)}` onto this same collection URL rather than a second
/// `.format()` call — `data_usage.py:343`). `cadence` is optional on the collection form,
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
/// Ported from `DataUsageAPI.get_eeros_summary` (`data_usage.py:406-452`). `cadence` is
/// required. API-Reference.md:991.
pub const GET_EEROS_DATA_USAGE_SUMMARY: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/eeros/summary",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/eeros` — data-usage statistics for every eero on a
/// network (`DataUsageApi::get_eeros_usage`), and the collection `DataUsageApi::get_eero_usage`
/// appends a single validated eero id onto.
///
/// Ported from `DataUsageAPI.get_eeros_usage` (`data_usage.py:354-404`, `cadence`
/// required, a path that is not in the observed spec) and `DataUsageAPI.get_eero_usage`
/// (`data_usage.py:454-502`): `.../data_usage/eeros/{_validate_child_id(eero_id)}`. `cadence` is
/// required on both. API-Reference.md:992.
pub const GET_EEROS_DATA_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/eeros",
    link: None,
};

/// `GET /2.2/networks/{id}/data_usage/profiles` — the collection
/// `DataUsageApi::get_profile_usage` appends a single validated profile id onto.
///
/// Ported from `DataUsageAPI.get_profile_usage` (`data_usage.py:504-552`):
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
/// Ported from `DataUsageAPI.get_unprofiled_devices` (`data_usage.py:554-596`). `cadence` is
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
/// Ported from `DataUsageAPI.get_unprofiled_summary` (`data_usage.py:598-644`). `cadence` is
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
/// Ported from `DataUsageAPI.get_report_settings` (`data_usage.py:646-670`).
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
/// Ported from `DataUsageAPI.set_report_settings` (`data_usage.py:672-720`), the only write in
/// this module: JSON body `{"cadence": cadence, "notification_day": notification_day}`
/// (`data_usage.py:719-720`); `cadence` is validated before this crate's own "not authenticated"
/// precondition even runs (`data_usage.py:710`, matched here by validating it before the
/// `Transport::request` call rather than inside it). API-Reference.md:996, unverified write.
pub const SET_DATA_USAGE_REPORT_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/data_usage/report_settings",
    link: None,
};
