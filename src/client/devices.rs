//! `Client` methods for the `DevicesAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the list of connected devices — returns the raw Eero API response.
    ///
    /// Ported from `get_devices()` (`client.py:436-461`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_devices(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::devices(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self.api.devices().get_devices(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a single device's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_device()` (`client.py:463-492`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note. Note the parameter order: `device_id` (required)
    /// comes first, `network_id` (optional) second — matching every cached getter that takes an
    /// item id (brief §6).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_device(
        &self,
        device_id: &str,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::device(network_id.as_str(), device_id);
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self
            .api
            .devices()
            .get_device(&network_id, device_id)
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    // ==================== Devices (mutations) ====================

    /// Sets a nickname for a device — returns the raw Eero API response.
    ///
    /// Ported from `set_device_nickname` (`eero-api src/eero/client.py:494-514`).
    /// `auto_discover = true` (before the `client.py:809` boundary). On success, invalidates
    /// both `devices[{nid}_{did}]` and `devices[{nid}_devices]` via `Client::invalidate_device_cache`
    /// — the same two-key drop Python's `_invalidate_device_cache` performs
    /// (`client.py:567-575`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn set_device_nickname(
        &self,
        device_id: &str,
        nickname: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .set_device_nickname(&network_id, device_id, nickname)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Blocks or unblocks a device via the `/blacklist` resource — returns the raw Eero API
    /// response.
    ///
    /// Ported from `block_device` (`eero-api src/eero/client.py:516-543`). `auto_discover =
    /// true`. [`crate::endpoints::DevicesApi::block_device`] itself performs up to two round
    /// trips (a `GET` to resolve the MAC, then the blacklist `POST`, when `blocked == true`);
    /// this method simply awaits that single call and invalidates once it succeeds overall. On
    /// success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`
    /// (`client.py:540-541`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. Also returns `Error::Api { status: 502, .. }` if `blocked ==
    /// true` and the device's MAC cannot be resolved — see
    /// [`crate::endpoints::DevicesApi::block_device`]'s own docs. The cache is left untouched on
    /// any `Err`.
    pub async fn block_device(
        &self,
        device_id: &str,
        blocked: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .block_device(&network_id, device_id, blocked)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Pauses or unpauses internet access for a device — returns the raw Eero API response.
    ///
    /// Ported from `pause_device` (`eero-api src/eero/client.py:545-565`). `auto_discover =
    /// true`. On success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`
    /// (`client.py:562-563`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn pause_device(
        &self,
        device_id: &str,
        paused: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .pause_device(&network_id, device_id, paused)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }
}
