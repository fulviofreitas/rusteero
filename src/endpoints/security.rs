//! Security settings API: `eero-api`'s `SecurityAPI`, v8.0.4.
//!
//! Ported from `eero-api src/eero/api/security.py` at v8.0.4. Every method here funnels through
//! [`crate::transport::Transport::request`]/[`crate::transport::Transport::resource`], which
//! already implements the "not authenticated" precondition Python repeats at the top of every
//! method and every status-to-error mapping a response can produce — so, unlike the Python
//! source, no method below duplicates that guard.
//!
//! Every write below is a settings-class or sub-resource write whose side effects have not been
//! confirmed against a live network (`security.py`'s own per-method warnings) — each calls
//! [`crate::links::warn_uncharacterised_write`] with Python's identical operation string
//! immediately before the request. Two warning classes exist and are not conflated: writes to the
//! shared `networks/{id}/settings` resource ([`SecurityApi::set_wpa3`],
//! [`SecurityApi::set_band_steering`], [`SecurityApi::set_upnp`], [`SecurityApi::set_ipv6`],
//! [`SecurityApi::configure_security`]) and to `networks/{id}/mlo_mode`
//! ([`SecurityApi::set_mlo_mode`]) carry the "-- may reboot every eero, like the confirmed DNS
//! write path" suffix; writes to a narrower dedicated sub-resource
//! ([`SecurityApi::set_fast_transition`], [`SecurityApi::set_passpoint_enabled`],
//! [`SecurityApi::set_proxied_nodes`]) carry a shorter warning with no reboot claim (g5 brief §3
//! note 4).

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::params::resolve_network_url;
use crate::routes;
use crate::routes::ApiVersion;
use crate::transport::{RequestBody, Transport};

/// Valid values for the network's MLO (Multi-Link Operation) mode.
///
/// Ported from `MLO_MODE_DISABLED`/`MLO_MODE_SINGLE`/`MLO_MODE_MULTI`/`_MLO_MODES`
/// (`security.py:19-23`).
pub const MLO_MODES: &[&str] = &["disabled", "multi", "single"];

/// `SecurityAPI` (`src/eero/api/security.py`), v8.0.4.
///
/// Build one with [`SecurityApi::new`], wrapping a [`Transport`] shared with the rest of the
/// [`crate::api::EeroApi`] aggregator — `SecurityApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct SecurityApi {
    transport: Arc<Transport>,
}

impl SecurityApi {
    /// Wraps `transport` as a `SecurityApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets security settings for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:53-80` (`SecurityAPI.get_security_settings`).
    /// Resolves via the module's own `_network_own_url` helper (`security.py:26-31`), which
    /// prefers a supplied `parent`'s own `url` field over the bare-id `networks/{id}` template —
    /// byte-for-byte [`crate::params::resolve_network_url`], which this method calls directly
    /// (see `src/routes/security.rs`'s module docs for why this is not a [`crate::routes::Resource`]).
    /// Security settings are part of the full network object: look for `wpa3`, `band_steering`,
    /// `upnp`, `ipv6_upstream`, `ipv6_downstream`, and any others the server includes.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_security_settings(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            ApiVersion::V2_2,
        )?;
        self.transport
            .request(reqwest::Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// `PUT /2.2/networks/{id}/settings` — enables or disables WPA3 encryption.
    ///
    /// Ported from `eero-api src/eero/api/security.py:82-134` (`SecurityAPI.set_wpa3`). Sends
    /// `{"wpa3": enabled}` (`security.py:129`).
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_wpa3(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_settings_class(
            network_id,
            json!({ "wpa3": enabled }),
            parent,
            "set WPA3 for network",
        )
        .await
    }

    /// `PUT /2.2/networks/{id}/settings` — enables or disables band steering.
    ///
    /// Ported from `eero-api src/eero/api/security.py:136-189` (`SecurityAPI.set_band_steering`).
    /// Sends `{"band_steering": enabled}` (`security.py:183`).
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_band_steering(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_settings_class(
            network_id,
            json!({ "band_steering": enabled }),
            parent,
            "set band steering for network",
        )
        .await
    }

    /// `PUT /2.2/networks/{id}/settings` — enables or disables `UPnP`.
    ///
    /// Ported from `eero-api src/eero/api/security.py:191-243` (`SecurityAPI.set_upnp`). Sends
    /// `{"upnp": enabled}` (`security.py:238`).
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_upnp(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_settings_class(
            network_id,
            json!({ "upnp": enabled }),
            parent,
            "set UPnP for network",
        )
        .await
    }

    /// `PUT /2.2/networks/{id}/settings` — enables or disables IPv6.
    ///
    /// Ported from `eero-api src/eero/api/security.py:245-297` (`SecurityAPI.set_ipv6`). Sends
    /// **both** `{"ipv6_upstream": enabled, "ipv6_downstream": enabled}` (`security.py:294-297`)
    /// — a single flag fans out to two wire keys.
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_ipv6(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_settings_class(
            network_id,
            json!({ "ipv6_upstream": enabled, "ipv6_downstream": enabled }),
            parent,
            "set IPv6 for network",
        )
        .await
    }

    /// Configures multiple security settings in one call — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:299-374` (`SecurityAPI.configure_security`).
    /// Builds a body from only the arguments actually supplied — `{"wpa3": ...}`,
    /// `{"band_steering": ...}`, `{"upnp": ...}` only when their argument is `Some`
    /// (`security.py:338-349`), `{"ipv6_upstream": ..., "ipv6_downstream": ...}` both set from
    /// `ipv6` when it is `Some` (the same one-flag-two-keys fan-out as [`SecurityApi::set_ipv6`]).
    /// **`thread` is not a parameter of this method** — the settings endpoint never accepted a
    /// `thread` field; that write moved to [`crate::endpoints::thread::ThreadApi::set_thread_enabled`]
    /// in v8.0.0, and `SecurityAPI.configure_security`'s `thread=` keyword was removed alongside it
    /// (a `TypeError` for any caller still passing it, not a silent no-op).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "settings", message: "at least one of wpa3,
    /// band_steering, upnp, ipv6 must be supplied" }` if every argument is `None`, before any
    /// request is sent. This is a **deliberate, permanent divergence** from Python, which instead
    /// fabricates a local `{"meta": {"code": 400}, "data": {}}` response (`security.py:356-358`)
    /// — a response that never actually came from the wire. `SecurityApi::configure_security`
    /// diverged from that shape before v8.0.4 already (raising rather than fabricating),
    /// consistent with [`DnsApi::set_dns_mode`](crate::endpoints::dns::DnsApi::set_dns_mode)'s own
    /// documented divergence; this port keeps it (g5 brief §3 note 7, §6 open question 3).
    /// Otherwise see [`SecurityApi::get_security_settings`].
    #[allow(clippy::too_many_arguments)]
    pub async fn configure_security(
        &self,
        network_id: &str,
        wpa3: Option<bool>,
        band_steering: Option<bool>,
        upnp: Option<bool>,
        ipv6: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut payload = Map::new();
        if let Some(wpa3) = wpa3 {
            payload.insert("wpa3".to_owned(), Value::Bool(wpa3));
        }
        if let Some(band_steering) = band_steering {
            payload.insert("band_steering".to_owned(), Value::Bool(band_steering));
        }
        if let Some(upnp) = upnp {
            payload.insert("upnp".to_owned(), Value::Bool(upnp));
        }
        if let Some(ipv6) = ipv6 {
            payload.insert("ipv6_upstream".to_owned(), Value::Bool(ipv6));
            payload.insert("ipv6_downstream".to_owned(), Value::Bool(ipv6));
        }
        if payload.is_empty() {
            return Err(Error::validation(
                "settings",
                "at least one of wpa3, band_steering, upnp, ipv6 must be supplied",
            ));
        }

        self.put_settings_class(
            network_id,
            Value::Object(payload),
            parent,
            "configure security settings for network",
        )
        .await
    }

    /// Sets the network's MLO (Multi-Link Operation) mode — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:376-425` (`SecurityAPI.set_mlo_mode`).
    /// Issues a JSON PUT to the network's `mlo_mode` sub-resource with `{"mlo_mode": mode}`
    /// (`security.py:424`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "mode", .. }` if `mode` is not one of
    /// [`MLO_MODES`] (`security.py:418-420`), before any request is sent. Otherwise see
    /// [`SecurityApi::get_security_settings`].
    pub async fn set_mlo_mode(
        &self,
        network_id: &str,
        mode: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        if !MLO_MODES.contains(&mode) {
            return Err(Error::validation(
                "mode",
                format!("must be one of {MLO_MODES:?}, got {mode:?}"),
            ));
        }
        let url = routes::security::SET_MLO_MODE.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write(
            "set MLO mode for network -- may reboot every eero, like the confirmed DNS write path",
        );
        self.transport
            .request(
                routes::security::SET_MLO_MODE.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "mlo_mode": mode })),
            )
            .await
    }

    /// Gets the network's 802.11r fast-transition setting — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:427-460` (`SecurityAPI.get_fast_transition`).
    /// GETs the network's `fast_transition` sub-resource. A verified read.
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn get_fast_transition(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::security::GET_FAST_TRANSITION,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Sets the network's 802.11r fast-transition setting — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:462-502` (`SecurityAPI.set_fast_transition`).
    /// Issues a JSON PUT to the network's `fast_transition` sub-resource with
    /// `{"fast_transition": enabled}` (`security.py:501`). Unverified upstream.
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_fast_transition(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::security::SET_FAST_TRANSITION.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("set fast transition for network");
        self.transport
            .request(
                routes::security::SET_FAST_TRANSITION.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "fast_transition": enabled })),
            )
            .await
    }

    /// Enables or disables Passpoint — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:504-541` (`SecurityAPI.set_passpoint_enabled`).
    /// Issues a JSON PUT to the network's `passpoint/enabled` sub-resource with
    /// `{"enabled": enabled}`. Unverified upstream.
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_passpoint_enabled(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::security::SET_PASSPOINT_ENABLED.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("set Passpoint enabled for network");
        self.transport
            .request(
                routes::security::SET_PASSPOINT_ENABLED.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "enabled": enabled })),
            )
            .await
    }

    /// Enables or disables proxied nodes — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:543-581` (`SecurityAPI.set_proxied_nodes`).
    /// Issues a JSON PUT to the network's `proxied_nodes` sub-resource with
    /// `{"enabled": enabled}`. Unverified upstream.
    ///
    /// # Errors
    ///
    /// See [`SecurityApi::get_security_settings`].
    pub async fn set_proxied_nodes(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::security::SET_PROXIED_NODES.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("set proxied nodes for network");
        self.transport
            .request(
                routes::security::SET_PROXIED_NODES.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "enabled": enabled })),
            )
            .await
    }

    /// Shared `PUT /2.2/networks/{id}/settings` call site behind
    /// [`SecurityApi::set_wpa3`]/[`SecurityApi::set_band_steering`]/[`SecurityApi::set_upnp`]/
    /// [`SecurityApi::set_ipv6`]/[`SecurityApi::configure_security`] — the five writers that share
    /// [`crate::routes::security::SECURITY_PUT_SETTINGS`] and the "-- may reboot every eero, like
    /// the confirmed DNS write path" warning suffix.
    async fn put_settings_class(
        &self,
        network_id: &str,
        payload: Value,
        parent: Option<&Value>,
        operation: &str,
    ) -> Result<Envelope, Error> {
        let url = routes::security::SECURITY_PUT_SETTINGS.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write(&format!(
            "{operation} -- may reboot every eero, like the confirmed DNS write path"
        ));
        self.transport
            .request(
                routes::security::SECURITY_PUT_SETTINGS.method.clone(),
                url,
                &[],
                RequestBody::Json(payload),
            )
            .await
    }
}
