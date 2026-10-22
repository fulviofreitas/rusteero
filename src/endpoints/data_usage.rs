//! Data Usage API: `eero-api`'s `DataUsageAPI`.
//!
//! Ported from `eero-api src/eero/api/data_usage.py` at `v8.0.4`
//! (`.claude/tasks/briefs/v8/g3-devices.md`). The whole module was rewritten in `05a2b07`
//! (v8.0.0): the pre-8.0.0 shape this crate previously shipped — one `get_data_usage(payload:
//! Value, resource: Option<&str>)` method that attached an arbitrary JSON body to a `GET`
//! request — is gone entirely, replaced by twelve distinct, **query-params-only** methods, one
//! per resource. `data_usage.py`'s own module docstring is explicit: "None of these endpoints
//! accept a request body; the API rejects a body-bearing `GET` with a `400`"
//! (`data_usage.py:10-11`).
//!
//! Every method resolves its network base through [`crate::params::resolve_network_url`] — the
//! "prefer `parent`'s own `self_url`, else the bare `network_id` template" rule of Python's
//! `_get_usage` (`data_usage.py:157`) — and then appends the route's literal suffix (the template
//! text after `{id}`, e.g. `/data_usage/breakdown`). A published `self_url` is therefore only
//! shape-checked, never fed back through [`crate::links::resource_url`] as if a caller had typed
//! it, so it is not confined to the `networks/{id}` family; only a caller-supplied `network_id`
//! is. See `DataUsageApi::route_url`.
//!
//! `cadence` is required on seven of the twelve methods (`get_data_usage`, `get_device_usage`,
//! `get_eeros_summary`, `get_eeros_usage`, `get_eero_usage`, `get_profile_usage`,
//! `get_unprofiled_summary` — taking `cadence: &str`) and optional on three more
//! (`get_breakdown`, `get_devices_usage`, `get_unprofiled_devices` — taking `cadence:
//! Option<&str>`); `get_report_settings` takes no query parameters at all, and
//! `set_report_settings` is the one write. Every method here funnels through [`crate::transport::Transport`],
//! which already implements the "not authenticated" precondition and every status-to-error
//! mapping a response can produce.
//!
//! # Per-call timeout
//!
//! All ten time-windowed reads (everything except `get_report_settings`/`set_report_settings`)
//! take `timeout: Option<RequestTimeout>` immediately before `parent` (`data_usage.py:111,
//! 159-160` at `v8.4.1`, where the keyword-only `timeout` is forwarded only when supplied).
//! `Some(timeout)` replaces the transport default (30 s total, 10 s read, unless the builder
//! changed them) for that one request, wholesale: a field left unset is unbounded and does not
//! fall back to the default. `None` keeps the default. A large time window can take the server
//! longer than the default allows, which is what this is for.

use std::sync::Arc;

use reqwest::Method;
use serde_json::{Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::params;
use crate::routes;
use crate::routes::Resource;
use crate::transport::{RequestBody, RequestTimeout, Transport};

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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "cadence"` if `cadence` is not `"hourly"` or
    /// `"daily"`, or if `parent`'s own `url` field (when present) is malformed, before any
    /// request is sent. Returns `Error::Authentication("Not authenticated")` if no valid session
    /// is configured, or whatever other status-mapped [`Error`] the request produces otherwise.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_data_usage(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        timeout: Option<RequestTimeout>,
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
            timeout,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/breakdown` — data-usage breakdown by category.
    ///
    /// Ported from `DataUsageAPI.get_breakdown` (`data_usage.py:199-236`). `cadence` is
    /// optional, validated only when supplied.
    ///
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_breakdown(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        timeout: Option<RequestTimeout>,
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
            timeout,
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
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
        timeout: Option<RequestTimeout>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence_opt(cadence)?;
        let url = self.route_url(
            &routes::data_usage::GET_DEVICES_DATA_USAGE,
            network_id,
            parent,
        )?;
        let mut query = usage_query(start, end, cadence, timezone);
        if let Some(profile_id) = profile_id {
            query.push(("profile_id", profile_id.to_owned()));
        }
        self.transport
            .request_with_timeout(Method::GET, url, &query, RequestBody::None, timeout)
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
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
        timeout: Option<RequestTimeout>,
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
            timeout,
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_eeros_summary(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        timeout: Option<RequestTimeout>,
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
            timeout,
            parent,
        )
        .await
    }

    /// `GET /2.2/networks/{network_id}/data_usage/eeros` — data-usage statistics for every eero
    /// on a network.
    ///
    /// Ported from `DataUsageAPI.get_eeros_usage` (`data_usage.py:354-404` at `v8.4.1`). The API
    /// requires `start`, `end` and `cadence` on this endpoint, so `cadence` is required. The
    /// path is not in the observed spec; it is used by a downstream integration and exposed here
    /// on that basis. Distinct from [`DataUsageApi::get_eeros_summary`]
    /// (`.../data_usage/eeros/summary`) and from [`DataUsageApi::get_eero_usage`]
    /// (`.../data_usage/eeros/{eero_id}`).
    ///
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_eeros_usage(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        timeout: Option<RequestTimeout>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = validated_cadence(cadence)?;
        self.get_usage(
            &routes::data_usage::GET_EEROS_DATA_USAGE,
            network_id,
            start,
            end,
            Some(cadence),
            timezone,
            timeout,
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
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
        timeout: Option<RequestTimeout>,
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
            timeout,
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
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
        timeout: Option<RequestTimeout>,
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
            timeout,
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_unprofiled_devices(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: Option<&str>,
        timezone: Option<&str>,
        timeout: Option<RequestTimeout>,
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
            timeout,
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
    /// `timeout`, when `Some`, replaces the transport default for this request only (see
    /// [`RequestTimeout`]); `None` keeps it.
    ///
    /// # Errors
    ///
    /// See [`DataUsageApi::get_data_usage`].
    #[allow(clippy::too_many_arguments)]
    pub async fn get_unprofiled_summary(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        timezone: Option<&str>,
        timeout: Option<RequestTimeout>,
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
            timeout,
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
        let url = self.route_url(
            &routes::data_usage::GET_DATA_USAGE_REPORT_SETTINGS,
            network_id,
            parent,
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
        let url = self.route_url(
            &routes::data_usage::SET_DATA_USAGE_REPORT_SETTINGS,
            network_id,
            parent,
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
        timeout: Option<RequestTimeout>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.route_url(route, network_id, parent)?;
        let query = usage_query(start, end, cadence, timezone);
        self.transport
            .request_with_timeout(Method::GET, url, &query, RequestBody::None, timeout)
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
        timeout: Option<RequestTimeout>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let validated_child = crate::links::validate_identifier(child)?;
        let cadence = validated_cadence(cadence)?;
        let collection = self.route_url(route, network_id, parent)?;
        let url = append_path(&collection, validated_child)?;
        let query = usage_query(start, end, Some(cadence), timezone);
        self.transport
            .request_with_timeout(Method::GET, url, &query, RequestBody::None, timeout)
            .await
    }

    /// Resolves `route`'s absolute URL for `network_id`: the network base from
    /// [`params::resolve_network_url`] (`parent`'s own published `url` when present, else the
    /// `networks/{id}` template at the route's API version), then the route's literal suffix.
    ///
    /// Every `DataUsageAPI` method does exactly this (`data_usage.py:156-157` at `v8.4.1`:
    /// `base_url = resolve_network_url(network, parent)` and
    /// `f"{base_url}/data_usage/{path}"`), so the suffix comes from [`Resource::template`], the
    /// single source of each path, as the text after `{id}`.
    ///
    /// A published `url` is returned by `resolve_network_url` after a plain shape check only
    /// (field `link`), and must stay that way: handing it back to [`Resource::resolve`] as if it
    /// were a caller-typed `id_or_url` would subject it to the caller-path family check, which
    /// rejects any parent whose url is not `/<version>/networks/<id>` — something Python never
    /// does to a published link. Only the template fallback, built from the caller's
    /// `network_id`, is checked as a caller-supplied id.
    fn route_url(
        &self,
        route: &Resource,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let base = params::resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            route.version,
        )?;
        let joined = format!(
            "{}{}",
            base.as_str().trim_end_matches('/'),
            route_suffix(route.template)
        );
        Url::parse(&joined)
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
    }
}

/// The literal path text after a route template's `{id}` placeholder (`/data_usage/breakdown` for
/// `networks/{id}/data_usage/breakdown`).
fn route_suffix(template: &str) -> &str {
    template.split_once("{id}").map_or("", |(_, suffix)| suffix)
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

#[cfg(test)]
mod tests {
    use super::route_suffix;
    use crate::routes::data_usage as routes;

    /// Each route's literal suffix is the text after `{id}`, and it is what `route_url`
    /// appends: every data-usage template hangs `/data_usage[/..]` off `networks/{id}`.
    #[test]
    fn route_suffix_is_the_template_text_after_the_id_placeholder() {
        for (template, suffix) in [
            (routes::V8_GET_DATA_USAGE.template, "/data_usage"),
            (
                routes::GET_DATA_USAGE_BREAKDOWN.template,
                "/data_usage/breakdown",
            ),
            (
                routes::GET_DEVICES_DATA_USAGE.template,
                "/data_usage/devices",
            ),
            (
                routes::GET_EEROS_DATA_USAGE_SUMMARY.template,
                "/data_usage/eeros/summary",
            ),
            (routes::GET_EEROS_DATA_USAGE.template, "/data_usage/eeros"),
            (
                routes::GET_PROFILES_DATA_USAGE.template,
                "/data_usage/profiles",
            ),
            (
                routes::GET_UNPROFILED_DEVICES_DATA_USAGE.template,
                "/data_usage/unprofiled/devices",
            ),
            (
                routes::GET_UNPROFILED_DATA_USAGE_SUMMARY.template,
                "/data_usage/unprofiled/summary",
            ),
            (
                routes::GET_DATA_USAGE_REPORT_SETTINGS.template,
                "/data_usage/report_settings",
            ),
            (
                routes::SET_DATA_USAGE_REPORT_SETTINGS.template,
                "/data_usage/report_settings",
            ),
        ] {
            assert!(template.starts_with("networks/{id}/"), "{template}");
            assert_eq!(route_suffix(template), suffix, "{template}");
        }
    }
}
