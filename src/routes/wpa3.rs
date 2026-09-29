//! `wpa3` routes (`Wpa3API`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/wpa3.py` at v8.0.4 — a module with no `v6.2.0`
//! predecessor. Both methods share the same sub-resource.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/wpa3_per_band` — the network's per-band WPA3 mode sub-resource.
///
/// Ported from `eero-api src/eero/api/wpa3.py:60-93` (`Wpa3API.get_wpa3_per_band`).
pub const GET_WPA3_PER_BAND: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/wpa3_per_band",
    link: Some("wpa3_per_band"),
};

/// `PUT /2.2/networks/{id}/wpa3_per_band` — writes the same sub-resource [`GET_WPA3_PER_BAND`]
/// reads.
///
/// Ported from `eero-api src/eero/api/wpa3.py:95-158` (`Wpa3API.set_wpa3_per_band`).
pub const SET_WPA3_PER_BAND: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/wpa3_per_band",
    link: Some("wpa3_per_band"),
};
