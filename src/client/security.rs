//! `Client` methods for the `SecurityAPI` domain, v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py:2205-2801` (the Security/MLO/fast-transition/
//! Passpoint/proxied-nodes group of `EeroClient`). Every method below passes
//! `**self._network_parent_kwargs(network_id)` in Python — this port's [`Client::network_parent`]
//! equivalent.
//!
//! **`Client::set_thread_enabled` is not defined here.** The old `SecurityAPI.set_thread` write
//! this method used to wrap is fully removed upstream (replaced by the `ThreadAPI` family) — see
//! [`crate::client::Client::set_thread_enabled`] in `src/client/thread.rs`, the v8.0.4
//! replacement wrapping the new `ThreadAPI.set_thread_enabled`.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    // ==================== Security ====================

    /// Gets security settings — returns the raw Eero API response.
    ///
    /// Ported from `get_security_settings()` (`client.py:2205-2208`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_security_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .security()
            .get_security_settings(&network_id, parent.as_ref())
            .await
    }

    // ==================== Security (mutations) ====================

    /// Enables or disables WPA3 encryption — returns the raw Eero API response.
    ///
    /// Ported from `set_wpa3` (`client.py:2212-2219`). `auto_discover = false`. On success,
    /// invalidates `network[nid]`.
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
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_wpa3(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Enables or disables band steering — returns the raw Eero API response.
    ///
    /// Ported from `set_band_steering` (`client.py:2221-2229`). `auto_discover = false`. On
    /// success, invalidates `network[nid]`.
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
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_band_steering(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Enables or disables `UPnP` — returns the raw Eero API response.
    ///
    /// Ported from `set_upnp` (`client.py:2232-2239`). `auto_discover = false`. On success,
    /// invalidates `network[nid]`.
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
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_upnp(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Enables or disables IPv6 (both upstream and downstream) — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_ipv6` (`client.py:2241-2248`). `auto_discover = false`. On success,
    /// invalidates `network[nid]`.
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
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_ipv6(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Configures multiple security settings in one call — returns the raw Eero API response.
    ///
    /// Ported from `configure_security` (`client.py:2250-2269`). **`thread` is not a parameter**
    /// — see [`crate::endpoints::security::SecurityApi::configure_security`]'s own docs for why.
    /// `auto_discover = false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if every argument is `None` — see
    /// [`crate::endpoints::security::SecurityApi::configure_security`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn configure_security(
        &self,
        wpa3: Option<bool>,
        band_steering: Option<bool>,
        upnp: Option<bool>,
        ipv6: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .configure_security(
                &network_id,
                wpa3,
                band_steering,
                upnp,
                ipv6,
                parent.as_ref(),
            )
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    // ==================== MLO, fast transition, Passpoint, proxied nodes (new in v8.0.0) ====================

    /// Sets the network's MLO (Multi-Link Operation) mode — returns the raw Eero API response
    /// (unverified; may reboot the mesh).
    ///
    /// Ported from `set_mlo_mode` (`client.py:2763-2769`). `auto_discover = false`. On success,
    /// invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "mode", .. }` for an unrecognised mode — see
    /// [`crate::endpoints::security::SecurityApi::set_mlo_mode`]. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_mlo_mode(
        &self,
        mode: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_mlo_mode(&network_id, mode, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Gets the network's 802.11r fast-transition setting — returns the raw Eero API response
    /// (verified read).
    ///
    /// Ported from `get_fast_transition` (`client.py:2772-2777`). `auto_discover = false`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_fast_transition(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .security()
            .get_fast_transition(&network_id, parent.as_ref())
            .await
    }

    /// Sets fast transition — returns the raw Eero API response (unverified; may reboot the
    /// mesh).
    ///
    /// Ported from `set_fast_transition` (`client.py:2779-2788`). `auto_discover = false`. On
    /// success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_fast_transition(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_fast_transition(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Enables or disables Passpoint — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_passpoint_enabled` (`client.py:2790-2799`). `auto_discover = false`. On
    /// success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_passpoint_enabled(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_passpoint_enabled(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Enables or disables proxied nodes — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_proxied_nodes` (`client.py:2801-2810`). `auto_discover = false`. On
    /// success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_proxied_nodes(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .security()
            .set_proxied_nodes(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
