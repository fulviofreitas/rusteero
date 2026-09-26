//! Device Blacklist API: `eero-api`'s `BlacklistAPI` — list, add and remove blacklisted
//! (blocked) devices.
//!
//! Ported from `eero-api src/eero/api/blacklist.py` at `v8.0.4`
//! (`.claude/tasks/briefs/v8/g3-devices.md`): `BlacklistAPI.get_blacklist`,
//! `BlacklistAPI.add_to_blacklist`, `BlacklistAPI.remove_from_blacklist`.
//!
//! `DevicesAPI.block_device`/`unblock_device` (`crate::endpoints::devices::DevicesApi`) delegate
//! entirely to [`BlacklistApi::add_to_blacklist`]/[`BlacklistApi::remove_from_blacklist`] below —
//! see that module's own docs.

use std::sync::Arc;

use reqwest::Method;
use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `BlacklistAPI` (`src/eero/api/blacklist.py`).
///
/// Build one with `BlacklistApi::new`, wrapping a `Transport` already shared with the rest of
/// the `EeroApi` aggregator — `BlacklistApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct BlacklistApi {
    transport: Arc<Transport>,
}

impl BlacklistApi {
    /// Wraps `transport` as a `BlacklistApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/blacklist` — list blacklisted (blocked) devices.
    ///
    /// Ported from `BlacklistAPI.get_blacklist` (`blacklist.py:72-96`). Prefers `parent`'s own
    /// published `device_blacklist` link over the `network_id` template when supplied
    /// (`routes::blacklist::V8_GET_BLACKLIST`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// [`Error::Validation`] if the resolved URL is malformed, or whatever other status-mapped
    /// error the request produces.
    pub async fn get_blacklist(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::blacklist::V8_GET_BLACKLIST,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/blacklist` — add a device (by MAC) to the blacklist.
    ///
    /// Ported from `BlacklistAPI.add_to_blacklist` (`blacklist.py:98-137`). Sends a
    /// **form-encoded** body `mac=<mac>` (`data={"mac": mac}`, `blacklist.py:137`) — **not**
    /// JSON, a breaking encoding change from the pre-8.0.0 shape this crate previously shipped.
    /// `mac` is passed through unchanged — no normalisation of separators or case happens here.
    /// Logs the fixed uncharacterised-write warning before issuing the request
    /// (`blacklist.py:135`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// [`Error::Validation`] if the resolved URL is malformed, or whatever other status-mapped
    /// error the request produces.
    pub async fn add_to_blacklist(
        &self,
        network_id: &str,
        mac: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("add_to_blacklist");
        self.transport
            .resource(
                &routes::blacklist::V8_ADD_TO_BLACKLIST,
                network_id,
                parent,
                &[],
                RequestBody::Form(vec![("mac".to_owned(), mac.to_owned())]),
            )
            .await
    }

    /// `DELETE /2.2/networks/{network_id}/blacklist/{mac_or_device_id}` — remove a device from
    /// the blacklist.
    ///
    /// Ported from `BlacklistAPI.remove_from_blacklist` (`blacklist.py:139-166`): resolves
    /// [`crate::routes::blacklist::V8_GET_BLACKLIST`] (preferring `parent`'s own published
    /// `device_blacklist` link, exactly like [`BlacklistApi::get_blacklist`]), then appends
    /// `mac_or_device_id` via [`crate::links::child_url`], which validates it as a single
    /// path-segment identifier **before** any request is sent — a hostile value (e.g.
    /// `"a/../../account"`, `"abc?x=1"`) is rejected locally, never reaching the network
    /// (`blacklist.py:87-89`'s `_validate_identifier`, ported to
    /// [`crate::links::validate_identifier`]).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "id"` if `mac_or_device_id` is not a single
    /// path-segment identifier, before any request is sent. Otherwise as
    /// [`BlacklistApi::get_blacklist`].
    pub async fn remove_from_blacklist(
        &self,
        network_id: &str,
        mac_or_device_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let base = routes::blacklist::V8_GET_BLACKLIST.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        let url = crate::links::child_url(&base, mac_or_device_id)?;
        self.transport
            .request(Method::DELETE, url, &[], RequestBody::None)
            .await
    }
}
