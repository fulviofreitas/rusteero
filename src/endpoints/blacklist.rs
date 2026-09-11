//! Device Blacklist API: the read-only (`GET`) half of `eero-api`'s `BlacklistAPI`.
//!
//! Ported from `eero-api src/eero/api/blacklist.py`. This phase (3, GET-only) covers
//! `BlacklistAPI.get_blacklist`.
//!
//! Every method here funnels through `Transport::send`, which already implements the "not
//! authenticated" precondition Python repeats at the top of each method (`get_auth_token()` /
//! `EeroAuthenticationException("Not authenticated")`) and every status-to-error mapping a
//! response can produce — so, unlike the Python source, no method below duplicates that guard.
//!
//! A blacklist response lists the MAC addresses of every blocked device on a network; nothing
//! in this module logs a response body (nor does anything else in this crate — see
//! `Transport`'s own module docs for the method+path+status-only logging discipline).

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `BlacklistAPI` (`src/eero/api/blacklist.py`).
///
/// Build one with `BlacklistApi::new`, wrapping a `Transport` already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `BlacklistApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct BlacklistApi {
    transport: Arc<Transport>,
}

impl BlacklistApi {
    /// Wraps `transport` as a `BlacklistApi`.
    ///
    /// Ported from `BlacklistAPI.__init__` (`blacklist.py:25-31`), which wraps an `AuthAPI`
    /// rather than a bare transport handle — `Transport` already owns both the current session
    /// and the "not authenticated" precondition every Python method above re-derives by hand,
    /// so wrapping it directly is a strict simplification, not a behaviour change.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/blacklist` — list blacklisted (blocked) devices.
    ///
    /// Ported from `BlacklistAPI.get_blacklist` (`blacklist.py:33-54`). Returns the raw
    /// `{"meta": …, "data": [...]}` envelope, one entry per blocked device, each carrying a MAC
    /// address; this method never inspects, redacts or reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn get_blacklist(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_BLACKLIST, &[("network_id", network_id)], None)
            .await
    }

    // -----------------------------------------------------------------------------------------
    // PHASE 5 (not implemented here — blacklist mutations, added by a later agent to this same
    // file):
    //
    // - `add_to_blacklist` POSTs `routes::ADD_TO_BLACKLIST` with a `{"mac": <mac>}` body.
    //   Ported from `BlacklistAPI.add_to_blacklist` (`blacklist.py:56-79`).
    // - `remove_from_blacklist` DELETEs `routes::REMOVE_FROM_BLACKLIST`
    //   (`networks/{network_id}/blacklist/{mac_or_device_id}`), where the trailing segment
    //   accepts either a colon-separated MAC or Eero's blacklist `device_id` — live-verified to
    //   be the same MAC with its colons stripped (`blacklist.py:87-89`). Ported from
    //   `BlacklistAPI.remove_from_blacklist` (`blacklist.py:81-106`).
    // - `DevicesApi::block_device` (phase 5, `src/endpoints/devices.rs`) is built on these two
    //   endpoints, not on a `PUT`: `PUT {"blocked": bool}` on the device resource is a confirmed
    //   no-op (`eero-api` issue #109
    //   <https://github.com/fulviofreitas/eero-api/issues/109>). Blocking a device is therefore
    //   two separate round-trips — `GET_DEVICE` to resolve the device's MAC, then
    //   `ADD_TO_BLACKLIST` to block it; unblocking is one round-trip to
    //   `REMOVE_FROM_BLACKLIST`. Ported from `DevicesAPI.block_device` (`devices.py:136-186`),
    //   which raises a 502-shaped exception when the resolved device has no MAC
    //   (`devices.py:173-177`) — the Rust port should surface the equivalent `Error` variant
    //   rather than sending a blacklist request with an empty MAC.
    // -----------------------------------------------------------------------------------------
}
