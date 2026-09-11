//! SQM/QoS settings API: the read-only (`GET`) half of `eero-api`'s `SqmAPI`.
//!
//! Ported from `eero-api src/eero/api/sqm.py`. This phase (3, GET-only) covers
//! `SqmAPI.get_sqm_settings` only.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `SqmAPI` (`src/eero/api/sqm.py`).
///
/// Build one with [`SqmApi::new`], wrapping a [`Transport`] already shared with the rest of the
/// (not-yet-built) `EeroApi` aggregator — `SqmApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct SqmApi {
    transport: Arc<Transport>,
}

impl SqmApi {
    /// Wraps `transport` as a `SqmApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets SQM (Smart Queue Management) / `QoS` settings for a network — returns the raw Eero
    /// API response.
    ///
    /// Ported from `eero-api src/eero/api/sqm.py:36-57` (`SqmAPI.get_sqm_settings`). Sends `GET`
    /// [`crate::routes::GET_SQM_SETTINGS`], an alias of
    /// [`crate::routes::GET_NETWORK`] — this call fetches the
    /// **full network object**, not a dedicated SQM sub-resource; there is no such sub-resource
    /// on the wire. The caller is expected to read the relevant keys (`sqm`, and any others the
    /// server includes) out of the returned envelope's `data`, exactly as Python's own docstring
    /// instructs. This method never extracts, renames or reshapes any field — doing so would
    /// transform the raw payload the rest of this crate promises never to touch.
    pub async fn get_sqm_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_SQM_SETTINGS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): SqmAPI's mutation methods go here — `set_sqm_enabled`
    // (`sqm.py:59-87`), `set_sqm_bandwidth` (`sqm.py:89-135`), `configure_sqm`
    // (`sqm.py:137-180`) and `set_sqm_auto` (`sqm.py:182-204`). Notes for whoever implements
    // them:
    //
    // - All four PUT `networks/{network_id}/settings` (`crate::routes::PUT_NETWORK_SETTINGS`),
    //   the exact same wire resource `DnsAPI` and `SecurityAPI`'s setters also target — phase 5
    //   should share one `put_network_settings(network_id, body)` helper on this resource rather
    //   than repeating the call four times across three files.
    // - `set_sqm_enabled` PUTs a **flat** `{"sqm": bool}` body (`sqm.py:86`) — unlike the other
    //   three setters below, which nest an object under `"sqm"`.
    // - `set_sqm_bandwidth`'s payload shape is UNVERIFIED upstream: Python leaves a `TODO:
    //   Verify` comment at `sqm.py:128-130` questioning whether the bandwidth payload should be
    //   flattened (e.g. `{"sqm": true, "upload_bandwidth": N}`) rather than nested (`{"sqm":
    //   {"enabled": true, "upload_bandwidth"?, "download_bandwidth"?}}`, what the code actually
    //   sends today). Phase 5 must implement it exactly as Python does — nested — and the
    //   corresponding `PARITY.md` row must read "ported — shape unverified upstream", not
    //   "ported".
    // - `configure_sqm`'s payload shape carries the same caveat, at `sqm.py:173-175`: nested
    //   `{"sqm": {"enabled": bool, "upload_bandwidth"?, "download_bandwidth"?}}` (bandwidth keys
    //   included only when `enabled` is true). Same "ported — shape unverified upstream" marking.
    // - `set_sqm_auto` carries the same caveat a third time, at `sqm.py:197-199`: nested
    //   `{"sqm": {"enabled": true, "mode": "auto"}}`. Same "ported — shape unverified upstream"
    //   marking. This method takes no bool argument in Python — it always enables SQM in auto
    //   mode.
    // ---------------------------------------------------------------------------------------
}
