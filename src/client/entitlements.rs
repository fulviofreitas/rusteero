//! `Client` methods for the `EntitlementsAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/entitlements.py` (v8.0.4) by way of `EeroClient`'s own
//! `entitlements`-scoped wrappers in `client.py:2294-2312`. None of these pass `parent=` — the
//! domain methods themselves accept none (see `crate::endpoints::entitlements`).
//!
//! **Facade-placement note** (`.claude/tasks/briefs/v8/g1-networks.md` §6): Python exposes
//! `get_premium_customer` under `EeroClient`'s "Session & Account" docs group (`wiki:96`), not
//! alongside `EntitlementsAPI`'s other three methods (grouped under "Networks", `wiki:119-125`,
//! and named `get_entitlement_features`/`get_upsell_features`/`get_model_capabilities` there) —
//! reproduced by [`Client::get_premium_customer`] taking no `network_id` at all, unlike its three
//! siblings below.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the network's entitled features — returns the raw Eero API response (verified read).
    ///
    /// Ported from `get_entitlement_features()` (`client.py:2294-2299`). `auto_discover = false`
    /// — see [`Client::get_diagnostics`]. Any premium-status interpretation belongs to the
    /// caller.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_entitlement_features(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.entitlements().get_features(&network_id).await
    }

    /// Gets the network's upsell features — returns the raw Eero API response (verified read).
    ///
    /// Ported from `get_upsell_features()` (`client.py:2301-2304`). `auto_discover = false`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_upsell_features(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .entitlements()
            .get_upsell_features(&network_id)
            .await
    }

    /// Gets eero model capabilities for the network — returns the raw Eero API response.
    ///
    /// Ported from `get_model_capabilities()` (`client.py:2306-2309`). `auto_discover = false`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_model_capabilities(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .entitlements()
            .get_model_capabilities(&network_id)
            .await
    }

    /// Gets the account's premium customer record — returns the raw Eero API response.
    ///
    /// Ported from `get_premium_customer()` (`client.py:2311-2313`). No `network_id` at all —
    /// see this module's own docs for the facade-placement note.
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn get_premium_customer(&self) -> Result<Envelope, Error> {
        self.api.entitlements().get_premium_customer().await
    }
}
