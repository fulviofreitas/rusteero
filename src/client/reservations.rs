//! `Client` methods for the `ReservationsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets DHCP reservations — returns the raw Eero API response.
    ///
    /// Ported from `get_reservations()` (`eero-api src/eero/client.py:1496-1501`). `auto_discover
    /// = false`. Passes the network's cached envelope (if fresh) as `parent`, matching
    /// `_network_parent_kwargs`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_reservations(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .reservations()
            .get_reservations(&network_id, parent.as_ref())
            .await
    }

    // ==================== Reservations ====================

    /// Creates a DHCP reservation on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_reservation` (`eero-api src/eero/client.py:1503-1510`). `auto_discover
    /// = false`. Invalidates nothing: there is no `reservations` cache bucket (behaviour brief
    /// §2.1).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn create_reservation(
        &self,
        reservation_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(&network_id);
        self.api
            .reservations()
            .create_reservation(&network_id, reservation_data, parent.as_ref())
            .await
    }

    /// Updates a DHCP reservation on the network — returns the raw Eero API response.
    ///
    /// Ported from `update_reservation` (`eero-api src/eero/client.py:1512-1522`).
    /// `auto_discover = false` — unlike every other row in this file, `network_id` is resolved
    /// and forwarded to the domain call as `network=Some(..)` unconditionally (Python's
    /// `network=network_id` after `_ensure_network_id`), not passed straight through as
    /// `Option`. No `parent=` forwarded, matching Python exactly. Invalidates nothing — see
    /// [`Client::create_reservation`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn update_reservation(
        &self,
        reservation_id: &str,
        reservation_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .reservations()
            .update_reservation(reservation_id, reservation_data, Some(&network_id), None)
            .await
    }

    /// Deletes a DHCP reservation from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_reservation` (`eero-api src/eero/client.py:1524-1536`).
    /// `auto_discover = false`. `delete_forwards` is forwarded to the domain call unchanged —
    /// `None` means the query parameter is omitted from the request entirely, matching Python's
    /// `kwargs` dict built only when `delete_forwards is not None` (brief §3.7). Invalidates
    /// nothing — see [`Client::create_reservation`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_reservation(
        &self,
        reservation_id: &str,
        network_id: Option<&str>,
        delete_forwards: Option<bool>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .reservations()
            .delete_reservation(&network_id, reservation_id, delete_forwards)
            .await
    }
}
