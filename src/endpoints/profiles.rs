//! Profiles API for Eero: per-network device-grouping "profiles" (e.g. "Kids", "Guests").
//!
//! Ported from `eero-api`'s `src/eero/api/profiles.py` at v8.0.4. Covers every `ProfilesAPI`
//! method. As in Python, every method here returns the raw `{"meta": …, "data": …}` envelope
//! unmodified — `ProfilesApi` never extracts, filters or reshapes anything out of a response.
//!
//! A profile's body has exactly four fields: `devices`, `name`, `paused`, `url`. Content
//! filtering, block lists, and blocked applications are **not** profile fields at all — writing
//! them here was a silent no-op (live-verified; they never persisted). That functionality is
//! served by [`crate::endpoints::dns_policies::DnsPoliciesApi`] instead; the four now-removed
//! methods that used to write those non-fields (`update_profile_content_filter`,
//! `update_profile_block_list`, `get_blocked_applications`, `set_blocked_applications`) have no
//! replacement *on this module* — see `dns_policies.rs`.
//!
//! `get_profile` and `get_profile_devices` are the *same* wire call
//! (`GET networks/{network_id}/profiles/{profile_id}`) returning the same full profile object;
//! `get_profile_devices` is a pure delegator, matching `profiles.py:183-206` exactly. Likewise,
//! `pause_profile`, `set_profile_devices` and `rename_profile` all `PUT` the same
//! `networks/{network_id}/profiles/{profile_id}` resource (via the shared private
//! `profile_request` helper, mirroring Python's `_update_profile`,
//! `profiles.py:122-155`) with a different JSON key each.

use std::sync::Arc;

use serde_json::{Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes::{self, Nested};
use crate::transport::{RequestBody, Transport};

/// The profiles API for a single Eero account, built on a shared [`Transport`].
#[derive(Debug)]
pub struct ProfilesApi {
    transport: Arc<Transport>,
}

impl ProfilesApi {
    /// Wraps `transport` as a `ProfilesApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Resolves a single profile's request URL: prefers [`links::self_url`] on the profile's own
    /// cached `parent` envelope when supplied and resolvable, else falls back to `route`'s
    /// `networks/{network_id}/profiles/{profile_id}` template.
    ///
    /// Mirrors `ProfilesAPI._profile_url`/the inline `self_url(resolved_parent) if
    /// resolved_parent is not None else None) or (...)` expression `get_profile` and
    /// `_update_profile` both use (`profiles.py:47,116-118,150-153`) — see `routes::profiles`'s
    /// module docs for why this cannot be expressed as a plain [`Nested::resolve`] call (its
    /// parent preference is a *named-link* lookup, not [`links::self_url`]).
    fn profile_url(
        &self,
        route: &Nested,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let host = self.transport.api_host();
        if let Some(parent) = parent
            && let Some(url) = links::self_url(host, parent)?
        {
            return Ok(url);
        }
        route.resolve(host, network_id, profile_id, None)
    }

    /// Issues a request against a single profile's resource, resolved via
    /// [`ProfilesApi::profile_url`].
    async fn profile_request(
        &self,
        route: &Nested,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
        body: RequestBody,
    ) -> Result<Envelope, Error> {
        let url = self.profile_url(route, network_id, profile_id, parent)?;
        self.transport
            .request(route.method.clone(), url, &[], body)
            .await
    }

    /// Like [`ProfilesApi::profile_request`], but logs a fixed uncharacterised-write warning
    /// after the URL is resolved and immediately before the request — mirroring
    /// `_update_profile`'s exact statement order (`profiles.py:122-155`: resolve the URL, warn,
    /// then `PUT`), rather than warning before URL resolution/validation can fail.
    async fn profile_write(
        &self,
        route: &Nested,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
        operation: &str,
        body: RequestBody,
    ) -> Result<Envelope, Error> {
        let url = self.profile_url(route, network_id, profile_id, parent)?;
        links::warn_uncharacterised_write(operation);
        self.transport
            .request(route.method.clone(), url, &[], body)
            .await
    }

    /// Lists every profile on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:53-80` (`ProfilesAPI.get_profiles`): sends
    /// `GET` `routes::profiles::GET_PROFILES` (`networks/{network_id}/profiles`), preferring the
    /// parent network envelope's own `profiles` link when `parent` is supplied and resolvable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, [`Error::Validation`]
    /// if `network_id`/`parent`'s link cannot be resolved to a URL, or whatever status-mapped
    /// [`Error`] the request produces otherwise.
    pub async fn get_profiles(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::profiles::GET_PROFILES,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Gets a single profile's full object — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:86-118` (`ProfilesAPI.get_profile`): sends
    /// `GET` `routes::profiles::GET_PROFILE` (`networks/{network_id}/profiles/{profile_id}`),
    /// preferring `parent`'s own `url` (via [`links::self_url`]) when supplied and resolvable —
    /// when it is, `network_id`/`profile_id` are ignored for URL resolution entirely, matching
    /// Python exactly.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, [`Error::Validation`]
    /// if neither `parent` nor `network_id`/`profile_id` resolve to a URL, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn get_profile(
        &self,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.profile_request(
            &routes::profiles::GET_PROFILE,
            network_id,
            profile_id,
            parent,
            RequestBody::None,
        )
        .await
    }

    /// Gets a profile's data "including devices" — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:183-206` (`ProfilesAPI.get_profile_devices`),
    /// a pure delegator to [`ProfilesApi::get_profile`] — the exact same request, the exact same
    /// untransformed response. Python's docstring notes "The devices are in the `devices` field
    /// of the response data" (`profiles.py:107`), but neither Python nor this port extracts that
    /// field.
    ///
    /// # Errors
    ///
    /// See [`ProfilesApi::get_profile`].
    pub async fn get_profile_devices(
        &self,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.get_profile(network_id, profile_id, parent).await
    }

    /// Pauses or unpauses internet access for a profile — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:157-181` (`ProfilesAPI.pause_profile`, via
    /// `_update_profile`): sends `PUT` `routes::profiles::PAUSE_PROFILE` with body `{"paused":
    /// paused}`. Logs one `warn_uncharacterised_write("update profile")` immediately before the
    /// request, matching `_update_profile`'s own single warning (`profiles.py:154`).
    ///
    /// # Errors
    ///
    /// See [`ProfilesApi::get_profile`].
    pub async fn pause_profile(
        &self,
        network_id: &str,
        profile_id: &str,
        paused: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.profile_write(
            &routes::profiles::PAUSE_PROFILE,
            network_id,
            profile_id,
            parent,
            "update profile",
            RequestBody::Json(json!({ "paused": paused })),
        )
        .await
    }

    /// Sets the devices assigned to a profile — returns the raw Eero API response.
    ///
    /// **Replaces the profile's entire device assignment list** (`profiles.py:138`).
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:208-245`
    /// (`ProfilesAPI.set_profile_devices`): sends `PUT` `routes::profiles::SET_PROFILE_DEVICES`
    /// with body `{"devices": [{"url": ...}, ...]}` — each URL wrapped in its own single-key
    /// object, matching Python's `devices_payload = [{"url": url} for url in device_urls]`
    /// exactly.
    ///
    /// **Double-warn, reproduced deliberately.** This method
    /// calls `warn_uncharacterised_write("set devices for profile")` itself
    /// (`profiles.py:241`) *and then* delegates to the same `_update_profile` helper
    /// [`ProfilesApi::pause_profile`]/[`ProfilesApi::rename_profile`] use, which logs a second,
    /// differently-worded `warn_uncharacterised_write("update profile")`
    /// (`profiles.py:154`) — two `tracing::warn!` lines per call, unlike its siblings, which log
    /// exactly one. This looks like a Python oversight, not a deliberate design, but is
    /// reproduced faithfully rather than silently "fixed" to one warning: this crate's own
    /// discipline (see `CLAUDE.md`'s lessons-learned entries) is to preserve observed server/SDK
    /// behaviour and flag it, not quietly diverge.
    ///
    /// # Errors
    ///
    /// See [`ProfilesApi::get_profile`].
    pub async fn set_profile_devices(
        &self,
        network_id: &str,
        profile_id: &str,
        device_urls: &[&str],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let devices: Vec<_> = device_urls
            .iter()
            .map(|url| json!({ "url": url }))
            .collect();
        let url = self.profile_url(
            &routes::profiles::SET_PROFILE_DEVICES,
            network_id,
            profile_id,
            parent,
        )?;
        links::warn_uncharacterised_write("set devices for profile");
        links::warn_uncharacterised_write("update profile");
        self.transport
            .request(
                routes::profiles::SET_PROFILE_DEVICES.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "devices": devices })),
            )
            .await
    }

    /// Creates a new profile on a network — returns the raw Eero API response.
    ///
    /// `name` is always sent. `devices` (device URL strings, wrapped as `[{"url": ...}, ...]`)
    /// and `paused` are each included in the request **only when not `None`** — an explicit
    /// `Some(false)`/`Some(&[])` is still sent, matching Python's omit-when-`None` (not
    /// omit-when-falsy) rule exactly (`profiles.py:278-283`).
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:247-300` (`ProfilesAPI.create_profile`):
    /// sends `POST` `routes::profiles::CREATE_PROFILE` (`networks/{network_id}/profiles`,
    /// preferring the parent network envelope's own `profiles` link when supplied). Logs one
    /// `warn_uncharacterised_write("create profile for network")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`ProfilesApi::get_profiles`].
    pub async fn create_profile(
        &self,
        network_id: &str,
        name: &str,
        devices: Option<&[&str]>,
        paused: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::profiles::CREATE_PROFILE.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;

        let mut payload = serde_json::Map::new();
        payload.insert("name".to_owned(), Value::String(name.to_owned()));
        if let Some(devices) = devices {
            let devices: Vec<_> = devices.iter().map(|url| json!({ "url": url })).collect();
            payload.insert("devices".to_owned(), Value::Array(devices));
        }
        if let Some(paused) = paused {
            payload.insert("paused".to_owned(), Value::Bool(paused));
        }

        links::warn_uncharacterised_write("create profile for network");
        self.transport
            .request(
                routes::profiles::CREATE_PROFILE.method.clone(),
                url,
                &[],
                RequestBody::Json(Value::Object(payload)),
            )
            .await
    }

    /// Renames an existing profile — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:304-330` (`ProfilesAPI.rename_profile`, via
    /// `_update_profile`): sends `PUT` `routes::profiles::RENAME_PROFILE` with body `{"name":
    /// name}`. Logs one `warn_uncharacterised_write("update profile")` immediately before the
    /// request.
    ///
    /// # Errors
    ///
    /// See [`ProfilesApi::get_profile`].
    pub async fn rename_profile(
        &self,
        network_id: &str,
        profile_id: &str,
        name: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.profile_write(
            &routes::profiles::RENAME_PROFILE,
            network_id,
            profile_id,
            parent,
            "update profile",
            RequestBody::Json(json!({ "name": name })),
        )
        .await
    }

    /// Deletes a profile from a network — returns the raw Eero API response.
    ///
    /// Devices previously assigned to `profile_id` become unassigned server-side
    /// (`profiles.py:394` at v6.2.0's equivalent wording); nothing here initiates that.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:330-355` (`ProfilesAPI.delete_profile`):
    /// sends `DELETE` `routes::profiles::DELETE_PROFILE`
    /// (`networks/{network_id}/profiles/{profile_id}`), with no request body. Unlike every other
    /// method in this module, Python's `delete_profile` takes **no `parent` kwarg at all** — this
    /// port matches that signature exactly, always resolving via the network/profile-id template.
    /// Logs one `warn_uncharacterised_write("delete profile")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`ProfilesApi::get_profile`].
    pub async fn delete_profile(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        self.profile_write(
            &routes::profiles::DELETE_PROFILE,
            network_id,
            profile_id,
            None,
            "delete profile",
            RequestBody::None,
        )
        .await
    }
}
