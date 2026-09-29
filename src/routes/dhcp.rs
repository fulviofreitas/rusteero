//! `dhcp` routes (`DhcpAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/dhcp.py` (v8.0.4).

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `PUT /2.2/networks/{id}/settings` (or the parent's own `settings` link) — set the network's
/// DHCP configuration.
///
/// Ported from `eero-api src/eero/api/dhcp.py:87-163` (`DhcpAPI.set_dhcp`):
/// `sub_resource_url(network_id, "networks/{id}/settings", link="settings", parent=..)` — the
/// same `settings` sub-resource `NetworksApi`/`DnsApi`/`SecurityApi`/`SqmApi` write to.
pub const DHCP_SET_DHCP: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};

/// `PUT /2.2/networks/{id}/settings` (or the parent's own `settings` link) — set the network's
/// WAN connection mode.
///
/// Ported from `eero-api src/eero/api/dhcp.py:166-221` (`DhcpAPI.set_connection_mode`): same URL
/// resolution as [`DHCP_SET_DHCP`].
pub const DHCP_SET_CONNECTION_MODE: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};

/// `PUT /2.2/networks/{id}/settings` (or the parent's own `settings` link) — enable or disable
/// NAT port randomization.
///
/// Ported from `eero-api src/eero/api/dhcp.py:223-269` (`DhcpAPI.set_nat_port_randomization`):
/// same URL resolution as [`DHCP_SET_DHCP`].
pub const DHCP_SET_NAT_PORT_RANDOMIZATION: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/settings",
    link: Some("settings"),
};

/// `POST /2.2/eeros/{id}/pppoe` — encrypt `PPPoE` credentials for an eero.
///
/// Ported from `eero-api src/eero/api/dhcp.py:271-309` (`DhcpAPI.set_pppoe`):
/// `resource_url(eero_serial_or_id, "eeros/{id}/pppoe")` — no `parent=` parameter in Python, no
/// `link` here either. Addressed by eero serial/id, not by network.
pub const DHCP_SET_PPPOE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "eeros/{id}/pppoe",
    link: None,
};
