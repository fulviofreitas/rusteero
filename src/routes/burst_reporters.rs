//! Burst-reporter routes (`BurstReportersAPI`).

// --------------------------- burst_reporters (`BurstReportersAPI`) ---------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/burst_reporters` — list burst reporters.
///
/// Ported from `eero-api src/eero/api/burst_reporters.py:33`
/// (`BurstReportersAPI.get_burst_reporters`).
pub const GET_BURST_REPORTERS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/burst_reporters",
};

/// `POST /2.2/networks/{network_id}/burst_reporters` — create a burst reporter.
///
/// Ported from `eero-api src/eero/api/burst_reporters.py:56`
/// (`BurstReportersAPI.create_burst_reporter`).
pub const CREATE_BURST_REPORTER: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/burst_reporters",
};
