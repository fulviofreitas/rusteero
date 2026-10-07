//! `Client` methods for the `NotificationsAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api`'s `EeroClient` notifications-scoped wrappers (entirely new in v8.0.0).
//! Every method passes
//! `auto_discover = false` and, except [`Client::set_push_settings`] (account-scoped, no
//! `network_id` at all), passes the cached network envelope as `parent=` (`+net`).

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the network's notification settings — returns the raw Eero API response.
    ///
    /// Ported from `get_notification_settings` (`eero-api src/eero/client.py:2380-2387`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_notification_settings(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .notifications()
            .get_settings(&network_id, parent.as_ref())
            .await
    }

    /// Sets the network's notification settings — returns the raw Eero API response.
    ///
    /// Ported from `set_notification_settings` (`eero-api src/eero/client.py:2387-2401`). On
    /// success, invalidates `network[{nid}]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_notification_settings(
        &self,
        settings: &[(&str, bool)],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .notifications()
            .set_settings(&network_id, settings, parent.as_ref())
            .await?;
        self.invalidate_network_cache(network_id.as_str());
        Ok(response)
    }

    /// Checks whether the network has unread notifications — returns the raw Eero API response.
    ///
    /// Ported from `has_unread_notifications` (`eero-api src/eero/client.py:2401-2408`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn has_unread_notifications(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .notifications()
            .has_unread(&network_id, parent.as_ref())
            .await
    }

    /// Marks the network's notifications as read — returns the raw Eero API response.
    ///
    /// Ported from `mark_notifications_read` (`eero-api src/eero/client.py:2408-2415`). Nothing
    /// invalidated: notifications are never cached.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn mark_notifications_read(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .notifications()
            .mark_read(&network_id, parent.as_ref())
            .await
    }

    /// Gets the network's notification history — returns the raw Eero API response.
    ///
    /// Ported from `get_notification_history` (`eero-api src/eero/client.py:2415-2424`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_notification_history(
        &self,
        timestamp: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .notifications()
            .get_history(&network_id, timestamp, parent.as_ref())
            .await
    }

    /// Sets the account's push notification settings — returns the raw Eero API response.
    ///
    /// Ported from `set_push_settings` (`eero-api src/eero/client.py:2424-2430`). Account-scoped:
    /// no `network_id`, no `parent=`. Nothing invalidated: never cached.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured, or
    /// whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn set_push_settings(&self, settings: &[(&str, bool)]) -> Result<Envelope, Error> {
        self.api.notifications().set_push_settings(settings).await
    }
}
