//! Backup Network API: the read-only (`GET`) half of `eero-api`'s `BackupAPI` (an Eero
//! Plus/Eero Secure feature).
//!
//! Ported from `eero-api src/eero/api/backup.py`. This phase (3, GET-only) covers
//! `BackupAPI.get_backup_network` and `BackupAPI.get_backup_status` — **two distinct wire
//! endpoints** (`.../backup` and `.../backup/status`), never aliases of one another.
//!
//! Every method here funnels through `Transport::send`, which already implements the "not
//! authenticated" precondition Python repeats at the top of each method (`get_auth_token()` /
//! `EeroAuthenticationException("Not authenticated")`) and every status-to-error mapping a
//! response can produce — so, unlike the Python source, no method below duplicates that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `BackupAPI` (`src/eero/api/backup.py`).
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

    // -----------------------------------------------------------------------------------------
    // PHASE 5 (not implemented here — backup mutations, added by a later agent to this same
    // file):
    //
    // - `set_backup_network` and `configure_backup_network` both PUT
    //   `routes::SET_BACKUP_NETWORK` (alias `routes::CONFIGURE_BACKUP_NETWORK`) — the exact
    //   `networks/{network_id}/backup` resource `get_backup_network` above reads. Ported from
    //   `BackupAPI.set_backup_network` (`backup.py:80-112`) and
    //   `BackupAPI.configure_backup_network` (`backup.py:114-156`).
    // - Python's `configure_backup_network`, called with neither `enabled` nor `phone_number`
    //   set, never sends a request at all: it logs a warning and fabricates a local
    //   `{"meta": {"code": 400}, "data": {}}` envelope (`backup.py:146-148`) — a response that
    //   never actually came from the server. The Rust port of this method must NOT reproduce
    //   that: an empty call is a client-side precondition failure, so it should return
    //   `Error::Validation { field, message }` before any request is sent, exactly like every
    //   other client-side precondition in this crate.
    // -----------------------------------------------------------------------------------------
}
