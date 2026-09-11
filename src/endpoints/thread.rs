//! Thread API: read-only Thread (smart-home mesh) status for a network.
//!
//! Ported from `eero-api src/eero/api/thread.py`. Python's `ThreadAPI` has exactly one method
//! (`get_thread`) and no mutating counterpart — there is no phase-5 work for this module; a
//! later reader need not look for one.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `ThreadAPI` (`src/eero/api/thread.py`): a single read-only method returning a
/// network's Thread status.
///
/// Build one with [`ThreadApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `ThreadApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct ThreadApi {
    transport: Arc<Transport>,
}

impl ThreadApi {
    /// Wraps `transport` as a `ThreadApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets Thread status for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/thread.py:33-54` (`ThreadAPI.get_thread`). Sends `GET`
    /// [`crate::routes::GET_THREAD`] with `network_id` substituted into the path.
    pub async fn get_thread(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_THREAD, &[("network_id", network_id)], None)
            .await
    }
}
