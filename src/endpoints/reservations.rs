//! Reservations API: `eero-api`'s `ReservationsAPI`.
//!
//! Ported from `eero-api src/eero/api/reservations.py` (v8.0.4): `ReservationsAPI.get_reservations`,
//! `create_reservation`, `update_reservation` and `delete_reservation`.
//!
//! Every method here funnels through [`crate::transport::Transport::resource`]/
//! [`crate::transport::Transport::nested`]/[`crate::transport::Transport::request`], which
//! already implement the "not authenticated" precondition Python repeats at the top of each
//! method (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.
//!
//! Reservation objects carry device MACs and internal IP addresses; nothing in this module logs
//! a response body (see the crate's security guidelines).
//!
//! # `update_reservation`'s `reservation` argument
//!
//! Python's `update_reservation(reservation: Any, data, *, network=None)` accepts either a bare
//! id, a host-relative path / absolute URL, or the reservation's own cached envelope (a
//! `Mapping`) for `reservation` (`reservations.py:26-65`, the free function
//! `_resolve_reservation_url`). This port keeps the same three-way dispatch but expresses it
//! across two parameters instead of one dynamically-typed one: see
//! [`crate::endpoints::forwards`]'s module docs — `update_reservation` mirrors
//! `update_forward`'s `forward`/`network`/`parent` shape exactly, field name `"reservation"`
//! everywhere Python names `"forward"`.

use std::sync::Arc;

use serde_json::Value;
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes;
use crate::transport::{RequestBody, Transport};

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
    /// Ported from `eero-api src/eero/api/reservations.py:82-107` (v8.0.4,
    /// `ReservationsAPI.get_reservations`). Sends `GET`
    /// [`crate::routes::reservations::RESERVATIONS_GET`], preferring `parent`'s published
    /// `reservations` link over the `networks/{id}/reservations` template when supplied.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn get_reservations(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::reservations::RESERVATIONS_GET,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Creates a DHCP reservation on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:114-153` (v8.0.4,
    /// `ReservationsAPI.create_reservation`). Sends `POST`
    /// [`crate::routes::reservations::RESERVATIONS_CREATE`] with `reservation_data` attached as
    /// the request's JSON body exactly as given — neither Python nor this port validates or
    /// reshapes it. Logs [`crate::links::warn_uncharacterised_write`] (`"create reservation for
    /// network"`) immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn create_reservation(
        &self,
        network_id: &str,
        reservation_data: Value,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = routes::reservations::RESERVATIONS_CREATE.resolve(
            self.transport.api_host(),
            network_id,
            parent,
        )?;
        links::warn_uncharacterised_write("create reservation for network");
        self.transport
            .request(
                routes::reservations::RESERVATIONS_CREATE.method.clone(),
                url,
                &[],
                RequestBody::Json(reservation_data),
            )
            .await
    }

    /// Updates a DHCP reservation via its own URL — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:154-184` (v8.0.4,
    /// `ReservationsAPI.update_reservation`, via the free function
    /// `_resolve_reservation_url`). See the module docs for how the id-or-path-or-URL-or-envelope
    /// dispatch maps onto this method's `reservation`/`network`/`parent` parameters. Sends `PUT`
    /// to the resolved URL with `data` attached as the request's JSON body exactly as given.
    /// Logs [`crate::links::warn_uncharacterised_write`] with the operation string
    /// `"update_reservation"` — the literal method name, **not** a human-readable phrase like
    /// every other write in this module; this is an intentional quirk of the Python source
    /// (`reservations.py:182`), preserved verbatim rather than "fixed" to match its neighbours.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "reservation"` if `parent` is supplied but has
    /// no resolvable `url` field. Returns [`Error::Validation`] with `field: "network"` if
    /// `parent` is `None`, `reservation` is a bare id (does not start with `http://`,
    /// `https://`, or `/`), and `network` is `None`. Returns [`Error::Authentication`] if no
    /// valid session is configured, or whatever status-mapped [`Error`] the request produces
    /// otherwise.
    pub async fn update_reservation(
        &self,
        reservation: &str,
        data: Value,
        network: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve_reservation_url(reservation, network, parent)?;
        links::warn_uncharacterised_write("update_reservation");
        self.transport
            .request(reqwest::Method::PUT, url, &[], RequestBody::Json(data))
            .await
    }

    /// Deletes a DHCP reservation from a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/reservations.py:187-216` (v8.0.4,
    /// `ReservationsAPI.delete_reservation`). Sends `DELETE`
    /// [`crate::routes::reservations::RESERVATIONS_DELETE`] with `network_id` (required, unlike
    /// `update_reservation`) and `reservation` (a bare id, path, or absolute URL) resolved via
    /// `resolve_nested_url`'s dispatch. `delete_forwards` is sent as the query parameter
    /// `delete_forwards=true`/`false` only when `Some`; when `None` the request carries no such
    /// query parameter at all (a tri-state, not a boolean default). Logs
    /// [`crate::links::warn_uncharacterised_write`] (`"delete reservation for network"`)
    /// immediately before issuing the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn delete_reservation(
        &self,
        network_id: &str,
        reservation: &str,
        delete_forwards: Option<bool>,
    ) -> Result<Envelope, Error> {
        let url = routes::reservations::RESERVATIONS_DELETE.resolve(
            self.transport.api_host(),
            network_id,
            reservation,
            None,
        )?;
        let query: Vec<(&str, String)> = match delete_forwards {
            Some(true) => vec![("delete_forwards", "true".to_owned())],
            Some(false) => vec![("delete_forwards", "false".to_owned())],
            None => Vec::new(),
        };
        links::warn_uncharacterised_write("delete reservation for network");
        self.transport
            .request(
                routes::reservations::RESERVATIONS_DELETE.method.clone(),
                url,
                &query,
                RequestBody::None,
            )
            .await
    }

    /// Resolves a reservation's absolute URL from a bare id, a path/URL, or a cached envelope.
    ///
    /// Ported from `_resolve_reservation_url` (`eero-api src/eero/api/reservations.py:26-65`);
    /// see the module docs for how the three Python branches map onto
    /// `reservation`/`network`/`parent`.
    fn resolve_reservation_url(
        &self,
        reservation: &str,
        network: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let host = self.transport.api_host();
        if let Some(parent) = parent {
            return links::self_url(host, parent)?.ok_or_else(|| {
                Error::validation("reservation", "envelope has no resolvable 'url' field")
            });
        }
        if reservation.starts_with("http://")
            || reservation.starts_with("https://")
            || reservation.starts_with('/')
        {
            return routes::reservations::RESERVATIONS_UPDATE.resolve(
                host,
                network.unwrap_or(""),
                reservation,
                None,
            );
        }
        let network = network.ok_or_else(|| {
            Error::validation(
                "network",
                "required when 'reservation' is a bare ID rather than a path/URL",
            )
        })?;
        routes::reservations::RESERVATIONS_UPDATE.resolve(host, network, reservation, None)
    }
}
