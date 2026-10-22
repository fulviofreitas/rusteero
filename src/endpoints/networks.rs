//! Networks API: `eero-api`'s `NetworksAPI`, at v8.4.1.
//!
//! Ported from `eero-api src/eero/api/networks.py` (v8.4.1). Every method that can use a published
//! link accepts an optional keyword-only `parent` — the caller's own cached network envelope (or,
//! for the guest-network password methods, guest-network envelope) — so the link the API
//! published on it is used instead of a locally-built template; see each method's own doc comment
//! for exactly which envelope it expects. [`NetworksApi::set_hide_5g`] takes no `parent`: the
//! network envelope publishes no link to that resource.
//!
//! [`NetworksApi::get_account`] is deprecated: at v8.4.1 the account read lives on `AccountAPI`,
//! and `eero-api`'s `EeroClient.get_account` calls it there.
//!
//! Every method here funnels through [`crate::transport::Transport::request`]/`resource`, which
//! already implement the "not authenticated" precondition Python repeats at the top of each
//! method (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method below
//! duplicates that guard. As a consequence the gate runs after local validation, URL resolution
//! and the uncharacterised-write warning, where Python checks it before resolving and warning.

use std::sync::Arc;

use reqwest::Method;
use serde_json::{Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::params::{require_non_empty_string, resolve_network_url};
use crate::routes::{self, ApiVersion};
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `NetworksAPI` (`src/eero/api/networks.py`), at v8.4.1.
///
/// Build one with [`NetworksApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `NetworksApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct NetworksApi {
    transport: Arc<Transport>,
}

impl NetworksApi {
    /// Wraps `transport` as a `NetworksApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the list of networks on the account — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/networks.py:79-95` (`NetworksAPI.get_networks`).
    /// Sends `GET` [`crate::routes::GET_NETWORKS`] (a fixed path). Unchanged since `v6.2.0`; no
    /// `parent`.
    pub async fn get_networks(&self) -> Result<Envelope, Error> {
        self.transport
            .resource(&routes::GET_NETWORKS, "", None, &[], RequestBody::None)
            .await
    }

    /// Resolves a network's own URL: `_network_own_url` (`networks.py:47-69`), shared by
    /// [`NetworksApi::get_network`] and [`NetworksApi::get_premium_status`].
    ///
    /// Prefers `parent`'s own top-level `url` field ([`crate::links::self_url`]) over the
    /// `networks/{id}` template — **not** a named `resources.<link>` lookup, so this does not go
    /// through [`crate::routes::Resource::resolve`]; see [`crate::routes::networks::NETWORKS_GET_NETWORK`]'s
    /// doc comment for why. Exactly [`crate::params::resolve_network_url`]'s behaviour.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] as [`resolve_network_url`].
    fn network_own_url(&self, network_id: &str, parent: Option<&Value>) -> Result<Url, Error> {
        resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            ApiVersion::V2_2,
        )
    }

    /// Gets a single network's full object — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/networks.py:97-121` (`NetworksAPI.get_network`).
    /// `network_id` accepts a bare id, a host-relative path, or an absolute API-host URL
    /// (id/path/URL polymorphism, `crate::links::resource_url`); `parent`, when supplied, is
    /// preferred via its own `url` field — see `NetworksApi::network_own_url`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise as [`crate::transport::Transport::request`].
    pub async fn get_network(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.network_own_url(network_id, parent)?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Gets Eero Plus/Eero Secure subscription status for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/networks.py:123-150`
    /// (`NetworksAPI.get_premium_status`). The exact same wire call as
    /// [`NetworksApi::get_network`] — see `NetworksApi::network_own_url` — returning the full
    /// network object; downstream clients are expected to extract `premium_status`, `eero_plus`
    /// or `premium_dns` themselves.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::get_network`].
    pub async fn get_premium_status(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.network_own_url(network_id, parent)?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Gets account information — returns the raw Eero API response.
    ///
    /// **Deprecated**: `AccountApi::get_account` is the same `GET` on the account resource,
    /// ported from `AccountAPI.get_account` (`eero-api src/eero/api/account.py:52-70`), which is
    /// where `EeroClient.get_account` (`client.py:581-598`) reads it at v8.4.1. This method is
    /// kept so existing callers keep compiling. Sends `GET` [`crate::routes::ACCOUNT`] (a fixed
    /// path), unchanged on the wire.
    #[deprecated(note = "use AccountApi::get_account")]
    pub async fn get_account(&self) -> Result<Envelope, Error> {
        self.transport
            .resource(&routes::ACCOUNT, "", None, &[], RequestBody::None)
            .await
    }

    /// `POST /2.2/networks/{id}/reboot` — reboot every Eero node on the network.
    ///
    /// Ported from `NetworksAPI.reboot_network` (`networks.py:152-185`). Sends the literal
    /// two-byte body `""` ([`RequestBody::EmptyJsonString`]) — **not** `{}` (the shape this crate
    /// sent pre-v8.0.4) — to [`crate::routes::networks::REBOOT_NETWORK_V8`], preferring `parent`'s
    /// own published `reboot` link. Unverified against a live account: logs one `WARNING` via
    /// [`warn_uncharacterised_write`] before issuing the request (`networks.py:181`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise as [`crate::transport::Transport::resource`].
    pub async fn reboot_network(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::networks::REBOOT_NETWORK_V8.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("reboot network");
        self.transport
            .request(
                routes::networks::REBOOT_NETWORK_V8.method.clone(),
                url,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// `POST /2.2/networks/{id}/speedtest` — run a speed test on the network.
    ///
    /// Ported from `NetworksAPI.run_speed_test` (`networks.py:187-222`). Sends the literal
    /// two-byte body `""` to [`crate::routes::networks::RUN_SPEED_TEST_V8`], preferring `parent`'s
    /// own published `speedtest` link. **Live-verified 2026-09-20** (HTTP 202, `data: null`) — no
    /// `warn_uncharacterised_write`.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn run_speed_test(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::networks::RUN_SPEED_TEST_V8,
                network_id,
                parent,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// Gets past speed-test results — returns the raw Eero API response.
    ///
    /// Ported from `NetworksAPI.get_speed_tests` (`networks.py:224-273`). Sends `GET`
    /// [`crate::routes::networks::NETWORKS_GET_SPEED_TESTS`] (the same `speedtest` sub-resource
    /// [`NetworksApi::run_speed_test`] posts to), preferring `parent`'s own published `speedtest`
    /// link. `limit` is sent as `limit`, `start_time`/`end_time` as `startTime`/`endTime` — each
    /// omitted from the query entirely when `None`; the request is always
    /// sent with an explicit (possibly empty) query slice, never a bare "no params at all"
    /// distinction Rust has no way to make anyway.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::get_network`].
    pub async fn get_speed_tests(
        &self,
        network_id: &str,
        limit: Option<u32>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(limit) = limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(start_time) = start_time {
            query.push(("startTime", start_time.to_owned()));
        }
        if let Some(end_time) = end_time {
            query.push(("endTime", end_time.to_owned()));
        }
        self.transport
            .resource(
                &routes::networks::NETWORKS_GET_SPEED_TESTS,
                network_id,
                parent,
                &query,
                RequestBody::None,
            )
            .await
    }

    /// `PUT /2.2/networks/{id}/settings` — set the network name (SSID).
    ///
    /// Ported from `NetworksAPI.set_network_name` (`networks.py:275-314`). Sends a **form-encoded**
    /// body (`name=<value>`, [`RequestBody::Form`]) to
    /// [`crate::routes::networks::NETWORK_SETTINGS_FORM`] — a distinct, form-encoded constant
    /// from the JSON settings routes DNS/security/SQM's own setters PUT onto — preferring
    /// `parent`'s own published `settings` link. `name` is forwarded exactly as given (no
    /// trimming). Unverified against a live account in this exact (form-encoded) shape: logs one
    /// `WARNING` via [`warn_uncharacterised_write`] before issuing the request
    /// (`networks.py:313`).
    ///
    /// Python also rejects a `name` that is not a `str`; the `&str` parameter makes that
    /// unrepresentable here, so only the emptiness half of the rule remains.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `name` if `name` is empty. That check is the first
    /// statement (`networks.py:305`), so it wins over an unresolvable `network_id`, a missing
    /// session and the uncharacterised-write warning, and nothing is sent. Otherwise see
    /// [`NetworksApi::reboot_network`].
    pub async fn set_network_name(
        &self,
        network_id: &str,
        name: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        require_non_empty_string("name", name)?;
        let url = routes::networks::NETWORK_SETTINGS_FORM.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("set network name for network");
        self.transport
            .request(
                routes::networks::NETWORK_SETTINGS_FORM.method.clone(),
                url,
                &[],
                RequestBody::Form(vec![("name".to_owned(), name.to_owned())]),
            )
            .await
    }

    /// `PUT /2.2/networks/{id}/password` — set the network's Wi-Fi password.
    ///
    /// Ported from `NetworksAPI.set_network_password` (`networks.py:316-354`). Sends a
    /// form-encoded body (`password=<value>`) to
    /// [`crate::routes::networks::SET_NETWORK_PASSWORD`], preferring `parent`'s own published
    /// `password` link. `password` is never logged and is forwarded exactly as given (no
    /// trimming, so a whitespace-only value is accepted, as in Python). Unverified: logs one
    /// `WARNING` via [`warn_uncharacterised_write`] before issuing the request
    /// (`networks.py:353`).
    ///
    /// Python also rejects a `password` that is not a `str` (`None`, `False`, `0`, `bytes`, a list,
    /// a dict); the `&str` parameter makes those unrepresentable here, so only the emptiness half
    /// of the rule remains.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `password` if `password` is empty. That check is
    /// the first statement (`networks.py:345`), so it wins over an unresolvable `network_id`, a
    /// missing session and the uncharacterised-write warning, and nothing is sent. Otherwise see
    /// [`NetworksApi::reboot_network`].
    pub async fn set_network_password(
        &self,
        network_id: &str,
        password: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        require_non_empty_string("password", password)?;
        let url = routes::networks::SET_NETWORK_PASSWORD.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("set network password for network");
        self.transport
            .request(
                routes::networks::SET_NETWORK_PASSWORD.method.clone(),
                url,
                &[],
                RequestBody::Form(vec![("password".to_owned(), password.to_owned())]),
            )
            .await
    }

    /// `DELETE /2.2/networks/{id}/password` — clear the network's Wi-Fi password.
    ///
    /// Ported from `NetworksAPI.clear_network_password` (`networks.py:356-389`). Sends `DELETE`
    /// [`crate::routes::networks::CLEAR_NETWORK_PASSWORD`], preferring `parent`'s own published
    /// `password` link. Unverified: logs one `WARNING` via [`warn_uncharacterised_write`] before
    /// issuing the request (`networks.py:388`).
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn clear_network_password(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::networks::CLEAR_NETWORK_PASSWORD.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("clear network password for network");
        self.transport
            .request(
                routes::networks::CLEAR_NETWORK_PASSWORD.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Gets guest network configuration — returns the raw Eero API response.
    ///
    /// Ported from `NetworksAPI.get_guest_network` (`networks.py:391-423`). Sends `GET`
    /// [`crate::routes::networks::GET_GUEST_NETWORK`], preferring `parent`'s own published
    /// `guestnetwork` link.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::get_network`].
    pub async fn get_guest_network(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::networks::GET_GUEST_NETWORK,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `PUT /2.2/networks/{id}/guestnetwork` — enable/disable/rename the guest network.
    ///
    /// Ported from `NetworksAPI.set_guest_network` (`networks.py:425-481`). Sends a form-encoded
    /// body: `enabled=<"true"|"false">` always, `+ name=<value>` only when `name` is `Some`
    /// (omitted entirely when `None`; a supplied name must be non-empty) — preferring `parent`'s
    /// own published `guestnetwork` link. **`password` is gone** at v8.0.4 (breaking change vs.
    /// `v6.2.0`; see [`NetworksApi::set_guest_password`]/[`NetworksApi::clear_guest_password`]).
    /// **Live-verified 2026-09-20** (enable/disable read back correctly, name unchanged) — no
    /// `warn_uncharacterised_write`.
    ///
    /// Python also requires `enabled` to be a real `bool` and a supplied `name` to be a `str`; the
    /// `bool` and `&str` parameters enforce both at compile time, so only the emptiness check on
    /// `name` remains at runtime.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `name` if `name` is `Some("")`
    /// (`networks.py:465-466`), before the URL is resolved or anything is sent. Otherwise see
    /// [`NetworksApi::reboot_network`].
    pub async fn set_guest_network(
        &self,
        network_id: &str,
        enabled: bool,
        name: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        if let Some(name) = name {
            require_non_empty_string("name", name)?;
        }
        let mut pairs = vec![(
            "enabled".to_owned(),
            (if enabled { "true" } else { "false" }).to_owned(),
        )];
        if let Some(name) = name {
            pairs.push(("name".to_owned(), name.to_owned()));
        }
        self.transport
            .resource(
                &routes::networks::SET_GUEST_NETWORK_V8,
                network_id,
                parent,
                &[],
                RequestBody::Form(pairs),
            )
            .await
    }

    /// Resolves the guest network's password sub-resource URL: `_guest_password_url`
    /// (`networks.py:28-44`), shared by [`NetworksApi::set_guest_password`] and
    /// [`NetworksApi::clear_guest_password`].
    ///
    /// **`guest_parent` is the guest network's own cached envelope (the result of
    /// [`NetworksApi::get_guest_network`]), not the network's own envelope** — a caller who
    /// passes the network envelope here gets no error: `resolve_link` simply finds no `password`
    /// key under that envelope's `resources` and falls back to the literal template
    /// (`networks.py:503-507` docstring).
    fn guest_password_url(
        &self,
        network_id: &str,
        guest_parent: Option<&Value>,
    ) -> Result<Url, Error> {
        routes::networks::SET_GUEST_PASSWORD.resolve(
            self.transport.api_host(),
            network_id,
            guest_parent,
        )
    }

    /// `PUT /2.2/networks/{id}/guestnetwork/password` — set the guest network's password.
    ///
    /// Ported from `NetworksAPI.set_guest_password` (`networks.py:483-523`). Sends a form-encoded
    /// body (`password=<value>`), preferring `parent`'s (the guest envelope's) own published
    /// `password` link — see `NetworksApi::guest_password_url`. `password` is never logged and is
    /// forwarded exactly as given. **Live-verified 2026-09-20** — no
    /// `warn_uncharacterised_write`.
    ///
    /// As for [`NetworksApi::set_network_password`], only the emptiness half of Python's
    /// non-empty-`str` rule applies, because the parameter is a `&str`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `password` if `password` is empty. That check is
    /// the first statement (`networks.py:517`), ahead of the URL resolution and the credential
    /// gate, and nothing is sent. Otherwise see [`NetworksApi::reboot_network`].
    pub async fn set_guest_password(
        &self,
        network_id: &str,
        password: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        require_non_empty_string("password", password)?;
        let url = self.guest_password_url(network_id, parent)?;
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Form(vec![("password".to_owned(), password.to_owned())]),
            )
            .await
    }

    /// `DELETE /2.2/networks/{id}/guestnetwork/password` — clear the guest network's password.
    ///
    /// Ported from `NetworksAPI.clear_guest_password` (`networks.py:525-560`). Preferring
    /// `parent`'s (the guest envelope's) own published `password` link — see
    /// `NetworksApi::guest_password_url`. **Live-verified 2026-09-20** — no
    /// `warn_uncharacterised_write`.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn clear_guest_password(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.guest_password_url(network_id, parent)?;
        self.transport
            .request(Method::DELETE, url, &[], RequestBody::None)
            .await
    }

    /// `PUT`/`DELETE /2.2/networks/{id}/temporary_flags/hide_5g` — temporarily hide (pause) the
    /// network's 5 GHz band, or clear that flag.
    ///
    /// Ported from `NetworksAPI.set_hide_5g` (`networks.py:562-598`). With `enabled` it sends a
    /// JSON `PUT` of exactly `{"value": true}` to
    /// [`crate::routes::networks::NETWORKS_SET_HIDE_5G`] (`networks.py:594-596`); without it, a
    /// body-less `DELETE` to [`crate::routes::networks::NETWORKS_CLEAR_HIDE_5G`]
    /// (`networks.py:597-598`). The flag is temporary: the API expires it on its own, and the
    /// network envelope's `temporary_flags.hide_5g` carries the current `value` and its
    /// expiration. The network envelope publishes no link to this resource, so the path is
    /// always built from the template and there is no `parent` parameter; `network_id` may be a
    /// bare id, a host-relative API path or an absolute API-host URL.
    ///
    /// **This write disconnects 5 GHz clients while it takes effect, and is unverified against a
    /// live account.** Read the current flag first, skip the write when it already matches, and
    /// never retry a failed write in a loop. Logs one `WARNING` via
    /// [`warn_uncharacterised_write`] after URL resolution and immediately before the request:
    /// `"hide 5 GHz for network"` when enabling, `"clear hide 5 GHz for network"` when clearing.
    ///
    /// Python validates `enabled` as a real `bool` at runtime (`require_bool`); the `bool`
    /// parameter enforces that at compile time, so there is no validation error path here.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id` cannot be resolved to a URL, before the
    /// warning is logged or anything is sent. Otherwise [`Error::Authentication`] if no valid
    /// session is configured (raised by the transport, which — unlike Python — runs after URL
    /// resolution and the warning), or whatever status-mapped [`Error`] the request produces.
    pub async fn set_hide_5g(&self, network_id: &str, enabled: bool) -> Result<Envelope, Error> {
        if enabled {
            let url = routes::networks::NETWORKS_SET_HIDE_5G.resolve(
                self.transport.api_host(),
                network_id,
                None,
            )?;
            warn_uncharacterised_write("hide 5 GHz for network");
            self.transport
                .request(
                    routes::networks::NETWORKS_SET_HIDE_5G.method.clone(),
                    url,
                    &[],
                    RequestBody::Json(json!({ "value": true })),
                )
                .await
        } else {
            let url = routes::networks::NETWORKS_CLEAR_HIDE_5G.resolve(
                self.transport.api_host(),
                network_id,
                None,
            )?;
            warn_uncharacterised_write("clear hide 5 GHz for network");
            self.transport
                .request(
                    routes::networks::NETWORKS_CLEAR_HIDE_5G.method.clone(),
                    url,
                    &[],
                    RequestBody::None,
                )
                .await
        }
    }
}
