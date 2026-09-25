//! `Client` methods for the `ProfilesAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the list of profiles on a network — returns the raw Eero API response.
    ///
    /// Ported from `get_profiles()` (`client.py:579-604`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note.
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
        let response = self.api.profiles().get_profiles(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a single profile's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_profile()` (`client.py:606-634`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note. Note the parameter order: `profile_id` (required)
    /// comes first, `network_id` (optional) second, matching [`Client::get_device`].
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
            .get_profile(&network_id, profile_id)
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    // ==================== Blocked Applications ====================

    /// Gets a profile's blocked applications (Eero Plus feature) — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_blocked_applications()` (`client.py:1332-1337`). `auto_discover = false`
    /// — see [`Client::get_diagnostics`]. Note the parameter order: `profile_id` first,
    /// `network_id` second, matching `client.py`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_blocked_applications(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .profiles()
            .get_blocked_applications(&network_id, profile_id)
            .await
    }

    // ==================== Profile Devices ====================

    /// Gets a profile's data "including devices" — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_devices()` (`client.py:1355-1360`). **`auto_discover = true`**
    /// — the module docs' "brief gotcha G5" exception: unlike every other method in this section
    /// (`get_diagnostics` through `get_blocked_applications`, all `auto_discover = false`), this
    /// method and, in phase 5, `set_profile_devices` (`client.py:1362-1372`) call
    /// `_ensure_network_id(network_id)` with no `auto_discover` argument at all, i.e. Python's
    /// default `True`. Do not "fix" this to `false` to match its neighbours — it would be a
    /// behavioural change, not a cleanup.
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
        // Brief gotcha G5 (see this method's doc comment): `true`, not `false`, deliberately.
        let network_id = self.ensure_network_id(network_id, true).await?;
        self.api
            .profiles()
            .get_profile_devices(&network_id, profile_id)
            .await
    }

    // ==================== Profiles (mutations) ====================

    /// Creates a new profile on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_profile` (`eero-api src/eero/client.py:675-691`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_profiles]` only — no single-profile key
    /// exists yet for whatever id the server just assigned (`client.py:689`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn create_profile(
        &self,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .create_profile(&network_id, name)
            .await?;
        self.invalidate_profiles_list_cache(network_id.as_str());
        Ok(response)
    }

    /// Renames an existing profile — returns the raw Eero API response.
    ///
    /// Ported from `rename_profile` (`eero-api src/eero/client.py:693-713`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` via
    /// `Client::invalidate_profile_cache` — Python calls both `_invalidate_profile_cache` (which
    /// already drops both keys itself) *and* `_invalidate_profiles_list_cache` at this call site
    /// (`client.py:710-711`), a redundant double-drop of the list key the behaviour brief calls
    /// out as harmless; this port calls the combined helper once, for the identical end state.
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
            .rename_profile(&network_id, profile_id, name)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Deletes a profile from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_profile` (`eero-api src/eero/client.py:715-736`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` —
    /// see [`Client::rename_profile`]'s docs for the same redundant-double-drop note
    /// (`client.py:733-734`).
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
    /// Ported from `pause_profile` (`eero-api src/eero/client.py:637-657`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`
    /// (`client.py:654-655`).
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
            .pause_profile(&network_id, profile_id, paused)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    // ==================== Blocked Applications & Profile Content (mutations) ====================

    /// Sets the blocked applications (Eero Plus feature) for a profile — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_blocked_applications` (`eero-api src/eero/client.py:1339-1351`).
    /// `auto_discover = false`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` (`client.py:1350`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_blocked_applications(
        &self,
        profile_id: &str,
        applications: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .profiles()
            .set_blocked_applications(&network_id, profile_id, applications)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Updates a profile's content-filtering settings — returns the raw Eero API response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped
    /// `ProfilesAPI.update_profile_content_filter` (`api/profiles.py:179`) on `EeroClient`.
    /// [`crate::endpoints::ProfilesApi::update_profile_content_filter`] `PUT`s the exact same
    /// `networks/{nid}/profiles/{pid}` resource [`Client::set_blocked_applications`],
    /// [`Client::rename_profile`], [`Client::pause_profile`] and [`Client::set_profile_devices`]
    /// all mutate — every one of which invalidates the profile cache in Python — so this method
    /// follows the policy already established for that resource rather than inventing a new one:
    /// `auto_discover = false` (grouped here with [`Client::set_blocked_applications`], not with
    /// the `auto_discover = true` "Profile Devices" section at the very end of `client.py`), and
    /// on success invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn update_profile_content_filter(
        &self,
        profile_id: &str,
        filters: &[(&str, bool)],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .profiles()
            .update_profile_content_filter(&network_id, profile_id, filters)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Updates a profile's custom domain block or allow list — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent — see [`Client::update_profile_content_filter`]'s docs, which
    /// apply identically here: [`crate::endpoints::ProfilesApi::update_profile_block_list`]
    /// `PUT`s the same profile resource. `auto_discover = false`. On success, invalidates
    /// `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn update_profile_block_list(
        &self,
        profile_id: &str,
        domains: &[&str],
        block: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .profiles()
            .update_profile_block_list(&network_id, profile_id, domains, block)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    // ==================== Profile Devices ====================

    /// Sets the devices assigned to a profile — returns the raw Eero API response.
    ///
    /// Ported from `set_profile_devices` (`eero-api src/eero/client.py:1362-1372`).
    /// **`auto_discover = true`** — the same brief gotcha G5 exception
    /// [`Client::get_profile_devices`]'s docs describe: this is one of the two methods at the
    /// very end of `client.py` that call `_ensure_network_id(network_id)` with no
    /// `auto_discover` argument at all, unlike every other method from `get_diagnostics` through
    /// `set_blocked_applications`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` (`client.py:1371`).
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
        // Brief gotcha G5 (see this method's doc comment): `true`, not `false`, deliberately —
        // matching `Client::get_profile_devices` above.
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .set_profile_devices(&network_id, profile_id, device_urls)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }
}
