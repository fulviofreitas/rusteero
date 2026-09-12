//! Devices API — device queries plus the nickname/pause/block mutations.
//!
//! Ported from `eero-api src/eero/api/devices.py:29-217` (`DevicesAPI.__init__`,
//! `DevicesAPI.get_devices`, `DevicesAPI.get_device`, `DevicesAPI.set_device_nickname`,
//! `DevicesAPI.pause_device`, `DevicesAPI.block_device`). `set_device_priority`
//! (`devices.py:219-271`) is deliberately not ported — see the module note below.
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

use serde_json::{Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `DevicesAPI` (`devices.py:29-42`): device listing, single-device lookup, and
/// the nickname/pause/block mutations.
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
    /// uses to resolve a device's MAC before blacklisting it (`devices.py:171`).
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

    /// `PUT /2.3/networks/{network_id}/devices/{device_id}` — set a device's nickname.
    ///
    /// **Must**, and does, use `routes::SET_DEVICE_NICKNAME` (`ApiVersion::V2_3`), never a
    /// `/2.2` route: the `/2.2` endpoint accepts the identical `PUT`, returns `200 OK`, and
    /// silently drops the write (`eero-api` issue #102
    /// <https://github.com/fulviofreitas/eero-api/issues/102>). Ported from
    /// `DevicesAPI._update_device` via `DevicesAPI.set_device_nickname`
    /// (`devices.py:44-65,111-134`), sending body `{"nickname": nickname}` (`devices.py:134`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_device_nickname(
        &self,
        network_id: &str,
        device_id: &str,
        nickname: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::SET_DEVICE_NICKNAME,
                &[("network_id", network_id), ("device_id", device_id)],
                Some(json!({ "nickname": nickname })),
            )
            .await
    }

    /// `PUT /2.3/networks/{network_id}/devices/{device_id}` — pause or unpause a device's
    /// internet access.
    ///
    /// **Must**, and does, use `routes::PAUSE_DEVICE` (alias of `routes::SET_DEVICE_NICKNAME`,
    /// `ApiVersion::V2_3`), never a `/2.2` route, for the same reason as `set_device_nickname`
    /// above (`eero-api` issue #102). Ported from `DevicesAPI._update_device` via
    /// `DevicesAPI.pause_device` (`devices.py:44-65,188-217`), sending body
    /// `{"paused": paused}` (`devices.py:217`).
    ///
    /// A paused device stays connected to the network but loses internet access; this is
    /// distinct from `block_device`, which removes the device from the network entirely.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn pause_device(
        &self,
        network_id: &str,
        device_id: &str,
        paused: bool,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::PAUSE_DEVICE,
                &[("network_id", network_id), ("device_id", device_id)],
                Some(json!({ "paused": paused })),
            )
            .await
    }

    /// Blocks or unblocks a device via the `/blacklist` resource — NOT a `PUT` on the device
    /// itself.
    ///
    /// The Eero cloud API does not honour a `blocked` field on the device object: sending
    /// `PUT /devices/{id}` with `{"blocked": true}` is a silent no-op (`200 OK`, state
    /// unchanged). Blocking is instead managed through `/blacklist`: `POST {"mac": mac}` adds
    /// the device, `DELETE /blacklist/{id}` removes it (`eero-api` issue #109
    /// <https://github.com/fulviofreitas/eero-api/issues/109>). Ported from
    /// `DevicesAPI.block_device` (`devices.py:136-186`), which builds both `/blacklist` URLs
    /// itself rather than delegating to `BlacklistApi`; this method mirrors that by calling
    /// `routes::ADD_TO_BLACKLIST` / `routes::REMOVE_FROM_BLACKLIST` directly through the shared
    /// `Transport`, rather than holding a `BlacklistApi` of its own.
    ///
    /// To block (`blocked == true`): calls `get_device` first to resolve the device's
    /// canonical, colon-separated MAC from `data.mac` (`devices.py:171-172`), then `POST`s that
    /// MAC to `routes::ADD_TO_BLACKLIST`. To unblock (`blocked == false`): `DELETE`s
    /// `routes::REMOVE_FROM_BLACKLIST` directly with the caller-supplied `device_id` as the URL
    /// segment, without a prior `GET` — exactly what `devices.py:184-186` does, since Eero
    /// accepts a blacklist entry's `device_id` (the MAC with colons stripped) as well as the
    /// bare MAC there (see `routes::REMOVE_FROM_BLACKLIST`'s own doc comment).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent. If `blocked == true` and the resolved device response has no
    /// non-empty `data.mac`, returns `Error::Api { status: 502, .. }` with the message
    /// `"Device {device_id} response missing 'mac' field; cannot blacklist"`, matching Python's
    /// client-fabricated `EeroAPIException(502, ...)` exactly (`devices.py:173-177`) — no
    /// `POST` to `/blacklist` is sent in that case. Otherwise returns whatever other
    /// status-mapped error either request produces — see `Transport::send`.
    pub async fn block_device(
        &self,
        network_id: &str,
        device_id: &str,
        blocked: bool,
    ) -> Result<Envelope, Error> {
        if !blocked {
            return self
                .transport
                .send(
                    &routes::REMOVE_FROM_BLACKLIST,
                    &[("network_id", network_id), ("mac_or_device_id", device_id)],
                    None,
                )
                .await;
        }

        let device_response = self.get_device(network_id, device_id).await?;
        let mac = device_response
            .data()
            .get("mac")
            .and_then(Value::as_str)
            .filter(|mac| !mac.is_empty());
        let Some(mac) = mac else {
            return Err(Error::Api {
                status: 502,
                message: format!(
                    "Device {device_id} response missing 'mac' field; cannot blacklist"
                ),
                url: None,
            });
        };

        self.transport
            .send(
                &routes::ADD_TO_BLACKLIST,
                &[("network_id", network_id)],
                Some(json!({ "mac": mac })),
            )
            .await
    }
}
