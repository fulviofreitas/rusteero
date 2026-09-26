//! Data Usage API: `eero-api`'s `DataUsageAPI`.
//!
//! Ported from `eero-api src/eero/api/data_usage.py` at `v8.0.4`
//! (`.claude/tasks/briefs/v8/g3-devices.md`). The whole module was rewritten in `05a2b07`
//! (v8.0.0): the pre-8.0.0 shape this crate previously shipped — one `get_data_usage(payload:
//! Value, resource: Option<&str>)` method that attached an arbitrary JSON body to a `GET`
//! request — is gone entirely, replaced by eleven distinct, **query-params-only** methods, one
//! per resource. `data_usage.py`'s own module docstring is explicit: "None of these endpoints
//! accept a request body; the API rejects a body-bearing `GET` with a `400`"
//! (`data_usage.py:10-11`).
//!
//! Every method resolves its network segment via the same "prefer `parent`'s own `self_url`,
//! else the bare `network_id` template" rule as [`crate::params::resolve_network_url`] — see
//! `DataUsageApi::network_id_or_self_url` for why this is implemented locally rather than by
//! threading `parent` through [`crate::routes::Resource::resolve`] itself.
//!
//! `cadence` is required on six of the eleven methods (`get_data_usage`, `get_device_usage`,
//! `get_eeros_summary`, `get_eero_usage`, `get_profile_usage`, `get_unprofiled_summary` — taking
//! `cadence: &str`) and optional on the other three (`get_breakdown`, `get_devices_usage`,
//! `get_unprofiled_devices` — taking `cadence: Option<&str>`); `get_report_settings` takes no
//! query parameters at all. Every method here funnels through [`crate::transport::Transport`],
//! which already implements the "not authenticated" precondition and every status-to-error
//! mapping a response can produce.

use std::sync::Arc;

use reqwest::Method;
use serde_json::{Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::params;
use crate::routes;
use crate::routes::Resource;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `DataUsageAPI` (`src/eero/api/data_usage.py`).
///
/// Build one with [`DataUsageApi::new`], wrapping a [`Transport`] already shared with the rest
/// of the `EeroApi` aggregator — `DataUsageApi` never constructs or owns a `Transport` itself.
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

    /// `GET /2.2/networks/{network_id}/data_usage` — network-wide data-usage statistics.
    ///
    /// Ported from `DataUsageAPI.get_data_usage` (`data_usage.py:155-197`). `start`/`end` are
    /// required; `cadence` is required (`cadence_required=True`); `timezone` is optional,
    /// included only when supplied.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "cadence"` if `cadence` is not `"hourly"` or
    /// `"daily"`, or if `parent`'s own `url` field (when present) is malformed, before any
    /// request is sent. Returns `Error::Authentication("Not authenticated")` if no valid session
    /// is configured, or whatever other status-mapped [`Error`] the request produces otherwise.
    pub async fn get_data_usage(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence(cadence)?;
        self.get_usage(
            &routes::data_usage::V8_GET_DATA_USAGE,
            network_id,
            start,
            end,
            Some(cadence),
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/breakdown` — data-usage breakdown by category.
    ///
    /// Ported from `DataUsageAPI.get_breakdown` (`data_usage.py:199-236`). `cadence` is
    /// optional, validated only when supplied.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    pub async fn get_breakdown(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence_opt(cadence)?;
        self.get_usage(
            &routes::data_usage::GET_DATA_USAGE_BREAKDOWN,
            network_id,
            start,
            end,
            cadence,
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/devices` — data-usage statistics for every
    /// device on a network, optionally filtered by `profile_id`.
    ///
    /// Ported from `DataUsageAPI.get_devices_usage` (`data_usage.py:238-280`). `cadence` and
    /// `profile_id` are both optional, each included only when supplied.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_devices_usage(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        profile_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence_opt(cadence)?;
        let id_or_url = self.network_id_or_self_url(network_id, parent)?;
        let url = routes::data_usage::GET_DEVICES_DATA_USAGE.resolve(
            self.transport.api_host(),
            &id_or_url,
            None,
        )?;
        let mut query = usage_query(start, end, cadence, timezone);
        if let Some(profile_id) = profile_id {
            query.push(("profile_id", profile_id.to_owned()));
        }
        self.transport
            .request(Method::GET, url, &query, RequestBody::None)
            .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/devices/{device_mac}` — data-usage statistics
    /// for a single device.
    ///
    /// Ported from `DataUsageAPI.get_device_usage` (`data_usage.py:282-325`): the collection URL
    /// ([`crate::routes::data_usage::GET_DEVICES_DATA_USAGE`]) with `device_mac` literal-appended
    /// after `validate_identifier` — never a second `.format()` call (`data_usage.py:317`).
    /// `cadence` is required.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "id"` if `device_mac` is not a single
    /// path-segment identifier, in addition to every case [`DataUsageApi::get_data_usage`]
    /// returns.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_device_usage(
        &self,
        network_id: &str,
        device_mac: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.get_child_usage(
            &routes::data_usage::GET_DEVICES_DATA_USAGE,
            network_id,
            device_mac,
            start,
            end,
            cadence,
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/eeros/summary` — aggregated data-usage summary
    /// across every eero on a network.
    ///
    /// Ported from `DataUsageAPI.get_eeros_summary` (`data_usage.py:327-368`). `cadence` is
    /// required.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    pub async fn get_eeros_summary(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence(cadence)?;
        self.get_usage(
            &routes::data_usage::GET_EEROS_DATA_USAGE_SUMMARY,
            network_id,
            start,
            end,
            Some(cadence),
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/eeros/{eero_id}` — data-usage statistics for a
    /// single eero.
    ///
    /// Ported from `DataUsageAPI.get_eero_usage` (`data_usage.py:370-413`). `cadence` is
    /// required.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_device_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_eero_usage(
        &self,
        network_id: &str,
        eero_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.get_child_usage(
            &routes::data_usage::GET_EEROS_DATA_USAGE,
            network_id,
            eero_id,
            start,
            end,
            cadence,
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/profiles/{profile_id}` — data-usage statistics
    /// for a single profile.
    ///
    /// Ported from `DataUsageAPI.get_profile_usage` (`data_usage.py:415-458`). `cadence` is
    /// required.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_device_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_profile_usage(
        &self,
        network_id: &str,
        profile_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.get_child_usage(
            &routes::data_usage::GET_PROFILES_DATA_USAGE,
            network_id,
            profile_id,
            start,
            end,
            cadence,
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/unprofiled/devices` — data-usage statistics
    /// for devices with no profile.
    ///
    /// Ported from `DataUsageAPI.get_unprofiled_devices` (`data_usage.py:460-497`). `cadence` is
    /// optional.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    pub async fn get_unprofiled_devices(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence_opt(cadence)?;
        self.get_usage(
            &routes::data_usage::GET_UNPROFILED_DEVICES_DATA_USAGE,
            network_id,
            start,
            end,
            cadence,
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/unprofiled/summary` — aggregated data-usage
    /// summary for unprofiled devices.
    ///
    /// Ported from `DataUsageAPI.get_unprofiled_summary` (`data_usage.py:499-540`). `cadence` is
    /// required.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    pub async fn get_unprofiled_summary(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence(cadence)?;
        self.get_usage(
            &routes::data_usage::GET_UNPROFILED_DATA_USAGE_SUMMARY,
            network_id,
            start,
            end,
            Some(cadence),
            timezone,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/report_settings` — the network's data-usage
    /// report settings. No query parameters at all.
    ///
    /// Ported from `DataUsageAPI.get_report_settings` (`data_usage.py:542-566`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `parent`'s own `url` field (when present) is malformed.
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever other status-mapped [`Error`] the request produces otherwise.
    pub async fn get_report_settings(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let id_or_url = self.network_id_or_self_url(network_id, parent)?;
        let url = routes::data_usage::GET_DATA_USAGE_REPORT_SETTINGS.resolve(
            self.transport.api_host(),
            &id_or_url,
            None,
        )?;
        self.transport
            .request(Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// `PUT /2.2/networks/{network_id}/data_usage/report_settings` — set the network's
    /// data-usage report settings.
    ///
    /// Ported from `DataUsageAPI.set_report_settings` (`data_usage.py:568-621`), the only write
    /// in this module: JSON body `{"cadence": cadence, "notification_day": notification_day}`
    /// (`data_usage.py:620`). `cadence` is validated before this crate's own "not authenticated"
    /// precondition even runs (`data_usage.py:614`) — matched here by validating it before the
    /// underlying [`Transport::request`] call. Logs the fixed uncharacterised-write warning
    /// before issuing the request (`data_usage.py:616`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "cadence"` if `cadence` is not `"hourly"` or
    /// `"daily"`, before any request is sent. Otherwise as [`DataUsageApi::get_report_settings`].
    pub async fn set_report_settings(
        &self,
        network_id: &str,
        cadence: &str,
        notification_day: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence(cadence)?;
        let id_or_url = self.network_id_or_self_url(network_id, parent)?;
        let url = routes::data_usage::SET_DATA_USAGE_REPORT_SETTINGS.resolve(
            self.transport.api_host(),
            &id_or_url,
            None,
        )?;
        crate::links::warn_uncharacterised_write("set data usage report settings for network");
        self.transport
            .request(
                Method::PUT,
                url,
                &[],
                RequestBody::Json(json!({
                    "cadence": cadence,
                    "notification_day": notification_day,
                })),
            )
            .await
    }

    /// Shared GET-with-query-only request builder for every network-level (non-per-item)
    /// resource in this module.
    #[allow(clippy::too_many_arguments)]
    async fn get_usage(
        &self,
        route: &Resource,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let id_or_url = self.network_id_or_self_url(network_id, parent)?;
        let url = route.resolve(self.transport.api_host(), &id_or_url, None)?;
        let query = usage_query(start, end, cadence, timezone);
        self.transport
            .request(Method::GET, url, &query, RequestBody::None)
            .await
    }

    /// Shared GET-with-query-only request builder for every per-item (device/eero/profile)
    /// resource in this module: resolves `route`'s collection URL, then literal-appends `child`
    /// after validating it as a single path-segment identifier.
    #[allow(clippy::too_many_arguments)]
    ///
    /// Validates `child` (a bare id) **before** `cadence` — mirroring Python's own evaluation
    /// order at every one of this method's three call sites (`data_usage.py:317,405,450`), where
    /// `_validate_child_id(...)` is embedded directly in the f-string argument expression handed
    /// to `_get_usage`, so it raises before `_get_usage`'s own `cadence` check ever runs
    /// (phase-G fix list item 15).
    async fn get_child_usage(
        &self,
        route: &Resource,
        network_id: &str,
        child: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let validated_child = crate::links::validate_identifier(child)?;
        let cadence = validated_cadence(cadence)?;
        let id_or_url = self.network_id_or_self_url(network_id, parent)?;
        let collection = route.resolve(self.transport.api_host(), &id_or_url, None)?;
        let url = append_path(&collection, validated_child)?;
        let query = usage_query(start, end, Some(cadence), timezone);
        self.transport
            .request(Method::GET, url, &query, RequestBody::None)
            .await
    }

    /// Resolves `network_id`'s id-or-self-url for use as a [`Resource`]'s `id_or_url`: prefers
    /// `parent`'s own published `url` ([`crate::links::self_url`]) when present, falling back to
    /// the bare `network_id` unchanged.
    ///
    /// Every `DataUsageAPI` method resolves its network segment via
    /// `resolve_network_url(network_id, parent)` (`data_usage.py:97-153`'s shared `_get_usage`),
    /// which prefers `parent`'s own `self_url` over the `network_id` template exactly like
    /// [`crate::params::resolve_network_url`] does. `Resource::resolve`'s own `parent` argument
    /// only supports a *named* link inside a *different* resource's `resources` map
    /// (`sub_resource_url`), not "prefer this same resource's own `url`" — so this helper performs
    /// that preference itself, up front, and hands the result to `Resource::resolve` as a bare
    /// `id_or_url` with `parent: None`: when `parent`'s `self_url` is present, the returned string
    /// is an absolute `http(s)://` URL on this transport's host, which `Resource::resolve`'s
    /// underlying `resource_url` already knows how to validate and append a template's suffix
    /// onto (the same branch an API-returned absolute URL takes) — so no separate "self-url
    /// preferred" branch needs to exist inside `Resource`/`Nested` itself.
    fn network_id_or_self_url(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<String, Error> {
        if let Some(parent) = parent
            && let Some(self_url) = crate::links::self_url(self.transport.api_host(), parent)?
        {
            return Ok(self_url.to_string());
        }
        Ok(network_id.to_owned())
    }
}

/// Validates a required `cadence` value against [`params::CADENCE_VALUES`] (`"hourly"`/
/// `"daily"`).
fn validated_cadence(cadence: &str) -> Result<&str, Error> {
    params::validate_cadence(cadence, params::CADENCE_VALUES)
}

/// Validates an optional `cadence` value, only when supplied.
fn validated_cadence_opt(cadence: Option<&str>) -> Result<Option<&str>, Error> {
    cadence.map(validated_cadence).transpose()
}

/// Builds the `start`/`end`/`cadence`/`timezone` query-parameter list every `DataUsageAPI`
/// method shares: `start`/`end` are always present, `cadence`/`timezone` only when `Some`.
fn usage_query(
    start: &str,
    end: &str,
    cadence: Option<&str>,
    timezone: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut query = vec![("start", start.to_owned()), ("end", end.to_owned())];
    if let Some(cadence) = cadence {
        query.push(("cadence", cadence.to_owned()));
    }
    if let Some(timezone) = timezone {
        query.push(("timezone", timezone.to_owned()));
    }
    query
}

/// Appends a validated child path segment to an already-resolved collection URL.
fn append_path(base: &Url, child: &str) -> Result<Url, Error> {
    let joined = format!("{}/{child}", base.as_str().trim_end_matches('/'));
    Url::parse(&joined).map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
}
