//! Network and account routes (`NetworksAPI`, plus `AuthApi`'s `GET /account`).

use super::{ApiVersion, Route};
use reqwest::Method;

// ---------------------------------- account (`AuthApi`/`NetworksAPI`) -------------------

/// `GET /2.2/account` — fetch the account resource.
///
/// Ported from `const.py:17` (`ACCOUNT_ENDPOINT`) — a constant the Python source itself never
/// imports outside `const.py` (dead in `eero-api`), but the endpoint is real and used by
/// `rusteero`'s `Client::get_account` and the `get_networks` `/account` fallback (port plan
/// §1.6).
pub const ACCOUNT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "account",
};

// --------------------------------- networks (`NetworksAPI`) -----------------------------

/// `GET /2.2/networks` — list every network on the account.
///
/// Ported from `eero-api src/eero/api/networks.py:33` (`NetworksAPI.get_networks`).
pub const GET_NETWORKS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks",
};

/// `GET /2.2/networks/{network_id}` — a single network's full object.
///
/// Shared by four Python methods that all read fields out of the same full network object
/// rather than a dedicated sub-resource: `NetworksAPI.get_network`,
/// `DnsAPI.get_dns_settings` (`GET_DNS_SETTINGS`), `SecurityAPI.get_security_settings`
/// (`GET_SECURITY_SETTINGS`), `SqmAPI.get_sqm_settings` (`GET_SQM_SETTINGS`), and
/// `NetworksAPI.get_premium_status` (`GET_PREMIUM_STATUS`). Ported from
/// `eero-api src/eero/api/networks.py:51` (`NetworksAPI.get_network`).
pub const GET_NETWORK: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}",
};

/// Alias of `GET_NETWORK`: `NetworksAPI.get_premium_status` reads Eero Plus/Secure fields
/// out of the same full network object.
///
/// Ported from `eero-api src/eero/api/networks.py:159` (`NetworksAPI.get_premium_status`).
pub const GET_PREMIUM_STATUS: Route = GET_NETWORK;

/// `PUT /2.2/networks/{network_id}/guestnetwork` — enable/disable/configure the guest
/// network.
///
/// Ported from `eero-api src/eero/api/networks.py:71` (`NetworksAPI.set_guest_network`).
pub const SET_GUEST_NETWORK: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/guestnetwork",
};

/// `POST /2.2/networks/{network_id}/speedtest` — run a speed test on the network.
///
/// Ported from `eero-api src/eero/api/networks.py:111` (`NetworksAPI.run_speed_test`).
pub const RUN_SPEED_TEST: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/speedtest",
};

/// `POST /2.2/networks/{network_id}/reboot` — reboot every Eero node on the network.
///
/// Ported from `eero-api src/eero/api/networks.py:134` (`NetworksAPI.reboot_network`).
pub const REBOOT_NETWORK: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reboot",
};

/// `PUT /2.2/networks/{network_id}/settings` — the network-wide settings resource.
///
/// The single wire endpoint behind well over a dozen Python setters spread across four
/// modules (`NetworksAPI.set_network_name` via `SET_NETWORK_NAME`; `DnsAPI`'s
/// `SET_DNS_CACHING`/`SET_CUSTOM_DNS`/`SET_DNS_MODE`/`SET_IPV6_DNS`; `SecurityAPI`'s
/// `SET_WPA3`/`SET_BAND_STEERING`/`SET_UPNP`/`SET_IPV6`/`SET_THREAD`/`CONFIGURE_SECURITY`;
/// `SqmAPI`'s `SET_SQM_ENABLED`/`SET_SQM_BANDWIDTH`/`CONFIGURE_SQM`/`SET_SQM_AUTO`) — each
/// PUTs a different JSON key onto the same settings object. Distinct from `GET_SETTINGS`,
/// which reads this same resource via `SettingsAPI.get_settings`. Ported from
/// `eero-api src/eero/api/networks.py:182` (`NetworksAPI.set_network_name`), the first
/// setter of this resource in the port plan's endpoint catalogue.
pub const PUT_NETWORK_SETTINGS: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/settings",
};

/// Alias of `PUT_NETWORK_SETTINGS`: `NetworksAPI.set_network_name` PUTs `{"name": str}`.
///
/// Ported from `eero-api src/eero/api/networks.py:182` (`NetworksAPI.set_network_name`).
pub const SET_NETWORK_NAME: Route = PUT_NETWORK_SETTINGS;
