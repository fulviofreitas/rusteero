//! Routing routes (`RoutingAPI`).

// ----------------------------------- routing (`RoutingAPI`) -----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/routing` — routing information for a network.
///
/// Ported from `eero-api src/eero/api/routing.py:33` (`RoutingAPI.get_routing`).
pub const GET_ROUTING: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/routing",
};
