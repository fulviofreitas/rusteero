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

/// `networks/{id}/subnets_config` — the collection
/// [`SubnetsApi::delete_subnet`](crate::endpoints::subnets::SubnetsApi::delete_subnet) resolves via
/// [`Resource::resolve`], then appends a validated `subnet_type` onto with
/// [`crate::links::child_url`] directly, rather than going through a [`Nested`] route.
///
/// Ported from `eero-api src/eero/api/subnets.py:113-141` (`SubnetsAPI.delete_subnet`):
/// `child_url(resource_url(network_id, "networks/{id}/subnets_config"), subnet_type)`.
/// **Deliberately not a [`Nested`]** (orchestrator decision, phase-G fix list item 12): a
/// `Nested`'s `child` accepts a path/URL as well as a bare id (`crate::params::resolve_nested_url`
/// treats anything starting with `/`/`http(s)://` as an already-resolved nested path to verify),
/// but Python's `child_url` is stricter — `subnet_type` must always be a bare single-segment
/// identifier, and a path or absolute URL is rejected outright, not interpreted. Using a
/// `Resource` for the collection plus a direct `child_url` call reproduces that stricter rule
/// exactly, instead of silently accepting a wider set of inputs than the Python SDK does.
/// [`Resource::method`] is `DELETE` even though this constant only ever resolves the *collection*
/// URL (the member id is appended afterwards) — matching every other route constant's convention
/// of carrying its call site's HTTP verb.
pub const SUBNETS_CONFIG_COLLECTION: Resource = Resource {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    template: "networks/{id}/subnets_config",
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
