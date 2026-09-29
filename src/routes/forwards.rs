//! Port-forward routes (`ForwardsAPI`).

// ---------------------------------- forwards (`ForwardsAPI`) ----------------------------

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

// ------------------------------------ v8.0.4 routes --------------------------------------

/// `GET /2.2/networks/{id}/forwards` (or the parent's own `forwards` link) — list port
/// forwards.
///
/// Ported from `eero-api src/eero/api/forwards.py:80-105` (v8.0.4, `ForwardsAPI.get_forwards`):
/// `sub_resource_url(network, "networks/{id}/forwards", link="forwards", parent=..,
/// version=API_VERSION_DEFAULT)`.
pub const FORWARDS_GET: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/forwards",
    link: Some("forwards"),
};

/// `POST /2.2/networks/{id}/forwards` (or the parent's own `forwards` link) — create a port
/// forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:113-151` (v8.0.4,
/// `ForwardsAPI.create_forward`): same URL resolution as [`FORWARDS_GET`].
pub const FORWARDS_CREATE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/forwards",
    link: Some("forwards"),
};

/// `PUT /2.2/networks/{network}/forwards/{forward}` (or an already-resolved path/URL) — update
/// a port forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:153-184` (v8.0.4, `ForwardsAPI.update_forward`
/// via the free function `_resolve_forward_url`): `resolve_nested_url(network, forward,
/// prefix="forwards")`, used by [`crate::endpoints::forwards::ForwardsApi::update_forward`] only
/// for its bare-id and path/URL dispatch branches — the "`forward` is a cached envelope" branch
/// resolves via [`crate::links::self_url`] directly instead, never through this route. No
/// `link`: Python's `_resolve_forward_url` never consults a parent's `resources` map.
pub const FORWARDS_UPDATE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "forwards",
    suffix: "",
    link: None,
};

/// `DELETE /2.2/networks/{network}/forwards/{forward}` (bare id, path, or URL) — delete a port
/// forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:186-203` (v8.0.4, `ForwardsAPI.delete_forward`):
/// `resolve_nested_url(network, forward, prefix="forwards")`.
pub const FORWARDS_DELETE: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "forwards",
    suffix: "",
    link: None,
};
