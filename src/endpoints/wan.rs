//! `WanApi`: `wan` endpoints (`eero-api src/eero/api/wan.py`, new in v8.0.0).
//!
//! This family is only served on API version 2.3 ([`crate::consts::API_VERSION_MULTISTATICIP`] /
//! [`crate::consts::API_VERSION_SECONDARY_WAN`]), unlike most of this SDK's default 2.2
//! endpoints. A network (or device) given as a path or an absolute URL keeps the version segment
//! it carries when the URL is resolved, and so does a link published on a parent envelope, so
//! every method here re-pins the resolved URL to 2.3 with [`crate::links::rewrite_version`]
//! before it warns or sends.

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
    /// Ported from `eero-api src/eero/api/wan.py:41-85` (`WanAPI.get_multistaticip`). Sends
    /// `GET` [`crate::routes::wan::WAN_GET_MULTISTATICIP`], preferring `parent`'s published
    /// `multistaticip` link over the template when supplied, and always on API version 2.3: the
    /// resolved URL is re-pinned with [`crate::links::rewrite_version`] whichever form
    /// `network_id` takes and even when `parent` publishes the link on another version. This is
    /// a verified read.
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
        let url = routes::wan::WAN_GET_MULTISTATICIP.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        let url = links::rewrite_version(&url, routes::wan::WAN_GET_MULTISTATICIP.version);
        self.transport
            .request(
                routes::wan::WAN_GET_MULTISTATICIP.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Sets the network's multi-static-IP configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/wan.py:87-127` (`WanAPI.set_multistaticip`). Issues a
    /// JSON `PUT` to [`crate::routes::wan::WAN_SET_MULTISTATICIP`] with `config` forwarded
    /// unchanged — no `parent=` parameter. The resolved URL is re-pinned to API version 2.3
    /// whichever form `network_id` takes, then
    /// [`crate::links::warn_uncharacterised_write`] (`"set multi-static-IP configuration for
    /// network"`) is logged immediately before the request.
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
        let url = links::rewrite_version(&url, routes::wan::WAN_SET_MULTISTATICIP.version);
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
    /// Ported from `eero-api src/eero/api/wan.py:129-178` (`WanAPI.set_secondary_wan_config`).
    /// Issues a JSON `PUT` to [`crate::routes::wan::WAN_SET_SECONDARY_WAN_CONFIG`] with `config`
    /// forwarded unchanged — no `parent=` parameter. The resolved URL is re-pinned to API version
    /// 2.3 whichever form `network_id` takes, then
    /// [`crate::links::warn_uncharacterised_write`] is logged with `"set secondary WAN
    /// configuration for network -- may reboot every eero, like the confirmed DNS write path"`
    /// immediately before the request.
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
        let url = links::rewrite_version(&url, routes::wan::WAN_SET_SECONDARY_WAN_CONFIG.version);
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
    /// Ported from `eero-api src/eero/api/wan.py:180-227`
    /// (`WanAPI.set_device_secondary_wan_access`). Issues a JSON `PUT` to
    /// [`crate::routes::wan::WAN_SET_DEVICE_SECONDARY_WAN_ACCESS`] with
    /// `{"secondary_wan_deny_access": deny}`.
    ///
    /// Unlike the devices module, `mac` is **not** reduced to its last path segment: a device
    /// given as a path or URL is checked against the addressed network by the nested-family rule,
    /// and may carry a `2.2` segment of its own, just as a path- or URL-form `network_id` may. The
    /// resolved URL is therefore re-pinned to API version 2.3 after that check, then
    /// [`crate::links::warn_uncharacterised_write`] is logged with `"set secondary WAN access for
    /// device on network -- may reboot every eero, like the confirmed DNS write path"`
    /// immediately before the request.
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
        let url = links::rewrite_version(
            &url,
            routes::wan::WAN_SET_DEVICE_SECONDARY_WAN_ACCESS.version,
        );
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
