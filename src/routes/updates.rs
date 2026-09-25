//! Firmware/software update routes (`UpdatesAPI`).

// ----------------------------------- updates (`UpdatesAPI`) -----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/updates` — available firmware/software updates.
///
/// Ported from `eero-api src/eero/api/updates.py:33` (`UpdatesAPI.get_updates`).
pub const GET_UPDATES: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/updates",
};
