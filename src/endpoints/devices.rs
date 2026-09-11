//! Devices API — read-only device queries (list + single device details).
//!
//! Ported from `eero-api src/eero/api/devices.py:29-109` (`DevicesAPI.__init__`,
//! `DevicesAPI.get_devices`, `DevicesAPI.get_device`). This module currently implements only the
//! `GET` half of `DevicesAPI` (phase 3); the three mutation methods (`set_device_nickname`,
//! `pause_device`, `block_device`) are phase 5 work and belong in this same file — see the
//! comment block at the bottom of `DevicesApi`'s `impl` for the routes and caveats a future
//! implementer needs.
//!
//! # Not ported: `set_device_priority` / `get_device_priority`
//!
//! `eero-api`'s `DevicesAPI.set_device_priority` (`devices.py:219-271`) is a confirmed no-op:
//! the Eero cloud API no longer exposes device-level priority — there is no `/priority` or
//! `/qos` endpoint, and the `prioritized` field observed on the device object
//! (`GET_DEVICE`/`GET_DEVICES`) does not respond to it. Sending `{"prioritized": bool}` through
//! the device PUT returns `200 OK` but changes no observable state (live-verified against a real
//! account). It is scheduled for removal from `eero-api` in `v6.0.0`
//! (<https://github.com/fulviofreitas/eero-api/issues/111>). `eero-api` has never had a
//! `get_device_priority` method for the same reason. **Neither is ported here, and neither
//! should be added later for "parity"** — the endpoint they would call does not do anything.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// Read-only half of `eero-api`'s `DevicesAPI` (`devices.py:29-42`): device listing and
/// single-device lookup.
///
/// Every method returns the raw, unmodified `{"meta": …, "data": …}` envelope from the Eero
/// cloud API — this type never extracts, reshapes, or otherwise transforms a response.
///
/// Build one with `DevicesApi::new`, wrapping the same `Transport` the rest of the endpoint
/// aggregator shares, so every domain module goes through one session and one credential store.
#[derive(Debug)]
pub struct DevicesApi {
    transport: Arc<Transport>,
}

impl DevicesApi {
    /// Wraps `transport` as a `DevicesApi`.
    ///
    /// Ported from `DevicesAPI.__init__` (`devices.py:36-42`), which wraps an `AuthAPI` rather
    /// than a bare transport handle. `rusteero` has no `AuthAPI`-shaped dependency here: a
    /// `Transport` already owns both the current session and the "not authenticated"
    /// precondition every Python method above re-derives by hand
    /// (`auth_token = await self._auth_api.get_auth_token()`), so wrapping it directly is a
    /// strict simplification, not a behaviour change.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/devices` — list devices connected to `network_id`.
    ///
    /// Ported from `DevicesAPI.get_devices` (`devices.py:67-85`). Returns the raw
    /// `{"meta": …, "data": [...]}` envelope; `data` is a JSON array with one object per device.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn get_devices(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_DEVICES, &[("network_id", network_id)], None)
            .await
    }

    /// `GET /2.2/networks/{network_id}/devices/{device_id}` — a single device's details.
    ///
    /// Ported from `DevicesAPI.get_device` (`devices.py:87-109`). Returns the raw
    /// `{"meta": …, "data": {...}}` envelope for the one device. Also the call `block_device`
    /// (phase 5) uses to resolve a device's MAC before blacklisting it (`devices.py:171`).
    ///
    /// `device_id` is passed through unchanged — no escaping or validation happens here.
    /// `routes::GET_DEVICE`'s path template is rendered through `Transport`'s
    /// `Url::path_segments_mut`-based renderer, which percent-encodes every segment for the
    /// path-segment position, so a `device_id` containing `/`, `%`, or a literal `..` cannot
    /// escape its own segment or traverse into a different resource (see the `routes` module
    /// docs for the exact guarantee and its one documented edge case).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// `Error::Api { status: 404, .. }` if `device_id` does not exist on `network_id`, or
    /// whatever other status-mapped error the request produces — see `Transport::send`.
    pub async fn get_device(&self, network_id: &str, device_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_DEVICE,
                &[("network_id", network_id), ("device_id", device_id)],
                None,
            )
            .await
    }

    // -----------------------------------------------------------------------------------------
    // PHASE 5 (not implemented here — device mutations, added by a later agent to this same
    // file):
    //
    // - `set_device_nickname` and `pause_device` both PUT the device resource, but MUST use
    //   `routes::SET_DEVICE_NICKNAME` / `routes::PAUSE_DEVICE` (`ApiVersion::V2_3`), never a
    //   `/2.2` route: the `/2.2` endpoint accepts the identical PUT, returns `200 OK`, and
    //   silently drops the write (`eero-api` issue #102
    //   <https://github.com/fulviofreitas/eero-api/issues/102>; see
    //   `routes::SET_DEVICE_NICKNAME`'s own doc comment). Ported from `DevicesAPI._update_device`
    //   / `DevicesAPI.set_device_nickname` / `DevicesAPI.pause_device` (`devices.py:44-65,111-134,
    //   188-217`).
    // - `block_device` is NOT a PUT on the device resource — `PUT {"blocked": bool}` is a
    //   documented no-op. It is two separate round-trips against `/blacklist`
    //   (`routes::ADD_TO_BLACKLIST` for `blocked = true`, after resolving the device's MAC via
    //   `get_device` above; `routes::REMOVE_FROM_BLACKLIST` for `blocked = false`). See
    //   `eero-api` issue #109 <https://github.com/fulviofreitas/eero-api/issues/109> and
    //   `DevicesAPI.block_device` (`devices.py:136-186`).
    // -----------------------------------------------------------------------------------------
}
