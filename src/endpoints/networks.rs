//! Networks API: the read-only (`GET`) half of `eero-api`'s `NetworksAPI`, plus the
//! client-level `get_account`.
//!
//! Ported from `eero-api src/eero/api/networks.py`. This phase (3, GET-only) covers
//! `NetworksAPI.get_networks`, `NetworksAPI.get_network` and `NetworksAPI.get_premium_status`.
//! `Client.get_account` (`eero-api src/eero/client.py:238-256`) is also ported here rather than
//! on a not-yet-built `Client` facade: `client.py:252-254` calls `self._api.auth.get("/account",
//! ...)` directly — the same bare, uncached network call as every other method in this file —
//! and the port plan gives the eventual `rusteero::Client` its own cache layer on top (port plan
//! §1.6), so the raw call belongs next to `networks`/`account`'s other raw `GET`s, not duplicated
//! there.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `NetworksAPI` (`src/eero/api/networks.py`), plus
/// `Client.get_account`.
///
/// Build one with [`NetworksApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `NetworksApi` never constructs or owns a
/// `Transport` itself.
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
    /// Ported from `eero-api src/eero/api/networks.py:33-49` (`NetworksAPI.get_networks`).
    /// Sends `GET` [`crate::routes::GET_NETWORKS`]. The response's `data` field may hold a
    /// `networks` list or another shape entirely, exactly as Python leaves it — this method
    /// never inspects or reshapes it.
    pub async fn get_networks(&self) -> Result<Envelope, Error> {
        self.transport.send(&routes::GET_NETWORKS, &[], None).await
    }

    /// Gets a single network's full object — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/networks.py:51-69` (`NetworksAPI.get_network`).
    /// Sends `GET` [`crate::routes::GET_NETWORK`] with `network_id` substituted into the path.
    pub async fn get_network(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_NETWORK, &[("network_id", network_id)], None)
            .await
    }

    /// Gets Eero Plus/Eero Secure subscription status for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/networks.py:159-180`
    /// (`NetworksAPI.get_premium_status`). Sends `GET` [`crate::routes::GET_PREMIUM_STATUS`],
    /// an alias of [`crate::routes::GET_NETWORK`] — this is the exact same wire call as
    /// [`NetworksApi::get_network`], returning the full network object; Python's own docstring
    /// notes downstream clients are expected to extract `premium_status`, `eero_plus` or
    /// `premium_dns` themselves, and this port does the same (no extraction here).
    pub async fn get_premium_status(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_PREMIUM_STATUS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// Gets account information — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/client.py:238-256` (`Client.get_account`), specifically
    /// the uncached network call at `client.py:252-254`
    /// (`self._api.auth.get("/account", ...)`); the surrounding `refresh_cache`/cache-read/
    /// cache-write logic (`client.py:246-251,256`) belongs to the not-yet-built `Client` facade
    /// (port plan §1.6), not this network-facing layer. Sends `GET` [`crate::routes::ACCOUNT`].
    pub async fn get_account(&self) -> Result<Envelope, Error> {
        self.transport.send(&routes::ACCOUNT, &[], None).await
    }

    /// `PUT /2.2/networks/{network_id}/guestnetwork` — enable/disable/configure the guest
    /// network.
    ///
    /// Ported from `NetworksAPI.set_guest_network` (`networks.py:71-109`). Sends
    /// `routes::SET_GUEST_NETWORK` with body `{"enabled": enabled}` (`networks.py:97`), plus
    /// `{"name": ...}` and/or `{"password": ...}` only when `name`/`password` are `Some`
    /// (`networks.py:99-103`) — neither key is ever sent as `null` for an omitted argument.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_guest_network(
        &self,
        network_id: &str,
        enabled: bool,
        name: Option<&str>,
        password: Option<&str>,
    ) -> Result<Envelope, Error> {
        let mut payload = Map::new();
        payload.insert("enabled".to_owned(), Value::Bool(enabled));
        if let Some(name) = name {
            payload.insert("name".to_owned(), Value::String(name.to_owned()));
        }
        if let Some(password) = password {
            payload.insert("password".to_owned(), Value::String(password.to_owned()));
        }
        self.transport
            .send(
                &routes::SET_GUEST_NETWORK,
                &[("network_id", network_id)],
                Some(Value::Object(payload)),
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/speedtest` — run a speed test on the network.
    ///
    /// Ported from `NetworksAPI.run_speed_test` (`networks.py:111-132`). Sends
    /// `routes::RUN_SPEED_TEST` with an empty body `{}` (`networks.py:131`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn run_speed_test(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::RUN_SPEED_TEST,
                &[("network_id", network_id)],
                Some(json!({})),
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/reboot` — reboot every Eero node on the network.
    ///
    /// Ported from `NetworksAPI.reboot_network` (`networks.py:134-157`). Sends
    /// `routes::REBOOT_NETWORK` with an empty body `{}` (`networks.py:156`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn reboot_network(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::REBOOT_NETWORK,
                &[("network_id", network_id)],
                Some(json!({})),
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/settings` — set the network name (SSID).
    ///
    /// Ported from `NetworksAPI.set_network_name` (`networks.py:182-208`). Sends
    /// `{"name": name}` (`networks.py:207`) through `put_network_settings`, the shared call site
    /// this crate factors out of `routes::PUT_NETWORK_SETTINGS`'s dozen-plus Python setters —
    /// see that function's own doc comment for the full list of callers across `DnsApi`,
    /// `SecurityApi` and `SqmApi`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_network_name(&self, network_id: &str, name: &str) -> Result<Envelope, Error> {
        put_network_settings(&self.transport, network_id, json!({ "name": name })).await
    }
}

/// Shared `PUT /2.2/networks/{network_id}/settings` call site behind every setter in
/// `NetworksApi`, `DnsApi`, `SecurityApi` and `SqmApi` that targets `routes::PUT_NETWORK_SETTINGS`
/// — see that route constant's own doc comment for the full list of Python setters it aliases.
/// Centralising the call here, rather than repeating
/// `transport.send(&routes::PUT_NETWORK_SETTINGS, &[("network_id", network_id)], Some(body))`
/// across four files, means a future change to this one resource's request shape only needs to
/// be made in one place.
///
/// `pub(crate)`, not `pub`: this is plumbing shared across `src/endpoints/`, not part of the
/// domain-module API surface any of the four `*Api` structs expose to a caller. It has no direct
/// Python original — Python repeats the equivalent `self.put(f"networks/{network_id}/settings",
/// ...)` call inline in every one of the fifteen setters this function replaces.
pub(crate) async fn put_network_settings(
    transport: &Transport,
    network_id: &str,
    body: Value,
) -> Result<Envelope, Error> {
    transport
        .send(
            &routes::PUT_NETWORK_SETTINGS,
            &[("network_id", network_id)],
            Some(body),
        )
        .await
}
