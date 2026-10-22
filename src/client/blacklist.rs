//! `Client` methods for the `BlacklistAPI` domain.
//!
//! [`Client::get_blacklist`] reads the block list. [`Client::add_to_blacklist`] and
//! [`Client::remove_from_blacklist`] (`client.py:1899-1960` at `v8.4.1`) call the blacklist
//! domain methods directly — never `DevicesAPI.block_device`/`unblock_device`, the
//! device-centric aliases [`Client::block_device`]/[`Client::unblock_device`] go through — and,
//! unlike those two, resolve the network with `auto_discover = false`.

use serde_json::Value;

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::util::id_from_url;

impl Client {
    /// Gets the device blacklist — returns the raw Eero API response.
    ///
    /// Ported from `get_blacklist()` (`eero-api src/eero/client.py:1489-1495` at `v8.0.4`).
    /// `auto_discover = false` — see [`Client::get_diagnostics`]. Passes the cached network
    /// envelope as `parent=` (`+net`, `_network_parent_kwargs`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_blacklist(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .blacklist()
            .get_blacklist(&network_id, parent.as_ref())
            .await
    }

    /// Adds a MAC address to the network's block list — returns the raw Eero API response.
    ///
    /// Ported from `add_to_blacklist()` (`eero-api src/eero/client.py:1899-1931` at `v8.4.1`).
    /// [`Client::block_device`] is the device-centric alias that delegates to the same operation;
    /// this is the direct form for a caller that holds a MAC but no device record.
    /// Live-verified, so no uncharacterised-write warning is logged. `mac` is the colon-separated
    /// address (`"aa:bb:cc:11:22:33"`); it is sent as the form field `mac=<mac>`.
    ///
    /// `auto_discover = false` (unlike [`Client::block_device`]). `parent` is the network's own
    /// envelope; when `None`, the cached network envelope is used if one is fresh
    /// (`_network_parent_kwargs_or`), and its `device_blacklist` link is preferred over the
    /// template. Read only. On success, invalidates the device's own cache entry and
    /// `devices[{nid}_devices]`; other devices, other networks and the network cache are
    /// untouched — there is no blacklist cache.
    ///
    /// An empty `mac` is rejected with [`Error::Validation`] (`id_or_url`, "must be a non-empty
    /// string") **before** the write. Python sends the POST and only then raises that error from
    /// its cache invalidation, reporting failure for a write the server accepted and leaving the
    /// device list cached; this port fails first instead, so nothing is sent and nothing is
    /// invalidated.
    ///
    /// # Errors
    ///
    /// [`Error::Validation`] as above, otherwise see [`Client::get_diagnostics`]. The cache is
    /// left untouched on any `Err`.
    pub async fn add_to_blacklist(
        &self,
        mac: &str,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        // Python derives the cache key from `mac` only after the write; validate first.
        id_from_url(mac)?;
        let parent = self.network_parent_or(network_id.as_str(), parent);
        let response = self
            .api
            .blacklist()
            .add_to_blacklist(&network_id, mac, parent.as_ref())
            .await?;
        self.invalidate_device_cache(network_id.as_str(), &[mac]);
        Ok(response)
    }

    /// Removes a device from the network's block list — returns the raw Eero API response.
    ///
    /// Ported from `remove_from_blacklist()` (`eero-api src/eero/client.py:1933-1960` at
    /// `v8.4.1`). Live-verified. Accepts a raw MAC or the colon-stripped id the block list
    /// reports. [`Client::unblock_device`] is the device-centric alias.
    ///
    /// `auto_discover = false`. `parent` is resolved as for [`Client::add_to_blacklist`]. On
    /// success, invalidates the device's own cache entry and `devices[{nid}_devices]`; the
    /// network cache is untouched. An empty or non-single-segment `mac_or_device_id` is rejected
    /// with [`Error::Validation`] before any request.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn remove_from_blacklist(
        &self,
        mac_or_device_id: &str,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent_or(network_id.as_str(), parent);
        let response = self
            .api
            .blacklist()
            .remove_from_blacklist(&network_id, mac_or_device_id, parent.as_ref())
            .await?;
        self.invalidate_device_cache(network_id.as_str(), &[mac_or_device_id]);
        Ok(response)
    }
}
