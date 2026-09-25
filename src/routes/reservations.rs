//! DHCP-reservation routes (`ReservationsAPI`).

// ------------------------------ reservations (`ReservationsAPI`) ------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/reservations` — list DHCP reservations.
///
/// Ported from `eero-api src/eero/api/reservations.py:33`
/// (`ReservationsAPI.get_reservations`).
pub const GET_RESERVATIONS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations",
};

/// `POST /2.2/networks/{network_id}/reservations` — create a DHCP reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:56`
/// (`ReservationsAPI.create_reservation`).
pub const CREATE_RESERVATION: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations",
};

/// `PUT /2.2/networks/{network_id}/reservations/{reservation_id}` — update a DHCP
/// reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:83`
/// (`ReservationsAPI.update_reservation`).
pub const UPDATE_RESERVATION: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations/{reservation_id}",
};

/// `DELETE /2.2/networks/{network_id}/reservations/{reservation_id}` — delete a DHCP
/// reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:116`
/// (`ReservationsAPI.delete_reservation`).
pub const DELETE_RESERVATION: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations/{reservation_id}",
};
