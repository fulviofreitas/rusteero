//! Backup-internet routes (`BackupAPI`, rewritten for v8.0.4).
//!
//! `BackupAPI.get_backup_network`/`get_backup_status`/`set_backup_network`/
//! `configure_backup_network` were removed upstream in `eero-api` v8.0.0 — the `networks/{id}/
//! backup` resource they targeted no longer exists. `get_backup_internet`/`set_backup_internet`
//! below target a different resource (`networks/{id}/backupinternet`) with a narrower shape (no
//! `phone_number` field).

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/backupinternet` — cellular-backup-internet configuration.
///
/// Built directly via `resource_url`, never a published `resources` link (module docstring,
/// `backup.py:22-27`), so [`Resource::link`] is `None`. Ported from `eero-api
/// src/eero/api/backup.py:41` (`BackupAPI.get_backup_internet`).
pub const GET_BACKUP_INTERNET: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backupinternet",
    link: None,
};

/// `PUT /2.2/networks/{network_id}/backupinternet` — enable/disable cellular-backup internet.
///
/// Same resource `GET_BACKUP_INTERNET` reads. Ported from `eero-api src/eero/api/backup.py:66`
/// (`BackupAPI.set_backup_internet`).
pub const SET_BACKUP_INTERNET: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "networks/{id}/backupinternet",
    link: None,
};

/// `GET /2.2/networks/{network_id}/cellular_backup_usage` — cellular-backup data usage.
///
/// Ported from `eero-api src/eero/api/backup.py:105` (`BackupAPI.get_cellular_backup_usage`).
pub const GET_CELLULAR_BACKUP_USAGE: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/cellular_backup_usage",
    link: None,
};

/// `GET /2.2/networks/{network_id}/cellular_backup_events` — cellular-backup event log.
///
/// Ported from `eero-api src/eero/api/backup.py:130` (`BackupAPI.get_cellular_backup_events`).
pub const GET_CELLULAR_BACKUP_EVENTS: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/cellular_backup_events",
    link: None,
};
