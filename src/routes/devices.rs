//! Device routes (`DevicesAPI`).

// --------------------------------- devices (`DevicesAPI`) ------------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/devices` — list devices connected to a network.
///
/// Ported from `eero-api src/eero/api/devices.py:67` (`DevicesAPI.get_devices`).
pub const GET_DEVICES: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/devices",
};

/// `GET /2.2/networks/{network_id}/devices/{device_id}` — a single device's details.
///
/// Ported from `eero-api src/eero/api/devices.py:87` (`DevicesAPI.get_device`). Also used by
/// `DevicesAPI.block_device` (`devices.py:136-186`) to resolve the device's MAC before
/// blacklisting it.
pub const GET_DEVICE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/devices/{device_id}",
};

/// `PUT /2.3/networks/{network_id}/devices/{device_id}` — mutate a device's nickname.
///
/// **Must** use API version 2.3: version 2.2 accepts this `PUT`, returns `200 OK`, and
/// silently drops the write (`eero-api` issue #102; see `ApiVersion::V2_3`). Ported from
/// `eero-api src/eero/api/devices.py:44-65,111` (`DevicesAPI._update_device` via
/// `DevicesAPI.set_device_nickname`), which builds this exact URL against
/// `DEVICE_UPDATE_ENDPOINT` rather than the module's own (2.2) base.
pub const SET_DEVICE_NICKNAME: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_3,
    path: "networks/{network_id}/devices/{device_id}",
};

/// Alias of `SET_DEVICE_NICKNAME`: `DevicesAPI.pause_device` PUTs the same 2.3 device URL
/// with a `{"paused": bool}` body instead of `{"nickname": str}`.
///
/// Ported from `eero-api src/eero/api/devices.py:44-65,188` (`DevicesAPI.pause_device` via
/// `DevicesAPI._update_device`). See `SET_DEVICE_NICKNAME` for why 2.3 is required.
pub const PAUSE_DEVICE: Route = SET_DEVICE_NICKNAME;
