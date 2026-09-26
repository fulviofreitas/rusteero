//! `Client` methods for the `SubnetsAPI` domain (new in v8.0.0).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    // ==================== Subnets ====================

    /// Gets the subnets configuration — returns the raw Eero API response.
    ///
    /// Ported from `get_subnets_config` (`eero-api src/eero/client.py:2993-2999`).
    /// `auto_discover = false`. Passes the network's cached envelope (if fresh) as `parent`.
    /// This is a verified read.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_subnets_config(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .subnets()
            .get_config(&network_id, parent.as_ref())
            .await
    }

    /// Sets the subnets configuration — returns the raw Eero API response.
    ///
    /// Ported from `set_subnets_config` (`eero-api src/eero/client.py:3000-3007`).
    /// `auto_discover = false`. No `parent=` forwarded — matches
    /// [`crate::endpoints::subnets::SubnetsApi::set_config`], which never accepts one.
    /// Invalidates `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_subnets_config(
        &self,
        config: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.subnets().set_config(&network_id, config).await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Deletes a subnet — returns the raw Eero API response.
    ///
    /// Ported from `delete_subnet` (`eero-api src/eero/client.py:3009-3016`). `auto_discover =
    /// false`. Invalidates `network[{nid}]` unconditionally after the write.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_subnet(
        &self,
        subnet_type: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .subnets()
            .delete_subnet(&network_id, subnet_type)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Sets subnet content filters — returns the raw Eero API response.
    ///
    /// Ported from `set_subnet_content_filters` (`eero-api src/eero/client.py:3018-3024`).
    /// `auto_discover = false`. **Invalidates nothing** — deliberately different from
    /// [`Client::set_subnets_config`]; matches Python exactly (`client.py` never calls
    /// `_invalidate_network_cache` here).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_subnet_content_filters(
        &self,
        filters: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .subnets()
            .set_content_filters(&network_id, filters)
            .await
    }

    /// Gets a subnet's content filters — returns the raw Eero API response.
    ///
    /// Ported from `get_subnet_content_filters` (`eero-api src/eero/client.py:3025-3031`).
    /// `auto_discover = false`. No parent — matches
    /// [`crate::endpoints::subnets::SubnetsApi::get_content_filters`], which never accepts one.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_subnet_content_filters(
        &self,
        subnet_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .subnets()
            .get_content_filters(&network_id, subnet_id)
            .await
    }
}
