//! `Client` methods for the `DevicesAPI` domain.

use super::Client;
use crate::cache::{Bucket, CacheKey};
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the list of connected devices — returns the raw Eero API response.
    ///
    /// Ported from `get_devices()` (`eero-api src/eero/client.py:674-715` at `v8.0.4`);
    /// `auto_discover = true` — see [`Client::get_network`] for the shared note. Passes the
    /// cached network envelope as `parent=` (`+net`, `_network_parent_kwargs`).
    ///
    /// `thread`/`proxied_node` are new since v6.2.0: `filtered = thread.is_some() ||
    /// proxied_node.is_some()` gates **both** cache directions (`client.py:696-715`) — a
    /// filtered call never reads the `devices[{nid}_devices]` cache, even if fresh, and never
    /// writes it either, so a filtered response can never silently overwrite the unfiltered
    /// cached list the next unfiltered call would otherwise read.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_devices(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
        thread: Option<bool>,
        proxied_node: Option<bool>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let filtered = thread.is_some() || proxied_node.is_some();
        let key = CacheKey::devices(network_id.as_str());
        if !filtered
            && !refresh_cache
            && let Some(cached) = self.cache.get(&key)
        {
            return Ok(cached);
        }
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .devices()
            .get_devices(&network_id, thread, proxied_node, parent.as_ref())
            .await?;
        if !filtered {
            self.cache.put(key, response.clone());
        }
        Ok(response)
    }

    /// Gets a single device's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_device()` (`client.py:717-747`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note. Note the parameter order: `device_id` (required)
    /// comes first, `network_id` (optional) second — matching every cached getter that takes an
    /// item id (brief §6). This call never passes `parent=` itself — it *populates* the cache
    /// `Client::device_parent` later reads from.
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
            .get_device(&network_id, device_id, None)
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a single device's full object **without ever touching the cache** — returns the raw
    /// Eero API response.
    ///
    /// Ported from `get_device_priority()` (`client.py:2194-2200`): calls the exact same domain
    /// method as [`Client::get_device`] (`DevicesApi::get_device`), but deliberately never reads
    /// or writes `devices[{nid}_{did}]`, and resolves `network_id` with `auto_discover = false`
    /// (`client.py:2199`) — unlike `get_device`, this method returns [`Error::MissingNetworkId`]
    /// rather than probing `/networks` when no id/preferred network is set. **Do not** wire this
    /// into the `devices` cache bucket to "fix" the apparent duplication with `get_device` — the
    /// uncached-ness is the documented v8.0.4 behaviour, not an oversight
    /// (`.claude/tasks/briefs/v8/client.md`, devices table, `get_device_priority` row).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set.
    /// Otherwise, whatever status-mapped [`Error`] the request produces.
    pub async fn get_device_priority(
        &self,
        device_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .devices()
            .get_device(&network_id, device_id, None)
            .await
    }

    // ==================== Devices (mutations) ====================

    /// Sets a nickname for a device — returns the raw Eero API response.
    ///
    /// Ported from `set_device_nickname` (`eero-api src/eero/client.py:748-769`).
    /// `auto_discover = true`. On success, invalidates both `devices[{nid}_{did}]` and
    /// `devices[{nid}_devices]` via `Client::invalidate_device_cache`. No `parent=`.
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

    /// Blocks a device via the `/blacklist` resource — returns the raw Eero API response.
    ///
    /// Ported from `block_device` (`eero-api src/eero/client.py:770-789`). `auto_discover =
    /// true`. **Breaking shape change**: the pre-8.0.0 `blocked: bool` parameter this crate
    /// previously shipped is gone — unblocking is [`Client::unblock_device`], a separate method.
    /// On success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`
    /// (`client.py:786-787`). No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn block_device(
        &self,
        device_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .block_device(&network_id, device_id)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Unblocks a device via the `/blacklist` resource — returns the raw Eero API response.
    ///
    /// Ported from `unblock_device` (`eero-api src/eero/client.py:790-809`). New since v6.2.0
    /// (the pre-8.0.0 port folded this into `block_device(blocked: false)`). `auto_discover =
    /// true`. On success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`,
    /// same as [`Client::block_device`]. No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn unblock_device(
        &self,
        device_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .unblock_device(&network_id, device_id)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Pauses or unpauses internet access for a device — returns the raw Eero API response.
    ///
    /// Ported from `pause_device` (`eero-api src/eero/client.py:810-831`). `auto_discover =
    /// true`. On success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`.
    /// No `parent=`.
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

    /// Updates a device's nickname/paused/profile via its own published URL — returns the raw
    /// Eero API response.
    ///
    /// Ported from `update_device_via_link` (`eero-api src/eero/client.py:832-867`).
    /// `auto_discover = true`. Passes the cached single-device envelope as `parent=` (`+dev`,
    /// `_device_parent_kwargs`). On success, invalidates both `devices[{nid}_{did}]` and
    /// `devices[{nid}_devices]`, **and**, only when `profile` is `Some`, every cached profile
    /// entry of this network (`Cache::invalidate_bucket(Bucket::Profiles, ..)`,
    /// `client.py:1028-1038`'s `_invalidate_all_profile_caches` — a device's profile membership
    /// changed, so nothing single-profile can be trusted any more).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. Also returns [`Error::Validation`] with `field: "device"` if
    /// `nickname`, `paused` and `profile` are all `None`, before any request is sent. The cache
    /// is left untouched on any `Err`.
    pub async fn update_device_via_link(
        &self,
        device_id: &str,
        nickname: Option<&str>,
        paused: Option<bool>,
        profile: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let parent = self.device_parent(network_id.as_str(), device_id);
        let response = self
            .api
            .devices()
            .update_device_via_link(
                &network_id,
                device_id,
                nickname,
                paused,
                profile,
                parent.as_ref(),
            )
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        if profile.is_some() {
            self.cache
                .invalidate_bucket(Bucket::Profiles, network_id.as_str());
        }
        Ok(response)
    }

    /// Sets a device's type — returns the raw Eero API response.
    ///
    /// Ported from `set_device_type` (`eero-api src/eero/client.py:868-880`). `auto_discover =
    /// true`. On success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`.
    /// No `parent=`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn set_device_type(
        &self,
        device_id: &str,
        device_type: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .set_device_type(&network_id, device_id, device_type)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Gets a device's labels — returns the raw Eero API response.
    ///
    /// Ported from `get_device_labels` (`eero-api src/eero/client.py:881-887`). `auto_discover =
    /// true`. Not cached (only the eight getters in the behaviour brief §1.4 are).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_device_labels(
        &self,
        device_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        self.api
            .devices()
            .get_device_labels(&network_id, device_id)
            .await
    }

    /// Sets a device's labels — returns the raw Eero API response.
    ///
    /// Ported from `set_device_labels` (`eero-api src/eero/client.py:888-...`). `auto_discover =
    /// true`. Confirmed server-side **no-op** as of 2026-09-20 (ported for parity, documented as
    /// such — see [`crate::endpoints::devices::DevicesApi::set_device_labels`]). On success,
    /// invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]` regardless, matching
    /// every other device write in this file.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    #[allow(clippy::too_many_arguments)]
    pub async fn set_device_labels(
        &self,
        device_id: &str,
        make_label: Option<&str>,
        model_label: Option<&str>,
        version_label: Option<&str>,
        type_label: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .set_device_labels(
                &network_id,
                device_id,
                make_label,
                model_label,
                version_label,
                type_label,
            )
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }
}
