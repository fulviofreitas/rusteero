//! `Client` methods for the `DnsAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== DNS ====================

    /// Gets DNS settings — returns the raw Eero API response.
    ///
    /// Ported from `get_dns_settings()` (`client.py:1173-1176`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_dns_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.dns().get_dns_settings(&network_id).await
    }

    // ==================== DNS (mutations) ====================
    //
    // Divergence from eero-api: every setter in this group invalidates `network[nid]`, even
    // though not one of them does in Python (behaviour brief §2/§2.1, gotcha G3). `DnsApi`'s six
    // setters all `PUT networks/{nid}/settings` through the shared `put_network_settings` call
    // site (`src/endpoints/networks.rs`) — the exact same resource `Client::get_network` caches —
    // so a `get_network()` call served from cache right after any of these would echo back
    // pre-write data for as long as the TTL lasts. `rust-port-plan.md` §3.8 and `cache.rs`'s own
    // module docs ("what's new" (b)) name this as one of this port's two deliberate improvements
    // over Python; it applies to every DNS setter uniformly, including the two below with no
    // `client.py` precedent at all — not just the three Python happens to wrap.

    /// Enables or disables DNS caching — returns the raw Eero API response.
    ///
    /// Ported from `set_dns_caching` (`eero-api src/eero/client.py:1178-1183`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_dns_caching(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.dns().set_dns_caching(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets custom DNS servers — returns the raw Eero API response.
    ///
    /// Ported from `set_custom_dns` (`eero-api src/eero/client.py:1185-1190`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_custom_dns(
        &self,
        dns_servers: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .dns()
            .set_custom_dns(&network_id, dns_servers)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Clears custom DNS servers (reverts to automatic DNS) — returns the raw Eero API response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped `DnsAPI.clear_custom_dns`
    /// (`api/dns.py:126`) on `EeroClient`. [`crate::endpoints::DnsApi::clear_custom_dns`] itself
    /// delegates to `DnsApi::set_custom_dns([])` — the same resource [`Client::set_custom_dns`]
    /// mutates — so this method follows the same cache behaviour: `auto_discover = false`,
    /// invalidates `network[nid]` on success.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn clear_custom_dns(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.dns().clear_custom_dns(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets DNS mode to a preset or a custom server list — returns the raw Eero API response.
    ///
    /// Ported from `set_dns_mode` (`eero-api src/eero/client.py:1192-1200`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "mode", .. }` for an unrecognised `mode` — see
    /// [`crate::endpoints::DnsApi::set_dns_mode`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_dns_mode(
        &self,
        mode: &str,
        custom_servers: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .dns()
            .set_dns_mode(&network_id, mode, custom_servers)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables IPv6 DNS (upstream only) — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::clear_custom_dns`]'s docs; the same reasoning
    /// applies here. `auto_discover = false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_ipv6_dns(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.dns().set_ipv6_dns(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }
}
