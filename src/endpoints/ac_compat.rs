//! AC Compatibility API: read-only AC-compatibility information for a network.
//!
//! Ported from `eero-api src/eero/api/ac_compat.py`. Python's `ACCompatAPI` has exactly one
//! method (`get_ac_compat`) and no mutating counterpart — there is no phase-5 work for this
//! module; a later reader need not look for one.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `ACCompatAPI` (`src/eero/api/ac_compat.py`): a single read-only method returning
/// a network's AC-compatibility information.
///
/// Build one with [`ACCompatApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `ACCompatApi` never constructs or owns a
/// `Transport` itself.
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
    /// Ported from `eero-api src/eero/api/ac_compat.py:33-54` (`ACCompatAPI.get_ac_compat`).
    /// Sends `GET` [`crate::routes::GET_AC_COMPAT`] with `network_id` substituted into the path.
    pub async fn get_ac_compat(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_AC_COMPAT, &[("network_id", network_id)], None)
            .await
    }
}
