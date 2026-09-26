//! `SubnetsApi`: `subnets` endpoints (`eero-api src/eero/api/subnets.py`, new in v8.0.0).
//!
//! `set_config` and `set_content_filters` forward the caller's mapping to the API unchanged —
//! neither field, key, nor value is validated here. The declared `SubnetConfig` fields are:
//! `dedicated_subnet`, `enabled`, `open_network`, `name`, `network_id`, `password`,
//! `rate_limit_pct`, `subnet_id`, `subnet_kind`, `subnet_type`, `wan_access`.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `SubnetsAPI` (`src/eero/api/subnets.py`, new in v8.0.0).
///
/// Build one with [`SubnetsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `SubnetsApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct SubnetsApi {
    transport: Arc<Transport>,
}

impl SubnetsApi {
    /// Wraps `transport` as a `SubnetsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the network's subnets configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/subnets.py:43-75` (`SubnetsAPI.get_config`). Sends
    /// `GET` [`crate::routes::subnets::SUBNETS_GET_CONFIG`], preferring `parent`'s published
    /// `subnets_config` link over the `networks/{id}/subnets_config` template when supplied.
    /// This is a verified read.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn get_config(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::subnets::SUBNETS_GET_CONFIG,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Creates or edits a subnet configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/subnets.py:78-107` (`SubnetsAPI.set_config`). Issues a
    /// JSON `PUT` to [`crate::routes::subnets::SUBNETS_SET_CONFIG`] with `config` forwarded
    /// unchanged — no `parent=` parameter in Python, unlike `get_config`. `config`'s `password`
    /// field, when present, is never logged. Logs
    /// [`crate::links::warn_uncharacterised_write`] (`"set subnet configuration for network"`)
    /// immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_config(&self, network_id: &str, config: Value) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("set subnet configuration for network");
        self.transport
            .resource(
                &routes::subnets::SUBNETS_SET_CONFIG,
                network_id,
                None,
                &[],
                RequestBody::Json(config),
            )
            .await
    }

    /// Deletes a subnet's configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/subnets.py:113-141` (`SubnetsAPI.delete_subnet`).
    /// Sends `DELETE` [`crate::routes::subnets::SUBNETS_DELETE_SUBNET`] with `network_id` and
    /// `subnet_type` (as returned in `get_config`'s `subnet_type` field). Logs
    /// [`crate::links::warn_uncharacterised_write`] (`"delete subnet for network"`) immediately
    /// before issuing the request — the log line never includes `subnet_type` itself.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn delete_subnet(
        &self,
        network_id: &str,
        subnet_type: &str,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("delete subnet for network");
        self.transport
            .nested(
                &routes::subnets::SUBNETS_DELETE_SUBNET,
                network_id,
                subnet_type,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Sets content filters for one or more subnets — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/subnets.py:143-177`
    /// (`SubnetsAPI.set_content_filters`). Issues a JSON `PUT` to
    /// [`crate::routes::subnets::SUBNETS_SET_CONTENT_FILTERS`] with `filters` forwarded
    /// unchanged — no `parent=` parameter. Logs [`crate::links::warn_uncharacterised_write`]
    /// (`"set subnet content filters for network"`) immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_content_filters(
        &self,
        network_id: &str,
        filters: Value,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("set subnet content filters for network");
        self.transport
            .resource(
                &routes::subnets::SUBNETS_SET_CONTENT_FILTERS,
                network_id,
                None,
                &[],
                RequestBody::Json(filters),
            )
            .await
    }

    /// Gets content filters for a subnet — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/subnets.py:181-203`
    /// (`SubnetsAPI.get_content_filters`). Sends `GET`
    /// [`crate::routes::subnets::SUBNETS_GET_CONTENT_FILTERS`] with `network_id` and `subnet_id`
    /// (as returned in `get_config`'s `subnet_id` field) — no `link=`/`parent=` at all, unlike
    /// `get_config`. This is a verified read.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn get_content_filters(
        &self,
        network_id: &str,
        subnet_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .nested(
                &routes::subnets::SUBNETS_GET_CONTENT_FILTERS,
                network_id,
                subnet_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }
}
