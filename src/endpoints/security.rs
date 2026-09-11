//! Security settings API: the read-only (`GET`) half of `eero-api`'s `SecurityAPI`.
//!
//! Ported from `eero-api src/eero/api/security.py`. This phase (3, GET-only) covers
//! `SecurityAPI.get_security_settings` only.
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

/// The read-only half of `eero-api`'s `SecurityAPI` (`src/eero/api/security.py`).
///
/// Build one with [`SecurityApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `SecurityApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct SecurityApi {
    transport: Arc<Transport>,
}

impl SecurityApi {
    /// Wraps `transport` as a `SecurityApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets security settings for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/security.py:36-57`
    /// (`SecurityAPI.get_security_settings`). Sends `GET`
    /// [`crate::routes::GET_SECURITY_SETTINGS`], an alias of
    /// [`crate::routes::GET_NETWORK`] — this call fetches the
    /// **full network object**, not a dedicated security sub-resource; there is no such
    /// sub-resource on the wire. The caller is expected to read the relevant keys (`wpa3`,
    /// `band_steering`, `upnp`, `ipv6_upstream`, `ipv6_downstream`, `thread`, and any others the
    /// server includes) out of the returned envelope's `data`, exactly as Python's own docstring
    /// instructs. This method never extracts, renames or reshapes any field — doing so would
    /// transform the raw payload the rest of this crate promises never to touch.
    pub async fn get_security_settings(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_SECURITY_SETTINGS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): SecurityAPI's mutation methods go here — `set_wpa3`
    // (`security.py:59-90`), `set_band_steering` (`security.py:92-123`), `set_upnp`
    // (`security.py:125-156`), `set_ipv6` (`security.py:158-189`), `set_thread`
    // (`security.py:191-222`) and `configure_security` (`security.py:224-282`). Notes for
    // whoever implements them:
    //
    // - All six PUT `networks/{network_id}/settings`
    //   (`crate::routes::PUT_NETWORK_SETTINGS`), the exact same wire resource `DnsAPI` and
    //   `SqmAPI`'s setters also target — phase 5 should share one `put_network_settings(network_id,
    //   body)` helper on this resource rather than repeating the call six times across three
    //   files.
    // - `set_ipv6` PUTs *both* `ipv6_upstream` and `ipv6_downstream` set to the same bool
    //   (`security.py:185-188`) — a single flag fans out to two wire keys.
    // - `configure_security` with every optional argument `None` builds an empty payload.
    //   Python fabricates a local `{"meta": {"code": 400}, "data": {}}` response without ever
    //   calling the server (`security.py:272-274`); this port should instead return
    //   `Error::Validation` for that case — inventing a fake envelope would be a worse violation
    //   of the raw-payload contract than simply refusing before any request is sent.
    // ---------------------------------------------------------------------------------------
}
