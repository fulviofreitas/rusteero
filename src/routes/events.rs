//! `events` routes (`EventsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/events.py` (v8.0.4). All three routes below resolve
//! against the *network's own* URL (`_params.resolve_network_url`, which prefers a parent
//! envelope's `url` field — `self_url` — over the bare-id template) with a literal suffix
//! appended, **not** a named `resources` link. [`Resource::link`]
//! is therefore `None` on every constant here: [`crate::endpoints::events::EventsApi`] does not
//! call [`Resource::resolve`] for these three routes at all — it resolves the network part with
//! [`crate::params::resolve_network_url`] (self-url preferring) and appends the suffix captured
//! after `{id}` in `template` itself, so the template stays the single source of truth for the
//! wire path even though the ordinary link-preferring resolution path is bypassed. See that
//! module's own docs for why.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/app_events` — app-facing event log.
///
/// Ported from `eero-api src/eero/api/events.py:87` (`EventsAPI.get_app_events`). See the
/// module docs for why [`Resource::link`] is `None` here despite `parent=` being a real,
/// live-verified preference for this method.
pub const GET_APP_EVENTS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/app_events",
    link: None,
};

/// `GET /2.2/networks/{network_id}/network_scan` — network scan results.
///
/// Ported from `eero-api src/eero/api/events.py:131` (`EventsAPI.get_network_scan`). See the
/// module docs for why [`Resource::link`] is `None` here despite `parent=` being a real,
/// live-verified preference for this method.
pub const GET_NETWORK_SCAN: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/network_scan",
    link: None,
};

/// `GET /2.2/networks/{network_id}/channel_utilization` — channel-utilization report.
///
/// Ported from `eero-api src/eero/api/events.py:159` (`EventsAPI.get_channel_utilization`). See
/// the module docs for why [`Resource::link`] is `None` here despite `parent=` being a real,
/// live-verified preference for this method (`start`/`end` only).
pub const GET_CHANNEL_UTILIZATION: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/channel_utilization",
    link: None,
};
