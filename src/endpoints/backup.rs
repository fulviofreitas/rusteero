//! Backup Network API: `eero-api`'s `BackupAPI` (an Eero Plus/Eero Secure feature).
//!
//! Ported from `eero-api src/eero/api/backup.py`: `BackupAPI.get_backup_network` and
//! `BackupAPI.get_backup_status` — **two distinct wire endpoints** (`.../backup` and
//! `.../backup/status`), never aliases of one another — plus the two mutation methods,
//! `BackupAPI.set_backup_network` and `BackupAPI.configure_backup_network`.
//!
//! Every method here funnels through `Transport::send`, which already implements the "not
//! authenticated" precondition Python repeats at the top of each method (`get_auth_token()` /
//! `EeroAuthenticationException("Not authenticated")`) and every status-to-error mapping a
//! response can produce — so, unlike the Python source, no method below duplicates that guard.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `BackupAPI` (`src/eero/api/backup.py`).
///
/// Backup network features require an active Eero Plus/Eero Secure subscription — they let a
/// mobile phone stand in as a backup internet connection when the primary connection fails.
///
/// Build one with `BackupApi::new`, wrapping a `Transport` already shared with the rest of the
/// (not-yet-built) `EeroApi` aggregator — `BackupApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct BackupApi {
    transport: Arc<Transport>,
}

impl BackupApi {
    /// Wraps `transport` as a `BackupApi`.
    ///
    /// Ported from `BackupAPI.__init__` (`backup.py:29-35`), which wraps an `AuthAPI` rather
    /// than a bare transport handle — `Transport` already owns both the current session and the
    /// "not authenticated" precondition every Python method above re-derives by hand, so
    /// wrapping it directly is a strict simplification, not a behaviour change.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/backup` — backup-network (Eero Plus) configuration.
    ///
    /// Ported from `BackupAPI.get_backup_network` (`backup.py:37-55`). Returns the raw
    /// `{"meta": …, "data": {...}}` envelope; this method never inspects or reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn get_backup_network(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_BACKUP_NETWORK,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/backup/status` — current backup-network status.
    ///
    /// A distinct wire endpoint from `get_backup_network` above (`.../backup/status`, not
    /// `.../backup`) — the two share no `Route` and are never aliases of one another. Ported
    /// from `BackupAPI.get_backup_status` (`backup.py:57-78`). Returns the raw
    /// `{"meta": …, "data": {...}}` envelope.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn get_backup_status(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_BACKUP_STATUS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/backup` — enable or disable the backup network.
    ///
    /// Ported from `BackupAPI.set_backup_network` (`backup.py:80-112`). Sends
    /// `routes::SET_BACKUP_NETWORK` with body `{"enabled": enabled}` (`backup.py:111`) — the
    /// exact `networks/{network_id}/backup` resource `get_backup_network` reads.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn set_backup_network(
        &self,
        network_id: &str,
        enabled: bool,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::SET_BACKUP_NETWORK,
                &[("network_id", network_id)],
                Some(json!({ "enabled": enabled })),
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/backup` — configure backup network settings.
    ///
    /// Ported from `BackupAPI.configure_backup_network` (`backup.py:114-156`). Sends
    /// `routes::CONFIGURE_BACKUP_NETWORK` (alias of `routes::SET_BACKUP_NETWORK`, the same
    /// `networks/{network_id}/backup` resource `set_backup_network` above PUTs) with a body
    /// built from only the arguments actually supplied: `{"enabled": ...}` is included only when
    /// `enabled` is `Some` (`backup.py:140-141`), `{"phone_number": ...}` only when
    /// `phone_number` is `Some` (`backup.py:143-144`). Neither key is ever sent as `null` for an
    /// omitted argument — the omitted argument's key is absent from the body entirely.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if both `enabled` and `phone_number` are `None`, before any
    /// request is sent. This is a deliberate divergence from Python, which never contacts the
    /// server in that case either but instead fabricates a local
    /// `{"meta": {"code": 400}, "data": {}}` response (`backup.py:146-148`) — a response that
    /// never actually came from the wire. Inventing a fake envelope here would violate this
    /// crate's raw-payload contract more than simply refusing before any request is built (port
    /// plan §3.3). Returns `Error::Authentication("Not authenticated")` if no valid session is
    /// configured, or whatever other status-mapped error the request produces otherwise — see
    /// `Transport::send`.
    pub async fn configure_backup_network(
        &self,
        network_id: &str,
        enabled: Option<bool>,
        phone_number: Option<&str>,
    ) -> Result<Envelope, Error> {
        let mut payload = Map::new();
        if let Some(enabled) = enabled {
            payload.insert("enabled".to_owned(), Value::Bool(enabled));
        }
        if let Some(phone_number) = phone_number {
            payload.insert(
                "phone_number".to_owned(),
                Value::String(phone_number.to_owned()),
            );
        }
        if payload.is_empty() {
            return Err(Error::validation(
                "enabled, phone_number",
                "at least one of `enabled` or `phone_number` must be provided",
            ));
        }

        self.transport
            .send(
                &routes::CONFIGURE_BACKUP_NETWORK,
                &[("network_id", network_id)],
                Some(Value::Object(payload)),
            )
            .await
    }
}
