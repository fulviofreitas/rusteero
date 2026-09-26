//! DHCP-reservation routes (`ReservationsAPI`).

// ------------------------------ reservations (`ReservationsAPI`) ------------------------

use super::{ApiVersion, Nested, Resource};
use reqwest::Method;

// ------------------------------------ v8.0.4 routes --------------------------------------

/// `GET /2.2/networks/{id}/reservations` (or the parent's own `reservations` link) — list DHCP
/// reservations.
///
/// Ported from `eero-api src/eero/api/reservations.py:82-107` (v8.0.4,
/// `ReservationsAPI.get_reservations`): `sub_resource_url(network,
/// "networks/{id}/reservations", link="reservations", parent=.., version=API_VERSION_DEFAULT)`.
pub const RESERVATIONS_GET: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "networks/{id}/reservations",
    link: Some("reservations"),
};

/// `POST /2.2/networks/{id}/reservations` (or the parent's own `reservations` link) — create a
/// DHCP reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:114-153` (v8.0.4,
/// `ReservationsAPI.create_reservation`): same URL resolution as [`RESERVATIONS_GET`].
pub const RESERVATIONS_CREATE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "networks/{id}/reservations",
    link: Some("reservations"),
};

/// `PUT /2.2/networks/{network}/reservations/{reservation}` (or an already-resolved path/URL) —
/// update a DHCP reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:154-184` (v8.0.4,
/// `ReservationsAPI.update_reservation` via the free function `_resolve_reservation_url`):
/// `resolve_nested_url(network, reservation, prefix="reservations")`, used by
/// [`crate::endpoints::reservations::ReservationsApi::update_reservation`] only for its bare-id
/// and path/URL dispatch branches — the "`reservation` is a cached envelope" branch resolves via
/// [`crate::links::self_url`] directly instead, never through this route. No `link`: Python's
/// `_resolve_reservation_url` never consults a parent's `resources` map.
pub const RESERVATIONS_UPDATE: Nested = Nested {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    prefix: "reservations",
    suffix: "",
    link: None,
};

/// `DELETE /2.2/networks/{network}/reservations/{reservation}` (bare id, path, or URL) — delete
/// a DHCP reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:187-216` (v8.0.4,
/// `ReservationsAPI.delete_reservation`): `resolve_nested_url(network, reservation,
/// prefix="reservations")`.
pub const RESERVATIONS_DELETE: Nested = Nested {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    prefix: "reservations",
    suffix: "",
    link: None,
};
