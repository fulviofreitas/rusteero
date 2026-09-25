//! `Client` methods for the `BlacklistAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the device blacklist — returns the raw Eero API response.
    ///
    /// Ported from `get_blacklist()` (`client.py:874-877`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_blacklist(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.blacklist().get_blacklist(&network_id).await
    }

    // ==================== Insights, Support, Blacklist, Burst Reporters, OUICheck ====================
    //
    // None of these five has a `client.py` counterpart — `eero-api` never wrapped
    // `InsightsAPI.run_insights` (`api/insights.py:96`), `SupportAPI.request_support`
    // (`api/support.py:61`), `BlacklistAPI.add_to_blacklist`/`remove_from_blacklist`
    // (`api/blacklist.py:56,81`), `BurstReportersAPI.create_burst_reporter`
    // (`api/burst_reporters.py:56`) or `OUICheckAPI.run_ouicheck` (`api/ouicheck.py:56`) on
    // `EeroClient`, confirmed absent by grepping the committed `client.py` for each name — see
    // this file's own module docs for why they are added here anyway (item 1 of this phase's
    // task: every mutating endpoint method under `src/endpoints/` gets a `Client` wrapper, with
    // or without a `client.py` precedent). `auto_discover = false`, matching every sibling `GET`
    // in this same region of `client.py` (`get_insights`, `get_support`, `get_blacklist`,
    // `get_burst_reporters`, `get_ouicheck`, all between the `client.py:809` boundary and
    // `get_premium_status`). None of these five domains has a cache bucket at all (behaviour
    // brief §2.1), so every method below invalidates nothing — the same "no bucket to keep
    // consistent" reasoning as [`Client::create_reservation`]/[`Client::create_forward`] above,
    // not a judgement call specific to any one of them.

    /// Adds a device (by MAC) to the blacklist — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. Distinct from
    /// [`Client::block_device`], which routes through this same `/blacklist` resource but also
    /// resolves the device's MAC first; this method is otherwise the bare
    /// [`crate::endpoints::BlacklistApi::add_to_blacklist`] pass-through, `mac` unchanged and
    /// unresolved. There is still no `blacklist` cache bucket of its own (behaviour brief §2.1;
    /// `get_blacklist` is not one of the eight cached getters), but security review finding F2
    /// corrected the previous "invalidates nothing" banner claim: this issues the *identical*
    /// `POST .../blacklist` request [`Client::block_device`] makes for `blocked: true`, which
    /// mutates state the `devices` bucket caches — a blocked device's status would otherwise
    /// keep reading as unblocked from a cached `get_devices()`/`get_device()` for the rest of the
    /// TTL. On success, invalidates `devices[{nid}_devices]` — the list bucket only, since (unlike
    /// `block_device`) this method never resolves a specific `device_id` to also drop from
    /// `devices[{nid}_{did}]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn add_to_blacklist(
        &self,
        mac: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .blacklist()
            .add_to_blacklist(&network_id, mac)
            .await?;
        self.cache
            .invalidate(&CacheKey::devices(network_id.as_str()));
        Ok(response)
    }

    /// Removes a device from the blacklist — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::add_to_blacklist`]'s docs, which apply
    /// identically here: `mac_or_device_id` is forwarded unchanged, exactly like
    /// [`crate::endpoints::BlacklistApi::remove_from_blacklist`] itself, and this is the same
    /// `DELETE .../blacklist/{id}` [`Client::block_device`] issues for `blocked: false`. On
    /// success, invalidates `devices[{nid}_devices]` — see [`Client::add_to_blacklist`]'s docs
    /// for why only the list key, not a single-device key, is dropped here.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn remove_from_blacklist(
        &self,
        mac_or_device_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .blacklist()
            .remove_from_blacklist(&network_id, mac_or_device_id)
            .await?;
        self.cache
            .invalidate(&CacheKey::devices(network_id.as_str()));
        Ok(response)
    }
}
