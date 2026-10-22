//! Updates API: `eero-api`'s `UpdatesAPI`.
//!
//! Ported from `eero-api src/eero/api/updates.py` (v8.4.1). `get_updates` existed at `v6.2.0`;
//! `apply_update` is new at v8.0.4 and `set_preferred_update_hour` at v8.2.0.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::params::require_hour;
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
    /// Ported from `eero-api src/eero/api/updates.py:36-64` (`UpdatesAPI.get_updates`). Sends
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
    /// Ported from `eero-api src/eero/api/updates.py:66-105` (`UpdatesAPI.apply_update`). Sends
    /// the literal two-byte body `""` ([`RequestBody::EmptyJsonString`]) to
    /// [`crate::routes::updates::UPDATES_APPLY_UPDATE`], preferring `parent`'s own published `updates`
    /// link. **Reboot-class write**: applying an update reboots every node on the network.
    /// Unverified against a live network: logs one `WARNING` via [`warn_uncharacterised_write`]
    /// before issuing the request (`updates.py:102`).
    ///
    /// # Errors
    ///
    /// See [`UpdatesApi::get_updates`].
    pub async fn apply_update(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::updates::UPDATES_APPLY_UPDATE.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        warn_uncharacterised_write("apply update for network — reboots every node");
        self.transport
            .request(
                routes::updates::UPDATES_APPLY_UPDATE.method.clone(),
                url,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// Sets the hour of day the network applies firmware updates — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/updates.py:107-137`
    /// (`UpdatesAPI.set_preferred_update_hour`). Sends a JSON `POST` of exactly
    /// `{"preferred_update_hour": hour}` to
    /// [`crate::routes::updates::UPDATES_SET_PREFERRED_UPDATE_HOUR`]. `hour` is the hour of day in
    /// the network's timezone, `0` (midnight to 1 am) through `23` (11 pm to midnight). The
    /// current value is readable from the network envelope's `updates.preferred_update_hour`. The
    /// network envelope publishes no link to this resource, so the path is always built from the
    /// template and there is no `parent` parameter; `network_id` may be a bare id, a host-relative
    /// API path or an absolute API-host URL.
    ///
    /// **Unverified against a live account**: read the current value first, skip the write when
    /// it already matches, and never retry a failed write in a loop. Logs one `WARNING` via
    /// [`warn_uncharacterised_write`] after validation and URL resolution, immediately before the
    /// request (`updates.py:136`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with field `hour` if `hour` is outside `0..=23`. That check
    /// is the first statement (`updates.py:129`), so it wins over an unresolvable `network_id`
    /// and a missing session, and nothing is sent. Python also rejects a `bool`, `float`, `str` or
    /// `None` hour; the `i32` parameter makes those unrepresentable. Then
    /// [`Error::Validation`] if `network_id` cannot be resolved to a URL. Otherwise
    /// [`Error::Authentication`] if no valid session is configured (raised by the transport,
    /// which — unlike Python — runs after URL resolution and the warning), or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn set_preferred_update_hour(
        &self,
        network_id: &str,
        hour: i32,
    ) -> Result<Envelope, Error> {
        require_hour("hour", hour)?;
        let url = routes::updates::UPDATES_SET_PREFERRED_UPDATE_HOUR.resolve(
            self.transport.api_host(),
            network_id,
            None,
        )?;
        warn_uncharacterised_write("set preferred update hour for network");
        self.transport
            .request(
                routes::updates::UPDATES_SET_PREFERRED_UPDATE_HOUR
                    .method
                    .clone(),
                url,
                &[],
                RequestBody::Json(json!({ "preferred_update_hour": hour })),
            )
            .await
    }
}
