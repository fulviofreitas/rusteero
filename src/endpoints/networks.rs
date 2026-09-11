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

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): NetworksAPI's mutation methods go here —
    // `set_guest_network` (`networks.py:71-109`), `run_speed_test` (`networks.py:111-132`),
    // `reboot_network` (`networks.py:134-157`) and `set_network_name` (`networks.py:182-208`).
    // Each already has a `Route` constant in `src/routes.rs`
    // (`SET_GUEST_NETWORK`/`RUN_SPEED_TEST`/`REBOOT_NETWORK`/`SET_NETWORK_NAME`).
    // ---------------------------------------------------------------------------------------
}
