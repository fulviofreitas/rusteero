//! `entitlements` routes (`EntitlementsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/entitlements.py` (v8.0.4).

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/entitlements/networks/{id}/features` — a network's entitled features.
///
/// Ported from `EntitlementsAPI.get_features` (`entitlements.py:37-61`). No `parent` in Python;
/// `link: None` — this resource is never published as a named link.
pub const GET_FEATURES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "entitlements/networks/{id}/features",
    link: None,
};

/// `GET /2.2/entitlements/networks/{id}/upsell_features` — a network's upsell features.
///
/// Ported from `EntitlementsAPI.get_upsell_features` (`entitlements.py:62-86`).
pub const GET_UPSELL_FEATURES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "entitlements/networks/{id}/upsell_features",
    link: None,
};

/// `GET /2.2/eero_models/capabilities` — eero model capabilities.
///
/// Ported from `EntitlementsAPI.get_model_capabilities` (`entitlements.py:87-116`). **Fixed
/// path, no `{id}`**: this endpoint has no path-scoped variant, so id/path/URL polymorphism does
/// not apply (`entitlements.py:99-101` docstring) — the network id is sent verbatim as the
/// `networkId` query parameter instead, with no `resource_url`/`validate_identifier` call at
/// all.
pub const GET_MODEL_CAPABILITIES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "eero_models/capabilities",
    link: None,
};

/// `GET /2.2/premium/customer` — the premium customer record.
///
/// Ported from `EntitlementsAPI.get_premium_customer` (`entitlements.py:117-136`). Fixed path,
/// no `{id}`, not network-scoped at all.
pub const GET_PREMIUM_CUSTOMER: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "premium/customer",
    link: None,
};
