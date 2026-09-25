//! Diagnostics routes (`DiagnosticsAPI`).

// ------------------------------- diagnostics (`DiagnosticsAPI`) ------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/diagnostics` — network diagnostics information.
///
/// Ported from `eero-api src/eero/api/diagnostics.py:33` (`DiagnosticsAPI.get_diagnostics`).
pub const GET_DIAGNOSTICS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/diagnostics",
};

/// `POST /2.2/networks/{network_id}/diagnostics` — run network diagnostics.
///
/// Ported from `eero-api src/eero/api/diagnostics.py:56` (`DiagnosticsAPI.run_diagnostics`).
pub const RUN_DIAGNOSTICS: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/diagnostics",
};
