//! Smart Queue Management (SQM/QoS) API: `eero-api`'s `SqmAPI`, v8.0.4.
//!
//! Ported from `eero-api src/eero/api/sqm.py` at v8.0.4 (module docstring, `sqm.py:1-9`): SQM is
//! a single boolean toggle on the network's `settings` resource, written as a **query
//! parameter with no body** — there is no API counterpart for per-direction bandwidth limits or
//! an explicit "auto" mode. This entirely replaces the pre-v7.0.0 shape this file used to have
//! (four setters — `set_sqm_enabled`/`set_sqm_bandwidth`/`configure_sqm`/`set_sqm_auto` — each
//! writing a JSON body the API never declared, per that source's own `TODO: Verify` comments,
//! now confirmed dead; g5 brief §1, §2).

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::params::resolve_network_url;
use crate::routes;
use crate::routes::ApiVersion;
use crate::transport::{RequestBody, Transport};

/// `SqmAPI` (`src/eero/api/sqm.py`), v8.0.4.
///
/// Build one with [`SqmApi::new`], wrapping a [`Transport`] shared with the rest of the
/// [`crate::api::EeroApi`] aggregator — `SqmApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct SqmApi {
    transport: Arc<Transport>,
}

impl SqmApi {
    /// Wraps `transport` as a `SqmApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets SQM (Smart Queue Management) / `QoS` settings for a network — returns the raw Eero
    /// API response.
    ///
    /// Ported from `eero-api src/eero/api/sqm.py:50-77` (`SqmAPI.get_sqm_settings`). Resolves via
    /// the module's own `_network_own_url` helper (`sqm.py:24-29`, byte-for-byte identical to
    /// `security.py`'s), which prefers a supplied `parent`'s own `url` field over the bare-id
    /// `networks/{id}` template — byte-for-byte [`crate::params::resolve_network_url`], which
    /// this method calls directly (see `src/routes/sqm.rs`'s module docs for why this is not a
    /// [`crate::routes::Resource`]). SQM settings are part of the full network object, under the
    /// `sqm` field.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_sqm_settings(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            ApiVersion::V2_2,
        )?;
        self.transport
            .request(reqwest::Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Enables or disables SQM (Smart Queue Management) — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/sqm.py:79-125` (`SqmAPI.set_sqm`). Issues a `PUT` with
    /// **no request body** to the network's `settings` link, carrying the new value as the `sqm`
    /// query parameter — the literal lowercase strings `"true"`/`"false"`, not a JSON boolean
    /// (`sqm.py:127-131`). Replaces the four pre-v7.0.0 setters this file used to have — see the
    /// module docs.
    ///
    /// # Errors
    ///
    /// See [`SqmApi::get_sqm_settings`].
    pub async fn set_sqm(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        warn_uncharacterised_write("set SQM for network");
        let query_value = if enabled { "true" } else { "false" };
        self.transport
            .resource(
                &routes::sqm::SQM_PUT_SETTINGS,
                network_id,
                parent,
                &[("sqm", query_value.to_owned())],
                RequestBody::None,
            )
            .await
    }
}
