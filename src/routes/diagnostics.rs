//! Diagnostics routes (`DiagnosticsAPI`), ported from `eero-api src/eero/api/diagnostics.py` at
//! v8.0.4.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/diagnostics` — network diagnostics information.
///
/// Ported from `eero-api src/eero/api/diagnostics.py:35-64` (`DiagnosticsAPI.get_diagnostics`).
/// Resolved via [`Resource::resolve`]: prefers a parent envelope's `resources.diagnostics` link,
/// falls back to the `networks/{id}/diagnostics` template.
pub const GET_DIAGNOSTICS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/diagnostics",
    link: Some("diagnostics"),
};

/// Alias of [`GET_DIAGNOSTICS`], `POST`: run network diagnostics.
///
/// Ported from `eero-api src/eero/api/diagnostics.py:66-113` (`DiagnosticsAPI.run_diagnostics`).
/// Body is a JSON object carrying only the caller-supplied `device`/`symptom` keys — `{}` when
/// neither is given, never an absent body.
pub const RUN_DIAGNOSTICS: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/diagnostics",
    link: Some("diagnostics"),
};
