//! `Client` methods for the `ProfilesAPI` domain at v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py`'s profiles-scoped wrappers.
//! `update_profile_content_filter`,
//! `update_profile_block_list`, `get_blocked_applications` and `set_blocked_applications` are
//! **removed**: the domain methods they wrapped never persisted anything (silent
//! no-op, live-verified) and have no v8.0.4 `client.py` equivalent — replaced by
//! [`Client::allow_domain_for_profiles`]/[`Client::block_domain_for_profiles`]/
//! [`Client::get_dns_policy_applications`]/[`Client::set_profile_blocked_applications`]
//! (`crate::client::dns_policies`).

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the list of profiles on a network — returns the raw Eero API response.
    ///
    /// Ported from `get_profiles()` (`eero-api src/eero/client.py:930-957`); see
    /// [`Client::get_network`] for the shared `auto_discover = true` note. Passes the cached
    /// network envelope as `parent=` when fresh (`+net`), so the request prefers
    /// the network's own published `profiles` link.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_profiles(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::profiles(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .profiles()
            .get_profiles(&network_id, parent.as_ref())
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a single profile's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_profile()` (`eero-api src/eero/client.py:959-988`); see
    /// [`Client::get_network`] for the shared `auto_discover = true` note. No `parent=` is passed
    /// — `client.py` has no `_profile_parent_kwargs` helper: nothing in this
    /// client's cache is keyed in a way that could supply one.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_profile(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::profile(network_id.as_str(), profile_id);
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self
            .api
            .profiles()
            .get_profile(&network_id, profile_id, None)
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a profile's data "including devices" — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_devices()` (`eero-api src/eero/client.py:2273-2279`). Not
    /// cached; no `parent=` (see [`Client::get_profile`]'s docs).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent, no preferred network is set, *and*
    /// auto-discovery via [`Client::get_networks`] finds nothing. Otherwise, whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_profile_devices(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        self.api
            .profiles()
            .get_profile_devices(&network_id, profile_id, None)
            .await
    }

    /// Creates a new profile on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_profile` (`eero-api src/eero/client.py:1040-1073`). `auto_discover =
    /// true`. Passes the cached network envelope as `parent=` when fresh (`+net`). `devices=`/
    /// `paused=` are forwarded exactly as given — `None` omits the field from the wire request
    /// (see [`crate::endpoints::profiles::ProfilesApi::create_profile`]'s docs for the
    /// omit-when-`None` rule). On success, invalidates `profiles[{nid}_profiles]` only — no
    /// single-profile key exists yet for whatever id the server just assigned.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn create_profile(
        &self,
        name: &str,
        devices: Option<&[&str]>,
        paused: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .profiles()
            .create_profile(&network_id, name, devices, paused, parent.as_ref())
            .await?;
        self.invalidate_profiles_list_cache(network_id.as_str());
        Ok(response)
    }

    /// Renames an existing profile — returns the raw Eero API response.
    ///
    /// Ported from `rename_profile` (`eero-api src/eero/client.py:1073-1095`). `auto_discover =
    /// true`. No `parent=`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` via its private `invalidate_profile_cache` helper.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn rename_profile(
        &self,
        profile_id: &str,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .rename_profile(&network_id, profile_id, name, None)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Deletes a profile from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_profile` (`eero-api src/eero/client.py:1095-1120`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn delete_profile(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .delete_profile(&network_id, profile_id)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Pauses or unpauses internet access for a profile — returns the raw Eero API response.
    ///
    /// Ported from `pause_profile` (`eero-api src/eero/client.py:990-1040`). `auto_discover =
    /// true`. No `parent=`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn pause_profile(
        &self,
        profile_id: &str,
        paused: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .pause_profile(&network_id, profile_id, paused, None)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Sets the devices assigned to a profile — returns the raw Eero API response.
    ///
    /// Ported from `set_profile_devices` (`eero-api src/eero/client.py:2280-2288`).
    /// `auto_discover = true`. No `parent=`. On success, invalidates **both**
    /// `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` — Python's `_invalidate_profile_cache`
    /// (`client.py:1012-1020`, called at `:2289`) drops both keys, unlike
    /// [`Client::create_profile`], which only ever had the list key to drop.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent, no preferred network is set, *and*
    /// auto-discovery finds nothing. Otherwise, whatever status-mapped [`Error`] the request
    /// produces. The cache is left untouched on any `Err`.
    pub async fn set_profile_devices(
        &self,
        profile_id: &str,
        device_urls: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .set_profile_devices(&network_id, profile_id, device_urls, None)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }
}
