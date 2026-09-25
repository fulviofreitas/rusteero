//! Port-forward routes (`ForwardsAPI`).

// ---------------------------------- forwards (`ForwardsAPI`) ----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/forwards` — list port forwards.
///
/// Ported from `eero-api src/eero/api/forwards.py:33` (`ForwardsAPI.get_forwards`).
pub const GET_FORWARDS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/forwards",
};

/// `POST /2.2/networks/{network_id}/forwards` — create a port forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:56` (`ForwardsAPI.create_forward`).
pub const CREATE_FORWARD: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/forwards",
};

/// `DELETE /2.2/networks/{network_id}/forwards/{forward_id}` — delete a port forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:81` (`ForwardsAPI.delete_forward`).
pub const DELETE_FORWARD: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/forwards/{forward_id}",
};
