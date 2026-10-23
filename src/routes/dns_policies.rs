//! `dns_policies` routes (`DnsPoliciesAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/dns_policies.py` (v8.4.1). Premium (Eero Plus/Secure)
//! feature family; replaces the removed `ProfilesAPI` content-filter/block-list/blocked-app
//! writes (`crate::routes::profiles`'s module docs). No `DELETE` verb anywhere in this family —
//! removal is expressed as `is_delete: true` on a `PUT`.

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/dns_policies/advanced_content_filter` — the network's advanced
/// content filter allow/block lists. Live-verified.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:135-171`
/// (`DnsPoliciesAPI.get_advanced_content_filter`).
pub const DNS_POLICIES_GET_ADVANCED_CONTENT_FILTER: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/advanced_content_filter",
    link: Some("advanced_content_filter"),
};

/// `PUT /2.2/networks/{network_id}/dns_policies/network/allowed` — add/remove a domain from the
/// network-wide allow list.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:173-237` (`DnsPoliciesAPI.allow_domain`).
pub const DNS_POLICIES_ALLOW_DOMAIN: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/network/allowed",
    link: Some("dns_policies_network_allowed"),
};

/// `PUT /2.2/networks/{network_id}/dns_policies/network/allowed/cnames` — allow a list of CNAME
/// domains network-wide.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:239-279` (`DnsPoliciesAPI.allow_cnames`).
pub const DNS_POLICIES_ALLOW_CNAMES: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/network/allowed/cnames",
    link: Some("dns_policies_network_allowed_cnames"),
};

/// `PUT /2.2/networks/{network_id}/dns_policies/network/blocked` — add/remove a domain from the
/// network-wide block list.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:281-331` (`DnsPoliciesAPI.block_domain`).
pub const DNS_POLICIES_BLOCK_DOMAIN: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/network/blocked",
    link: Some("dns_policies_network_blocked"),
};

/// `PUT /2.2/networks/{network_id}/dns_policies/profiles/allowed` — add/remove a domain from one
/// or more profiles' allow lists.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:333-396`
/// (`DnsPoliciesAPI.allow_domain_for_profiles`).
pub const DNS_POLICIES_ALLOW_DOMAIN_FOR_PROFILES: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/profiles/allowed",
    link: Some("dns_policies_profiles_allowed"),
};

/// `PUT /2.2/networks/{network_id}/dns_policies/profiles/allowed/cnames` — allow a list of CNAME
/// domains for one or more profiles.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:398-441`
/// (`DnsPoliciesAPI.allow_cnames_for_profiles`).
pub const DNS_POLICIES_ALLOW_CNAMES_FOR_PROFILES: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/profiles/allowed/cnames",
    link: Some("dns_policies_profiles_allowed_cnames"),
};

/// `PUT /2.2/networks/{network_id}/dns_policies/profiles/blocked` — add/remove a domain from one
/// or more profiles' block lists.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:443-495`
/// (`DnsPoliciesAPI.block_domain_for_profiles`).
pub const DNS_POLICIES_BLOCK_DOMAIN_FOR_PROFILES: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/profiles/blocked",
    link: Some("dns_policies_profiles_blocked"),
};

/// `GET /2.2/networks/{network_id}/dns_policies/profiles/{profile_id}/applications` — the
/// applications a profile can block, and which are blocked. Live-verified.
///
/// `link: None` — this sub-tree shares no published link with any envelope this SDK can read
/// (`dns_policies.py:91-113`'s own docstring). `profile_id` must be normalised to its trailing id
/// via [`crate::util::id_from_url`] before resolving through this route — see
/// [`crate::endpoints::dns_policies::DnsPoliciesApi::get_profile_applications`]'s own docs for
/// why.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:91-113,497-525`
/// (`DnsPoliciesAPI._profile_applications_url`/`get_profile_applications`).
pub const DNS_POLICIES_GET_PROFILE_APPLICATIONS: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "dns_policies/profiles",
    suffix: "/applications",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/dns_policies/profiles/{profile_id}/applications/blocked` —
/// replace the blocked-applications list for a profile.
///
/// Same `profile_id` normalisation note as [`DNS_POLICIES_GET_PROFILE_APPLICATIONS`].
///
/// Ported from `eero-api src/eero/api/dns_policies.py:115-133,527-554`
/// (`DnsPoliciesAPI._profile_applications_blocked_url`/`set_profile_blocked_applications`).
pub const DNS_POLICIES_SET_PROFILE_BLOCKED_APPLICATIONS: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "dns_policies/profiles",
    suffix: "/applications/blocked",
    link: None,
};

/// `POST /2.2/networks/{network_id}/dns_policies/adblock` — switch ad blocking on or off,
/// network-wide or per profile.
///
/// `link: None`: the path is a literal sub-path of the network URL that no envelope publishes
/// as a link. `DnsPoliciesApi::set_network_ad_block` resolves the network base through
/// [`crate::params::resolve_network_url`] (the parent's own `url` when supplied, else the
/// `networks/{id}` template) and appends the text after `{id}` here, so this constant is the
/// single source of the suffix.
///
/// Ported from `eero-api src/eero/api/dns_policies.py:586-649` at `v8.4.1`
/// (`DnsPoliciesAPI.set_network_ad_block`).
pub const DNS_POLICIES_SET_NETWORK_AD_BLOCK: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/adblock",
    link: None,
};

/// `POST /2.2/networks/{network_id}/dns_policies/network` — set one boolean network-level DNS
/// policy switch (only `block_malware` is accepted).
///
/// `link: None`, resolved like [`DNS_POLICIES_SET_NETWORK_AD_BLOCK`].
///
/// Ported from `eero-api src/eero/api/dns_policies.py:651-739` at `v8.4.1`
/// (`DnsPoliciesAPI._set_network_dns_policy` / `set_network_block_malware`).
pub const DNS_POLICIES_SET_NETWORK_POLICY: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/dns_policies/network",
    link: None,
};
