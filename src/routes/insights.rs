//! Insights routes (`InsightsAPI`).

// ---------------------------------- insights (`InsightsAPI`) ----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/insights` — query insights time-series data.
///
/// The four query parameters (`start`, `end`, `insight_type`, `cadence`) are all required by
/// the server, but they are query-string parameters, not part of the path template — the
/// caller attaches them to the request at call time (e.g. via `reqwest::RequestBuilder::query`),
/// not through `Route::render`. Ported from `eero-api src/eero/api/insights.py:36`
/// (`InsightsAPI.get_insights`).
pub const GET_INSIGHTS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/insights",
};

/// `POST /2.2/networks/{network_id}/insights` — run insights analysis.
///
/// Ported from `eero-api src/eero/api/insights.py:115` (`InsightsAPI.run_insights`).
pub const RUN_INSIGHTS: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/insights",
};
