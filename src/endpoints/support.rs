//! Support API: `eero-api`'s `SupportAPI`.
//!
//! Ported from `eero-api src/eero/api/support.py` (v8.4.1): `SupportAPI.get_support` and
//! `SupportAPI.request_support`.
//!
//! A support request can carry account details; nothing in this module logs a response body (see
//! the crate's security guidelines).

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::{rewrite_version, warn_uncharacterised_write};
use crate::routes::{self, ApiVersion};
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `SupportAPI` (`src/eero/api/support.py`).
///
/// Build one with [`SupportApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `SupportApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct SupportApi {
    transport: Arc<Transport>,
}

impl SupportApi {
    /// Wraps `transport` as a `SupportApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets support information for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/support.py:35-69` (`SupportAPI.get_support`). Sends
    /// `GET` [`crate::routes::support::GET_SUPPORT_V8`], preferring `parent`'s own published
    /// `support` link, and always on API version 2.2: the resolved URL is re-pinned with
    /// [`crate::links::rewrite_version`] whichever form `network_id` takes (a bare id, a path or an
    /// absolute URL, on 2.2 or 2.3) and whatever version the published link names. The API
    /// publishes this link on 2.3 for a current `User-Agent`, and 2.3 answers with a `contacts`
    /// array where 2.2 answers with a flat object, so following the link would change the shape
    /// of the response. [`SupportApi::request_support`] is not pinned.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_support(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::support::GET_SUPPORT_V8.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        let url = rewrite_version(&url, ApiVersion::V2_2);
        self.transport
            .request(
                routes::support::GET_SUPPORT_V8.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Files a support request for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/support.py:71-110` (`SupportAPI.request_support`).
    /// Sends `POST` [`crate::routes::support::REQUEST_SUPPORT_V8`] with `request_data` attached as
    /// the request's JSON body exactly as given (Python's one write in this group that keeps JSON
    /// encoding, `support.py:107-110`), preferring `parent`'s own published `support` link. Unlike
    /// [`SupportApi::get_support`] the version is **not** pinned: the link is followed on whatever
    /// version it names. Neither Python nor this port validates or reshapes `request_data` in any
    /// way; it is a pure passthrough. Unverified against a live account: logs one `WARNING` via
    /// [`warn_uncharacterised_write`] before issuing the request (`support.py:104`).
    ///
    /// # Errors
    ///
    /// See [`SupportApi::get_support`].
    pub async fn request_support(
        &self,
        network_id: &str,
        request_data: Value,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::support::REQUEST_SUPPORT_V8.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("request support for network");
        self.transport
            .request(
                routes::support::REQUEST_SUPPORT_V8.method.clone(),
                url,
                &[],
                RequestBody::Json(request_data),
            )
            .await
    }
}
