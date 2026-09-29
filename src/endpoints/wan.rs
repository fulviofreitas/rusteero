//! `WanApi`: `wan` endpoints (`eero-api src/eero/api/wan.py`, new in v8.0.0).
//!
//! This family is only served on API version 2.3 ([`crate::consts::API_VERSION_MULTISTATICIP`] /
//! [`crate::consts::API_VERSION_SECONDARY_WAN`]), unlike most of this SDK's default 2.2
//! endpoints.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `WanAPI` (`src/eero/api/wan.py`, new in v8.0.0).
///
/// Build one with [`WanApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `WanApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct WanApi {
    transport: Arc<Transport>,
}

impl WanApi {
    /// Wraps `transport` as a `WanApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the network's multi-static-IP configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wan.py:41-78` (`WanAPI.get_multistaticip`). Sends
    /// `GET` [`crate::routes::wan::WAN_GET_MULTISTATICIP`] (API version 2.3), preferring
    /// `parent`'s published `multistaticip` link over the template when supplied. This is a
    /// verified read.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`]/[`Error::Api`] with status 404 (`error.network.
    /// multistaticip_not_found`) on a network without the feature — observed behaviour, not a
    /// distinguished exception subtype in the wire contract. Returns [`Error::Authentication`]
    /// if no valid session is configured, or whatever status-mapped [`Error`] the request
    /// produces otherwise.
    pub async fn get_multistaticip(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::wan::WAN_GET_MULTISTATICIP,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Sets the network's multi-static-IP configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wan.py:83-119` (`WanAPI.set_multistaticip`). Issues a
    /// JSON `PUT` to [`crate::routes::wan::WAN_SET_MULTISTATICIP`] (API version 2.3) with
    /// `config` forwarded unchanged — no `parent=` parameter. Logs
    /// [`crate::links::warn_uncharacterised_write`] (`"set multi-static-IP configuration for
    /// network"`) immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_multistaticip(
        &self,
        network_id: &str,
        config: Value,
    ) -> Result<Envelope, Error> {
        let url = routes::wan::WAN_SET_MULTISTATICIP.resolve(
            self.transport.api_host(),
            network_id,
            None,
        )?;
        links::warn_uncharacterised_write("set multi-static-IP configuration for network");
        self.transport
            .request(
                routes::wan::WAN_SET_MULTISTATICIP.method.clone(),
                url,
                &[],
                RequestBody::Json(config),
            )
            .await
    }

    /// Sets per-device secondary-WAN access for the whole network in one call — returns the raw
    /// Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wan.py:123-169` (`WanAPI.set_secondary_wan_config`).
    /// Issues a JSON `PUT` to [`crate::routes::wan::WAN_SET_SECONDARY_WAN_CONFIG`] (API version
    /// 2.3) with `config` forwarded unchanged — no `parent=` parameter. Logs
    /// [`crate::links::warn_uncharacterised_write`] with `"set secondary WAN configuration for
    /// network -- may reboot every eero, like the confirmed DNS write path"` immediately before
    /// issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_secondary_wan_config(
        &self,
        network_id: &str,
        config: Value,
    ) -> Result<Envelope, Error> {
        let url = routes::wan::WAN_SET_SECONDARY_WAN_CONFIG.resolve(
            self.transport.api_host(),
            network_id,
            None,
        )?;
        links::warn_uncharacterised_write(
            "set secondary WAN configuration for network -- may reboot every eero, like the \
             confirmed DNS write path",
        );
        self.transport
            .request(
                routes::wan::WAN_SET_SECONDARY_WAN_CONFIG.method.clone(),
                url,
                &[],
                RequestBody::Json(config),
            )
            .await
    }

    /// Sets a single device's secondary-WAN access — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wan.py:172-216`
    /// (`WanAPI.set_device_secondary_wan_access`). Issues a JSON `PUT` to
    /// [`crate::routes::wan::WAN_SET_DEVICE_SECONDARY_WAN_ACCESS`] (API version 2.3) with
    /// `{"secondary_wan_deny_access": deny}`. Logs [`crate::links::warn_uncharacterised_write`]
    /// with `"set secondary WAN access for device on network -- may reboot every eero, like the
    /// confirmed DNS write path"` immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_device_secondary_wan_access(
        &self,
        network_id: &str,
        mac: &str,
        deny: bool,
    ) -> Result<Envelope, Error> {
        let url = routes::wan::WAN_SET_DEVICE_SECONDARY_WAN_ACCESS.resolve(
            self.transport.api_host(),
            network_id,
            mac,
            None,
        )?;
        links::warn_uncharacterised_write(
            "set secondary WAN access for device on network -- may reboot every eero, like the \
             confirmed DNS write path",
        );
        self.transport
            .request(
                routes::wan::WAN_SET_DEVICE_SECONDARY_WAN_ACCESS
                    .method
                    .clone(),
                url,
                &[],
                RequestBody::Json(json!({ "secondary_wan_deny_access": deny })),
            )
            .await
    }
}
