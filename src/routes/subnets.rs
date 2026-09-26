//! `subnets` routes (`SubnetsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/subnets.py` (v8.0.4).

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}/subnets_config` (or the parent's own `subnets_config` link) — get the
/// network's subnets configuration.
///
/// Ported from `eero-api src/eero/api/subnets.py:43-75` (`SubnetsAPI.get_config`):
/// `sub_resource_url(network_id, "networks/{id}/subnets_config", link="subnets_config",
/// parent=..)`.
pub const SUBNETS_GET_CONFIG: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/subnets_config",
    link: Some("subnets_config"),
};

/// `PUT /2.2/networks/{id}/subnets_config` — create or edit a subnet configuration.
///
/// Ported from `eero-api src/eero/api/subnets.py:78-107` (`SubnetsAPI.set_config`):
/// `resource_url(network_id, "networks/{id}/subnets_config")` — **no `parent=` parameter**,
/// unlike [`SUBNETS_GET_CONFIG`]; always resolves from the template.
pub const SUBNETS_SET_CONFIG: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/subnets_config",
    link: None,
};

/// `DELETE /2.2/networks/{network}/subnets_config/{subnet_type}` — delete a subnet's
/// configuration.
///
/// Ported from `eero-api src/eero/api/subnets.py:113-141` (`SubnetsAPI.delete_subnet`):
/// `child_url(resource_url(network_id, "networks/{id}/subnets_config"), subnet_type)`. Resolved
/// via [`crate::routes::Nested::resolve`], which reduces to the identical two-step
/// `resource_url` + `child_url` call for the bare-identifier `subnet_type` values this method is
/// ever called with in practice (there is no fixed vocabulary the API publishes, but every
/// documented `subnet_type` — e.g. `"main"`, `"guest"` — is a single path segment). Deliberate,
/// low-impact deviation: unlike Python's `child_url`, which rejects a `subnet_type` value that
/// happens to start with `/` or `http(s)://` outright, `Nested::resolve` would instead try to
/// resolve it as an already-encoded nested path/URL (and still fail, just via a different
/// validation branch, for anything that is not actually a `subnets_config` path on this
/// network) — see `crate::params::resolve_nested_url`'s own docs.
pub const SUBNETS_DELETE_SUBNET: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "subnets_config",
    suffix: "",
    link: None,
};

/// `PUT /2.2/networks/{id}/subnets_config/dns_policies/content_filters` — set content filters
/// for one or more subnets.
///
/// Ported from `eero-api src/eero/api/subnets.py:143-177` (`SubnetsAPI.set_content_filters`):
/// `resource_url(network_id, "networks/{id}/subnets_config/dns_policies/content_filters")` — no
/// `parent=` parameter.
pub const SUBNETS_SET_CONTENT_FILTERS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/subnets_config/dns_policies/content_filters",
    link: None,
};

/// `GET /2.2/networks/{network}/subnets_config/{subnet_id}/dns_policies/content_filters` — get
/// content filters for a subnet.
///
/// Ported from `eero-api src/eero/api/subnets.py:181-203` (`SubnetsAPI.get_content_filters`):
/// `resolve_nested_url(network_id, subnet_id, prefix="subnets_config",
/// suffix="/dns_policies/content_filters")` — no `link=`/`parent=` at all, unlike
/// [`SUBNETS_GET_CONFIG`].
pub const SUBNETS_GET_CONTENT_FILTERS: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "subnets_config",
    suffix: "/dns_policies/content_filters",
    link: None,
};
