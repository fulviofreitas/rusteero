//! Routing routes (`RoutingAPI`).

// ----------------------------------- routing (`RoutingAPI`) -----------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/routing` — routing information for a network.
///
/// Ported from `eero-api src/eero/api/routing.py:35-74` (`RoutingAPI.get_routing`). Prefers the
/// network's own published `routing` link, but the endpoint pins the resolved URL back to 2.2 with
/// [`crate::links::rewrite_version`] (`routing.py:72`), "for safety": the API publishes the link
/// on 2.3 for a current `User-Agent`, and the known shape is the same on both versions. The pin
/// lives at the call site, so this constant's `version` is only the template fallback.
pub const GET_ROUTING_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/routing",
    link: Some("routing"),
};
