//! Updates API: read-only firmware/software update information for a network.
//!
//! Ported from `eero-api src/eero/api/updates.py`. Python's `UpdatesAPI` has exactly one method
//! (`get_updates`) and no mutating counterpart — there is no phase-5 work for this module; a
//! later reader need not look for one.
//!
//! (The task brief that commissioned this module flagged `updates` as the one module in this
//! batch expected to carry phase-5 work; the Python source does not bear that out — see the
//! handoff report for the discrepancy.)

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `UpdatesAPI` (`src/eero/api/updates.py`): a single read-only method returning a
/// network's available firmware/software updates.
///
/// Build one with [`UpdatesApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `UpdatesApi` never constructs or owns a
/// `Transport` itself.
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
    /// Ported from `eero-api src/eero/api/updates.py:33-54` (`UpdatesAPI.get_updates`). Sends
    /// `GET` [`crate::routes::GET_UPDATES`] with `network_id` substituted into the path.
    pub async fn get_updates(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_UPDATES, &[("network_id", network_id)], None)
            .await
    }
}
