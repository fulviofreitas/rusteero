//! Burst-reporter routes (`BurstReportersAPI`, POST-only since v8.0.0).
//!
//! `BurstReportersAPI.get_burst_reporters` was removed upstream in v8.0.0 — the endpoint 404s;
//! the resource is POST-only. Only `create_burst_reporter` remains.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `POST /2.2/networks/{network_id}/burst_reporters` — request a live-usage burst.
///
/// Published as the `burst_reporters` link on a network envelope (verified live,
/// `test_create_burst_reporter_prefers_parent_link`). Ported from `eero-api
/// src/eero/api/burst_reporters.py:36-112` at `v8.4.1`
/// (`BurstReportersAPI.create_burst_reporter`): the URL via `sub_resource_url(..,
/// link="burst_reporters")` at `:98-103`, then a POST of the `""` body (live-verified
/// 2026-10-20; a deprecated non-empty JSON body is still sent, and still warned about, by the
/// endpoint).
pub const CREATE_BURST_REPORTER: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/burst_reporters",
    link: Some("burst_reporters"),
};
