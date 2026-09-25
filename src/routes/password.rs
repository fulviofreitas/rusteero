//! Wi-Fi password routes (`PasswordAPI`).

// --------------------------------- password (`PasswordAPI`) -----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/password` — the network's Wi-Fi password.
///
/// Sensitive: callers must route the response through the same secure-logging discipline as
/// `eero-api`'s `get_secure_logger` (`password.py:14`) — never log the raw envelope at `debug`
/// or below. Ported from `eero-api src/eero/api/password.py:33` (`PasswordAPI.get_password`).
pub const GET_PASSWORD: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/password",
};
