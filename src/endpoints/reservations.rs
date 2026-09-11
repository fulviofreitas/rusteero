//! Reservations API: the read-only (`GET`) half of `eero-api`'s `ReservationsAPI`.
//!
//! Ported from `eero-api src/eero/api/reservations.py`. This phase (GET-only) covers
//! `ReservationsAPI.get_reservations`; the mutation methods are listed at the bottom of this
//! file for phase 5.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! Reservation objects carry device MACs and internal IP addresses; nothing in this module logs
//! a response body (see `.claude/rules/security-review.md`).

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The read-only half of `eero-api`'s `ReservationsAPI` (`src/eero/api/reservations.py`).
///
/// Build one with [`ReservationsApi::new`], wrapping a [`Transport`] already shared with the
/// rest of the `EeroApi` aggregator — `ReservationsApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct ReservationsApi {
    transport: Arc<Transport>,
}

impl ReservationsApi {
    /// Wraps `transport` as a `ReservationsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the DHCP reservations configured on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:33-54`
    /// (`ReservationsAPI.get_reservations`). Sends `GET` [`crate::routes::GET_RESERVATIONS`]
    /// with `network_id` substituted into the path.
    pub async fn get_reservations(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_RESERVATIONS,
                &[("network_id", network_id)],
                None,
            )
            .await
    }

    // ---------------------------------------------------------------------------------------
    // Phase 5 (not this phase): ReservationsAPI's mutation methods go here —
    // `create_reservation` (POST, `reservations.py:56-81`), `update_reservation`
    // (PUT, `reservations.py:83-114`) and `delete_reservation`
    // (DELETE `networks/{network_id}/reservations/{reservation_id}`, `reservations.py:116-137`),
    // all passthrough bodies. Each already has a `Route` constant in `src/routes.rs`
    // (`CREATE_RESERVATION`/`UPDATE_RESERVATION`/`DELETE_RESERVATION`).
    // ---------------------------------------------------------------------------------------
}
