//! Device Blacklist API: `eero-api`'s `BlacklistAPI` — list, add and remove blacklisted
//! (blocked) devices.
//!
//! Ported from `eero-api src/eero/api/blacklist.py`: `BlacklistAPI.get_blacklist`,
//! `BlacklistAPI.add_to_blacklist`, `BlacklistAPI.remove_from_blacklist`.
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

use serde_json::json;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `BlacklistAPI` (`src/eero/api/blacklist.py`).
///
/// Build one with `BlacklistApi::new`, wrapping a `Transport` already shared with the rest of
/// the `EeroApi` aggregator — `BlacklistApi` never constructs or owns a `Transport` itself.
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

    /// `POST /2.2/networks/{network_id}/blacklist` — add a device (by MAC) to the blacklist.
    ///
    /// Ported from `BlacklistAPI.add_to_blacklist` (`blacklist.py:56-79`). Sends body
    /// `{"mac": mac}` (`blacklist.py:75-79`); `mac` is passed through unchanged — no
    /// normalisation of separators or case happens here, matching Python exactly.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn add_to_blacklist(&self, network_id: &str, mac: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::ADD_TO_BLACKLIST,
                &[("network_id", network_id)],
                Some(json!({ "mac": mac })),
            )
            .await
    }

    /// `DELETE /2.2/networks/{network_id}/blacklist/{mac_or_device_id}` — remove a device from
    /// the blacklist.
    ///
    /// Ported from `BlacklistAPI.remove_from_blacklist` (`blacklist.py:81-106`).
    /// `mac_or_device_id` is passed through unchanged, exactly as Python does — no normalisation
    /// happens here. Per `blacklist.py:87-89`, the trailing segment accepts either a
    /// colon-separated MAC or Eero's blacklist `device_id` (live-verified to be the same MAC
    /// with its colons stripped); callers decide which form to pass, this method does not
    /// convert between them.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::send`.
    pub async fn remove_from_blacklist(
        &self,
        network_id: &str,
        mac_or_device_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::REMOVE_FROM_BLACKLIST,
                &[
                    ("network_id", network_id),
                    ("mac_or_device_id", mac_or_device_id),
                ],
                None,
            )
            .await
    }
}
