//! Backup Internet API: `eero-api`'s `BackupAPI`, rewritten for v8.0.4.
//!
//! Ported from `eero-api src/eero/api/backup.py` (v8.0.4). `get_backup_network`/
//! `get_backup_status`/`set_backup_network`/`configure_backup_network` (the pre-v8.0.0 shape
//! this file used to port) were removed upstream and are **not** reproduced here — replaced by
//! the `backup_access_points` module.
//!
//! Every method here funnels through `Transport::resource`, which already implements the "not
//! authenticated" precondition Python repeats at the top of each method (`get_auth_token()` /
//! `EeroAuthenticationException("Not authenticated")`) and every status-to-error mapping a
//! response can produce — so, unlike the Python source, no method below duplicates that guard.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `BackupAPI` (`src/eero/api/backup.py`), rewritten for v8.0.4.
///
/// Backup-internet features require an active Eero Plus/Eero Secure subscription — they let a
/// cellular connection stand in as a backup internet connection when the primary connection
/// fails.
///
/// Build one with `BackupApi::new`, wrapping a `Transport` already shared with the rest of the
/// `EeroApi` aggregator — `BackupApi` never constructs or owns a `Transport` itself.
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

    /// `GET /2.2/networks/{network_id}/backupinternet` — cellular-backup-internet configuration.
    ///
    /// Ported from `BackupAPI.get_backup_internet` (`backup.py:41-64`). `parent` is accepted for
    /// signature parity with the Python method but never consulted: `get_backup_internet`'s
    /// resource is built directly via `resource_url`, never a published `resources` link (module
    /// docstring, `backup.py:22-27`) — the same reason `routes::GET_BACKUP_INTERNET::link` is
    /// `None`. Returns the raw `{"meta": …, "data": {...}}` envelope; this method never inspects
    /// or reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::resource`.
    pub async fn get_backup_internet(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_BACKUP_INTERNET,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/backupinternet` — enable or disable cellular-backup
    /// internet.
    ///
    /// Ported from `BackupAPI.set_backup_internet` (`backup.py:66-103`). Sends
    /// `{"backup_internet_enabled": enabled}` (`backup.py:103`) — note this key, not `enabled`,
    /// and there is no `phone_number` field on this resource at all (`Deprecations.md:84`).
    /// `parent` is accepted but never consulted — see [`Self::get_backup_internet`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::resource`.
    pub async fn set_backup_internet(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url =
            routes::SET_BACKUP_INTERNET.resolve(self.transport.api_host(), network_id, parent)?;
        warn_uncharacterised_write("set backup internet for network");
        self.transport
            .request(
                routes::SET_BACKUP_INTERNET.method.clone(),
                url,
                &[],
                RequestBody::Json(json!({ "backup_internet_enabled": enabled })),
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/cellular_backup_usage` — cellular-backup data usage.
    ///
    /// Ported from `BackupAPI.get_cellular_backup_usage` (`backup.py:105-128`). `parent` is
    /// accepted but never consulted — see [`Self::get_backup_internet`].
    ///
    /// # Errors
    ///
    /// See [`Self::get_backup_internet`].
    pub async fn get_cellular_backup_usage(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_CELLULAR_BACKUP_USAGE,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/cellular_backup_events` — cellular-backup event log.
    ///
    /// Ported from `BackupAPI.get_cellular_backup_events` (`backup.py:130-153`). `parent` is
    /// accepted but never consulted — see [`Self::get_backup_internet`].
    ///
    /// # Errors
    ///
    /// See [`Self::get_backup_internet`].
    pub async fn get_cellular_backup_events(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_CELLULAR_BACKUP_EVENTS,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }
}
