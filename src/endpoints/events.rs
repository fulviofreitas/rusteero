//! `EventsApi`: `events` endpoints (`eero-api src/eero/api/events.py`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/events.py` (v8.0.4). See
//! `.claude/tasks/briefs/v8/g7-backup-members.md` §1 for the per-method table this file is
//! scoped by.
//!
//! Every method here resolves its URL from the network's own self-url-preferring resolution
//! (`crate::params::resolve_network_url`) with a literal suffix appended — see
//! `crate::routes::events`'s module docs for why this bypasses the ordinary
//! `Transport::resource` link-preferring path — then sends through `Transport::request`
//! directly, which already implements the "not authenticated" precondition Python repeats at
//! the top of each method (`get_auth_token()` / `EeroAuthenticationException("Not
//! authenticated")`) and every status-to-error mapping a response can produce.

use std::sync::Arc;

use serde_json::Value;
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes::{self, Resource};
use crate::transport::{RequestBody, Transport};

/// Valid `band` values for [`GetChannelUtilizationOptions::band`].
///
/// Ported from `CHANNEL_UTILIZATION_BANDS` (`events.py:20-26`).
pub const CHANNEL_UTILIZATION_BANDS: &[&str] = &[
    "band_2_4GHz",
    "band_5GHz_low",
    "band_5GHz_high",
    "band_5GHz_full",
    "band_6GHz",
];

/// Every optional keyword argument [`EventsApi::get_channel_utilization`] accepts.
///
/// More than four optional keyword arguments, per this port's conventions
/// (`.claude/tasks/briefs/v8/phase-g-rules.md` item 2).
#[derive(Debug, Default, Clone)]
pub struct GetChannelUtilizationOptions<'a> {
    /// Minimum "busy" threshold, as a positive integer. Validated the same way as
    /// [`GetChannelUtilizationOptions::granularity`] — see that field's docs.
    pub busy_threshold: Option<u32>,
    /// Restrict the report to a single eero, by id.
    pub eero_id: Option<&'a str>,
    /// Restrict the report to a single radio band; must be one of [`CHANNEL_UTILIZATION_BANDS`].
    pub band: Option<&'a str>,
    /// Bucket granularity, as a positive integer. Rust's `u32` already excludes every
    /// non-integer or negative value Python's own runtime check
    /// (`_validate_positive_int`, `events.py:50-68`) exists to reject; only the *zero* case still
    /// needs an explicit check here, since `u32` does not exclude it.
    pub granularity: Option<u32>,
    /// Opaque pass-through value with no validation on either side (`events.py:213-227`: plain
    /// `str(gap_data_placeholder)`, no positive-int check unlike the two fields above).
    pub gap_data_placeholder: Option<&'a str>,
}

/// `eero-api`'s `EventsAPI` (`src/eero/api/events.py`, new in v8.0.0).
///
/// Build one with [`EventsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `EventsApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct EventsApi {
    transport: Arc<Transport>,
}

impl EventsApi {
    /// Wraps `transport` as an `EventsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Resolves `route`'s network-scoped URL, preferring `parent`'s own self-url over the
    /// bare-`network_id` template — see `crate::routes::events`'s module docs for why this,
    /// rather than [`Resource::resolve`], is what every method in this file calls. The suffix
    /// appended after the network URL is read straight out of `route.template` (the text after
    /// its one `{id}` placeholder), so `route` stays the single source of truth for the wire
    /// path.
    fn resolve_network_scoped_url(
        &self,
        route: &Resource,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let suffix = route.template.split("{id}").nth(1).unwrap_or("");
        let base = crate::params::resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            route.version,
        )?;
        let joined = format!("{}{suffix}", base.as_str().trim_end_matches('/'));
        Url::parse(&joined)
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
    }

    /// `GET /2.2/networks/{network_id}/app_events` — app-facing event log.
    ///
    /// Ported from `EventsAPI.get_app_events` (`events.py:87-129`). `page_size` is sent as
    /// `str(page_size)` iff `Some`; `timestamp` iff `Some` (`events.py:120-124`) — both are
    /// simply omitted from the query string when `None`, matching Python's own
    /// `params={}`-when-both-omitted behaviour. Returns the raw `{"meta": …, "data": {...}}`
    /// envelope; this method never inspects or reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::request`.
    pub async fn get_app_events(
        &self,
        network_id: &str,
        page_size: Option<u32>,
        timestamp: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve_network_scoped_url(&routes::GET_APP_EVENTS, network_id, parent)?;
        let mut query = Vec::new();
        if let Some(page_size) = page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(timestamp) = timestamp {
            query.push(("timestamp", timestamp.to_owned()));
        }
        self.transport
            .request(
                routes::GET_APP_EVENTS.method.clone(),
                url,
                &query,
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/network_scan` — network scan results.
    ///
    /// Ported from `EventsAPI.get_network_scan` (`events.py:131-157`).
    ///
    /// # Errors
    ///
    /// See [`Self::get_app_events`].
    pub async fn get_network_scan(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve_network_scoped_url(&routes::GET_NETWORK_SCAN, network_id, parent)?;
        self.transport
            .request(
                routes::GET_NETWORK_SCAN.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/channel_utilization` — channel-utilization report.
    ///
    /// Ported from `EventsAPI.get_channel_utilization` (`events.py:159-228`). `start`/`end` are
    /// always sent as-is (required, no format check, `events.py:213-227`); every field of
    /// `options` is sent only when `Some`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "band", .. }` if `options.band` is `Some` and not one
    /// of [`CHANNEL_UTILIZATION_BANDS`]. Returns `Error::Validation` with `field:
    /// "busy_threshold"`/`"granularity"` if that field is `Some(0)` — the one case Rust's `u32`
    /// does not already exclude, see [`GetChannelUtilizationOptions::granularity`]'s own docs.
    /// Either check runs before any request is sent. Otherwise see [`Self::get_app_events`].
    pub async fn get_channel_utilization(
        &self,
        network_id: &str,
        start: &str,
        end: &str,
        options: &GetChannelUtilizationOptions<'_>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        // Validated in Python's exact order (`events.py:216-223`): `busy_threshold`, then
        // `band`, then `granularity`.
        if let Some(busy_threshold) = options.busy_threshold {
            validate_positive("busy_threshold", busy_threshold)?;
        }
        if let Some(band) = options.band
            && !CHANNEL_UTILIZATION_BANDS.contains(&band)
        {
            return Err(Error::validation(
                "band",
                format!(
                    "must be one of {}, got {}",
                    crate::params::py_tuple(CHANNEL_UTILIZATION_BANDS),
                    crate::params::py_quote(band)
                ),
            ));
        }
        if let Some(granularity) = options.granularity {
            validate_positive("granularity", granularity)?;
        }

        let url =
            self.resolve_network_scoped_url(&routes::GET_CHANNEL_UTILIZATION, network_id, parent)?;
        let mut query: Vec<(&str, String)> =
            vec![("start", start.to_owned()), ("end", end.to_owned())];
        if let Some(busy_threshold) = options.busy_threshold {
            query.push(("busy_threshold", busy_threshold.to_string()));
        }
        if let Some(eero_id) = options.eero_id {
            query.push(("eero_id", eero_id.to_owned()));
        }
        if let Some(band) = options.band {
            query.push(("band", band.to_owned()));
        }
        if let Some(granularity) = options.granularity {
            query.push(("granularity", granularity.to_string()));
        }
        if let Some(gap_data_placeholder) = options.gap_data_placeholder {
            query.push(("gap_data_placeholder", gap_data_placeholder.to_owned()));
        }
        self.transport
            .request(
                routes::GET_CHANNEL_UTILIZATION.method.clone(),
                url,
                &query,
                RequestBody::None,
            )
            .await
    }
}

/// Validates that `value` is non-zero — the one invalid state Rust's `u32` does not already
/// exclude (negative and non-integer values are unrepresentable). Ported from
/// `_validate_positive_int` (`events.py:50-68`).
fn validate_positive(field: &str, value: u32) -> Result<u32, Error> {
    if value == 0 {
        return Err(Error::validation(
            field,
            format!("must be a positive integer, got {value}"),
        ));
    }
    Ok(value)
}
