//! Reservations API: `eero-api`'s `ReservationsAPI`.
//!
//! Ported from `eero-api src/eero/api/reservations.py`: `ReservationsAPI.get_reservations` plus
//! the three mutation methods, `create_reservation`, `update_reservation` and
//! `delete_reservation`.
//!
//! Every method here funnels through [`crate::transport::Transport::send`], which already
//! implements the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! Reservation objects carry device MACs and internal IP addresses; nothing in this module logs
//! a response body (see the crate's security guidelines).

use std::sync::Arc;

use serde_json::Value;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `ReservationsAPI` (`src/eero/api/reservations.py`).
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

    /// Creates a DHCP reservation on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:56-81`
    /// (`ReservationsAPI.create_reservation`). Sends `POST`
    /// [`crate::routes::CREATE_RESERVATION`] with `reservation_data` attached as the request's
    /// JSON body exactly as given. Neither Python nor this port validates or reshapes
    /// `reservation_data` in any way; it is a pure passthrough.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn create_reservation(
        &self,
        network_id: &str,
        reservation_data: Value,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::CREATE_RESERVATION,
                &[("network_id", network_id)],
                Some(reservation_data),
            )
            .await
    }

    /// Updates a DHCP reservation on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:83-114`
    /// (`ReservationsAPI.update_reservation`). Sends `PUT`
    /// [`crate::routes::UPDATE_RESERVATION`] with `network_id` and `reservation_id` substituted
    /// into the path, and `reservation_data` attached as the request's JSON body exactly as
    /// given — a pure passthrough, like [`ReservationsApi::create_reservation`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn update_reservation(
        &self,
        network_id: &str,
        reservation_id: &str,
        reservation_data: Value,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::UPDATE_RESERVATION,
                &[
                    ("network_id", network_id),
                    ("reservation_id", reservation_id),
                ],
                Some(reservation_data),
            )
            .await
    }

    /// Deletes a DHCP reservation from a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:116-138`
    /// (`ReservationsAPI.delete_reservation`). Sends `DELETE`
    /// [`crate::routes::DELETE_RESERVATION`] with `network_id` and `reservation_id` substituted
    /// into the path, and no body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn delete_reservation(
        &self,
        network_id: &str,
        reservation_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::DELETE_RESERVATION,
                &[
                    ("network_id", network_id),
                    ("reservation_id", reservation_id),
                ],
                None,
            )
            .await
    }
}
