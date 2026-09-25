//! Settings routes (`SettingsAPI`).

// ---------------------------------- settings (`SettingsAPI`) ----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/settings` — read the network-wide settings resource.
///
/// Distinct from `PUT_NETWORK_SETTINGS`: same resource path, opposite verb. Ported from
/// `eero-api src/eero/api/settings.py:33` (`SettingsAPI.get_settings`).
pub const GET_SETTINGS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/settings",
};
