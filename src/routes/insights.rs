//! Insights routes (`InsightsAPI`).
//!
//! `InsightsAPI.run_insights` (`insights.py:115-137` at `v6.2.0`) was removed entirely in
//! `05a2b07` (v8.0.0) — no endpoint operation corresponds to it any more, and it is not ported.

// ---------------------------------- insights (`InsightsAPI`) ----------------------------

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

// ============================================================================================
// v8.0.4 constants (`.claude/tasks/briefs/v8/g3-devices.md`). Every `InsightsApi` method in
// `src/endpoints/insights.rs` is built on one of these.
// ============================================================================================

/// `GET /2.2/networks/{id}/insights` — network-level insights time-series data. No `parent=`
/// kwarg exists on the Python method this ports, so this route is always resolved with
/// `parent: None`.
///
/// Ported from `InsightsAPI.get_insights` (`eero-api src/eero/api/insights.py:72-153` at
/// `v8.0.4`): `resolve_network_url(network_id) + "/insights"` (`insights.py:150-151`).
/// API-Reference.md:864, read.
pub const V8_GET_INSIGHTS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/insights",
    link: None,
};

/// `GET /2.2/networks/{id}/insights/devices` — insights for every device on a network,
/// preferring the network's own published `insights_devices` link when a `parent` envelope is
/// supplied.
///
/// Ported from `InsightsAPI.get_devices_insights` (`insights.py:155-196`):
/// `sub_resource_url(network, "networks/{id}/insights/devices", link="insights_devices",
/// parent=as_envelope(parent), version=API_VERSION_DEFAULT)`. API-Reference.md:865, verified
/// read.
pub const GET_DEVICES_INSIGHTS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/insights/devices",
    link: Some("insights_devices"),
};

/// `GET /2.2/networks/{network}/insights/devices/{mac}` — insights for a single device. No
/// `parent=` kwarg on the Python method this ports.
///
/// Ported from `InsightsAPI.get_device_insights` (`insights.py:198-233`):
/// `resolve_nested_url(network, mac, prefix="insights/devices")` (`insights.py:229`).
/// API-Reference.md:866, verified read.
pub const GET_DEVICE_INSIGHTS: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "insights/devices",
    suffix: "",
    link: None,
};

/// `GET /2.2/networks/{id}/insights/profiles` — insights for every profile on a network,
/// preferring the network's own published `insights_profiles` link when a `parent` envelope is
/// supplied.
///
/// Ported from `InsightsAPI.get_profiles_insights` (`insights.py:235-276`):
/// `sub_resource_url(network, "networks/{id}/insights/profiles", link="insights_profiles",
/// parent=as_envelope(parent), version=API_VERSION_DEFAULT)`. API-Reference.md:867, verified
/// read.
pub const GET_PROFILES_INSIGHTS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/insights/profiles",
    link: Some("insights_profiles"),
};

/// `GET /2.2/networks/{network}/insights/profiles/{profile}` — insights for a single profile.
/// No `parent=` kwarg on the Python method this ports.
///
/// Ported from `InsightsAPI.get_profile_insights` (`insights.py:278-313`):
/// `resolve_nested_url(network, profile, prefix="insights/profiles")`. API-Reference.md:868,
/// verified read.
pub const GET_PROFILE_INSIGHTS: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "insights/profiles",
    suffix: "",
    link: None,
};

/// `GET /2.2/networks/{network}/insights/profiles/{profile}/devices` — insights for every
/// device belonging to a single profile. No `parent=` kwarg on the Python method this ports.
///
/// Ported from `InsightsAPI.get_profile_devices_insights` (`insights.py:315-357`):
/// `resolve_nested_url(network, profile, prefix="insights/profiles", suffix="/devices")`.
/// API-Reference.md:869, verified read.
pub const GET_PROFILE_DEVICES_INSIGHTS: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "insights/profiles",
    suffix: "/devices",
    link: None,
};
