//! Data Usage API: `eero-api`'s `DataUsageAPI`, whose one method sends a `GET` request with a
//! JSON body attached.
//!
//! Ported from `eero-api src/eero/api/data_usage.py`. Python's own module docstring
//! (`data_usage.py:1-5`) and its `get_data_usage` docstring (`data_usage.py:40-44`) both flag
//! this as unusual: the Eero cloud API expects a JSON body on a `GET` request (timezone/period
//! filters), a shape `reqwest` supports and this crate's [`Transport::send`] already sends
//! unmodified — see `tests/transport.rs`'s
//! `a_get_request_with_a_json_body_actually_arrives_with_that_body` for proof the body reaches
//! the wire on a `GET`.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `DataUsageAPI` (`src/eero/api/data_usage.py`).
///
/// Build one with [`DataUsageApi::new`], wrapping a [`Transport`] already shared with the rest
/// of the (not-yet-built) `EeroApi` aggregator — `DataUsageApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct DataUsageApi {
    transport: Arc<Transport>,
}

impl DataUsageApi {
    /// Wraps `transport` as a `DataUsageApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets data usage statistics for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/data_usage.py:33-64` (`DataUsageAPI.get_data_usage`).
    /// Sends `GET` [`crate::routes::GET_DATA_USAGE`] (`networks/{network_id}/data_usage`) when
    /// `resource` is `None`, or [`crate::routes::GET_DATA_USAGE_RESOURCE`]
    /// (`networks/{network_id}/data_usage/{resource}`) when `resource` is `Some` — mirroring
    /// the `path = f"{path}/{resource}"` branch at `data_usage.py:58-60`.
    ///
    /// `payload` is attached as the request's JSON body exactly as given, on a `GET` request —
    /// see this module's docs for why that is intentional, not a mistake. Neither Python nor
    /// this port validates or reshapes `payload` in any way; it is a pure passthrough.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_data_usage(
        &self,
        network_id: &str,
        payload: Value,
        resource: Option<&str>,
    ) -> Result<Envelope, Error> {
        match resource {
            Some(resource) => {
                self.transport
                    .send(
                        &routes::GET_DATA_USAGE_RESOURCE,
                        &[("network_id", network_id), ("resource", resource)],
                        Some(payload),
                    )
                    .await
            }
            None => {
                self.transport
                    .send(
                        &routes::GET_DATA_USAGE,
                        &[("network_id", network_id)],
                        Some(payload),
                    )
                    .await
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // `eero-api src/eero/api/data_usage.py` has exactly one method (`get_data_usage`) and no
    // mutating counterpart — there is no phase-5 work for this module; a later reader need not
    // look for one.
    // ---------------------------------------------------------------------------------------
}
