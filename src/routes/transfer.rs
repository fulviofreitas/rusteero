//! Transfer-statistics routes (`TransferAPI`).

// ---------------------------------- transfer (`TransferAPI`) ----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/transfer` — network-wide transfer statistics.
///
/// Ported from `eero-api src/eero/api/transfer.py:33` (`TransferAPI.get_transfer_stats`
/// called with `device_id=None`).
pub const GET_TRANSFER_STATS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/transfer",
};

/// `GET /2.2/networks/{network_id}/devices/{device_id}/transfer` — one device's transfer
/// statistics.
///
/// Ported from `eero-api src/eero/api/transfer.py:33` (`TransferAPI.get_transfer_stats`
/// called with a non-`None` `device_id`).
pub const GET_DEVICE_TRANSFER_STATS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/devices/{device_id}/transfer",
};
