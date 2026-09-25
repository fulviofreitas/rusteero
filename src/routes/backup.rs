//! Backup-network routes (`BackupAPI`).

// -------------------------------- backup (`BackupAPI`) ---------------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/backup` — backup-network (Eero Plus) configuration.
///
/// Ported from `eero-api src/eero/api/backup.py:37` (`BackupAPI.get_backup_network`).
pub const GET_BACKUP_NETWORK: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/backup",
};

/// `GET /2.2/networks/{network_id}/backup/status` — current backup-network status.
///
/// Ported from `eero-api src/eero/api/backup.py:57` (`BackupAPI.get_backup_status`).
pub const GET_BACKUP_STATUS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/backup/status",
};

/// `PUT /2.2/networks/{network_id}/backup` — enable/disable or configure the backup network.
///
/// Ported from `eero-api src/eero/api/backup.py:80` (`BackupAPI.set_backup_network`). Also
/// the target of `BackupAPI.configure_backup_network` (`backup.py:114`; see
/// `CONFIGURE_BACKUP_NETWORK`) — same endpoint, a subset payload.
pub const SET_BACKUP_NETWORK: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/backup",
};

/// Alias of `SET_BACKUP_NETWORK`: `BackupAPI.configure_backup_network` PUTs the same
/// `networks/{network_id}/backup` resource with a partial `{enabled?, phone_number?}` body.
///
/// Ported from `eero-api src/eero/api/backup.py:114`.
pub const CONFIGURE_BACKUP_NETWORK: Route = SET_BACKUP_NETWORK;
