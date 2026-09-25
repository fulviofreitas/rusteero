//! Eero-node routes (`EerosAPI`).

// ----------------------------------- eeros (`EerosAPI`) ---------------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/eeros` — list Eero devices (nodes) on a network.
///
/// Ported from `eero-api src/eero/api/eeros.py:33` (`EerosAPI.get_eeros`).
pub const GET_EEROS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/eeros",
};

/// `GET /2.2/eeros/{eero_id}` — a single Eero node's details.
///
/// Not nested under `networks/` — `EerosAPI` addresses nodes by their own top-level
/// resource; `network_id` is accepted by the Python method but unused. Ported from
/// `eero-api src/eero/api/eeros.py:53` (`EerosAPI.get_eero`).
pub const GET_EERO: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "eeros/{eero_id}",
};

/// Alias of `GET_EERO`: `EerosAPI.get_led_status` reads `led_on`/`led_brightness` out of the
/// same Eero node object.
///
/// Ported from `eero-api src/eero/api/eeros.py:95` (`EerosAPI.get_led_status`).
pub const GET_LED_STATUS: Route = GET_EERO;

/// Alias of `GET_EERO`: `EerosAPI.get_nightlight` reads the `nightlight` field out of the
/// same Eero node object (Beacon devices only).
///
/// Ported from `eero-api src/eero/api/eeros.py:185` (`EerosAPI.get_nightlight`).
pub const GET_NIGHTLIGHT: Route = GET_EERO;

/// `POST /2.2/eeros/{eero_id}/reboot` — reboot a single Eero node.
///
/// Ported from `eero-api src/eero/api/eeros.py:74` (`EerosAPI.reboot_eero`).
pub const REBOOT_EERO: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "eeros/{eero_id}/reboot",
};

/// `PUT /2.2/eeros/{eero_id}` — mutate a single Eero node (LED, brightness, nightlight).
///
/// Unlike device nickname/pause writes, this stays on API version 2.2 — `EerosAPI` never
/// switches to `DEVICE_UPDATE_ENDPOINT`; only `DevicesAPI` does (`eero-api` issue #102 is
/// device-specific). Ported from `eero-api src/eero/api/eeros.py:118` (`EerosAPI.set_led`).
pub const SET_LED: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "eeros/{eero_id}",
};

/// Alias of `SET_LED`: `EerosAPI.set_led_brightness` PUTs the same node URL with
/// `{"led_brightness": 0..=100}` (clamped). Also the target of
/// `EerosAPI.set_nightlight_brightness` (`eeros.py:282`), which delegates through
/// `set_nightlight` — no separate route needed there.
///
/// Ported from `eero-api src/eero/api/eeros.py:150` (`EerosAPI.set_led_brightness`).
pub const SET_LED_BRIGHTNESS: Route = SET_LED;

/// Alias of `SET_LED`: `EerosAPI.set_nightlight` PUTs the same node URL with a nested
/// `{"nightlight": {...}}` body. Also the target of `EerosAPI.set_nightlight_schedule`
/// (`eeros.py:302`), which delegates through `set_nightlight` — no separate route needed
/// there.
///
/// Ported from `eero-api src/eero/api/eeros.py:209` (`EerosAPI.set_nightlight`).
pub const SET_NIGHTLIGHT: Route = SET_LED;
