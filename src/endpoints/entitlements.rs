//! `EntitlementsApi`: `entitlements` endpoints (`eero-api src/eero/api/entitlements.py`, new in
//! v8.0.0).
//!
//! Ported from `eero-api src/eero/api/entitlements.py` (v8.0.4). This module makes no attempt to
//! interpret premium/subscription status from any of the envelopes it returns — that
//! interpretation belongs entirely to the caller.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `EntitlementsAPI` (`src/eero/api/entitlements.py`, new in v8.0.0).
///
/// Build one with [`EntitlementsApi::new`], wrapping a [`Transport`] already shared with the
/// rest of the [`crate::api::EeroApi`] aggregator — `EntitlementsApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct EntitlementsApi {
    transport: Arc<Transport>,
}

impl EntitlementsApi {
    /// Wraps `transport` as an `EntitlementsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the network's entitled features — returns the raw Eero API response.
    ///
    /// Ported from `EntitlementsAPI.get_features` (`entitlements.py:37-61`). Sends `GET`
    /// [`crate::routes::entitlements::GET_FEATURES`] with `network_id` substituted into the
    /// path (bare id, host-relative path, or absolute API URL — no `parent`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn get_features(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::entitlements::GET_FEATURES,
                network_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Gets the network's upsell features — returns the raw Eero API response.
    ///
    /// Ported from `EntitlementsAPI.get_upsell_features` (`entitlements.py:62-86`). Sends `GET`
    /// [`crate::routes::entitlements::GET_UPSELL_FEATURES`].
    ///
    /// # Errors
    ///
    /// See [`EntitlementsApi::get_features`].
    pub async fn get_upsell_features(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::entitlements::GET_UPSELL_FEATURES,
                network_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Gets eero model capabilities for a network — returns the raw Eero API response.
    ///
    /// Ported from `EntitlementsAPI.get_model_capabilities` (`entitlements.py:87-116`). Sends
    /// `GET` [`crate::routes::entitlements::GET_MODEL_CAPABILITIES`] (a fixed path, no `{id}`)
    /// with `network_id` sent **verbatim** as the `networkId` query parameter — this endpoint has
    /// no path-scoped variant, so id/path/URL polymorphism does not apply here; a caller who
    /// passes a path or absolute URL gets it echoed unchanged into the query string.
    ///
    /// # Errors
    ///
    /// See [`EntitlementsApi::get_features`].
    pub async fn get_model_capabilities(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::entitlements::GET_MODEL_CAPABILITIES,
                "",
                None,
                &[("networkId", network_id.to_owned())],
                RequestBody::None,
            )
            .await
    }

    /// Gets the premium customer record — returns the raw Eero API response.
    ///
    /// Ported from `EntitlementsAPI.get_premium_customer` (`entitlements.py:117-136`). Sends
    /// `GET` [`crate::routes::entitlements::GET_PREMIUM_CUSTOMER`] (a fixed path, not
    /// network-scoped, no arguments at all).
    ///
    /// # Errors
    ///
    /// See [`EntitlementsApi::get_features`].
    pub async fn get_premium_customer(&self) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::entitlements::GET_PREMIUM_CUSTOMER,
                "",
                None,
                &[],
                RequestBody::None,
            )
            .await
    }
}
