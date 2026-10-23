//! Firmware/software update routes (`UpdatesAPI`).

// ----------------------------------- updates (`UpdatesAPI`) -----------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/updates` — available firmware/software updates.
///
/// Ported from `eero-api src/eero/api/updates.py:36-64` (`UpdatesAPI.get_updates`). Preferring
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
/// Ported from `eero-api src/eero/api/updates.py:66-105` (`UpdatesAPI.apply_update`). New at
/// v8.0.4; no legacy `Route` equivalent (`apply_update` never existed pre-v8.0.0), so no rename
/// collision.
pub const UPDATES_APPLY_UPDATE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/updates",
    link: Some("updates"),
};

/// `POST /2.2/networks/{id}/updates/preferred_update_hour` — set the hour of day the network
/// applies firmware updates.
///
/// Ported from `eero-api src/eero/api/updates.py:107-137` (`UpdatesAPI.set_preferred_update_hour`),
/// new at v8.2.0. Template only (`link: None`): the network envelope publishes no link to this
/// resource, so the path is always built from the template.
pub const UPDATES_SET_PREFERRED_UPDATE_HOUR: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/updates/preferred_update_hour",
    link: None,
};
