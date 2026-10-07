//! `Client` methods for the `NetworksAPI` domain (plus the `get_premium_status` alias read).
//!
//! **No `reboot_network` wrapper.** `v8.0.4`'s `client.py` has never wrapped
//! `NetworksAPI.reboot_network` (`api/networks.py:151`) on `EeroClient` at all — an earlier
//! version of this crate added one anyway, with no Python precedent to cite; removed to match
//! `client.py` exactly. [`crate::endpoints::networks::NetworksApi::reboot_network`]
//! itself is unaffected and still callable directly through [`crate::api::EeroApi`].

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets a network's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_network()` (`client.py:489-513`). Resolves `network_id` with
    /// `auto_discover = true` (Python's default, since this call omits the argument), matching
    /// every other cached getter that takes a network id. This call *populates* `network[nid]`;
    /// it never passes `parent=` itself.
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
        let response = self.api.networks().get_network(&network_id, None).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets Eero Plus/Eero Secure subscription status — returns the raw Eero API response.
    ///
    /// Ported from `get_premium_status()` (`client.py:515-519`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. Passes the
    /// cached network envelope as `parent=` (`+net`, `_network_parent_kwargs`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_premium_status(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .networks()
            .get_premium_status(&network_id, parent.as_ref())
            .await
    }

    // ==================== Networks ====================

    /// Enables, disables or reconfigures the guest network — returns the raw Eero API response.
    ///
    /// Ported from `set_guest_network` (`eero-api src/eero/client.py:1127-1158`). Resolves
    /// `network_id` with `auto_discover = true`. **`password=` is gone since `v6.2.0`** — split
    /// into [`Client::set_guest_password`]/[`Client::clear_guest_password`]. Passes the cached
    /// network envelope as `parent=` (`+net`). On success, invalidates `network[nid]`.
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
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .networks()
            .set_guest_network(&network_id, enabled, name, parent.as_ref())
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Gets guest network configuration — returns the raw Eero API response.
    ///
    /// Ported from `get_guest_network()` (`client.py:1120-1125`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`).
    /// Never cached (`get_guest_network` is not one of the eight cached getters).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_guest_network(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .networks()
            .get_guest_network(&network_id, parent.as_ref())
            .await
    }

    /// Sets the guest network's password — returns the raw Eero API response.
    ///
    /// Ported from `set_guest_password()` (`client.py:1159-1171`). `auto_discover = false`.
    /// **No `parent=` is passed** — Python's own wrapper calls the domain method with no parent
    /// kwargs at all (`client.py:1170`, unlike its sibling [`Client::set_guest_network`], which
    /// does), even though the domain method accepts one; reproduced exactly. On success,
    /// invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn set_guest_password(
        &self,
        password: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .networks()
            .set_guest_password(&network_id, password, None)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Clears the guest network's password — returns the raw Eero API response.
    ///
    /// Ported from `clear_guest_password()` (`client.py:1173-1185`). `auto_discover = false`, no
    /// `parent=` — see [`Client::set_guest_password`]. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn clear_guest_password(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .networks()
            .clear_guest_password(&network_id, None)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Runs a speed test on the network — returns the raw Eero API response.
    ///
    /// Ported from `run_speed_test` (`eero-api src/eero/client.py:1188-1206`). `auto_discover =
    /// true` — see [`Client::set_guest_network`]. Passes the cached network envelope as `parent=`
    /// (`+net`). On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn run_speed_test(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .networks()
            .run_speed_test(&network_id, parent.as_ref())
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Gets past speed-test results — returns the raw Eero API response.
    ///
    /// Ported from `get_speed_tests()` (`client.py:1208-1226`). `auto_discover = false`. Passes
    /// the cached network envelope as `parent=` (`+net`). Never cached (time-windowed read).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_speed_tests(
        &self,
        network_id: Option<&str>,
        limit: Option<u32>,
        start_time: Option<&str>,
        end_time: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .networks()
            .get_speed_tests(&network_id, limit, start_time, end_time, parent.as_ref())
            .await
    }

    /// Sets the network name (SSID) — returns the raw Eero API response.
    ///
    /// Ported from `set_network_name` (`eero-api src/eero/client.py:522-529`). Resolves
    /// `network_id` with `auto_discover = false` — see [`Client::get_diagnostics`]. Passes the
    /// cached network envelope as `parent=` (`+net`). On success, invalidates `network[nid]`.
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
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .networks()
            .set_network_name(&network_id, name, parent.as_ref())
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets the network's Wi-Fi password — returns the raw Eero API response.
    ///
    /// Ported from `set_network_password()` (`client.py:531-546`). `auto_discover = false`.
    /// Passes the cached network envelope as `parent=` (`+net`). On success, invalidates
    /// `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::set_network_name`].
    pub async fn set_network_password(
        &self,
        password: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .networks()
            .set_network_password(&network_id, password, parent.as_ref())
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Clears the network's Wi-Fi password — returns the raw Eero API response.
    ///
    /// Ported from `clear_network_password()` (`client.py:548-561`). `auto_discover = false`.
    /// Passes the cached network envelope as `parent=` (`+net`). On success, invalidates
    /// `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::set_network_name`].
    pub async fn clear_network_password(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .networks()
            .clear_network_password(&network_id, parent.as_ref())
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }
}
