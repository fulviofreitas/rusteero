//! DNS routes (`DnsAPI`) — all aliases of `networks::GET_NETWORK`/`networks::PUT_NETWORK_SETTINGS`.

// ------------------------------------ dns (`DnsAPI`) ------------------------------------

use super::Route;
use super::networks::{GET_NETWORK, PUT_NETWORK_SETTINGS};

/// Alias of `GET_NETWORK`: `DnsAPI.get_dns_settings` reads DNS fields (`dns_caching`,
/// `custom_dns`, `ipv6_upstream`, ...) out of the full network object — same wire call.
///
/// Ported from `eero-api src/eero/api/dns.py:36` (`DnsAPI.get_dns_settings`).
pub const GET_DNS_SETTINGS: Route = GET_NETWORK;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_dns_caching` PUTs `{"dns_caching": bool}`.
///
/// Ported from `eero-api src/eero/api/dns.py:59` (`DnsAPI.set_dns_caching`).
pub const SET_DNS_CACHING: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_custom_dns` PUTs `{"custom_dns": [..]}`
/// (truncated to at most 2 servers). Also the target of `DnsAPI.clear_custom_dns`
/// (`dns.py:126`), which delegates to `set_custom_dns([])` — no separate route needed there.
///
/// Ported from `eero-api src/eero/api/dns.py:89` (`DnsAPI.set_custom_dns`).
pub const SET_CUSTOM_DNS: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_dns_mode` PUTs a `custom_dns` list resolved
/// from a named preset (`"cloudflare"`, `"google"`, `"opendns"`, `"custom"`, `"auto"`).
///
/// Ported from `eero-api src/eero/api/dns.py:137` (`DnsAPI.set_dns_mode`).
pub const SET_DNS_MODE: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_ipv6_dns` PUTs `{"ipv6_upstream": bool}`.
///
/// Ported from `eero-api src/eero/api/dns.py:186` (`DnsAPI.set_ipv6_dns`).
pub const SET_IPV6_DNS: Route = PUT_NETWORK_SETTINGS;
