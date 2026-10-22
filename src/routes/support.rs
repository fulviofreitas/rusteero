//! Support routes (`SupportAPI`).

// --------------------------------- support (`SupportAPI`) -------------------------------

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/support` — support information for a network.
///
/// Ported from `eero-api src/eero/api/support.py:35-69` (`SupportAPI.get_support`). Prefers the
/// network's own published `support` link, but the endpoint pins the resolved URL back to 2.2
/// with [`crate::links::rewrite_version`] (`support.py:67`), because the API publishes the link
/// on 2.3 for a current `User-Agent` and 2.3 returns a `contacts` array where 2.2 returns a flat
/// object. The pin lives at the call site, so this constant's `version` is only the template
/// fallback.
pub const GET_SUPPORT_V8: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/support",
    link: Some("support"),
};

/// `POST /2.2/networks/{id}/support` — file a support request, same sub-resource as
/// [`GET_SUPPORT_V8`].
///
/// Ported from `eero-api src/eero/api/support.py:71-110` (`SupportAPI.request_support`). Not
/// version-pinned: a published `support` link is followed on whatever version it names.
pub const REQUEST_SUPPORT_V8: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/support",
    link: Some("support"),
};
