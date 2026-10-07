//! Insights API: `eero-api`'s `InsightsAPI`.
//!
//! Ported from `eero-api src/eero/api/insights.py` at `v8.0.4`:
//! `get_insights`, `get_devices_insights`,
//! `get_device_insights`, `get_profiles_insights`, `get_profile_insights`,
//! `get_profile_devices_insights`. `run_insights` (`insights.py:115-137` at `v6.2.0`) was
//! removed entirely in `05a2b07` (v8.0.0) — **no** `Error`/endpoint replacement of any kind, and
//! it is not ported here.
//!
//! # `cadence`: `"weekly"` is gone
//!
//! `v6.2.0` declared `INSIGHTS_CADENCES = ("hourly", "daily", "weekly")` but never validated
//! `cadence` against it anywhere in `get_insights`. `v8.0.4` narrows the set to
//! `INSIGHTS_CADENCES = ("hourly", "daily")` (`insights.py:19`) — identical to
//! [`crate::params::CADENCE_VALUES`] — and validates every method's `cadence` against it
//! (`insights.py:134`, and `_insights_params`, `insights.py:33-56`, for the five sub-resource
//! methods). Every method below therefore now rejects `cadence == "weekly"` locally, before any
//! request is sent — a **local, non-network-visible behaviour change** from what this crate
//! previously shipped (no validation at all).
//!
//! Every method here funnels through [`crate::transport::Transport`], which already implements
//! the "not authenticated" precondition Python repeats at the top of each method and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::params;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// Valid values for `get_insights`'s `cadence` query parameter, in Python's declared order.
///
/// Ported from `INSIGHTS_CADENCES` (`insights.py:30`): `("hourly", "daily")` — note the order
/// differs from [`crate::params::CADENCE_VALUES`] (`["daily", "hourly"]`), which every other
/// method in this module validates against instead (`_insights_params`'s default `allowed=`,
/// `_params.py:28-53`). The order only matters for the exact wording of a rejection's message
/// (`params::validate_cadence`'s `{allowed:?}`); the accepted *set* is identical either way.
const INSIGHTS_CADENCES: &[&str] = &["hourly", "daily"];

/// `eero-api`'s `InsightsAPI` (`src/eero/api/insights.py`).
///
/// Build one with [`InsightsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the `EeroApi` aggregator — `InsightsApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct InsightsApi {
    transport: Arc<Transport>,
}

impl InsightsApi {
    /// Wraps `transport` as an `InsightsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/insights` — network-level insights time-series data.
    ///
    /// Ported from `InsightsAPI.get_insights` (`insights.py:72-153`). `start`, `end` and
    /// `insight_type` are required, matching Python; unlike Python's SDK-supplied default
    /// (`cadence: str = "daily"`, `insights.py:43`), this port keeps `cadence` a plain required
    /// argument — a deliberate divergence carried over from the pre-8.0.4 port, revisited and
    /// kept: callers wanting `"daily"` pass it explicitly. `cadence` is validated against
    /// `INSIGHTS_CADENCES` (`"hourly"`/`"daily"` — `"weekly"` is no longer accepted, see the
    /// module docs) before any request is sent. No `parent=` kwarg exists on this Python method.
    /// Query parameters are sent in Python's exact order — `start`, `end`, `cadence`,
    /// `insight_type` (`insights.py:134-139`) — not alphabetical or declaration order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "cadence"` if `cadence` is not `"hourly"` or
    /// `"daily"`, before any request is sent. Returns `Error::Authentication("Not
    /// authenticated")` if no valid session is configured, or whatever other status-mapped
    /// [`Error`] the request produces otherwise.
    pub async fn get_insights(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        insight_type: &str,
        cadence: &str,
    ) -> Result<Envelope, Error> {
        let cadence = params::validate_cadence(cadence, INSIGHTS_CADENCES)?;
        self.transport
            .resource(
                &routes::insights::V8_GET_INSIGHTS,
                network_id,
                None,
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/insights/devices` — insights for every device on a
    /// network.
    ///
    /// Ported from `InsightsAPI.get_devices_insights` (`insights.py:155-196`). `start`, `end`,
    /// `cadence` and `insight_type` are all required. Prefers `parent`'s own published
    /// `insights_devices` link over the `network_id` template when supplied.
    ///
    /// # Errors
    ///
    /// See [`InsightsApi::get_insights`].
    pub async fn get_devices_insights(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = params::validate_cadence(cadence, params::CADENCE_VALUES)?;
        self.transport
            .resource(
                &routes::insights::GET_DEVICES_INSIGHTS,
                network_id,
                parent,
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/insights/devices/{mac}` — insights for a single device.
    ///
    /// Ported from `InsightsAPI.get_device_insights` (`insights.py:198-233`). No `parent=` kwarg
    /// on the Python method this ports; `mac` is resolved via `resolve_nested_url`
    /// (`routes::insights::GET_DEVICE_INSIGHTS`), which never double-`.format()`s a
    /// caller-supplied `network_id` that might itself contain a stray `{`/`}`.
    ///
    /// # Errors
    ///
    /// See [`InsightsApi::get_insights`].
    pub async fn get_device_insights(
        &self,
        network_id: &str,
        mac: &str,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let cadence = params::validate_cadence(cadence, params::CADENCE_VALUES)?;
        self.transport
            .nested(
                &routes::insights::GET_DEVICE_INSIGHTS,
                network_id,
                mac,
                None,
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/insights/profiles` — insights for every profile on a
    /// network.
    ///
    /// Ported from `InsightsAPI.get_profiles_insights` (`insights.py:235-276`). Prefers
    /// `parent`'s own published `insights_profiles` link over the `network_id` template when
    /// supplied.
    ///
    /// # Errors
    ///
    /// See [`InsightsApi::get_insights`].
    pub async fn get_profiles_insights(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let cadence = params::validate_cadence(cadence, params::CADENCE_VALUES)?;
        self.transport
            .resource(
                &routes::insights::GET_PROFILES_INSIGHTS,
                network_id,
                parent,
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/insights/profiles/{profile}` — insights for a single
    /// profile.
    ///
    /// Ported from `InsightsAPI.get_profile_insights` (`insights.py:278-313`). No `parent=`
    /// kwarg on the Python method this ports.
    ///
    /// # Errors
    ///
    /// See [`InsightsApi::get_insights`].
    pub async fn get_profile_insights(
        &self,
        network_id: &str,
        profile_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let cadence = params::validate_cadence(cadence, params::CADENCE_VALUES)?;
        self.transport
            .nested(
                &routes::insights::GET_PROFILE_INSIGHTS,
                network_id,
                profile_id,
                None,
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/insights/profiles/{profile}/devices` — insights for
    /// every device belonging to a single profile.
    ///
    /// Ported from `InsightsAPI.get_profile_devices_insights` (`insights.py:315-357`). No
    /// `parent=` kwarg on the Python method this ports.
    ///
    /// # Errors
    ///
    /// See [`InsightsApi::get_insights`].
    pub async fn get_profile_devices_insights(
        &self,
        network_id: &str,
        profile_id: &str,
        start: &str,
        end: &str,
        cadence: &str,
        insight_type: &str,
    ) -> Result<Envelope, Error> {
        let cadence = params::validate_cadence(cadence, params::CADENCE_VALUES)?;
        self.transport
            .nested(
                &routes::insights::GET_PROFILE_DEVICES_INSIGHTS,
                network_id,
                profile_id,
                None,
                &[
                    ("start", start.to_owned()),
                    ("end", end.to_owned()),
                    ("cadence", cadence.to_owned()),
                    ("insight_type", insight_type.to_owned()),
                ],
                RequestBody::None,
            )
            .await
    }
}
