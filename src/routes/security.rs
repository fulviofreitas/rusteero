//! Security routes (`SecurityAPI`), v8.0.4.
//!
//! Ported from `eero-api src/eero/api/security.py` at v8.0.4. `GET_SECURITY_SETTINGS` is *not*
//! modelled as a [`Resource`]: `SecurityAPI.get_security_settings` resolves via the module's own
//! `_network_own_url` helper (`security.py:26-31`), which prefers a supplied parent's own `url`
//! field (`self_url`) — a different preference rule than [`Resource`]'s `link`-based
//! `resolve_link`. That is byte-for-byte the same rule [`crate::params::resolve_network_url`]
//! already implements (shared with `NetworksAPI`/`SqmAPI`'s identical local reimplementations —
//! unifying on the shared helper avoids replicating
//! the duplication), so [`crate::endpoints::security::SecurityApi::get_security_settings`] calls
//! that helper directly plus [`crate::transport::Transport::request`], rather than going through
//! a `Resource` constant that cannot express "prefer `self_url`" at all.
//!
//! Every write below, by contrast, targets a genuine sub-resource published as a named link in
//! the network's own `resources` object, which *is* exactly what [`Resource`]'s `link` field
//! models — each constant pairs one PUT with the `resources.<name>` link `sub_resource_url`
//! prefers.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `PUT /2.2/networks/{id}/settings` — the network's settings sub-resource, shared by
/// `set_wpa3`/`set_band_steering`/`set_upnp`/`set_ipv6`/`configure_security`.
///
/// Ported from `eero-api src/eero/api/security.py:82-134` (`SecurityAPI.set_wpa3`, the first of
/// the five settings-class writes sharing this resource; see each method's own endpoint doc
/// comment for its citation). A separate constant from [`crate::routes::dns::DNS_PUT_SETTINGS`]
/// (same template/link, different domain module) — route constant names must be unique
/// crate-wide.
pub const SECURITY_PUT_SETTINGS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};

/// `PUT /2.2/networks/{id}/mlo_mode` — the network's MLO (Multi-Link Operation) mode sub-resource.
///
/// Ported from `eero-api src/eero/api/security.py:376-425` (`SecurityAPI.set_mlo_mode`).
pub const SET_MLO_MODE: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/mlo_mode",
    link: Some("mlo_mode"),
};

/// `GET /2.2/networks/{id}/fast_transition` — the network's 802.11r fast-transition sub-resource.
///
/// Ported from `eero-api src/eero/api/security.py:427-460` (`SecurityAPI.get_fast_transition`).
pub const GET_FAST_TRANSITION: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/fast_transition",
    link: Some("fast_transition"),
};

/// `PUT /2.2/networks/{id}/fast_transition` — writes the same sub-resource
/// [`GET_FAST_TRANSITION`] reads.
///
/// Ported from `eero-api src/eero/api/security.py:462-502` (`SecurityAPI.set_fast_transition`).
pub const SET_FAST_TRANSITION: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/fast_transition",
    link: Some("fast_transition"),
};

/// `PUT /2.2/networks/{id}/passpoint/enabled` — the network's Passpoint enable/disable
/// sub-resource.
///
/// Ported from `eero-api src/eero/api/security.py:504-541` (`SecurityAPI.set_passpoint_enabled`).
pub const SET_PASSPOINT_ENABLED: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/passpoint/enabled",
    link: Some("passpoint"),
};

/// `PUT /2.2/networks/{id}/proxied_nodes` — the network's proxied-nodes sub-resource.
///
/// Ported from `eero-api src/eero/api/security.py:543-581` (`SecurityAPI.set_proxied_nodes`, the
/// last method in the module).
pub const SET_PROXIED_NODES: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/proxied_nodes",
    link: Some("proxied_nodes"),
};
