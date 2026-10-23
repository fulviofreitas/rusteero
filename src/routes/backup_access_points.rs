//! `backup_access_points` routes (`BackupAccessPointsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/backup_access_points.py` (v8.0.4).

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/backup_access_points` — list backup access points.
///
/// Published as the `backup_access_points` link on a network envelope (verified live,
/// `test_list_prefers_parent_link`). Ported from `eero-api
/// src/eero/api/backup_access_points.py:39` (`BackupAccessPointsAPI.list`).
pub const BACKUP_ACCESS_POINTS_LIST: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backup_access_points",
    link: Some("backup_access_points"),
};

/// `POST /2.2/networks/{network_id}/backup_access_points` — add a backup access point.
///
/// Same collection resource as [`BACKUP_ACCESS_POINTS_LIST`], but `link: None`: Python's `add`
/// takes no `parent=` parameter at all (`backup_access_points.py:74-117`) — literal template
/// only. Ported from `eero-api src/eero/api/backup_access_points.py:74`
/// (`BackupAccessPointsAPI.add`).
pub const BACKUP_ACCESS_POINTS_ADD: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backup_access_points",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/backup_access_points/{backup_network_id}` — update a backup
/// access point.
///
/// `link: None`: Python's `resolve_nested_url` call for this method passes no `link=` either
/// (`backup_access_points.py:119-195`), so it never checks a published link. Ported from
/// `eero-api src/eero/api/backup_access_points.py:119` (`BackupAccessPointsAPI.update`).
pub const BACKUP_ACCESS_POINTS_UPDATE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "backup_access_points",
    suffix: "",
    link: None,
};

/// `DELETE /2.2/networks/{network_id}/backup_access_points/{backup_network_id}` — delete a
/// backup access point.
///
/// Ported from `eero-api src/eero/api/backup_access_points.py:197`
/// (`BackupAccessPointsAPI.delete_backup_access_point`).
pub const BACKUP_ACCESS_POINTS_DELETE: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "backup_access_points",
    suffix: "",
    link: None,
};

/// `POST /2.2/networks/{network_id}/backup_access_points/rearrange` — reorder backup access
/// points.
///
/// Ported from `eero-api src/eero/api/backup_access_points.py:231`
/// (`BackupAccessPointsAPI.rearrange`).
pub const BACKUP_ACCESS_POINTS_REARRANGE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backup_access_points/rearrange",
    link: None,
};

/// `GET /2.2/networks/{network_id}/backup_access_points/ssid_discovery` — read discovered SSIDs.
///
/// A distinct wire endpoint from [`BACKUP_ACCESS_POINTS_START_SSID_DISCOVERY`] only in verb —
/// same path, `GET` reads the result `POST` starts. Ported from `eero-api
/// src/eero/api/backup_access_points.py:261` (`BackupAccessPointsAPI.discover_ssids`).
pub const BACKUP_ACCESS_POINTS_DISCOVER_SSIDS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backup_access_points/ssid_discovery",
    link: None,
};

/// `POST /2.2/networks/{network_id}/backup_access_points/ssid_discovery` — start SSID discovery.
///
/// Same path as [`BACKUP_ACCESS_POINTS_DISCOVER_SSIDS`], `POST` instead of `GET`. Ported from
/// `eero-api src/eero/api/backup_access_points.py:285`
/// (`BackupAccessPointsAPI.start_ssid_discovery`).
pub const BACKUP_ACCESS_POINTS_START_SSID_DISCOVERY: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backup_access_points/ssid_discovery",
    link: None,
};

/// `POST /2.2/networks/{network_id}/backup_access_points/connectivity_check` — start a backup
/// connectivity check.
///
/// Ported from `eero-api src/eero/api/backup_access_points.py:316`
/// (`BackupAccessPointsAPI.connectivity_check`).
pub const BACKUP_ACCESS_POINTS_CONNECTIVITY_CHECK: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backup_access_points/connectivity_check",
    link: None,
};
