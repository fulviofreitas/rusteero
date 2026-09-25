//! Profile routes (`ProfilesAPI`).

// --------------------------------- profiles (`ProfilesAPI`) -----------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/profiles` — list profiles on a network.
///
/// Ported from `eero-api src/eero/api/profiles.py:33` (`ProfilesAPI.get_profiles`).
pub const GET_PROFILES: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles",
};

/// `POST /2.2/networks/{network_id}/profiles` — create a profile.
///
/// Ported from `eero-api src/eero/api/profiles.py:336` (`ProfilesAPI.create_profile`).
pub const CREATE_PROFILE: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles",
};

/// `GET /2.2/networks/{network_id}/profiles/{profile_id}` — a single profile's full object.
///
/// Shared by four Python methods that all read fields out of the same full profile object:
/// `ProfilesAPI.get_profile`, `ProfilesAPI.get_profile_devices` (`GET_PROFILE_DEVICES`),
/// `ProfilesAPI.get_blocked_applications` (`GET_BLOCKED_APPLICATIONS`), and
/// `ScheduleAPI.get_profile_schedule` (`GET_PROFILE_SCHEDULE`). Ported from
/// `eero-api src/eero/api/profiles.py:53` (`ProfilesAPI.get_profile`).
pub const GET_PROFILE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles/{profile_id}",
};

/// `PUT /2.2/networks/{network_id}/profiles/{profile_id}` — the profile mutation endpoint.
///
/// The single wire endpoint behind seven Python setters across `ProfilesAPI` and
/// `ScheduleAPI` (`PAUSE_PROFILE`, `SET_PROFILE_DEVICES`, `UPDATE_PROFILE_CONTENT_FILTER`,
/// `UPDATE_PROFILE_BLOCK_LIST`, `SET_BLOCKED_APPLICATIONS`, `RENAME_PROFILE`,
/// `SET_PROFILE_SCHEDULE`) — each PUTs a different JSON key onto the same profile object.
/// Ported from `eero-api src/eero/api/profiles.py:77` (`ProfilesAPI.pause_profile`), the
/// first setter of this resource in the port plan's endpoint catalogue.
pub const PUT_PROFILE: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles/{profile_id}",
};

/// Alias of `PUT_PROFILE`: `ProfilesAPI.pause_profile` PUTs `{"paused": bool}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:77` (`ProfilesAPI.pause_profile`).
pub const PAUSE_PROFILE: Route = PUT_PROFILE;

/// Alias of `GET_PROFILE`: `ProfilesAPI.get_profile_devices` reads the `devices` field out
/// of the same full profile object.
///
/// Ported from `eero-api src/eero/api/profiles.py:104` (`ProfilesAPI.get_profile_devices`).
pub const GET_PROFILE_DEVICES: Route = GET_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.set_profile_devices` PUTs
/// `{"devices": [{"url": ...}, ...]}`, replacing all device assignments.
///
/// Ported from `eero-api src/eero/api/profiles.py:130` (`ProfilesAPI.set_profile_devices`).
pub const SET_PROFILE_DEVICES: Route = PUT_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.update_profile_content_filter` PUTs
/// `{"content_filter": {...}}` (server-side key allowlist applied client-side first).
///
/// Ported from `eero-api src/eero/api/profiles.py:179`
/// (`ProfilesAPI.update_profile_content_filter`).
pub const UPDATE_PROFILE_CONTENT_FILTER: Route = PUT_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.update_profile_block_list` PUTs
/// `{"custom_block_list": [..]}` or `{"custom_allow_list": [..]}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:227`
/// (`ProfilesAPI.update_profile_block_list`).
pub const UPDATE_PROFILE_BLOCK_LIST: Route = PUT_PROFILE;

/// Alias of `GET_PROFILE`: `ProfilesAPI.get_blocked_applications` reads the
/// `blocked_applications` (or `premium_dns.blocked_applications`) field out of the same full
/// profile object.
///
/// Ported from `eero-api src/eero/api/profiles.py:267`
/// (`ProfilesAPI.get_blocked_applications`).
pub const GET_BLOCKED_APPLICATIONS: Route = GET_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.set_blocked_applications` PUTs
/// `{"blocked_applications": [..]}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:294`
/// (`ProfilesAPI.set_blocked_applications`).
pub const SET_BLOCKED_APPLICATIONS: Route = PUT_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.rename_profile` PUTs `{"name": str}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:364` (`ProfilesAPI.rename_profile`).
pub const RENAME_PROFILE: Route = PUT_PROFILE;

/// `DELETE /2.2/networks/{network_id}/profiles/{profile_id}` — delete a profile.
///
/// Ported from `eero-api src/eero/api/profiles.py:391` (`ProfilesAPI.delete_profile`).
pub const DELETE_PROFILE: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles/{profile_id}",
};
