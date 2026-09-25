//! `BackupAccessPointsApi`: `backup_access_points` endpoints (`eero-api src/eero/api/backup_access_points.py`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/backup_access_points.py` (v8.0.4); methods are added by the domain port
//! (see `.claude/tasks/briefs/v8/g7-backup-members.md` for the method table this domain is scoped by). This
//! file currently defines only the struct and constructor every other module in
//! [`crate::endpoints`] already establishes, so [`crate::api::EeroApi`] can construct and expose
//! it ahead of the endpoint methods themselves landing.

use std::sync::Arc;

use crate::transport::Transport;

/// `eero-api`'s `BackupAccessPointsAPI` (`src/eero/api/backup_access_points.py`, new in v8.0.0).
///
/// Build one with [`BackupAccessPointsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `BackupAccessPointsApi` never constructs or owns a `Transport`
/// itself. No methods yet: this struct exists so the aggregator can wire it up ahead of the
/// domain port landing (see the module docs).
#[derive(Debug)]
pub struct BackupAccessPointsApi {
    // No method reads this field yet (that lands with the domain port, phase G) —
    // `#[derive(Debug)]` alone does not count as a "use" for dead-code analysis.
    #[allow(dead_code)]
    transport: Arc<Transport>,
}

impl BackupAccessPointsApi {
    /// Wraps `transport` as a `BackupAccessPointsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }
}
