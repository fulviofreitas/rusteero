//! `Client` methods for the `DhcpAPI` domain (new in v8.0.0).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::{Map, Value};

impl Client {
    // ==================== DHCP, Connection Mode & NAT ====================

    /// Sets the network's DHCP configuration — returns the raw Eero API response.
    ///
    /// Ported from `set_dhcp` (`eero-api src/eero/client.py:2682-2703`). `auto_discover =
    /// false`. Passes the network's cached envelope (if fresh) as `parent`. Invalidates
    /// `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`], plus whatever [`crate::endpoints::dhcp::DhcpApi::set_dhcp`]
    /// itself validates.
    pub async fn set_dhcp(
        &self,
        network_id: Option<&str>,
        mode: Option<&str>,
        custom: Option<&Map<String, Value>>,
        custom_v2: Option<&Map<String, Value>>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dhcp()
            .set_dhcp(&network_id, mode, custom, custom_v2, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets the network's WAN connection mode — returns the raw Eero API response.
    ///
    /// Ported from `set_connection_mode` (`eero-api src/eero/client.py:2706-2714`).
    /// `auto_discover = false`. Passes the network's cached envelope (if fresh) as `parent`.
    /// Invalidates `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`], plus whatever
    /// [`crate::endpoints::dhcp::DhcpApi::set_connection_mode`] itself validates.
    pub async fn set_connection_mode(
        &self,
        mode: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dhcp()
            .set_connection_mode(&network_id, mode, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets NAT port randomization — returns the raw Eero API response.
    ///
    /// Ported from `set_nat_port_randomization` (`eero-api src/eero/client.py:2717-2725`).
    /// `auto_discover = false`. Passes the network's cached envelope (if fresh) as `parent`.
    /// Invalidates `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_nat_port_randomization(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .dhcp()
            .set_nat_port_randomization(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets `PPPoE` credentials on an eero — returns the raw Eero API response.
    ///
    /// Ported from `set_pppoe` (`eero-api src/eero/client.py:2728-2734`). Not network-scoped —
    /// no `_ensure_network_id` call, no cache interaction — `eero_serial_or_id` addresses the
    /// eero directly.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_pppoe(
        &self,
        eero_serial_or_id: &str,
        username: &str,
        password: &str,
    ) -> Result<Envelope, Error> {
        self.api
            .dhcp()
            .set_pppoe(eero_serial_or_id, username, password)
            .await
    }
}
