//! `Client` methods for the `Wpa3API` domain (new in v8.0.0), v8.0.4.
//!
//! Ported from `eero-api src/eero/client.py:2738-2761` (the WPA3-per-band group of
//! `EeroClient`). Both methods pass `**self._network_parent_kwargs(network_id)` in Python — this
//! port's [`Client::network_parent`] equivalent.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets WPA3 mode per band — returns the raw Eero API response (verified read).
    ///
    /// Ported from `get_wpa3_per_band` (`client.py:2738-2743`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_wpa3_per_band(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .wpa3()
            .get_wpa3_per_band(&network_id, parent.as_ref())
            .await
    }

    /// Sets WPA3 mode per band — returns the raw Eero API response (unverified; may reboot the
    /// mesh).
    ///
    /// Ported from `set_wpa3_per_band` (`client.py:2745-2761`). Python's own signature is
    /// `set_wpa3_per_band(network_id=None, *, band_2_4_ghz=None, band_5_ghz=None)` — `network_id`
    /// positional-or-keyword first, then two keyword-only bands; this port keeps `network_id`
    /// as the trailing parameter instead, per this crate's own convention (every other wrapper in
    /// this file does the same). `auto_discover = false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` for an unrecognised mode, or if neither band is supplied — see
    /// [`crate::endpoints::wpa3::Wpa3Api::set_wpa3_per_band`]. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_wpa3_per_band(
        &self,
        band_2_4_ghz: Option<&str>,
        band_5_ghz: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        let response = self
            .api
            .wpa3()
            .set_wpa3_per_band(&network_id, band_2_4_ghz, band_5_ghz, parent.as_ref())
            .await?;
        self.invalidate_network_cache(&network_id);
        Ok(response)
    }
}
