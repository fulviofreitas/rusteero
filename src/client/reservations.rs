//! `Client` methods for the `ReservationsAPI` domain.

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;
use serde_json::Value;

impl Client {
    /// Gets DHCP reservations — returns the raw Eero API response.
    ///
    /// Ported from `get_reservations()` (`client.py:879-882`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_reservations(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.reservations().get_reservations(&network_id).await
    }

    // ==================== Reservations ====================

    /// Creates a DHCP reservation on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_reservation` (`eero-api src/eero/client.py:884-889`). `auto_discover =
    /// false`. Invalidates nothing: there is no `reservations` cache bucket (behaviour brief
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
        self.api
            .reservations()
            .create_reservation(&network_id, reservation_data)
            .await
    }

    /// Updates a DHCP reservation on the network — returns the raw Eero API response.
    ///
    /// Ported from `update_reservation` (`eero-api src/eero/client.py:891-901`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::create_reservation`].
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
            .update_reservation(&network_id, reservation_id, reservation_data)
            .await
    }

    /// Deletes a DHCP reservation from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_reservation` (`eero-api src/eero/client.py:903-908`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::create_reservation`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_reservation(
        &self,
        reservation_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .reservations()
            .delete_reservation(&network_id, reservation_id)
            .await
    }
}
