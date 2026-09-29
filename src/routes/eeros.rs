//! Eero-node routes (`EerosAPI`), ported from `eero-api src/eero/api/eeros.py` at v8.0.4.
//!
//! Several methods here (`get_eero`, `get_led_status`, `set_location`) resolve their URL by
//! preferring a parent envelope's own `url` field (`crate::links::self_url`) over the template —
//! not a named `resources` link. [`crate::routes::Resource::resolve`] only implements the
//! link-preferring shape (`crate::links::sub_resource_url`), so those methods do not call
//! `Resource::resolve` at all; the constants below still record their `method`/`version`/
//! `template` for documentation, and `src/endpoints/eeros.rs` resolves them by hand via
//! `crate::links::self_url`/`crate::links::resource_url` directly (transport-api.md's "rarely
//! `.request(..)`" case). Nightlight discovery (`get_nightlight`/`set_nightlight`) has no fixed
//! route at all — its URL is read out of a parent's `data.nightlight.url` field, or discovered
//! with a live `GET` of the eero's own URL; see that endpoint's own docs.
//!
//! `port_action` addresses two ids (an eero id and a port/interface number) and so cannot be
//! modelled as a single [`Resource`] (which allows exactly one `{id}` placeholder) either; its
//! URL is built by hand from [`crate::links::resource_url`] + [`crate::links::child_url`] plus a
//! literal `"/action"` suffix, in the endpoint module.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/eeros` — list Eero devices (nodes) on a network.
///
/// Ported from `eero-api src/eero/api/eeros.py:168-195` (`EerosAPI.get_eeros`). Resolved via
/// [`Resource::resolve`]: prefers a parent envelope's `resources.eeros` link, falls back to the
/// `networks/{id}/eeros` template.
pub const GET_EEROS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/eeros",
    link: Some("eeros"),
};

/// `GET /2.2/eeros/{eero_id}` — a single Eero node's details.
///
/// Ported from `eero-api src/eero/api/eeros.py:197-225` (`EerosAPI.get_eero`). **Not** resolved
/// via [`Resource::resolve`] — see this module's own docs: `get_eero` prefers a parent's own
/// `url` field ([`crate::links::self_url`]), not a named `resources` link, so
/// `src/endpoints/eeros.rs` resolves it by hand from this constant's `template`/`version`.
pub const GET_EERO: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "eeros/{id}",
    link: None,
};

/// Alias of [`GET_EERO`]: `EerosAPI.get_led_status` reads `led_on`/`led_brightness` out of the
/// same Eero node object, resolved the same self-url-preferred way.
///
/// Ported from `eero-api src/eero/api/eeros.py:271-300` (`EerosAPI.get_led_status`).
pub const GET_LED_STATUS: Resource = GET_EERO;

/// `PUT /2.2/eeros/{eero_id}` — set an Eero node's Wi-Fi location label.
///
/// Ported from `eero-api src/eero/api/eeros.py:401-441` (`EerosAPI.set_location`). Same
/// self-url-preferred resolution as [`GET_EERO`] (not [`Resource::resolve`]) — see this module's
/// own docs.
pub const EEROS_SET_LOCATION: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "eeros/{id}",
    link: None,
};

/// `POST /2.2/eeros/{eero_id}/reboot` — reboot a single Eero node.
///
/// Ported from `eero-api src/eero/api/eeros.py:227-269` (`EerosAPI.reboot_eero`). Sends the
/// literal two-byte body `""` (`RequestBody::EmptyJsonString`), not a JSON object.
pub const REBOOT_EERO: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/reboot",
    link: Some("reboot"),
};

/// `PUT /2.2/eeros/{eero_id}/led` — turn an Eero node's status LED on/off, or set its brightness.
///
/// Ported from `eero-api src/eero/api/eeros.py:302-399` (`EerosAPI.set_led`,
/// `EerosAPI.set_led_brightness`). A dedicated `led` sub-resource, **not** the eero's own URL —
/// the pre-v8.0.0 shape (a JSON PUT to `eeros/{id}` itself) was verified to change nothing
/// server-side (`wiki/Migration.md:451`). Form-encoded (`RequestBody::Form`), not JSON.
pub const SET_LED: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/led",
    link: Some("led_action"),
};

/// Alias of [`SET_LED`]: `EerosAPI.set_led_brightness` PUTs the same `led` sub-resource with
/// `data={"led_brightness": str(brightness)}`.
///
/// Ported from `eero-api src/eero/api/eeros.py:352-399` (`EerosAPI.set_led_brightness`).
pub const SET_LED_BRIGHTNESS: Resource = SET_LED;

/// `GET /2.2/eeros/{eero_id}/connections` — a single Eero node's client connections.
///
/// Ported from `eero-api src/eero/api/eeros.py:606-639` (`EerosAPI.get_connections`).
pub const GET_CONNECTIONS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/connections",
    link: Some("connections"),
};

/// `POST /2.2/eeros/{eero_id}/action` — power-cycle an Eero node's ports (optionally rebooting
/// the node too).
///
/// Ported from `eero-api src/eero/api/eeros.py:641-693` (`EerosAPI.node_action`). `action` must
/// be one of `_NODE_ACTIONS`; see [`crate::endpoints::eeros::EerosApi::node_action`].
pub const EEROS_NODE_ACTION: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/action",
    link: Some("action"),
};

/// `POST /2.2/eeros/{eero_serial}/led_cycle` — cycle an Eero node's status LED through a colour
/// sequence.
///
/// Ported from `eero-api src/eero/api/eeros.py:744-792` (`EerosAPI.led_cycle`). No `network_id`
/// parameter and no `parent=` support in Python — resolved from a bare id/path/URL only, via
/// [`Resource::resolve`] with `link: None`. Form-encoded, with a repeated `colors[]` field per
/// colour.
pub const EEROS_LED_CYCLE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/led_cycle",
    link: None,
};

/// `POST /2.2/eeros/{eero_id}/nightlight/override` — preview a nightlight brightness value
/// without persisting it.
///
/// Ported from `eero-api src/eero/api/eeros.py:794-834` (`EerosAPI.nightlight_override`). No
/// `network_id` parameter and no `parent=` support in Python.
pub const NIGHTLIGHT_OVERRIDE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/nightlight/override",
    link: None,
};

/// `GET /2.2/eeros/{eero_serial}/support` — an Eero node's support/diagnostics summary.
///
/// Ported from `eero-api src/eero/api/eeros.py:836-863` (`EerosAPI.get_eero_support`). No
/// `network_id` parameter and no `parent=` support in Python; observed to 404 on some nodes.
pub const GET_EERO_SUPPORT: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/support",
    link: None,
};
