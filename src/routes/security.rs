//! Security routes (`SecurityAPI`) — all aliases of `networks::GET_NETWORK`/`networks::PUT_NETWORK_SETTINGS`.

// ---------------------------------- security (`SecurityAPI`) ----------------------------

use super::Route;
use super::networks::{GET_NETWORK, PUT_NETWORK_SETTINGS};

/// Alias of `GET_NETWORK`: `SecurityAPI.get_security_settings` reads security fields
/// (`wpa3`, `band_steering`, `upnp`, `ipv6_upstream`, `ipv6_downstream`, `thread`, ...) out
/// of the same full network object.
///
/// Ported from `eero-api src/eero/api/security.py:36`
/// (`SecurityAPI.get_security_settings`).
pub const GET_SECURITY_SETTINGS: Route = GET_NETWORK;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_wpa3` PUTs `{"wpa3": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:59` (`SecurityAPI.set_wpa3`).
pub const SET_WPA3: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_band_steering` PUTs
/// `{"band_steering": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:92` (`SecurityAPI.set_band_steering`).
pub const SET_BAND_STEERING: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_upnp` PUTs `{"upnp": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:125` (`SecurityAPI.set_upnp`).
pub const SET_UPNP: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_ipv6` PUTs
/// `{"ipv6_upstream": bool, "ipv6_downstream": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:158` (`SecurityAPI.set_ipv6`).
pub const SET_IPV6: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_thread` PUTs `{"thread": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:191` (`SecurityAPI.set_thread`).
pub const SET_THREAD: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.configure_security` PUTs a partial union of
/// `wpa3`/`band_steering`/`upnp`/`ipv6_upstream`+`ipv6_downstream`/`thread`.
///
/// Ported from `eero-api src/eero/api/security.py:224` (`SecurityAPI.configure_security`).
pub const CONFIGURE_SECURITY: Route = PUT_NETWORK_SETTINGS;
