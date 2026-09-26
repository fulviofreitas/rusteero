//! `PermissionsApi`: `permissions` endpoints (`eero-api src/eero/api/permissions.py`, new in
//! v8.0.0).
//!
//! Ported from `eero-api src/eero/api/permissions.py` (v8.0.4).

use std::sync::Arc;

use reqwest::Method;
use serde_json::Value;
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::params::resolve_network_url;
use crate::routes::ApiVersion;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `PermissionsAPI` (`src/eero/api/permissions.py`, new in v8.0.0).
///
/// Build one with [`PermissionsApi::new`], wrapping a [`Transport`] already shared with the rest
/// of the [`crate::api::EeroApi`] aggregator — `PermissionsApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct PermissionsApi {
    transport: Arc<Transport>,
}

impl PermissionsApi {
    /// Wraps `transport` as a `PermissionsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the caller's permissions on a network — returns the raw Eero API response.
    ///
    /// Ported from `PermissionsAPI.get_permissions` (`permissions.py:35-60`). Returns
    /// `permissions` (a per-capability mapping) and `role`. Resolves the URL as
    /// `f"{resolve_network_url(network_id, parent)}/permissions"`
    /// ([`crate::params::resolve_network_url`], which prefers `parent`'s own top-level `url`
    /// field over the `networks/{id}` template — see
    /// [`crate::routes::permissions::PERMISSIONS_GET_PERMISSIONS`]'s doc comment for why this is not an
    /// ordinary [`crate::routes::Resource::resolve`] call).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_permissions(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.permissions_url(network_id, parent)?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Resolves the permissions sub-path: the network's own URL (self-link preferring), plus the
    /// literal `/permissions` suffix — `permissions.py:60`.
    fn permissions_url(&self, network_id: &str, parent: Option<&Value>) -> Result<Url, Error> {
        let host = self.transport.api_host();
        let network_url = resolve_network_url(host, network_id, parent, ApiVersion::V2_2)?;
        let joined = format!("{}/permissions", network_url.as_str().trim_end_matches('/'));
        Url::parse(&joined)
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
    }
}
