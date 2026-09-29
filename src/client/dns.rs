//! `Client` methods for the `DnsAPI` domain, v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py:2080-2158` (the DNS group of `EeroClient`). Every
//! write below passes `**self._network_parent_kwargs(network_id)` in Python
//! (`client.py:2080-2158`) — this port's [`Client::network_parent`] equivalent — except
//! [`Client::get_dns_settings`], whose underlying [`crate::endpoints::dns::DnsApi::get_dns_settings`]
//! has no `parent` parameter at all (see that method's own docs).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== DNS ====================

    /// Gets DNS settings — returns the raw Eero API response.
    ///
    /// Ported from `get_dns_settings()` (`client.py:2080-2083`). `auto_discover = false` — see
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

    /// Enables or disables DNS caching — returns the raw Eero API response.
    ///
    /// Ported from `set_dns_caching` (`client.py:2094-2102`). `auto_discover = false`. On
    /// success, invalidates `network[nid]` — DNS settings live inside the network resource, so
    /// any DNS write makes a cached snapshot stale (mirrors `client.py`'s own
    /// `_invalidate_network_cache`, `client.py:2085-2092`).
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
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dns()
            .set_dns_caching(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets custom DNS servers from a mixed IPv4/IPv6 list — returns the raw Eero API response.
    ///
    /// Ported from `set_custom_dns` (`client.py:2105-2113`). `auto_discover = false`. On success,
    /// invalidates `network[nid]` — see [`Client::set_dns_caching`].
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::dns::DnsApi::set_custom_dns`], otherwise
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_custom_dns(
        &self,
        dns_servers: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dns()
            .set_custom_dns(&network_id, dns_servers, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets the IPv4 custom DNS servers, leaving IPv6 untouched — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_custom_dns_ipv4` (`client.py:2116-2124`). `auto_discover = false`. On
    /// success, invalidates `network[nid]` — see [`Client::set_dns_caching`].
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::dns::DnsApi::set_custom_dns_ipv4`], otherwise
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_custom_dns_ipv4(
        &self,
        dns_servers: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dns()
            .set_custom_dns_ipv4(&network_id, dns_servers, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets the IPv6 custom DNS servers, leaving IPv4 untouched — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_custom_dns_ipv6` (`client.py:2127-2135`). `auto_discover = false`. On
    /// success, invalidates `network[nid]` — see [`Client::set_dns_caching`].
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::dns::DnsApi::set_custom_dns_ipv6`], otherwise
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_custom_dns_ipv6(
        &self,
        dns_servers: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dns()
            .set_custom_dns_ipv6(&network_id, dns_servers, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Switches DNS back to automatic (non-destructive) — returns the raw Eero API response.
    ///
    /// Ported from `clear_custom_dns` (`client.py:2138-2151`). `family = Some("ipv4")`/
    /// `Some("ipv6")` clears one family only; `None` (the default) clears both.
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see
    /// [`Client::set_dns_caching`].
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::dns::DnsApi::clear_custom_dns`], otherwise
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn clear_custom_dns(
        &self,
        family: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dns()
            .clear_custom_dns(&network_id, family, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets DNS mode — returns the raw Eero API response.
    ///
    /// Ported from `set_dns_mode` (`client.py:2153-2163`). `auto_discover = false`. On success,
    /// invalidates `network[nid]` — see [`Client::set_dns_caching`].
    ///
    /// # Errors
    ///
    /// See [`crate::endpoints::dns::DnsApi::set_dns_mode`], otherwise
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_dns_mode(
        &self,
        mode: &str,
        custom_servers: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dns()
            .set_dns_mode(&network_id, mode, custom_servers, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
