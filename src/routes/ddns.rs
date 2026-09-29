//! `ddns` routes (`DdnsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/ddns.py` (v8.0.4). Both writes are parameterless PUTs —
//! the API declares no request body for either endpoint, so neither
//! [`crate::endpoints::ddns::DdnsApi::enable`] nor [`crate::endpoints::ddns::DdnsApi::disable`]
//! sends one (`crate::transport::RequestBody::None`, mirroring Python's `RequestEncoding.NONE`).

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `PUT /2.2/networks/{id}/ddns/enable` (or the parent's own `ddns_enable` link) — enable
/// dynamic DNS for a network.
///
/// Ported from `eero-api src/eero/api/ddns.py:40-79` (`DdnsAPI.enable`): `sub_resource_url(
/// network_id, "networks/{id}/ddns/enable", link="ddns_enable", parent=..)`.
pub const DDNS_ENABLE: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/ddns/enable",
    link: Some("ddns_enable"),
};

/// `PUT /2.2/networks/{id}/ddns/disable` (or the parent's own `ddns_disable` link) — disable
/// dynamic DNS for a network.
///
/// Ported from `eero-api src/eero/api/ddns.py:82-120` (`DdnsAPI.disable`): same shape as
/// [`DDNS_ENABLE`], `sub_resource_url(network_id, "networks/{id}/ddns/disable",
/// link="ddns_disable", parent=..)`.
pub const DDNS_DISABLE: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/ddns/disable",
    link: Some("ddns_disable"),
};
