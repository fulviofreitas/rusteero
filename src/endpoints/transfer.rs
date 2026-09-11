//! Transfer API: `eero-api`'s `TransferAPI`, in full — its only method is a `GET`.
//!
//! Ported from `eero-api src/eero/api/transfer.py`. There is no phase-5 mutation method for
//! this module (`TransferAPI` has exactly one method).
//!
//! [`TransferApi::get_transfer_stats`] funnels through [`crate::transport::Transport::send`],
//! which already implements the "not authenticated" precondition Python repeats at the top of
//! each method (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and
//! every status-to-error mapping a response can produce — so, unlike the Python source, this
//! method does not duplicate that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

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
    /// Ported from `eero-api src/eero/api/transfer.py:33-62` (`TransferAPI.get_transfer_stats`).
    /// Python selects the request path at call time based on whether `device_id` was supplied;
    /// this method does the same by selecting between two distinct `Route` constants rather than
    /// templating one path over the other:
    ///
    /// - `device_id.is_none()`: sends `GET` [`crate::routes::GET_TRANSFER_STATS`]
    ///   (`networks/{network_id}/transfer`).
    /// - `device_id.is_some()`: sends `GET` [`crate::routes::GET_DEVICE_TRANSFER_STATS`]
    ///   (`networks/{network_id}/devices/{device_id}/transfer`).
    ///
    /// Divergence from `eero-api`: Python's `if device_id:` also falls back to the network-level
    /// path for an *empty* `device_id` string, not just `None`. This method takes `Some("")` as a
    /// (degenerate) device id and renders the device-level path — real Eero device ids are never
    /// empty, so this is a documented edge case, not a behavioural change for any real caller.
    pub async fn get_transfer_stats(
        &self,
        network_id: &str,
        device_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        match device_id {
            Some(device_id) => {
                self.transport
                    .send(
                        &routes::GET_DEVICE_TRANSFER_STATS,
                        &[("network_id", network_id), ("device_id", device_id)],
                        None,
                    )
                    .await
            }
            None => {
                self.transport
                    .send(
                        &routes::GET_TRANSFER_STATS,
                        &[("network_id", network_id)],
                        None,
                    )
                    .await
            }
        }
    }
}
