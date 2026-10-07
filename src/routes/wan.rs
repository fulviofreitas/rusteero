//! `wan` routes (`WanAPI`, new in v8.0.0) — every endpoint here is served on API version 2.3
//! ([`crate::consts::API_VERSION_MULTISTATICIP`] / [`crate::consts::API_VERSION_SECONDARY_WAN`],
//! both `"2.3"`), unlike most of this crate's default-2.2 endpoints.
//!
//! Ported from `eero-api src/eero/api/wan.py` (v8.0.4).

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.3/networks/{id}/multistaticip` (or the parent's own `multistaticip` link) — get the
/// network's multi-static-IP configuration.
///
/// Ported from `eero-api src/eero/api/wan.py:41-78` (`WanAPI.get_multistaticip`):
/// `sub_resource_url(network_id, "networks/{id}/multistaticip", link="multistaticip", parent=..,
/// version=API_VERSION_MULTISTATICIP)`. On a network without the feature, the API has been
/// observed to return HTTP 404 with `error.network.multistaticip_not_found` — a generic 404,
/// mapped the same as any other (`Error::Api { status: 404, .. }`).
pub const WAN_GET_MULTISTATICIP: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_3,
    template: "networks/{id}/multistaticip",
    link: Some("multistaticip"),
};

/// `PUT /2.3/networks/{id}/multistaticip` — set the network's multi-static-IP configuration.
///
/// Ported from `eero-api src/eero/api/wan.py:83-119` (`WanAPI.set_multistaticip`):
/// `resource_url(network_id, "networks/{id}/multistaticip", version=API_VERSION_MULTISTATICIP)`
/// — no `parent=` parameter, unlike [`WAN_GET_MULTISTATICIP`].
pub const WAN_SET_MULTISTATICIP: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_3,
    template: "networks/{id}/multistaticip",
    link: None,
};

/// `PUT /2.3/networks/{id}/devices/secondary_wan_config` — set per-device secondary-WAN access
/// for the whole network in one call.
///
/// Ported from `eero-api src/eero/api/wan.py:123-169` (`WanAPI.set_secondary_wan_config`):
/// `resource_url(network_id, "networks/{id}/devices/secondary_wan_config",
/// version=API_VERSION_SECONDARY_WAN)` — no `parent=` parameter. Docstring calls this the
/// sibling of the `networks/{id}/settings` mesh-reboot-risk endpoint.
pub const WAN_SET_SECONDARY_WAN_CONFIG: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_3,
    template: "networks/{id}/devices/secondary_wan_config",
    link: None,
};

/// `PUT /2.3/networks/{network}/devices/{mac}` — set a single device's secondary-WAN access.
///
/// Ported from `eero-api src/eero/api/wan.py:172-216`
/// (`WanAPI.set_device_secondary_wan_access`): `resolve_nested_url(network_id, mac,
/// prefix="devices", version=API_VERSION_SECONDARY_WAN)` — no `suffix`, `mac` is the final path
/// segment; no `parent=` parameter.
pub const WAN_SET_DEVICE_SECONDARY_WAN_ACCESS: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_3,
    prefix: "devices",
    suffix: "",
    link: None,
};
