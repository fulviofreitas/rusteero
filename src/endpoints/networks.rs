//! Networks API: `eero-api`'s `NetworksAPI`, at v8.0.4, plus the client-level `get_account`.
//!
//! Ported from `eero-api src/eero/api/networks.py` (v8.0.4). Every method accepts an optional
//! keyword-only `parent` — the caller's own cached network (or, for the guest-network password
//! methods, guest-network) envelope — so the link the API published on it is used instead of a
//! locally-built template; see each method's own doc comment for exactly which envelope it
//! expects.
//!
//! `Client.get_account` (`eero-api src/eero/client.py:392-406`) is also ported here rather than
//! on `Client`: `client.py:403-404` calls `self._api.auth.get("/account", ...)` directly — the
//! same bare, uncached network call as every other method in this file — and `rusteero::Client`
//! gives its own cache layer on top, so the raw call belongs next to
//! `networks`/`account`'s other raw `GET`s, not duplicated there. Unchanged at v8.0.4.
//!
//! Every method here funnels through [`crate::transport::Transport::request`]/`resource`, which
//! already implement the "not authenticated" precondition Python repeats at the top of each
//! method (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method below
//! duplicates that guard.

use std::sync::Arc;

use reqwest::Method;
use serde_json::Value;
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::params::resolve_network_url;
use crate::routes::{self, ApiVersion};
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `NetworksAPI` (`src/eero/api/networks.py`), at v8.0.4, plus `Client.get_account`.
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
    /// Ported from `eero-api src/eero/api/networks.py:78-93` (`NetworksAPI.get_networks`).
    /// Sends `GET` [`crate::routes::GET_NETWORKS`] (a fixed path). Unchanged since `v6.2.0`; no
    /// `parent`.
    pub async fn get_networks(&self) -> Result<Envelope, Error> {
        self.transport
            .resource(&routes::GET_NETWORKS, "", None, &[], RequestBody::None)
            .await
    }

    /// Resolves a network's own URL: `_network_own_url` (`networks.py:46-59`), shared by
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
    /// Ported from `eero-api src/eero/api/networks.py:96-121` (`NetworksAPI.get_network`).
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
    /// Ported from `eero-api src/eero/api/networks.py:122-149`
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
    /// Ported from `eero-api src/eero/client.py:392-406` (`Client.get_account`), specifically the
    /// uncached network call at `client.py:403-404` (`self._api.auth.get("/account", ...)`); the
    /// surrounding `refresh_cache`/cache-read/cache-write logic belongs to `Client`. Sends `GET`
    /// [`crate::routes::ACCOUNT`] (a fixed path). Unchanged at v8.0.4.
    pub async fn get_account(&self) -> Result<Envelope, Error> {
        self.transport
            .resource(&routes::ACCOUNT, "", None, &[], RequestBody::None)
            .await
    }

    /// `POST /2.2/networks/{id}/reboot` — reboot every Eero node on the network.
    ///
    /// Ported from `NetworksAPI.reboot_network` (`networks.py:151-184`). Sends the literal
    /// two-byte body `""` ([`RequestBody::EmptyJsonString`]) — **not** `{}` (the shape this crate
    /// sent pre-v8.0.4) — to [`crate::routes::networks::REBOOT_NETWORK_V8`], preferring `parent`'s
    /// own published `reboot` link. Unverified against a live account: logs one `WARNING` via
    /// [`warn_uncharacterised_write`] before issuing the request (`networks.py:170`).
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
    /// Ported from `NetworksAPI.run_speed_test` (`networks.py:186-219`). Sends the literal
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
    /// Ported from `NetworksAPI.get_speed_tests` (`networks.py:223-267`). Sends `GET`
    /// [`crate::routes::networks::NETWORKS_GET_SPEED_TESTS`] (the same `speedtest` sub-resource
    /// [`NetworksApi::run_speed_test`] posts to), preferring `parent`'s own published `speedtest`
    /// link. `limit` is sent as `limit`, `start_time`/`end_time` as `startTime`/`endTime` — each
    /// omitted from the query entirely when `None` (`networks.py:257-263`); the request is always
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
    /// Ported from `NetworksAPI.set_network_name` (`networks.py:269-312`). Sends a **form-encoded**
    /// body (`name=<value>`, [`RequestBody::Form`]) to
    /// [`crate::routes::networks::NETWORK_SETTINGS_FORM`] — a distinct, form-encoded constant
    /// from the JSON settings routes DNS/security/SQM's own setters PUT onto — preferring
    /// `parent`'s own published `settings` link. Unverified against a live account in this exact
    /// (form-encoded) shape: logs one `WARNING` via [`warn_uncharacterised_write`] before issuing
    /// the request (`networks.py:307`).
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn set_network_name(
        &self,
        network_id: &str,
        name: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
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
    /// Ported from `NetworksAPI.set_network_password` (`networks.py:313-351`). Sends a
    /// form-encoded body (`password=<value>`) to
    /// [`crate::routes::networks::SET_NETWORK_PASSWORD`], preferring `parent`'s own published
    /// `password` link. `password` is never logged. Unverified: logs one `WARNING` via
    /// [`warn_uncharacterised_write`] before issuing the request (`networks.py:348`).
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn set_network_password(
        &self,
        network_id: &str,
        password: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
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
    /// Ported from `NetworksAPI.clear_network_password` (`networks.py:353-384`). Sends `DELETE`
    /// [`crate::routes::networks::CLEAR_NETWORK_PASSWORD`], preferring `parent`'s own published
    /// `password` link. Unverified: logs one `WARNING` via [`warn_uncharacterised_write`] before
    /// issuing the request (`networks.py:381`).
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
    /// Ported from `NetworksAPI.get_guest_network` (`networks.py:386-418`). Sends `GET`
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
    /// Ported from `NetworksAPI.set_guest_network` (`networks.py:420-471`). Sends a form-encoded
    /// body: `enabled=<"true"|"false">` always, `+ name=<value>` only when `name` is `Some`
    /// (never sent, not even empty, when `None`) — preferring `parent`'s own published
    /// `guestnetwork` link. **`password` is gone** at v8.0.4 (breaking change vs. `v6.2.0`; see
    /// [`NetworksApi::set_guest_password`]/[`NetworksApi::clear_guest_password`]).
    /// **Live-verified 2026-09-20** (enable/disable read back correctly, name unchanged) — no
    /// `warn_uncharacterised_write`.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn set_guest_network(
        &self,
        network_id: &str,
        enabled: bool,
        name: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
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
    /// (`networks.py:27-42`), shared by [`NetworksApi::set_guest_password`] and
    /// [`NetworksApi::clear_guest_password`].
    ///
    /// **`guest_parent` is the guest network's own cached envelope (the result of
    /// [`NetworksApi::get_guest_network`]), not the network's own envelope** — a caller who
    /// passes the network envelope here gets no error: `resolve_link` simply finds no `password`
    /// key under that envelope's `resources` and falls back to the literal template
    /// (`networks.py:481-483` docstring).
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
    /// Ported from `NetworksAPI.set_guest_password` (`networks.py:473-511`). Sends a form-encoded
    /// body (`password=<value>`), preferring `parent`'s (the guest envelope's) own published
    /// `password` link — see `NetworksApi::guest_password_url`. `password` is never logged.
    /// **Live-verified 2026-09-20** — no `warn_uncharacterised_write`.
    ///
    /// # Errors
    ///
    /// See [`NetworksApi::reboot_network`].
    pub async fn set_guest_password(
        &self,
        network_id: &str,
        password: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
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
    /// Ported from `NetworksAPI.clear_guest_password` (`networks.py:513-548`). Preferring
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
}
