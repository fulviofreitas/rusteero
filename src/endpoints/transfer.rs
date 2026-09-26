//! Transfer API: `eero-api`'s `TransferAPI`, in full — its only method is a `GET`.
//!
//! Ported from `eero-api src/eero/api/transfer.py` (v8.0.4). `TransferAPI` has exactly one
//! method.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `TransferAPI` (`src/eero/api/transfer.py`).
///
/// Build one with [`TransferApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `TransferApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct TransferApi {
    transport: Arc<Transport>,
}

impl TransferApi {
    /// Wraps `transport` as a `TransferApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets transfer statistics for a network, or for a single device on that network — returns
    /// the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/transfer.py:36-79` (`TransferAPI.get_transfer_stats`).
    /// Python selects the request path at call time based on whether `device_id` was supplied;
    /// this method does the same by selecting between a [`crate::routes::Resource`] and a
    /// [`crate::routes::Nested`] route:
    ///
    /// - `device_id.is_none()`: sends `GET` [`crate::routes::transfer::GET_TRANSFER_STATS_V8`]
    ///   (`networks/{id}/transfer`), preferring `parent`'s own published `transfer` link.
    /// - `device_id.is_some()`: sends `GET`
    ///   [`crate::routes::transfer::GET_DEVICE_TRANSFER_STATS_V8`]
    ///   (`networks/{network}/devices/{device}/transfer`), a **literal** path — `parent` is
    ///   ignored on this branch, exactly like Python (`transfer.py:75`, no `parent=` argument at
    ///   all on the `device_id` branch). `resolve_nested_url`'s anti-double-format guard
    ///   ([`crate::params::resolve_nested_url`]/`_require_nested_family`) protects a `device_id`
    ///   containing a stray `{`/`}` from corrupting the rendered template — the exact regression
    ///   `test_get_transfer_stats_device_id_with_brace_does_not_break_template` pins.
    ///
    /// Matches Python's `if device_id:` exactly (`transfer.py:71`): a truthy check, not an
    /// `is None` check. `Some("")` is therefore treated identically to `None` — the network-level
    /// path is used, `parent` is honoured, and the (never actually reachable) device-level branch
    /// is not taken for an empty device id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `network_id`/`device_id`/`parent` cannot be resolved to a
    /// URL. Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_transfer_stats(
        &self,
        network_id: &str,
        device_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        match device_id.filter(|id| !id.is_empty()) {
            Some(device_id) => {
                self.transport
                    .nested(
                        &routes::transfer::GET_DEVICE_TRANSFER_STATS_V8,
                        network_id,
                        device_id,
                        None,
                        &[],
                        RequestBody::None,
                    )
                    .await
            }
            None => {
                self.transport
                    .resource(
                        &routes::transfer::GET_TRANSFER_STATS_V8,
                        network_id,
                        parent,
                        &[],
                        RequestBody::None,
                    )
                    .await
            }
        }
    }
}
