//! Support routes (`SupportAPI`).

// --------------------------------- support (`SupportAPI`) -------------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/support` — support information for a network.
///
/// Ported from `eero-api src/eero/api/support.py:35-60` (`SupportAPI.get_support`). Preferring
/// the network's own published `support` link.
pub const GET_SUPPORT_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/support",
    link: Some("support"),
};

/// `POST /2.2/networks/{id}/support` — file a support request, same sub-resource as
/// [`GET_SUPPORT_V8`].
///
/// Ported from `eero-api src/eero/api/support.py:65-101` (`SupportAPI.request_support`).
pub const REQUEST_SUPPORT_V8: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/support",
    link: Some("support"),
};
