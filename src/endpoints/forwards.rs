//! Forwards API: `eero-api`'s `ForwardsAPI`.
//!
//! Ported from `eero-api src/eero/api/forwards.py` (v8.0.4): `ForwardsAPI.get_forwards`,
//! `create_forward`, `update_forward` and `delete_forward`.
//!
//! Every method here funnels through [`crate::transport::Transport::resource`]/
//! [`crate::transport::Transport::nested`]/[`crate::transport::Transport::request`], which
//! already implement the "not authenticated" precondition Python repeats at the top of each
//! method (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! Port-forward objects carry device MACs and internal IP addresses; nothing in this module
//! logs a response body (see the crate's security guidelines).
//!
//! # `update_forward`'s `forward` argument
//!
//! Python's `update_forward(forward: Any, data, *, network=None)` accepts either a bare id, a
//! host-relative path / absolute URL, or the forward's own cached envelope (a `Mapping`) for
//! `forward` (`forwards.py:26-63`, the free function `_resolve_forward_url`). This port keeps
//! the same three-way dispatch but expresses it across two parameters instead of one
//! dynamically-typed one: [`ForwardsApi::update_forward`] takes `forward: &str` (a bare id, a
//! host-relative path, or an absolute API URL) plus an optional `parent: Option<&Value>` that
//! stands in for the `Mapping` case — when `parent` is supplied, it is resolved via
//! [`crate::links::self_url`] exactly like Python's `Mapping` branch, and `forward`/`network`
//! are ignored entirely, matching Python's own dispatch order (the `Mapping` check runs first).

use std::sync::Arc;

use serde_json::Value;
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes;
use crate::transport::{RequestBody, Transport};

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
    /// Ported from `eero-api src/eero/api/forwards.py:80-105` (v8.0.4,
    /// `ForwardsAPI.get_forwards`). Sends `GET` [`crate::routes::forwards::FORWARDS_GET`],
    /// preferring `parent`'s published `forwards` link over the `networks/{id}/forwards`
    /// template when supplied.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn get_forwards(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::forwards::FORWARDS_GET,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Creates a port forward on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:113-151` (v8.0.4,
    /// `ForwardsAPI.create_forward`). Sends `POST`
    /// [`crate::routes::forwards::FORWARDS_CREATE`] with `forward_data` attached as the
    /// request's JSON body exactly as given — neither Python nor this port validates or
    /// reshapes it. Logs [`crate::links::warn_uncharacterised_write`] (`"create forward for
    /// network"`) immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn create_forward(
        &self,
        network_id: &str,
        forward_data: Value,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("create forward for network");
        self.transport
            .resource(
                &routes::forwards::FORWARDS_CREATE,
                network_id,
                parent,
                &[],
                RequestBody::Json(forward_data),
            )
            .await
    }

    /// Updates a port forward via its own URL — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:153-184` (v8.0.4,
    /// `ForwardsAPI.update_forward`, via the free function `_resolve_forward_url`). See the
    /// module docs for how the id-or-path-or-URL-or-envelope dispatch maps onto this method's
    /// `forward`/`network`/`parent` parameters. Sends `PUT` to the resolved URL with `data`
    /// attached as the request's JSON body exactly as given. Logs
    /// [`crate::links::warn_uncharacterised_write`] with the operation string `"update_forward"`
    /// — the literal method name, **not** a human-readable phrase like every other write in this
    /// module; this is an intentional quirk of the Python source (`forwards.py:181`), preserved
    /// verbatim rather than "fixed" to match its neighbours.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "forward"` if `parent` is supplied but has no
    /// resolvable `url` field. Returns [`Error::Validation`] with `field: "network"` if `parent`
    /// is `None`, `forward` is a bare id (does not start with `http://`, `https://`, or `/`),
    /// and `network` is `None`. Returns [`Error::Authentication`] if no valid session is
    /// configured, or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn update_forward(
        &self,
        forward: &str,
        data: Value,
        network: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve_forward_url(forward, network, parent)?;
        links::warn_uncharacterised_write("update_forward");
        self.transport
            .request(reqwest::Method::PUT, url, &[], RequestBody::Json(data))
            .await
    }

    /// Deletes a port forward from a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/forwards.py:186-203` (v8.0.4,
    /// `ForwardsAPI.delete_forward`). Sends `DELETE`
    /// [`crate::routes::forwards::FORWARDS_DELETE`] with `network_id` (required, unlike
    /// `update_forward`) and `forward` (a bare id, path, or absolute URL) resolved via
    /// `resolve_nested_url`'s dispatch, and no body. Logs
    /// [`crate::links::warn_uncharacterised_write`] (`"delete forward for network"`) immediately
    /// before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn delete_forward(&self, network_id: &str, forward: &str) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("delete forward for network");
        self.transport
            .nested(
                &routes::forwards::FORWARDS_DELETE,
                network_id,
                forward,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Resolves a forward's absolute URL from a bare id, a path/URL, or a cached envelope.
    ///
    /// Ported from `_resolve_forward_url` (`eero-api src/eero/api/forwards.py:26-63`); see the
    /// module docs for how the three Python branches map onto `forward`/`network`/`parent`.
    fn resolve_forward_url(
        &self,
        forward: &str,
        network: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let host = self.transport.api_host();
        if let Some(parent) = parent {
            return links::self_url(host, parent)?.ok_or_else(|| {
                Error::validation("forward", "envelope has no resolvable 'url' field")
            });
        }
        if forward.starts_with("http://")
            || forward.starts_with("https://")
            || forward.starts_with('/')
        {
            return routes::forwards::FORWARDS_UPDATE.resolve(
                host,
                network.unwrap_or(""),
                forward,
                None,
            );
        }
        let network = network.ok_or_else(|| {
            Error::validation(
                "network",
                "required when 'forward' is a bare ID rather than a path/URL",
            )
        })?;
        routes::forwards::FORWARDS_UPDATE.resolve(host, network, forward, None)
    }
}
