//! `permissions` routes (`PermissionsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/permissions.py` (v8.0.4).

use super::{ApiVersion, Resource};
use reqwest::Method;

/// Documentation-only route marker for `networks/{id}/permissions`.
///
/// `PermissionsApi::get_permissions` does **not** resolve through this constant via
/// [`crate::routes::Resource::resolve`]: Python builds the URL as
/// `f"{resolve_network_url(network_id, parent)}/permissions"` (`permissions.py:60`) —
/// `_params.resolve_network_url`, which prefers `parent`'s own top-level `url` field (self-link
/// preference), not a named `resources.<link>` lookup — then appends the literal `/permissions`
/// suffix. There is no published `permissions` link anywhere in the API's `resources` maps; this
/// is a conventional sub-path, not a link. See
/// [`PermissionsApi::get_permissions`](crate::endpoints::permissions::PermissionsApi::get_permissions)'s
/// own doc comment for the resolution this constant documents.
pub const PERMISSIONS_GET_PERMISSIONS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/permissions",
    link: None,
};
