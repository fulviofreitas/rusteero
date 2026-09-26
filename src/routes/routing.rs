//! Routing routes (`RoutingAPI`).

// ----------------------------------- routing (`RoutingAPI`) -----------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/routing` — routing information for a network.
///
/// Ported from `eero-api src/eero/api/routing.py:35-65` (`RoutingAPI.get_routing`). The template
/// fallback (no `parent`, or a `parent` with no `routing` link) stays on
/// `API_VERSION_DEFAULT` (2.2); when `parent` carries a `routing` link, that link's own path is
/// used verbatim ([`crate::links::resolve_link`]) — which may itself encode a different API
/// version (observed: 2.3) directly in its string. No special-casing needed here: this is
/// ordinary [`Resource`]/`sub_resource_url` link preference, the version override falls out of
/// the link value itself.
pub const GET_ROUTING_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/routing",
    link: Some("routing"),
};
