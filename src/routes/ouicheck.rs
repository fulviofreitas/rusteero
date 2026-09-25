//! OUI-check routes (`OUICheckAPI`).

// --------------------------------- ouicheck (`OUICheckAPI`) -----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/ouicheck` — OUI (vendor MAC prefix) check results.
///
/// Ported from `eero-api src/eero/api/ouicheck.py:33` (`OUICheckAPI.get_ouicheck`).
pub const GET_OUICHECK: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/ouicheck",
};

/// `POST /2.2/networks/{network_id}/ouicheck` — run an OUI check.
///
/// Ported from `eero-api src/eero/api/ouicheck.py:56` (`OUICheckAPI.run_ouicheck`).
pub const RUN_OUICHECK: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/ouicheck",
};
