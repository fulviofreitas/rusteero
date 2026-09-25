//! `Client` methods for the `NetworksAPI` domain (plus the `get_premium_status` alias read).

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets a network's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_network()` (`client.py:333-357`). Resolves `network_id` with
    /// `auto_discover = true` (Python's default, since this call omits the argument), matching
    /// every other cached getter that takes a network id.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no network can be resolved
    /// (see `Client::ensure_network_id`). Otherwise, whatever status-mapped [`Error`] the
    /// request produces.
    pub async fn get_network(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::network(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self.api.networks().get_network(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets Eero Plus/Eero Secure subscription status — returns the raw Eero API response.
    ///
    /// Ported from `get_premium_status()` (`client.py:1015-1018`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_premium_status(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.networks().get_premium_status(&network_id).await
    }

    // ==================== Networks ====================

    /// Enables, disables or reconfigures the guest network — returns the raw Eero API response.
    ///
    /// Ported from `set_guest_network` (`eero-api src/eero/client.py:740-766`). Resolves
    /// `network_id` with `auto_discover = true` (Python's default; this call sits before the
    /// `client.py:809` "Diagnostics & Settings" boundary — see `Client::get_diagnostics`'s docs
    /// for that boundary and every method after it). On success, invalidates `network[nid]`
    /// (`client.py:763-764`).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no network can be resolved.
    /// Otherwise, whatever status-mapped [`Error`] the request produces. The cache is left
    /// untouched on any `Err`.
    pub async fn set_guest_network(
        &self,
        enabled: bool,
        name: Option<&str>,
        password: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .networks()
            .set_guest_network(&network_id, enabled, name, password)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Runs a speed test on the network — returns the raw Eero API response.
    ///
    /// Ported from `run_speed_test` (`eero-api src/eero/client.py:770-787`). `auto_discover =
    /// true` — see [`Client::set_guest_network`]. On success, invalidates `network[nid]`
    /// (`client.py:784-785`).
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn run_speed_test(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self.api.networks().run_speed_test(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Reboots every Eero node on the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent: `eero-api` has never wrapped `NetworksAPI.reboot_network`
    /// (`api/networks.py:134`) on `EeroClient` at all, so there is nothing to port faithfully
    /// here — this method exists only because [`crate::endpoints::NetworksApi::reboot_network`]
    /// does. `auto_discover = true`, matching its two closest siblings,
    /// [`Client::set_guest_network`] and [`Client::run_speed_test`] — both plain, id-only network
    /// actions resolved the same way. On success, invalidates `network[nid]`, for the same reason
    /// `run_speed_test` does even though neither is a settings write in the DNS/SQM/security
    /// sense: a reboot is a network-wide action indistinguishable in kind from its two siblings,
    /// both of which drop this same key. This is a judgement call, not a brief citation — there is
    /// no Python behaviour here to diverge from or match.
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn reboot_network(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self.api.networks().reboot_network(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets the network name (SSID) — returns the raw Eero API response.
    ///
    /// Ported from `set_network_name` (`eero-api src/eero/client.py:1020-1028`). Unlike its three
    /// Networks-domain siblings above, this call is **after** the `client.py:809` boundary and so
    /// resolves `network_id` with `auto_discover = false` — see [`Client::get_diagnostics`]. On
    /// success, invalidates `network[nid]` (`client.py:1025-1026`).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set —
    /// auto-discovery is not attempted. Otherwise, whatever status-mapped [`Error`] the request
    /// produces. The cache is left untouched on any `Err`.
    pub async fn set_network_name(
        &self,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .networks()
            .set_network_name(&network_id, name)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }
}
