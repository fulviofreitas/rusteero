//! DNS routes (`DnsAPI`), v8.0.4.
//!
//! Ported from `eero-api src/eero/api/dns.py` at v8.0.4. Every write funnels through the
//! network's `settings` sub-resource — `DNS_PUT_SETTINGS` below — resolved via the v8
//! [`Resource`] model (`crate::links::sub_resource_url`, preferring a supplied parent's
//! published `settings` link over the bare-id template). The read (`GET_DNS_SETTINGS`) never
//! receives a `parent` from Python (`dns.py:222`'s `get_dns_settings` has no `parent` keyword at
//! all), so it resolves via the plain bare-id template every time — see
//! [`crate::endpoints::dns::DnsApi::get_dns_settings`].

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{id}` — DNS settings live inside the full network object; there is no
/// dedicated DNS sub-resource on the wire.
///
/// Ported from `eero-api src/eero/api/dns.py:222-249` (`DnsAPI.get_dns_settings`), which resolves
/// via `_params.resolve_network_url(network_id)` with no `parent` argument at all — since this
/// route's `link` is `None`, [`Resource::resolve`] falls straight to
/// [`crate::links::resource_url`] regardless of what `parent` a caller passes, matching that
/// behaviour exactly.
pub const GET_DNS_SETTINGS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}",
    link: None,
};

/// `PUT /2.2/networks/{id}/settings` — the network's settings sub-resource, the single wire
/// endpoint behind every `DnsAPI` write.
///
/// Ported from `eero-api src/eero/api/dns.py:183-220` (`DnsAPI._put_settings`), which resolves
/// via `links.sub_resource_url(network_id, "networks/{id}/settings", link="settings",
/// parent=...)` — preferring a supplied parent's published `settings` link over the bare-id
/// template, exactly what [`Resource::resolve`] does when [`Resource::link`] is `Some("settings")`.
pub const DNS_PUT_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};
