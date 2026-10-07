//! Device routes (`DevicesAPI`).

// --------------------------------- devices (`DevicesAPI`) ------------------------------

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

// ============================================================================================
// v8.0.4 constants. Every `DevicesApi` method in
// `src/endpoints/devices.rs` is built on one of these.
// ============================================================================================

/// `GET /2.2/networks/{id}/devices` — list devices connected to a network, preferring the
/// network's own published `devices` link when a `parent` envelope is supplied.
///
/// Ported from `DevicesAPI.get_devices` (`eero-api src/eero/api/devices.py:114-165` at
/// `v8.0.4`): `sub_resource_url(network, "networks/{id}/devices", link="devices",
/// parent=as_envelope(parent), version=API_VERSION_DEFAULT)`. API-Reference.md:631, read.
pub const V8_GET_DEVICES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/devices",
    link: Some("devices"),
};

/// `GET /2.2/networks/{network}/devices/{mac}` — a single device's details, used only as the
/// fallback when no `parent` (or a parent with no `url` field) is supplied.
///
/// Ported from `DevicesAPI._device_url` (`devices.py:69-80`), the template
/// `get_device`/`update_device_via_link` fall back to when they cannot prefer the parent's own
/// `self_url` — see `DevicesApi::get_device`/`DevicesApi::update_device_via_link` for the
/// self-url-preferred resolution those two methods perform before ever consulting this constant.
/// API-Reference.md:632, read.
pub const V8_GET_DEVICE: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "devices",
    suffix: "",
    link: None,
};

/// `PUT /2.3/networks/{network}/devices/{mac}` — set a device's nickname.
///
/// Ported from `DevicesAPI._update_device` via `DevicesAPI.set_device_nickname`
/// (`devices.py:44-65,201-225`). **Must** stay pinned to API version 2.3 — version 2.2 accepts
/// this exact `PUT`, returns `200 OK`, and silently drops the write (`eero-api` issue #102).
/// `mac` **must** be normalised through [`crate::util::id_from_url`] before being passed as this
/// route's `child` argument — see `DevicesApi::set_device_nickname`'s own docs for why a
/// path/URL-shaped `mac` must never reach this route unnormalised (it could otherwise carry its
/// own, different version segment and smuggle the write past the 2.3 pin). API-Reference.md:633,
/// **verified** write.
pub const V8_SET_DEVICE_NICKNAME: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_3,
    prefix: "devices",
    suffix: "",
    link: None,
};

/// Alias of [`V8_SET_DEVICE_NICKNAME`]: `DevicesAPI.pause_device` PUTs the same 2.3 device URL
/// with a `{"paused": bool}` body instead of `{"nickname": str}`.
///
/// Ported from `DevicesAPI._update_device` via `DevicesAPI.pause_device`
/// (`devices.py:44-65,227-257`). API-Reference.md:634, **verified** write.
pub const V8_PAUSE_DEVICE: Nested = V8_SET_DEVICE_NICKNAME;

/// `PUT /2.2/networks/{network}/devices/{mac}` — set a device's type.
///
/// Ported from `DevicesAPI.set_device_type` (`devices.py:330-356`). Deliberately **not** an
/// alias of [`V8_GET_DEVICE`] despite the identical prefix/suffix/version, because its `method`
/// differs (`PUT`, not `GET`). Unlike `set_device_nickname`/`pause_device`, this stays on the
/// **default** (2.2) API version and does **not** normalise `mac` through `id_from_url` first —
/// Python passes `mac` straight to `_device_url` here (`devices.py:356`), and there is no
/// version pin here for a path/URL-shaped `mac` to smuggle past. Live-verified 2026-09-20:
/// persists and reads back (API-Reference.md:636).
pub const SET_DEVICE_TYPE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "devices",
    suffix: "",
    link: None,
};

/// `GET /2.2/networks/{network}/devices/{mac}/labels` — a device's labels.
///
/// Ported from `DevicesAPI.get_device_labels` (`devices.py:358-378`):
/// `f"{self._device_url(network, mac)}/labels"`. API-Reference.md:637, read.
pub const GET_DEVICE_LABELS: Nested = Nested {
    method: Method::GET,
    version: ApiVersion::V2_2,
    prefix: "devices",
    suffix: "/labels",
    link: None,
};

/// `PUT /2.2/networks/{network}/devices/{mac}/labels` — set a device's labels (confirmed
/// server-side no-op as of 2026-09-20: `200 OK`, labels echoed back unchanged, never persists).
///
/// Ported from `DevicesAPI.set_device_labels` (`devices.py:380-435`): same URL as
/// [`GET_DEVICE_LABELS`], different verb — every label value is a **query parameter**, never a
/// request body (`devices.py:424-433`). API-Reference.md:638, **verified no-op**.
pub const SET_DEVICE_LABELS: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "devices",
    suffix: "/labels",
    link: None,
};

/// `PUT /2.2/networks/{network}/devices/{mac}` — update a device's nickname/paused/profile via
/// its own published URL.
///
/// Ported from `DevicesAPI.update_device_via_link` (`devices.py:259-328`). Identical
/// prefix/suffix/version to [`V8_GET_DEVICE`] — targets the **default** (2.2) API version, not
/// 2.3, unlike `set_device_nickname`/`pause_device` — but kept as its own constant since its
/// `method` (`PUT`) differs. See `DevicesApi::update_device_via_link` for the self-url-preferred
/// resolution it shares with `DevicesApi::get_device`. API-Reference.md:635, unverified write.
pub const UPDATE_DEVICE_VIA_LINK: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "devices",
    suffix: "",
    link: None,
};

// `DevicesAPI.block_device`/`unblock_device` (`devices.py:437-472`) delegate entirely to
// `BlacklistAPI.add_to_blacklist`/`remove_from_blacklist` — no route constant of their own here;
// see `src/routes/blacklist.rs`'s `V8_GET_BLACKLIST`/`V8_ADD_TO_BLACKLIST`.
