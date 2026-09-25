//! Device-blacklist routes (`BlacklistAPI`).

// ------------------------------ blacklist (`BlacklistAPI`) -----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/blacklist` — list blacklisted (blocked) devices.
///
/// Ported from `eero-api src/eero/api/blacklist.py:33` (`BlacklistAPI.get_blacklist`).
pub const GET_BLACKLIST: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/blacklist",
};

/// `POST /2.2/networks/{network_id}/blacklist` — add a device (by MAC) to the blacklist.
///
/// Ported from `eero-api src/eero/api/blacklist.py:56` (`BlacklistAPI.add_to_blacklist`).
/// Also the first of the two round-trips behind `DevicesAPI.block_device(blocked=true)`
/// (`devices.py:136-186`; `eero-api` issue #109) — no separate route is needed there.
pub const ADD_TO_BLACKLIST: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/blacklist",
};

/// `DELETE /2.2/networks/{network_id}/blacklist/{mac_or_device_id}` — remove a device from
/// the blacklist.
///
/// `mac_or_device_id` accepts either a colon-separated MAC or Eero's blacklist `device_id`
/// (the same MAC with colons stripped). Ported from
/// `eero-api src/eero/api/blacklist.py:81` (`BlacklistAPI.remove_from_blacklist`). Also
/// what `DevicesAPI.block_device(blocked=false)` calls (`devices.py:136-186`; `eero-api`
/// issue #109) — no separate route is needed there.
pub const REMOVE_FROM_BLACKLIST: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/blacklist/{mac_or_device_id}",
};
