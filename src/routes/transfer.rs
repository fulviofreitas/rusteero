//! Transfer-statistics routes (`TransferAPI`).

// ---------------------------------- transfer (`TransferAPI`) ----------------------------

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

// ============================= v8.0.4 (`Resource`/`Nested`) constants =============================

/// `GET /2.2/networks/{id}/transfer` — network-wide transfer statistics.
///
/// Ported from `eero-api src/eero/api/transfer.py:36-79` (`TransferAPI.get_transfer_stats`
/// called with `device_id=None`). Preferring the network's own published `transfer` link.
pub const GET_TRANSFER_STATS_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/transfer",
    link: Some("transfer"),
};

/// `GET /2.2/networks/{network}/devices/{device}/transfer` — one device's transfer statistics.
///
/// Ported from `eero-api src/eero/api/transfer.py:36-79` (`TransferAPI.get_transfer_stats`
/// called with a non-empty `device_id`), resolved via `resolve_nested_url(network_id, device_id,
/// prefix="devices", suffix="/transfer")` — a **literal** path, not a published link (`link:
/// None`): no `parent`/`link` argument is passed on this branch in Python at all.
pub const GET_DEVICE_TRANSFER_STATS_V8: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "devices",
    suffix: "/transfer",
    link: None,
};
