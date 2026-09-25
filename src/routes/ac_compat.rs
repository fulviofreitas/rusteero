//! AC-compatibility routes (`ACCompatAPI`).

// ------------------------------ ac_compat (`ACCompatAPI`) ------------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/ac_compat` — AC compatibility information for a network.
///
/// Ported from `eero-api src/eero/api/ac_compat.py:33` (`ACCompatAPI.get_ac_compat`).
pub const GET_AC_COMPAT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/ac_compat",
};
