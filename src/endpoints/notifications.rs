//! `NotificationsApi`: `notifications` endpoints (`eero-api src/eero/api/notifications.py`, new
//! in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/notifications.py` (v8.0.4). Every method except
//! [`NotificationsApi::set_push_settings`] is network-scoped and resolved via
//! `resolve` (a literal path suffix appended to the network's own resolved
//! base URL — see `routes::notifications`'s module docs for why this cannot be expressed as a
//! plain [`crate::routes::Resource::resolve`] call).

use std::sync::Arc;

use serde_json::{Map, Value};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes::{self, Resource};
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `NotificationsAPI` (`src/eero/api/notifications.py`, new in v8.0.0).
#[derive(Debug)]
pub struct NotificationsApi {
    transport: Arc<Transport>,
}

impl NotificationsApi {
    /// Wraps `transport` as a `NotificationsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Resolves a network-scoped notifications request URL.
    ///
    /// Ported from the shared `f"{resolve_network_url(network_id, parent)}{suffix}"` expression
    /// every method but `set_push_settings` builds (`notifications.py:62,109,139,176,208`):
    /// prefers [`links::self_url`] on `parent` when supplied and resolvable, appending `suffix`
    /// verbatim; otherwise resolves `route` (whose `template` already embeds the same `suffix`
    /// after `networks/{id}`) from `network_id` alone.
    fn resolve(
        &self,
        route: &Resource,
        network_id: &str,
        parent: Option<&Value>,
        suffix: &str,
    ) -> Result<Url, Error> {
        let host = self.transport.api_host();
        if let Some(parent) = parent
            && let Some(base) = links::self_url(host, parent)?
        {
            let joined = format!("{}{suffix}", base.as_str().trim_end_matches('/'));
            return Url::parse(&joined)
                .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")));
        }
        route.resolve(host, network_id, None)
    }

    /// Gets the network's notification settings — returns the raw Eero API response.
    /// Live-verified.
    ///
    /// Ported from `eero-api src/eero/api/notifications.py:36-64` (`NotificationsAPI.get_settings`):
    /// sends `GET` `routes::notifications::NOTIFICATIONS_GET_SETTINGS`
    /// (`networks/{network_id}/notifications`). The response's `data` carries one boolean per
    /// event key (e.g. `"network.updated"`, `"device.new"`, `"permissions.updates"`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, [`Error::Validation`]
    /// if the URL cannot be resolved, or whatever status-mapped [`Error`] the request produces.
    pub async fn get_settings(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve(
            &routes::notifications::NOTIFICATIONS_GET_SETTINGS,
            network_id,
            parent,
            "/notifications",
        )?;
        self.transport
            .request(reqwest::Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Sets the network's notification settings — returns the raw Eero API response.
    ///
    /// `settings` is sent **exactly as supplied**: each `(key, value)` pair becomes one JSON
    /// boolean field, dotted keys verbatim (e.g. `"network.updated"`,
    /// `"backup.internet.status.change"`); no closed-vocabulary check is performed.
    ///
    /// Ported from `eero-api src/eero/api/notifications.py:66-111` (`NotificationsAPI.set_settings`):
    /// sends `PUT` `routes::notifications::NOTIFICATIONS_SET_SETTINGS`. Logs one
    /// `warn_uncharacterised_write("set notification settings for network")` immediately before
    /// the request.
    ///
    /// # Errors
    ///
    /// See [`NotificationsApi::get_settings`].
    pub async fn set_settings(
        &self,
        network_id: &str,
        settings: &[(&str, bool)],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve(
            &routes::notifications::NOTIFICATIONS_SET_SETTINGS,
            network_id,
            parent,
            "/notifications",
        )?;
        let mut body = Map::new();
        for (key, value) in settings {
            body.insert((*key).to_owned(), Value::Bool(*value));
        }
        links::warn_uncharacterised_write("set notification settings for network");
        self.transport
            .request(
                reqwest::Method::PUT,
                url,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// Checks whether the network has unread notifications — returns the raw Eero API response.
    /// Live-verified.
    ///
    /// Ported from `eero-api src/eero/api/notifications.py:113-141` (`NotificationsAPI.has_unread`):
    /// sends `GET` `routes::notifications::NOTIFICATIONS_HAS_UNREAD`
    /// (`networks/{network_id}/notifications/has_unread`).
    ///
    /// # Errors
    ///
    /// See [`NotificationsApi::get_settings`].
    pub async fn has_unread(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve(
            &routes::notifications::NOTIFICATIONS_HAS_UNREAD,
            network_id,
            parent,
            "/notifications/has_unread",
        )?;
        self.transport
            .request(reqwest::Method::GET, url, &[], RequestBody::None)
            .await
    }

    /// Marks the network's notifications as read — returns the raw Eero API response.
    ///
    /// Sends the literal two-character body `""` (`RequestBody::EmptyJsonString`), not `json:
    /// {}`, matching Python's `encoding=RequestEncoding.EMPTY_JSON_STRING`.
    ///
    /// Ported from `eero-api src/eero/api/notifications.py:142-178` (`NotificationsAPI.mark_read`):
    /// sends `POST` `routes::notifications::NOTIFICATIONS_MARK_READ`. Logs one
    /// `warn_uncharacterised_write("mark notifications read for network")` immediately before the
    /// request.
    ///
    /// # Errors
    ///
    /// See [`NotificationsApi::get_settings`].
    pub async fn mark_read(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve(
            &routes::notifications::NOTIFICATIONS_MARK_READ,
            network_id,
            parent,
            "/notifications/mark_read",
        )?;
        links::warn_uncharacterised_write("mark notifications read for network");
        self.transport
            .request(
                reqwest::Method::POST,
                url,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// Gets the network's notification history — returns the raw Eero API response.
    /// Live-verified.
    ///
    /// `timestamp`, when `Some`, is sent as the `timestamp` query parameter; omitted entirely
    /// when `None` (Python always passes a `params` dict, empty or not — `notifications.py:
    /// 209-211` — the wire effect is identical either way: no `timestamp` in the query string).
    ///
    /// Ported from `eero-api src/eero/api/notifications.py:180-217` (`NotificationsAPI.get_history`):
    /// sends `GET` `routes::notifications::NOTIFICATIONS_GET_HISTORY`
    /// (`networks/{network_id}/notifications_history`).
    ///
    /// # Errors
    ///
    /// See [`NotificationsApi::get_settings`].
    pub async fn get_history(
        &self,
        network_id: &str,
        timestamp: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url = self.resolve(
            &routes::notifications::NOTIFICATIONS_GET_HISTORY,
            network_id,
            parent,
            "/notifications_history",
        )?;
        let query: Vec<(&str, String)> = timestamp
            .map(|ts| vec![("timestamp", ts.to_owned())])
            .unwrap_or_default();
        self.transport
            .request(reqwest::Method::GET, url, &query, RequestBody::None)
            .await
    }

    /// Sets the account's push notification settings — returns the raw Eero API response.
    ///
    /// The sole account-scoped (not network-scoped) method in this module: no `network_id`, no
    /// `parent`. `settings` is sent **exactly as supplied**; the API's declared keys are
    /// `networkOffline`/`nodeOffline` (camelCase, unlike every other event-key mapping in this
    /// module, which uses dotted names) — no closed-vocabulary check is performed here either.
    ///
    /// Ported from `eero-api src/eero/api/notifications.py:219-249`
    /// (`NotificationsAPI.set_push_settings`): sends `PUT`
    /// `routes::notifications::NOTIFICATIONS_SET_PUSH_SETTINGS` (`account/push_settings`). Logs
    /// one `warn_uncharacterised_write("set account push settings")` immediately before the
    /// request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_push_settings(&self, settings: &[(&str, bool)]) -> Result<Envelope, Error> {
        let mut body = Map::new();
        for (key, value) in settings {
            body.insert((*key).to_owned(), Value::Bool(*value));
        }
        links::warn_uncharacterised_write("set account push settings");
        self.transport
            .resource(
                &routes::notifications::NOTIFICATIONS_SET_PUSH_SETTINGS,
                "",
                None,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }
}
