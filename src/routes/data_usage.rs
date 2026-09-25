//! Data-usage routes (`DataUsageAPI`).

// ------------------------------- data_usage (`DataUsageAPI`) ---------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/data_usage` — data-usage statistics for the whole network.
///
/// Unusually for a `GET`, the Eero cloud API expects a JSON body on this request (timezone /
/// period filters); the request body is supplied by the caller at call time, not by this
/// route. Ported from `eero-api src/eero/api/data_usage.py:33` (`DataUsageAPI.get_data_usage`
/// called with `resource=None`).
pub const GET_DATA_USAGE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/data_usage",
};

/// `GET /2.2/networks/{network_id}/data_usage/{resource}` — data-usage statistics scoped to
/// one resource (e.g. `"devices"`, `"eeros"`).
///
/// Same JSON-body-on-GET caveat as `GET_DATA_USAGE`. Ported from
/// `eero-api src/eero/api/data_usage.py:33` (`DataUsageAPI.get_data_usage` called with a
/// non-`None` `resource`).
pub const GET_DATA_USAGE_RESOURCE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/data_usage/{resource}",
};
