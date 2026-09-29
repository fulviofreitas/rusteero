//! AC-compatibility routes (`ACCompatAPI`).

// ------------------------------ ac_compat (`ACCompatAPI`) ------------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/ac_compat` — AC compatibility information for a network.
///
/// Ported from `eero-api src/eero/api/ac_compat.py:35-60` (`ACCompatAPI.get_ac_compat`).
/// Preferring the network's own published `ac_compat` link.
pub const GET_AC_COMPAT_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/ac_compat",
    link: Some("ac_compat"),
};
