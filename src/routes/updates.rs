//! Firmware/software update routes (`UpdatesAPI`).

// ----------------------------------- updates (`UpdatesAPI`) -----------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/updates` — available firmware/software updates.
///
/// Ported from `eero-api src/eero/api/updates.py:35-60` (`UpdatesAPI.get_updates`). Preferring
/// the network's own published `updates` link.
pub const GET_UPDATES_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/updates",
    link: Some("updates"),
};

/// `POST /2.2/networks/{id}/updates` — apply a pending update, same sub-resource as
/// [`GET_UPDATES_V8`].
///
/// Ported from `eero-api src/eero/api/updates.py:65-98` (`UpdatesAPI.apply_update`). New at
/// v8.0.4; no legacy `Route` equivalent (`apply_update` never existed pre-v8.0.0), so no rename
/// collision.
pub const APPLY_UPDATE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/updates",
    link: Some("updates"),
};
