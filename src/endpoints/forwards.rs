//! Forwards API: `eero-api`'s `ForwardsAPI`.
//!
//! Ported from `eero-api src/eero/api/forwards.py`: `ForwardsAPI.get_forwards` plus the two
//! mutation methods, `create_forward` and `delete_forward`.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! Port-forward objects carry device MACs and internal IP addresses; nothing in this module
//! logs a response body (see the crate's security guidelines).

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `ForwardsAPI` (`src/eero/api/forwards.py`).
///
/// Build one with [`ForwardsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `ForwardsApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct ForwardsApi {
    transport: Arc<Transport>,
}

impl ForwardsApi {
    /// Wraps `transport` as a `ForwardsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the port forwards configured on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:33-54` (`ForwardsAPI.get_forwards`).
    /// Sends `GET` [`crate::routes::GET_FORWARDS`] with `network_id` substituted into the path.
    pub async fn get_forwards(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_FORWARDS, &[("network_id", network_id)], None)
            .await
    }

    /// Creates a port forward on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:56-79` (`ForwardsAPI.create_forward`).
    /// Sends `POST` [`crate::routes::CREATE_FORWARD`] with `forward_data` attached as the
    /// request's JSON body exactly as given. Neither Python nor this port validates or reshapes
    /// `forward_data` in any way; it is a pure passthrough.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn create_forward(
        &self,
        network_id: &str,
        forward_data: Value,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::CREATE_FORWARD,
                &[("network_id", network_id)],
                Some(forward_data),
            )
            .await
    }

    /// Deletes a port forward from a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:81-103` (`ForwardsAPI.delete_forward`).
    /// Sends `DELETE` [`crate::routes::DELETE_FORWARD`] with `network_id` and `forward_id`
    /// substituted into the path, and no body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn delete_forward(
        &self,
        network_id: &str,
        forward_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::DELETE_FORWARD,
                &[("network_id", network_id), ("forward_id", forward_id)],
                None,
            )
            .await
    }
}
