//! `Client` methods for the `NetworksAPI` domain (plus the `get_premium_status` alias read).
//!
//! [`Client::reboot_network`] wraps `NetworksAPI.reboot_network` (`eero-api
//! src/eero/client.py:1480-1514`), added to `EeroClient` at `v8.4.0`.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::params::require_non_empty_string;
use serde_json::Value;

impl Client {
    /// Gets a network's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_network()` (`client.py:697-722`). Resolves `network_id` with
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
    /// Ported from `get_premium_status()` (`client.py:724-729`). `auto_discover = false` — see [`Client::get_diagnostics`]. Passes the
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
    /// Ported from `set_guest_network` (`eero-api src/eero/client.py:1391-1428`). Resolves
    /// `network_id` with `auto_discover = true`. A supplied `name` is validated first, before the
    /// network id is resolved (`client.py:1414-1415`), so an invalid name never triggers network
    /// discovery. **`password=` is gone since `v6.2.0`** — split into
    /// [`Client::set_guest_password`]/[`Client::clear_guest_password`]. Passes the cached network
    /// envelope as `parent=` (`+net`). On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `name` if `name` is `Some("")`, before the network
    /// id is resolved and before any request. [`Error::MissingNetworkId`] if `network_id` is absent
    /// and no network can be resolved. Otherwise, whatever status-mapped [`Error`] the request
    /// produces. The cache is left untouched on any `Err`.
    pub async fn set_guest_network(
        &self,
        enabled: bool,
        name: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        if let Some(name) = name {
            require_non_empty_string("name", name)?;
        }
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
    /// Ported from `get_guest_network()` (`client.py:1384-1389`). `auto_discover = false` — see
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
    /// Ported from `set_guest_password()` (`client.py:1430-1444`). `auto_discover = false`.
    /// `password` is validated first, before the network id is resolved. **No `parent=` is
    /// passed** — Python's own wrapper calls the domain method with no parent kwargs at all
    /// (`client.py:1441`, unlike its sibling [`Client::set_guest_network`], which does), even
    /// though the domain method accepts one; reproduced exactly. On success, invalidates
    /// `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `password` if `password` is empty, before the
    /// network id is resolved and before any request. Otherwise see [`Client::set_guest_network`].
    pub async fn set_guest_password(
        &self,
        password: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        require_non_empty_string("password", password)?;
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
    /// Ported from `clear_guest_password()` (`client.py:1445-1458`). `auto_discover = false`, no
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
    /// Ported from `run_speed_test` (`eero-api src/eero/client.py:1460-1478`). `auto_discover =
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

    /// Reboots the entire network — returns the raw Eero API response.
    ///
    /// Ported from `reboot_network()` (`eero-api src/eero/client.py:1480-1514`). `auto_discover =
    /// false`: with no `network_id` and no preferred network this fails with
    /// [`Error::MissingNetworkId`] and never calls `get_networks`. Forwards the caller's `parent`
    /// when supplied, otherwise the fresh cached network envelope, otherwise none (`parent` wins;
    /// `_network_parent_kwargs_or`), so the request goes to the envelope's published `reboot` link
    /// (default `POST /2.2/networks/{id}/reboot`) — see
    /// [`crate::endpoints::networks::NetworksApi::reboot_network`] for the wire shape. The request
    /// is sent exactly once and the domain envelope is returned untouched.
    ///
    /// On success, invalidates `network[nid]`, `eeros[{nid}_eeros]` and `devices[{nid}_devices]`.
    /// The device invalidation drops the device **list** only (Python's `_invalidate_device_cache`
    /// gets no device ids here), so single-device entries and every other network's entries
    /// survive.
    ///
    /// **Every eero on the network restarts and all clients lose connectivity for minutes.** A
    /// success only means the request was accepted, not that the network has settled: the reboot
    /// lands later. Check before calling, and never wrap this in a retry loop. The write is
    /// unverified against a live account and logs the uncharacterised-write warning.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set.
    /// Otherwise, whatever [`Error`] the request produces. The cache is left untouched on any
    /// `Err`.
    pub async fn reboot_network(
        &self,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent_or(&network_id, parent);
        let response = self
            .api
            .networks()
            .reboot_network(&network_id, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        self.invalidate_eeros_cache(&network_id);
        self.invalidate_device_cache(&network_id, &[]);
        Ok(response)
    }

    /// Gets past speed-test results — returns the raw Eero API response.
    ///
    /// Ported from `get_speed_tests()` (`client.py:1516-1532`). `auto_discover = false`. Passes
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
    /// Ported from `set_network_name` (`eero-api src/eero/client.py:731-750`). `name` is validated
    /// first (`client.py:744`), before `network_id` is resolved with `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Passes the cached network envelope as `parent=` (`+net`). On
    /// success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `name` if `name` is empty, before the network id
    /// is resolved and before any request. [`Error::MissingNetworkId`] if `network_id` is absent
    /// and no preferred network is set — auto-discovery is not attempted. Otherwise, whatever
    /// status-mapped [`Error`] the request produces. The cache is left untouched on any `Err`.
    pub async fn set_network_name(
        &self,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        require_non_empty_string("name", name)?;
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

    /// Temporarily hides (or un-hides) the network's 5 GHz band — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_hide_5g` (`eero-api src/eero/client.py:752-779`). `auto_discover =
    /// false`. `enabled = true` sends `PUT {"value": true}` to the network's
    /// `temporary_flags/hide_5g` resource, `false` sends a `DELETE` to it — see
    /// [`crate::endpoints::networks::NetworksApi::set_hide_5g`]. **Passes no `parent=`**: the
    /// resource has no published link, so Python builds the path from the template and neither
    /// takes nor forwards a cached envelope. The flag is reported in the network envelope's
    /// `temporary_flags`, so a successful write drops `network[nid]`.
    ///
    /// **Hiding 5 GHz disconnects 5 GHz clients while it takes effect, and the write is
    /// unverified against a live account.** Read the current flag first, skip the write when it
    /// already matches, and never retry a failed write in a loop.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set —
    /// auto-discovery is not attempted. Otherwise, whatever status-mapped [`Error`] the request
    /// produces. The cache is left untouched on any `Err`.
    pub async fn set_hide_5g(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .networks()
            .set_hide_5g(&network_id, enabled)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets the network's Wi-Fi password — returns the raw Eero API response.
    ///
    /// Ported from `set_network_password()` (`client.py:781-796`). `auto_discover = false`.
    /// `password` is validated first, before the network id is resolved. Passes the cached
    /// network envelope as `parent=` (`+net`). On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `password` if `password` is empty, before the
    /// network id is resolved and before any request. Otherwise see [`Client::set_network_name`].
    pub async fn set_network_password(
        &self,
        password: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        require_non_empty_string("password", password)?;
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
    /// Ported from `clear_network_password()` (`client.py:799-814`). `auto_discover = false`.
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
