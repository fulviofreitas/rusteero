//! Support API: `eero-api`'s `SupportAPI`.
//!
//! Ported from `eero-api src/eero/api/support.py`: `SupportAPI.get_support` and
//! `SupportAPI.request_support`.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! A support request can carry account details; nothing in this module logs a response body
//! (see the crate's security guidelines).

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

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
    /// Ported from `eero-api src/eero/api/support.py:33-54` (`SupportAPI.get_support`). Sends
    /// `GET` [`crate::routes::GET_SUPPORT`] with `network_id` substituted into the path.
    pub async fn get_support(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_SUPPORT, &[("network_id", network_id)], None)
            .await
    }

    /// Files a support request for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/support.py:56-81` (`SupportAPI.request_support`).
    /// Sends `POST` [`crate::routes::REQUEST_SUPPORT`] with `request_data` attached as the
    /// request's JSON body exactly as given. Neither Python nor this port validates or reshapes
    /// `request_data` in any way; it is a pure passthrough.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn request_support(
        &self,
        network_id: &str,
        request_data: Value,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::REQUEST_SUPPORT,
                &[("network_id", network_id)],
                Some(request_data),
            )
            .await
    }
}
