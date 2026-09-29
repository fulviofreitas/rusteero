//! `Client` methods for the `ThreadAPI` domain, v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py:1420-1479` (the Thread group of `EeroClient`).
//! [`Client::get_thread`] passes `**self._network_parent_kwargs(network_id)`, like every other
//! cached-network read in this crate; the three write wrappers below do **not** — Python's own
//! `set_thread_enabled`/`update_thread`/`regenerate_thread_credentials` never build or pass a
//! `parent` kwarg at all (`client.py:1427-1479`), matching `ThreadAPI`'s own writes, which accept
//! but ignore it (see [`crate::endpoints::thread::ThreadApi::set_thread_enabled`]'s own docs).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets Thread protocol status — returns the raw Eero API response.
    ///
    /// Ported from `get_thread()` (`client.py:1420-1425`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_thread(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .thread()
            .get_thread(&network_id, parent.as_ref())
            .await
    }

    /// Enables or disables Thread — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_thread_enabled` (`client.py:1427-1441`). `auto_discover = false`. On
    /// success, invalidates `network[nid]`. Passes no `parent` — see this module's own docs.
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
        let response = self
            .api
            .thread()
            .set_thread_enabled(&network_id, enabled, None)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Updates Thread configuration — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `update_thread` (`client.py:1443-1465`). `auto_discover = false`. On success,
    /// invalidates `network[nid]`. Passes no `parent` — see this module's own docs.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "thread", .. }` if both `thread_enable` and
    /// `enable_credential_syncing` are `None` — see
    /// [`crate::endpoints::thread::ThreadApi::update_thread`]. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn update_thread(
        &self,
        thread_enable: Option<bool>,
        enable_credential_syncing: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .thread()
            .update_thread(&network_id, thread_enable, enable_credential_syncing, None)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }

    /// Regenerates Thread network credentials — returns the raw Eero API response (unverified
    /// write; response's `network` key is of unknown shape).
    ///
    /// Ported from `regenerate_thread_credentials` (`client.py:1467-1479`). `auto_discover =
    /// false`. On success, invalidates `network[nid]`. Passes no `parent` — see this module's own
    /// docs.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn regenerate_thread_credentials(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .thread()
            .regenerate_thread_credentials(&network_id, None)
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
