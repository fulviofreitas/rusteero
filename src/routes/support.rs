//! Support routes (`SupportAPI`).

// --------------------------------- support (`SupportAPI`) -------------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/support` — support information for a network.
///
/// Ported from `eero-api src/eero/api/support.py:33` (`SupportAPI.get_support`).
pub const GET_SUPPORT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/support",
};

/// `POST /2.2/networks/{network_id}/support` — file a support request.
///
/// Ported from `eero-api src/eero/api/support.py:56` (`SupportAPI.request_support`).
pub const REQUEST_SUPPORT: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/support",
};
