//! `DdnsApi`: `ddns` endpoints (`eero-api src/eero/api/ddns.py`, new in v8.0.0).
//!
//! Both operations are parameterless `PUT`s: the API declares no request body for either
//! endpoint, so neither method sends one — [`RequestBody::None`], mirroring Python's
//! `RequestEncoding.NONE`.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `DdnsAPI` (`src/eero/api/ddns.py`, new in v8.0.0).
///
/// Build one with [`DdnsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `DdnsApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct DdnsApi {
    transport: Arc<Transport>,
}

impl DdnsApi {
    /// Wraps `transport` as a `DdnsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Enables dynamic DNS for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/ddns.py:40-79` (`DdnsAPI.enable`). Issues a `PUT` with
    /// no request body to [`crate::routes::ddns::DDNS_ENABLE`], preferring `parent`'s published
    /// `ddns_enable` link over the `networks/{id}/ddns/enable` template when supplied. Logs
    /// [`crate::links::warn_uncharacterised_write`] (`"enable DDNS for network"`) immediately
    /// before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn enable(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("enable DDNS for network");
        self.transport
            .resource(
                &routes::ddns::DDNS_ENABLE,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Disables dynamic DNS for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/ddns.py:82-120` (`DdnsAPI.disable`). Issues a `PUT`
    /// with no request body to [`crate::routes::ddns::DDNS_DISABLE`], preferring `parent`'s
    /// published `ddns_disable` link over the `networks/{id}/ddns/disable` template when
    /// supplied. Logs [`crate::links::warn_uncharacterised_write`] (`"disable DDNS for
    /// network"`) immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn disable(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("disable DDNS for network");
        self.transport
            .resource(
                &routes::ddns::DDNS_DISABLE,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }
}
