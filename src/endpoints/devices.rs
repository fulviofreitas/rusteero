//! Devices API — device queries plus the nickname/pause/type/label/block mutations.
//!
//! Ported from `eero-api src/eero/api/devices.py` at `v8.0.4`
//! (`.claude/tasks/briefs/v8/g3-devices.md`). `set_device_priority`/`get_device_priority` remain
//! deliberately unported — see the module note below, unchanged from the pre-8.0.4 port.
//!
//! # Not ported: `set_device_priority`
//!
//! `eero-api`'s `DevicesAPI.set_device_priority` (v6.2.0 `devices.py:219-271`) is a confirmed
//! no-op: the Eero cloud API no longer exposes device-level priority — there is no `/priority` or
//! `/qos` endpoint, and the `prioritized` field observed on the device object
//! (`GET_DEVICE`/`GET_DEVICES`) does not respond to it. It was removed from `eero-api` entirely
//! in `05a2b07` (v8.0.0). **Not ported here, and should not be added later for "parity"** — the
//! endpoint it would call does not do anything. (`Client::get_device_priority`, by contrast, is a
//! real v8.0.4 `EeroClient` method — see `crate::client`'s `devices` module — it simply calls this
//! module's ordinary [`DevicesApi::get_device`] without caching; nothing here is unported for it.)
//!
//! # `mac` vs. `device_id`
//!
//! Every method below names its device parameter `mac`, matching `v8.0.4`'s own rename (the
//! wiki is explicit: "`device_id` on the facade is the device's MAC address (the domain methods
//! name it `mac`)"). `Client`'s own wrapper methods keep the `device_id` name for backward
//! compatibility at that layer only — see `crate::client`'s `devices` module.
//!
//! # `block_device`/`unblock_device`: pure delegates, no round trip
//!
//! Unlike the pre-8.0.0 shape this crate previously shipped (a single `block_device(..., blocked:
//! bool)` that `GET`s the device first to resolve its MAC, then `POST`s/`DELETE`s `/blacklist`),
//! `v8.0.4` splits this into two methods that are **pure delegates** to [`BlacklistApi`] — no
//! `GET` at all (`devices.py:437-472`).

use std::sync::Arc;

use reqwest::Method;
use serde_json::{Map, Value, json};
use url::Url;

use crate::endpoints::blacklist::BlacklistApi;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `DevicesAPI` (`devices.py`): device listing, single-device lookup, and the
/// nickname/pause/type/label/block mutations.
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
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/devices` — list devices connected to `network_id`.
    ///
    /// Ported from `DevicesAPI.get_devices` (`devices.py:114-165`). `thread`/`proxied_node` are
    /// attached as `"true"`/`"false"` query parameters, each omitted entirely when `None`
    /// (`devices.py:163`'s `_bool_param` — no query string at all when both are `None`). Prefers
    /// `parent`'s own published `devices` link over the `network_id` template when supplied.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// [`Error::Validation`] if the resolved URL is malformed, or whatever other status-mapped
    /// error the request produces.
    pub async fn get_devices(
        &self,
        network_id: &str,
        thread: Option<bool>,
        proxied_node: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut query = Vec::new();
        if let Some(thread) = thread {
            query.push(("thread", bool_param(thread)));
        }
        if let Some(proxied_node) = proxied_node {
            query.push(("proxied_node", bool_param(proxied_node)));
        }
        self.transport
            .resource(
                &routes::devices::V8_GET_DEVICES,
                network_id,
                parent,
                &query,
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/devices/{mac}` — a single device's details.
    ///
    /// Ported from `DevicesAPI.get_device` (`devices.py:166-199`). Prefers `parent`'s own
    /// `self_url` (i.e. the device's own previously-fetched `url` field) when supplied, falling
    /// back to the `network_id`/`mac` template (`routes::devices::V8_GET_DEVICE`) otherwise.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// [`Error::Validation`] if `parent`'s `url` field (when present) or the resolved template is
    /// malformed, `Error::NotFound { status: 404, .. }` if `mac` does not exist on `network_id`,
    /// or whatever other status-mapped error the request produces.
    pub async fn get_device(
        &self,
        network_id: &str,
        mac: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve_device_url(network_id, mac, parent)?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Resolves a single device's URL, preferring `parent`'s own `self_url` over the
    /// `network_id`/`mac` template. Shared by [`DevicesApi::get_device`] and
    /// [`DevicesApi::update_device_via_link`] — both target API version 2.2 and both prefer the
    /// device's own previously-published URL identically (`devices.py:166-199,259-328`).
    fn resolve_device_url(
        &self,
        network_id: &str,
        mac: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        if let Some(parent) = parent
            && let Some(url) = crate::links::self_url(self.transport.api_host(), parent)?
        {
            return Ok(url);
        }
        routes::devices::V8_GET_DEVICE.resolve(self.transport.api_host(), network_id, mac, None)
    }

    /// `PUT /2.3/networks/{network_id}/devices/{mac}` — set a device's nickname.
    ///
    /// Ported from `DevicesAPI._update_device` via `DevicesAPI.set_device_nickname`
    /// (`devices.py:44-65,201-225`), sending body `{"nickname": nickname}` (`devices.py:225`).
    /// `mac` is normalised through [`crate::util::id_from_url`] **before** being resolved against
    /// the 2.3 route — exactly like `_update_device` (`devices.py:82-112`'s docstring) —
    /// specifically so a path/URL-form `mac` (which could carry its own, different version
    /// segment) cannot smuggle a device write past the 2.3 version pin. **Must**, and does, use
    /// `routes::devices::V8_SET_DEVICE_NICKNAME` (`ApiVersion::V2_3`), never a `/2.2` route: the
    /// `/2.2` endpoint accepts the identical `PUT`, returns `200 OK`, and silently drops the write
    /// (`eero-api` issue #102).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// [`Error::Validation`] if `mac` cannot be normalised or the resolved URL is malformed, or
    /// whatever other status-mapped error the request produces.
    pub async fn set_device_nickname(
        &self,
        network_id: &str,
        mac: &str,
        nickname: &str,
    ) -> Result<Envelope, Error> {
        let normalized_mac = crate::util::id_from_url(mac)?;
        let url = routes::devices::V8_SET_DEVICE_NICKNAME.resolve(
            self.transport.api_host(),
            network_id,
            &normalized_mac,
            None,
        )?;
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Json(json!({ "nickname": nickname })),
            )
            .await
    }

    /// `PUT /2.3/networks/{network_id}/devices/{mac}` — pause or unpause a device's internet
    /// access.
    ///
    /// Ported from `DevicesAPI._update_device` via `DevicesAPI.pause_device`
    /// (`devices.py:44-65,227-257`), sending body `{"paused": paused}` (`devices.py:257`). Same
    /// `mac`-normalisation and 2.3 version-pin rationale as [`DevicesApi::set_device_nickname`].
    ///
    /// A paused device stays connected to the network but loses internet access; this is
    /// distinct from `block_device`, which removes the device from the network entirely.
    ///
    /// # Errors
    ///
    /// See [`DevicesApi::set_device_nickname`].
    pub async fn pause_device(
        &self,
        network_id: &str,
        mac: &str,
        paused: bool,
    ) -> Result<Envelope, Error> {
        let normalized_mac = crate::util::id_from_url(mac)?;
        let url = routes::devices::V8_PAUSE_DEVICE.resolve(
            self.transport.api_host(),
            network_id,
            &normalized_mac,
            None,
        )?;
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Json(json!({ "paused": paused })),
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/devices/{mac}` — update a device's nickname/paused/profile
    /// via its own published URL.
    ///
    /// Ported from `DevicesAPI.update_device_via_link` (`devices.py:259-328`). New in `v8.0.4`
    /// (unverified write). Sends a JSON body of only the fields the caller supplied among
    /// `nickname`/`paused`/`profile` (`devices.py:304-315`) — `mac` is never itself added to the
    /// payload, despite the module docstring's wording (`devices.py:96-99`'s docstring mismatch;
    /// see this module's own port brief for the citation), targets the **default** (2.2) API
    /// version, not 2.3, and prefers `parent`'s own `self_url` exactly like
    /// [`DevicesApi::get_device`] (`resolve_device_url`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "device"` if `nickname`, `paused` and `profile`
    /// are all `None` (`devices.py:322-325`), before any request is sent. Otherwise as
    /// [`DevicesApi::get_device`].
    pub async fn update_device_via_link(
        &self,
        network_id: &str,
        mac: &str,
        nickname: Option<&str>,
        paused: Option<bool>,
        profile: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut body = Map::new();
        if let Some(nickname) = nickname {
            body.insert("nickname".to_owned(), json!(nickname));
        }
        if let Some(paused) = paused {
            body.insert("paused".to_owned(), json!(paused));
        }
        if let Some(profile) = profile {
            body.insert("profile".to_owned(), json!(profile));
        }
        if body.is_empty() {
            return Err(Error::validation(
                "device",
                "at least one of nickname, paused, profile must be supplied",
            ));
        }
        let url = self.resolve_device_url(network_id, mac, parent)?;
        crate::links::warn_uncharacterised_write("update_device_via_link");
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/devices/{mac}` — set a device's type.
    ///
    /// Ported from `DevicesAPI.set_device_type` (`devices.py:330-356`), sending body
    /// `{"device_type": device_type}` (`devices.py:356`). Live-verified 2026-09-20 to persist and
    /// read back, and to **not** log the uncharacterised-write warning (unlike
    /// `update_device_via_link`/`set_device_labels`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// [`Error::Validation`] if the resolved URL is malformed, or whatever other status-mapped
    /// error the request produces.
    pub async fn set_device_type(
        &self,
        network_id: &str,
        mac: &str,
        device_type: &str,
    ) -> Result<Envelope, Error> {
        let url = routes::devices::SET_DEVICE_TYPE.resolve(
            self.transport.api_host(),
            network_id,
            mac,
            None,
        )?;
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Json(json!({ "device_type": device_type })),
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/devices/{mac}/labels` — a device's labels.
    ///
    /// Ported from `DevicesAPI.get_device_labels` (`devices.py:358-378`).
    ///
    /// # Errors
    ///
    /// See [`DevicesApi::set_device_type`].
    pub async fn get_device_labels(&self, network_id: &str, mac: &str) -> Result<Envelope, Error> {
        let url = routes::devices::GET_DEVICE_LABELS.resolve(
            self.transport.api_host(),
            network_id,
            mac,
            None,
        )?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// `PUT /2.2/networks/{network_id}/devices/{mac}/labels` — set a device's labels.
    ///
    /// Ported from `DevicesAPI.set_device_labels` (`devices.py:380-435`). Every label is a
    /// **query parameter**, each omitted entirely when `None` (`devices.py:424-433`) — this
    /// request has no body at all, never a JSON payload. Confirmed server-side **no-op** as of
    /// 2026-09-20 (`200 OK`, labels echoed back unchanged, nothing persists) — ported for parity
    /// and documented as such, not removed, matching this crate's convention of porting
    /// confirmed-unverified/no-op writes rather than silently dropping them.
    ///
    /// # Errors
    ///
    /// See [`DevicesApi::set_device_type`].
    #[allow(clippy::too_many_arguments)]
    pub async fn set_device_labels(
        &self,
        network_id: &str,
        mac: &str,
        make_label: Option<&str>,
        model_label: Option<&str>,
        version_label: Option<&str>,
        type_label: Option<&str>,
    ) -> Result<Envelope, Error> {
        let url = routes::devices::SET_DEVICE_LABELS.resolve(
            self.transport.api_host(),
            network_id,
            mac,
            None,
        )?;
        let mut query = Vec::new();
        if let Some(value) = make_label {
            query.push(("make_label", value.to_owned()));
        }
        if let Some(value) = model_label {
            query.push(("model_label", value.to_owned()));
        }
        if let Some(value) = version_label {
            query.push(("version_label", value.to_owned()));
        }
        if let Some(value) = type_label {
            query.push(("type_label", value.to_owned()));
        }
        crate::links::warn_uncharacterised_write("set_device_labels");
        self.transport
            .request(Method::PUT, url, &query, RequestBody::None)
            .await
    }

    /// Adds a device (by MAC) to the blacklist — a pure delegate to
    /// [`BlacklistApi::add_to_blacklist`], no round trip of its own.
    ///
    /// Ported from `DevicesAPI.block_device` (`devices.py:437-454`): `BlacklistAPI(self._auth_api)
    /// .add_to_blacklist(network, mac)` (`devices.py:454`). **Breaking shape change** from the
    /// pre-8.0.0 port this crate previously shipped: there is no `blocked: bool` parameter any
    /// more (unblocking is [`DevicesApi::unblock_device`], a separate method) and no `GET` of the
    /// device first — v8.0.4 never resolves the MAC itself, it hands `mac` straight to
    /// `BlacklistApi`.
    ///
    /// # Errors
    ///
    /// See [`BlacklistApi::add_to_blacklist`].
    pub async fn block_device(&self, network_id: &str, mac: &str) -> Result<Envelope, Error> {
        BlacklistApi::new(Arc::clone(&self.transport))
            .add_to_blacklist(network_id, mac, None)
            .await
    }

    /// Removes a device from the blacklist — a pure delegate to
    /// [`BlacklistApi::remove_from_blacklist`].
    ///
    /// Ported from `DevicesAPI.unblock_device` (`devices.py:456-472`): `BlacklistAPI(
    /// self._auth_api).remove_from_blacklist(network, mac)` (`devices.py:472`).
    ///
    /// # Errors
    ///
    /// See [`BlacklistApi::remove_from_blacklist`].
    pub async fn unblock_device(&self, network_id: &str, mac: &str) -> Result<Envelope, Error> {
        BlacklistApi::new(Arc::clone(&self.transport))
            .remove_from_blacklist(network_id, mac, None)
            .await
    }
}

/// Renders a `bool` as the lowercase `"true"`/`"false"` string `DevicesApi::get_devices`'s
/// `thread`/`proxied_node` query parameters use.
///
/// Ported from `_bool_param` (`devices.py:40-51`).
fn bool_param(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}
