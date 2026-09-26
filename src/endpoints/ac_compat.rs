//! AC Compatibility API: read-only AC-compatibility information for a network.
//!
//! Ported from `eero-api src/eero/api/ac_compat.py` (v8.0.4). Python's `ACCompatAPI` has exactly
//! one method (`get_ac_compat`) and no mutating counterpart.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `ACCompatAPI` (`src/eero/api/ac_compat.py`): a single read-only method returning
/// a network's AC-compatibility information.
///
/// Build one with [`ACCompatApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `ACCompatApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct ACCompatApi {
    transport: Arc<Transport>,
}

impl ACCompatApi {
    /// Wraps `transport` as an `ACCompatApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets AC compatibility information for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/ac_compat.py:35-60` (`ACCompatAPI.get_ac_compat`).
    /// Sends `GET` [`crate::routes::ac_compat::GET_AC_COMPAT_V8`], preferring `parent`'s own
    /// published `ac_compat` link.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_ac_compat(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::ac_compat::GET_AC_COMPAT_V8,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }
}
