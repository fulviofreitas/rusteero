//! Updates API: `eero-api`'s `UpdatesAPI`.
//!
//! Ported from `eero-api src/eero/api/updates.py` (v8.0.4). `get_updates` existed at `v6.2.0`;
//! `apply_update` is new at v8.0.4.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `UpdatesAPI` (`src/eero/api/updates.py`).
///
/// Build one with [`UpdatesApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `UpdatesApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct UpdatesApi {
    transport: Arc<Transport>,
}

impl UpdatesApi {
    /// Wraps `transport` as an `UpdatesApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets available firmware/software updates for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/updates.py:35-60` (`UpdatesAPI.get_updates`). Sends
    /// `GET` [`crate::routes::updates::GET_UPDATES_V8`], preferring `parent`'s own published
    /// `updates` link.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`parent` cannot be resolved to a URL.
    /// Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_updates(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::updates::GET_UPDATES_V8,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Applies a pending update — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/updates.py:65-98` (`UpdatesAPI.apply_update`). Sends
    /// the literal two-byte body `""` ([`RequestBody::EmptyJsonString`]) to
    /// [`crate::routes::updates::APPLY_UPDATE`], preferring `parent`'s own published `updates`
    /// link. **Reboot-class write**: applying an update reboots every node on the network.
    /// Unverified against a live network: logs one `WARNING` via [`warn_uncharacterised_write`]
    /// before issuing the request (`updates.py:97`).
    ///
    /// # Errors
    ///
    /// See [`UpdatesApi::get_updates`].
    pub async fn apply_update(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        warn_uncharacterised_write("apply update for network — reboots every node");
        self.transport
            .resource(
                &routes::updates::APPLY_UPDATE,
                network_id,
                parent,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }
}
