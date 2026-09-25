//! `Client` methods for the `SecurityAPI` domain.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== Security ====================

    /// Gets security settings — returns the raw Eero API response.
    ///
    /// Ported from `get_security_settings()` (`client.py:1276-1279`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_security_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.security().get_security_settings(&network_id).await
    }

    // ==================== Security (mutations) ====================

    /// Enables or disables WPA3 encryption — returns the raw Eero API response.
    ///
    /// Ported from `set_wpa3` (`eero-api src/eero/client.py:1281-1284`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_wpa3(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_wpa3(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables band steering — returns the raw Eero API response.
    ///
    /// Ported from `set_band_steering` (`eero-api src/eero/client.py:1286-1291`).
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see this group's banner
    /// comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_band_steering(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .security()
            .set_band_steering(&network_id, enabled)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables `UPnP` — returns the raw Eero API response.
    ///
    /// Ported from `set_upnp` (`eero-api src/eero/client.py:1293-1296`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_upnp(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_upnp(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables IPv6 (both upstream and downstream) — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_ipv6` (`eero-api src/eero/client.py:1298-1301`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    /// Contrast with [`Client::set_ipv6_dns`], which sets only the upstream flag.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_ipv6(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_ipv6(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables Thread — returns the raw Eero API response.
    ///
    /// Ported from `set_thread_enabled` (`eero-api src/eero/client.py:1303-1308`), which
    /// delegates to [`crate::endpoints::SecurityApi::set_thread`] — the client-level name keeps
    /// Python's own `set_thread_enabled`, not `set_thread`, verbatim (`client.py:1303`).
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see this group's banner
    /// comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_thread_enabled(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_thread(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Configures multiple security settings in one call — returns the raw Eero API response.
    ///
    /// Ported from `configure_security` (`eero-api src/eero/client.py:1310-1328`).
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see this group's banner
    /// comment above.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if every argument is `None` — see
    /// [`crate::endpoints::SecurityApi::configure_security`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn configure_security(
        &self,
        wpa3: Option<bool>,
        band_steering: Option<bool>,
        upnp: Option<bool>,
        ipv6: Option<bool>,
        thread: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .security()
            .configure_security(&network_id, wpa3, band_steering, upnp, ipv6, thread)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }
}
