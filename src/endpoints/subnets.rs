//! `SubnetsApi`: `subnets` endpoints (`eero-api src/eero/api/subnets.py`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/subnets.py` (v8.0.4); methods are added by the domain port
//! (see `.claude/tasks/briefs/v8/g6-forwards-dhcp.md` for the method table this domain is scoped by). This
//! file currently defines only the struct and constructor every other module in
//! [`crate::endpoints`] already establishes, so [`crate::api::EeroApi`] can construct and expose
//! it ahead of the endpoint methods themselves landing.

use std::sync::Arc;

use crate::transport::Transport;

/// `eero-api`'s `SubnetsAPI` (`src/eero/api/subnets.py`, new in v8.0.0).
///
/// Build one with [`SubnetsApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `SubnetsApi` never constructs or owns a `Transport`
/// itself. No methods yet: this struct exists so the aggregator can wire it up ahead of the
/// domain port landing (see the module docs).
#[derive(Debug)]
pub struct SubnetsApi {
    // No method reads this field yet (that lands with the domain port, phase G) —
    // `#[derive(Debug)]` alone does not count as a "use" for dead-code analysis.
    #[allow(dead_code)]
    transport: Arc<Transport>,
}

impl SubnetsApi {
    /// Wraps `transport` as a `SubnetsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }
}
