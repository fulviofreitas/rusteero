//! Network and account routes (`NetworksAPI`, plus `AuthApi`'s `GET /account`).

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/account` — fetch the account resource.
///
/// Ported from `const.py:17` (`ACCOUNT_ENDPOINT`) — a constant the Python source itself never
/// imports outside `const.py` (dead in `eero-api`), but the endpoint is real and used by
/// `rusteero`'s `Client::get_account` and the `get_networks` `/account` fallback.
/// Also relied on by `tests/transport.rs`, `tests/harness_smoke.rs` and `tests/live.rs`
/// (shared test files) as a generic "any authenticated GET" example — do not delete. A fixed
/// path (no `{id}` placeholder): every call site resolves it with an empty id and no parent.
pub const ACCOUNT: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "account",
    link: None,
};

/// `GET /2.2/networks` — list every network on the account.
///
/// Ported from `eero-api src/eero/api/networks.py:78` (`NetworksAPI.get_networks`). Unchanged at
/// v8.0.4: shape unchanged from the pre-v8 port. A fixed path
/// (no `{id}` placeholder): resolved with an empty id and no parent.
pub const GET_NETWORKS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks",
    link: None,
};

// ============================= v8.0.4 (`Resource`) constants =============================
//
// `link: Some(name)` means the caller's
// `parent` envelope's `resources.<name>` is preferred over the template
// (`crate::links::sub_resource_url`); `link: None` on a template containing `{id}` means only the
// template is ever used (no named link exists for that resource); `link: None` on a template with
// no `{id}` is a fixed path that ignores its `id_or_url` argument entirely. Constants whose
// natural name collides with a legacy constant above carry an explicit `_V8` suffix.

/// Documentation-only route marker for `networks/{id}` at v8.0.4: `NetworksApi::get_network`
/// and `NetworksApi::get_premium_status` no longer resolve through this crate's `Resource` model
/// (which only supports a *named* `resources.<link>` preference, via
/// [`crate::links::sub_resource_url`]) — Python's `_network_own_url` (`networks.py:46-59`)
/// instead prefers the parent envelope's own top-level `url` field
/// ([`crate::links::self_url`]), which is exactly [`crate::params::resolve_network_url`]'s
/// behaviour. `NetworksApi::network_own_url` calls that helper directly; this constant exists
/// purely so a server-side path rename to this resource is still a one-line, grep-able fix.
pub const NETWORKS_GET_NETWORK: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}",
    link: None,
};

/// Same wire resource as [`NETWORKS_GET_NETWORK`]: `NetworksAPI.get_premium_status` (`networks.py:122-149`)
/// reads Eero Plus/Secure fields out of the same full network object, resolved the same way.
pub const PREMIUM_STATUS: Resource = NETWORKS_GET_NETWORK;

/// `POST /2.2/networks/{id}/reboot` — reboot every Eero node on the network, preferring the
/// network's own published `reboot` link.
///
/// Ported from `NetworksAPI.reboot_network` (`networks.py:151-184`).
pub const REBOOT_NETWORK_V8: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/reboot",
    link: Some("reboot"),
};

/// `POST /2.2/networks/{id}/speedtest` — run a speed test, preferring the network's own
/// published `speedtest` link.
///
/// Ported from `NetworksAPI.run_speed_test` (`networks.py:186-219`).
pub const RUN_SPEED_TEST_V8: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/speedtest",
    link: Some("speedtest"),
};

/// `GET /2.2/networks/{id}/speedtest` — past speed-test results, same sub-resource as
/// [`RUN_SPEED_TEST_V8`] but a different verb.
///
/// Ported from `NetworksAPI.get_speed_tests` (`networks.py:223-267`).
pub const NETWORKS_GET_SPEED_TESTS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/speedtest",
    link: Some("speedtest"),
};

/// `PUT /2.2/networks/{id}/settings` — rename the network (form-encoded `name=`), preferring the
/// network's own published `settings` link.
///
/// Python's `set_network_name` moved to a form-encoded body at v8.0.4 (`networks.py:269-312`) —
/// the same literal path DNS/security/SQM's own JSON setters PUT onto (each via its own
/// domain-local `Resource` constant — see e.g. `routes::dns::DNS_PUT_SETTINGS`), but a distinct,
/// form-encoded constant here.
pub const NETWORK_SETTINGS_FORM: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};

/// `PUT /2.2/networks/{id}/password` — set the network's Wi-Fi password (form-encoded
/// `password=`), preferring the network's own published `password` link.
///
/// Ported from `NetworksAPI.set_network_password` (`networks.py:313-351`).
pub const SET_NETWORK_PASSWORD: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/password",
    link: Some("password"),
};

/// `DELETE /2.2/networks/{id}/password` — clear the network's Wi-Fi password, same sub-resource
/// as [`SET_NETWORK_PASSWORD`].
///
/// Ported from `NetworksAPI.clear_network_password` (`networks.py:353-384`).
pub const CLEAR_NETWORK_PASSWORD: Resource = Resource {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    template: "networks/{id}/password",
    link: Some("password"),
};

/// `GET /2.2/networks/{id}/guestnetwork` — guest network configuration, preferring the network's
/// own published `guestnetwork` link.
///
/// Ported from `NetworksAPI.get_guest_network` (`networks.py:386-418`).
pub const GET_GUEST_NETWORK: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/guestnetwork",
    link: Some("guestnetwork"),
};

/// `PUT /2.2/networks/{id}/guestnetwork` — enable/disable/rename the guest network
/// (form-encoded), same sub-resource as [`GET_GUEST_NETWORK`].
///
/// Ported from `NetworksAPI.set_guest_network` (`networks.py:420-471`). No `password` field at
/// v8.0.4 (breaking change vs. `v6.2.0`) — see [`SET_GUEST_PASSWORD`].
pub const SET_GUEST_NETWORK_V8: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/guestnetwork",
    link: Some("guestnetwork"),
};

/// Documentation-only route marker for `networks/{id}/guestnetwork/password`:
/// `NetworksApi::set_guest_password`/`clear_guest_password` resolve through
/// `_guest_password_url` (`networks.py:27-42`), which prefers `resolve_link(guest_parent,
/// "password")` — a `password` link read from the **guest network's own** envelope, not the
/// network's — falling back to this literal template otherwise. This is the same
/// link-name-inside-an-arbitrary-parent shape [`crate::links::sub_resource_url`] already
/// implements; the only reason this domain doesn't use `Resource::resolve` directly for it is
/// that the "parent" here is semantically the guest envelope, which is exactly the value the
/// caller passes as `parent` anyway (see `NetworksApi::set_guest_password`'s own doc comment) —
/// so [`SET_GUEST_PASSWORD`]/[`CLEAR_GUEST_PASSWORD`] below *are* ordinary `Resource` constants.
pub const GUEST_PASSWORD_TEMPLATE: &str = "networks/{id}/guestnetwork/password";

/// `PUT /2.2/networks/{id}/guestnetwork/password` — set the guest network's password
/// (form-encoded), preferring the *guest network's own* `password` link (not the network's).
///
/// Ported from `NetworksAPI.set_guest_password` (`networks.py:473-511`).
pub const SET_GUEST_PASSWORD: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: GUEST_PASSWORD_TEMPLATE,
    link: Some("password"),
};

/// `DELETE /2.2/networks/{id}/guestnetwork/password` — clear the guest network's password, same
/// sub-resource as [`SET_GUEST_PASSWORD`].
///
/// Ported from `NetworksAPI.clear_guest_password` (`networks.py:513-548`).
pub const CLEAR_GUEST_PASSWORD: Resource = Resource {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    template: GUEST_PASSWORD_TEMPLATE,
    link: Some("password"),
};
