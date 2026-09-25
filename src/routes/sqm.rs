//! Smart Queue Management routes (`SqmAPI`) — all aliases of `networks::GET_NETWORK`/`networks::PUT_NETWORK_SETTINGS`.

// -------------------------------------- sqm (`SqmAPI`) -----------------------------------

use super::Route;
use super::networks::{GET_NETWORK, PUT_NETWORK_SETTINGS};

/// Alias of `GET_NETWORK`: `SqmAPI.get_sqm_settings` reads SQM/QoS fields out of the same
/// full network object.
///
/// Ported from `eero-api src/eero/api/sqm.py:36` (`SqmAPI.get_sqm_settings`).
pub const GET_SQM_SETTINGS: Route = GET_NETWORK;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.set_sqm_enabled` PUTs `{"sqm": bool}` (flat,
/// unlike the other SQM setters below).
///
/// Ported from `eero-api src/eero/api/sqm.py:59` (`SqmAPI.set_sqm_enabled`).
pub const SET_SQM_ENABLED: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.set_sqm_bandwidth` PUTs a nested
/// `{"sqm": {"enabled": true, "upload_bandwidth"?, "download_bandwidth"?}}` — payload shape
/// unverified against a live account (`sqm.py:128` `TODO`).
///
/// Ported from `eero-api src/eero/api/sqm.py:89` (`SqmAPI.set_sqm_bandwidth`).
pub const SET_SQM_BANDWIDTH: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.configure_sqm` PUTs a nested
/// `{"sqm": {"enabled": bool, ...}}` — payload shape unverified against a live account
/// (`sqm.py:173` `TODO`).
///
/// Ported from `eero-api src/eero/api/sqm.py:137` (`SqmAPI.configure_sqm`).
pub const CONFIGURE_SQM: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.set_sqm_auto` PUTs
/// `{"sqm": {"enabled": true, "mode": "auto"}}` — payload shape unverified against a live
/// account (`sqm.py:197` `TODO`).
///
/// Ported from `eero-api src/eero/api/sqm.py:182` (`SqmAPI.set_sqm_auto`).
pub const SET_SQM_AUTO: Route = PUT_NETWORK_SETTINGS;
